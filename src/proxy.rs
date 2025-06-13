use std::{io, net::SocketAddr};

use async_trait::async_trait;
use http::{header, HeaderValue};
use hyper::body::{Body, Incoming};
use hyper::Response;
use hyper::{server::conn::http1, Request};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::net::TcpListener;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

use crate::ThreadSafeError;

pub mod builder;
use builder::ProxyBuilder;

mod caching;
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

        let log_on_response = |res: &'_ _, _, _: &'_ _| logging::log_response(res);

        let service = tower::ServiceBuilder::new()
            .map_request(set_request_extensions)
            .layer(TraceLayer::new_for_http().on_response(log_on_response))
            .map_request(move |req| update_redirected_header(req, local_addr))
            .layer_fn(caching::CacheService::new)
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
    let peer_addr = extract_socket_addr_formatted(req.extensions());
    let header_entry = format!("by={local_addr};for={peer_addr}");

    let mut req = req;

    if let Ok(val) = HeaderValue::try_from(&header_entry) {
        req.headers_mut().append(header::FORWARDED, val);
    } else {
        tracing::warn!("Couldn't convert to valid HTTP header: {header_entry}");
    }

    req
}

fn extract_socket_addr_formatted(ext: &http::Extensions) -> String {
    ext.get::<SocketAddr>().map_or_else(
        || {
            tracing::warn!("Couldn't get peer address from response extension.");
            String::from("UNKNOWN")
        },
        ToString::to_string,
    )
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

    #[test]
    fn test_extract_existing_socket_address() {
        let addr = SocketAddr::from(([111, 222, 133, 144], 155));

        let mut ext = http::Extensions::new();
        ext.insert(addr);

        assert_eq!(extract_socket_addr_formatted(&ext), addr.to_string());
    }

    #[test]
    fn test_extract_nonexistent_socket_address() {
        assert_eq!(
            extract_socket_addr_formatted(&http::Extensions::default()),
            "UNKNOWN"
        );
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

    #[derive(Debug, Clone, Default)]
    struct MockRemoteHost {
        received_req: Arc<RwLock<Option<Request<Incoming>>>>,
        response_size: usize,
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
