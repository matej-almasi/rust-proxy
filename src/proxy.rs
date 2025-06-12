use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use std::vec::IntoIter;
use std::{io, net::SocketAddr};
use std::{mem, task};

use async_trait::async_trait;
use dashmap::DashMap;
use futures::stream::{self, Iter};
use http::{header, HeaderValue, Uri};
use http_body_util::StreamBody;
use hyper::body::{Body, Frame, Incoming};
use hyper::Response;
use hyper::{server::conn::http1, Request};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, UnboundedSender};
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

use crate::ThreadSafeError;

pub mod builder;
use builder::ProxyBuilder;

mod logging;

pub struct Proxy<R> {
    listener: TcpListener,
    host: R,
}

impl<R: RemoteHost> Proxy<R> {
    pub fn builder(remote_host: R) -> ProxyBuilder<R> {
        ProxyBuilder::new(remote_host)
    }

    /// Run the Proxy
    ///
    /// # Panics
    /// Panics if the local address of the listener is not available. This may
    /// only occur at startup, before any requests are served.
    pub async fn run(self) {
        let Ok(local_addr) = self.local_addr() else {
            tracing::error!("Local listener address not available! Shutting down...");
            panic!()
        };

        let service = tower::ServiceBuilder::new()
            .map_request(set_request_extensions)
            .layer(TraceLayer::new_for_http().on_response(logging::log_response))
            .map_request(move |req| update_redirected_header(req, local_addr))
            .layer_fn(CacheService::new)
            .service_fn(move |req: Request<_>| {
                let host = self.host.clone();
                let extensions = req.extensions().to_owned();

                async move {
                    host.pass_request(req).await.map(|mut resp| {
                        *resp.extensions_mut() = extensions;
                        resp
                    })
                }
            });

        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!("Failed establishing connection: {e}");
                    continue;
                }
            };

            let service = tower::ServiceBuilder::new()
                .layer(AddExtensionLayer::new(peer_addr))
                .service(service.clone());

            let service = TowerToHyperService::new(service);

            let io = TokioIo::new(stream);

            tokio::spawn(async move {
                if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                    tracing::error!("Failed serving peer {}: {e}", peer_addr.ip());
                }
            });
        }
    }

    /// Returns the local address on which the proxy listens.
    ///
    /// # Errors
    /// Returns an error if the local listener address is not available.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

#[async_trait]
pub trait RemoteHost: Send + Clone + 'static {
    type Error: ThreadSafeError;
    type ResponseBody: Body<Data: Send + Clone, Error: ThreadSafeError> + Send + Unpin;

    async fn pass_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<Self::ResponseBody>, Self::Error>;
}

#[derive(Debug, Clone)]
struct CacheService<S> {
    inner: S,
}

impl<S> CacheService<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S, RqB, RsB> tower::Service<Request<RqB>> for CacheService<S>
where
    S: tower::Service<Request<RqB>, Response = Response<RsB>> + Send + Clone + 'static,
    S::Future: Send,
    S::Response: Send,
    S::Error: std::error::Error,
    RqB: Send + 'static,
    RsB: Body + Send + 'static,
    RsB::Data: Send + 'static,
{
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Response<CacheableBody<RsB>>;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<RqB>) -> Self::Future {
        // the clone may not be ready, calling call on it may panic.
        // Therefore, we have to use the original service (which is guaranteed
        // to be ready if the user called poll_ready).
        let inner = self.inner.clone();
        let mut inner = mem::replace(&mut self.inner, inner);

        let fut = async move {
            let cache = DashMap::<Uri, Response<CacheableBody<RsB>>>::new();
            let uri = req.uri().to_owned();

            let (parts, body) = inner.call(req).await?.into_parts();

            let (tx, mut rx) = mpsc::unbounded_channel();

            let cached_parts = parts.clone();

            tokio::spawn(async move {
                let mut frames = Vec::new();

                while let Some(frame) = rx.recv().await {
                    frames.push(frame);
                }

                let body = CacheableBody::CachedBody {
                    inner: StreamBody::new(stream::iter(frames)),
                };

                cache.insert(uri, Response::from_parts(cached_parts, body));
            });

            let caching_body = CacheableBody::CachingBody {
                inner: body,
                cache_tx: tx,
            };

            Ok(Response::from_parts(parts, caching_body))
        };

        Box::pin(fut)
    }
}

type ResultFrame<D> = crate::Result<Frame<D>>;

enum CacheableBody<B: Body> {
    CachedBody {
        inner: StreamBody<Iter<IntoIter<ResultFrame<B::Data>>>>,
    },

    CachingBody {
        inner: B,
        cache_tx: UnboundedSender<ResultFrame<B::Data>>,
    },
}

impl<B> Body for CacheableBody<B>
where
    B: Body + Unpin,
    B::Data: Clone,
{
    type Data = B::Data;
    type Error = crate::Error;

    fn is_end_stream(&self) -> bool {
        match self {
            Self::CachingBody { inner, .. } => inner.is_end_stream(),
            Self::CachedBody { inner } => inner.is_end_stream(),
        }
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        match self {
            Self::CachingBody { inner, .. } => inner.size_hint(),
            Self::CachedBody { inner } => inner.size_hint(),
        }
    }

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        match *self {
            Self::CachedBody { ref mut inner } => Pin::new(inner).poll_frame(cx),
            Self::CachingBody {
                ref mut inner,
                ref cache_tx,
            } => {
                let poll = Pin::new(inner)
                    .poll_frame(cx)
                    .map_err(|_| crate::Error::CachingError);

                if let Poll::Ready(Some(ref frame)) = &poll {
                    let frame = frame
                        .as_ref()
                        .map(|frame| {
                            if frame.is_data() {
                                Frame::data(frame.data_ref().unwrap().clone())
                            } else {
                                Frame::trailers(frame.trailers_ref().unwrap().clone())
                            }
                        })
                        .map_err(|_| crate::Error::CachingError);

                    if let Err(e) = cache_tx.send(frame) {
                        tracing::warn!("Couldn't send cache body: {e}");
                    };
                }

                poll
            }
        }
    }
}

fn set_request_extensions<T>(mut req: Request<T>) -> Request<T> {
    let method = req.method().to_owned();
    let p_and_q = req.uri().path_and_query().map(ToOwned::to_owned);
    let headers = req.headers().to_owned();

    req.extensions_mut().insert(method);
    req.extensions_mut().insert(p_and_q);
    req.extensions_mut().insert(headers);

    req
}

fn update_redirected_header<T>(req: Request<T>, local_addr: SocketAddr) -> Request<T> {
    let peer_addr = logging::extract_socket_addr_formatted(req.extensions());
    let header_entry = format!("by={local_addr};for={peer_addr}");

    let mut req = req;

    if let Ok(val) = HeaderValue::try_from(&header_entry) {
        req.headers_mut().append(header::FORWARDED, val);
    } else {
        tracing::warn!("Couldn't convert to valid HTTP header: {header_entry}");
    }

    req
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, RwLock};

    use bytes::Bytes;
    use http::Uri;
    use http_body_util::Full;
    use hyper_util::{
        client::legacy::{connect::HttpConnector, Client},
        rt::TokioExecutor,
    };
    use regex::Regex;
    use tracing_test::traced_test;

    use super::*;
    // Unit tests for the Proxy::run function are omitted as they are essentially
    // identical to acceptance tests found in the `tests/acceptance.rs` file.

    #[tokio::test]
    async fn local_addr_is_correct() -> anyhow::Result<()> {
        let listener = TcpListener::bind("localhost:0").await?;
        let listener_addr = listener.local_addr()?;

        let host = MockRemoteHost::default();
        let proxy = Proxy { listener, host };

        assert_eq!(listener_addr, proxy.local_addr()?);

        Ok(())
    }

    #[tokio::test]
    async fn redirected_header_is_inserted() -> anyhow::Result<()> {
        let remote_host = MockRemoteHost::default();
        let test_address = setup_test_proxy(remote_host.clone()).await?;

        let uri = Uri::try_from(format!("http://{test_address}"))?;
        oneshot_request_proxy(uri).await?;

        let redirected_req = remote_host.received_req.write().unwrap().take().unwrap();
        let header_value = redirected_req
            .headers()
            .get(header::FORWARDED)
            .unwrap()
            .to_str()?;

        let pat = Regex::new(&format!(r"^by={test_address};for=127.0.0.1:\d{{1, 5}}"))?;

        assert!(
            pat.is_match(header_value),
            "Incorrect header: {header_value}"
        );

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_peer_address() -> anyhow::Result<()> {
        let test_address = setup_test_proxy(MockRemoteHost::default()).await?;

        let uri = Uri::try_from(format!("http://{test_address}"))?;
        oneshot_request_proxy(uri).await?;

        logs_assert(|lines| {
            check_log_contains(lines, "127.0.0.1:")?
                .then_some(())
                .ok_or(String::from("Peer address not found in log entry."))
        });

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_method() -> anyhow::Result<()> {
        let test_address = setup_test_proxy(MockRemoteHost::default()).await?;

        let uri = Uri::try_from(format!("http://{test_address}"))?;
        oneshot_request_proxy(uri).await?;

        assert!(logs_contain("GET"));

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_path_and_query() -> anyhow::Result<()> {
        let test_p_and_q = "/some/path?johnny=12";

        let test_address = setup_test_proxy(MockRemoteHost::default()).await?;

        let uri = Uri::try_from(format!("http://{test_address}{test_p_and_q}"))?;
        oneshot_request_proxy(uri).await?;

        assert!(logs_contain(test_p_and_q));

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_http_version() -> anyhow::Result<()> {
        let test_address = setup_test_proxy(MockRemoteHost::default()).await?;

        let uri = Uri::try_from(format!("http://{test_address}"))?;
        oneshot_request_proxy(uri).await?;

        assert!(logs_contain("HTTP/2.0"));

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_status() -> anyhow::Result<()> {
        let test_address = setup_test_proxy(MockRemoteHost::default()).await?;

        let uri = Uri::try_from(format!("http://{test_address}"))?;
        oneshot_request_proxy(uri).await?;

        assert!(logs_contain("200"));

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_response_size() -> anyhow::Result<()> {
        let response_size = 1_234_567;

        let remote_host = MockRemoteHost::with_response_size(response_size);
        let test_address = setup_test_proxy(remote_host.clone()).await?;

        let uri = Uri::try_from(format!("http://{test_address}"))?;
        oneshot_request_proxy(uri).await?;

        assert!(logs_contain(&response_size.to_string()));

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_referrer() -> anyhow::Result<()> {
        let referrer = "acmecompany.com/referrer";

        let remote_host = MockRemoteHost::default();
        let test_address = setup_test_proxy(remote_host.clone()).await?;

        let mut test_request = Request::<Full<Bytes>>::default();
        *test_request.uri_mut() = Uri::try_from(format!("http://{test_address}"))?;

        test_request
            .headers_mut()
            .append(header::REFERER, HeaderValue::from_static(referrer));

        test_client().request(test_request).await?;

        assert!(logs_contain(referrer));

        Ok(())
    }
    #[tokio::test]
    #[traced_test]
    async fn logs_contain_user_agent() -> anyhow::Result<()> {
        let user_agent = "firefox/1.0";

        let remote_host = MockRemoteHost::default();
        let test_address = setup_test_proxy(remote_host.clone()).await?;

        let mut test_request = Request::<Full<Bytes>>::default();
        *test_request.uri_mut() = Uri::try_from(format!("http://{test_address}"))?;

        test_request
            .headers_mut()
            .append(header::USER_AGENT, HeaderValue::from_static(user_agent));

        test_client().request(test_request).await?;

        assert!(logs_contain(user_agent));

        Ok(())
    }

    async fn oneshot_request_proxy(uri: Uri) -> anyhow::Result<Response<Incoming>> {
        let mut test_request = Request::<Full<Bytes>>::default();
        *test_request.uri_mut() = uri;
        Ok(test_client().request(test_request).await?)
    }

    async fn setup_test_proxy(remote_host: MockRemoteHost) -> crate::Result<SocketAddr> {
        let proxy = Proxy::builder(remote_host)
            .bind(([127, 0, 0, 1], 0).into())
            .await?;

        let test_address = proxy.local_addr()?;

        tokio::spawn(async {
            proxy.run().await;
        });

        Ok(test_address)
    }

    fn test_client<B>() -> Client<HttpConnector, B>
    where
        B: Body + Send + Unpin + 'static,
        B::Data: Send,
        B::Error: std::error::Error + Send + Sync,
    {
        Client::builder(TokioExecutor::new()).build_http()
    }

    fn check_log_contains(lines: &[&str], val: &str) -> Result<bool, String> {
        Ok(lines
            .iter()
            .find(|line| line.contains("INFO"))
            .ok_or(String::from("No proxy logging line found in logs."))?
            .contains(val))
    }

    #[derive(Debug, Clone, Default)]
    struct MockRemoteHost {
        received_req: Arc<RwLock<Option<Request<Incoming>>>>,
        response_size: usize,
    }

    impl MockRemoteHost {
        fn with_response_size(size: usize) -> Self {
            Self {
                response_size: size,
                ..Default::default()
            }
        }
    }

    #[async_trait]
    impl RemoteHost for MockRemoteHost {
        type Error = crate::Error;
        type ResponseBody = Full<Bytes>;

        async fn pass_request(
            &self,
            req: Request<Incoming>,
        ) -> Result<Response<Self::ResponseBody>, Self::Error> {
            *self.received_req.write().unwrap() = Some(req);
            let response = Bytes::from_owner(vec![1; self.response_size]);
            Ok(Response::new(Full::from(response)))
        }
    }
}
