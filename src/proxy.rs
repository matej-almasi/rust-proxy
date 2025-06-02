use std::{io, net::SocketAddr};

use async_trait::async_trait;
use http::uri::PathAndQuery;
use http::{header, Extensions, HeaderValue};
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

pub struct Proxy<R> {
    listener: TcpListener,
    host: R,
}

impl<R: RemoteHost> Proxy<R> {
    pub fn builder(remote_host: R) -> ProxyBuilder<R> {
        ProxyBuilder::new(remote_host)
    }

    pub async fn run(self) {
        let Ok(local_addr) = self.local_addr() else {
            tracing::error!("Local listener address not available! Shutting down...");
            panic!()
        };

        let service = tower::ServiceBuilder::new()
            .map_request(set_request_extensions)
            .map_request(move |req| update_redirected_header(req, local_addr))
            .layer(TraceLayer::new_for_http().on_response(log_response))
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
                };
            });
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

#[async_trait]
pub trait RemoteHost: Send + Clone + 'static {
    type Error: ThreadSafeError;
    type ResponseBody: Body<Data: Send, Error: ThreadSafeError> + Send;

    async fn pass_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<Self::ResponseBody>, Self::Error>;
}

fn update_redirected_header<T>(req: Request<T>, local_addr: SocketAddr) -> Request<T> {
    let peer_addr = extract_peer_addr_formatted(req.extensions());
    let header_entry = format!("by={};for={}", local_addr, peer_addr);

    let mut req = req;

    if let Ok(val) = HeaderValue::try_from(&header_entry) {
        req.headers_mut().append(header::FORWARDED, val);
    } else {
        tracing::warn!("Couldn't convert to valid HTTP header: {header_entry}");
    };

    req
}

fn set_request_extensions<T>(mut req: Request<T>) -> Request<T> {
    let method = req.method().to_owned();
    let p_and_q = req.uri().path_and_query().map(|v| v.to_owned());
    req.extensions_mut().insert(method);
    req.extensions_mut().insert(p_and_q);
    req
}

fn log_response<T, U, V>(resp: &Response<T>, _: U, _: &V) {
    let peer_addr = extract_peer_addr_formatted(resp.extensions());
    let method = extract_method_formatted(resp.extensions());
    let p_and_q = extract_p_and_q_formatted(resp.extensions());
    tracing::info!("{peer_addr} {method} {p_and_q}");
}

fn extract_peer_addr_formatted(ext: &Extensions) -> String {
    ext.get::<SocketAddr>()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| {
            tracing::warn!("Couldn't get peer address from response extension.");
            String::from("UNKNOWN")
        })
}

fn extract_method_formatted(ext: &Extensions) -> String {
    ext.get::<http::Method>()
        .map(|method| method.to_string())
        .unwrap_or_else(|| {
            tracing::warn!("Couldn't get http method for request.");
            String::from("UNKNOWN")
        })
}

fn extract_p_and_q_formatted(ext: &Extensions) -> String {
    ext.get::<Option<PathAndQuery>>()
        .map(|maybe_pq| {
            maybe_pq
                .as_ref()
                .map(|pq| pq.to_string())
                .unwrap_or_default()
        })
        .unwrap_or_else(|| {
            tracing::warn!("Couldn't get http path and query for request.");
            String::from("UNKNOWN")
        })
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, RwLock};

    use regex::Regex;
    use tracing_test::traced_test;

    use super::*;
    use crate::test_utils;
    // Unit tests for the Proxy::run function are omitted as they are essentially
    // identical to acceptance tests found in the `tests/acceptance.rs` file.

    #[tokio::test]
    async fn local_addr_is_correct() {
        let listener = TcpListener::bind("localhost:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        let host = MockRemoteHost::default();
        let proxy = Proxy { listener, host };

        assert_eq!(listener_addr, proxy.local_addr().unwrap());
    }

    #[tokio::test]
    async fn redirected_header_is_inserted() {
        let (remote_host, test_address) = setup_test_proxy().await;
        test_utils::make_simple_request(format!("http://{test_address}")).await;

        let redirected_req = remote_host.received_req.write().unwrap().take().unwrap();
        let header_value = redirected_req
            .headers()
            .get(header::FORWARDED)
            .unwrap()
            .to_str()
            .unwrap();

        let pat = Regex::new(&format!(r"^by={};for=127.0.0.1:\d{{1, 5}}", test_address)).unwrap();

        assert!(
            pat.is_match(header_value),
            "Incorrect header: {header_value}"
        );
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_peer_address() {
        let (_, test_address) = setup_test_proxy().await;
        test_utils::make_simple_request(format!("http://{test_address}")).await;

        logs_assert(|lines| {
            check_log_contains(lines, "127.0.0.1:")?
                .then_some(())
                .ok_or(String::from("Peer address not found in log entry."))
        });
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_method() {
        let (_, test_address) = setup_test_proxy().await;
        test_utils::make_simple_request(format!("http://{test_address}")).await;

        assert!(logs_contain("GET"))
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_path_and_query() {
        let test_p_and_q = "/some/path?johnny=12";

        let (_, test_address) = setup_test_proxy().await;
        test_utils::make_simple_request(format!("http://{test_address}{test_p_and_q}")).await;

        assert!(logs_contain(test_p_and_q))
    }

    async fn setup_test_proxy() -> (MockRemoteHost, SocketAddr) {
        let remote_host = MockRemoteHost::default();

        let proxy = Proxy::builder(remote_host.clone())
            .bind(([127, 0, 0, 1], 0).into())
            .await
            .unwrap();

        let test_address = proxy.local_addr().unwrap();

        tokio::spawn(async {
            proxy.run().await;
        });

        (remote_host, test_address)
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
    }

    #[async_trait]
    impl RemoteHost for MockRemoteHost {
        type Error = crate::Error;
        type ResponseBody = http_body_util::Empty<bytes::Bytes>;

        async fn pass_request(
            &self,
            req: Request<Incoming>,
        ) -> Result<Response<Self::ResponseBody>, Self::Error> {
            *self.received_req.write().unwrap() = Some(req);
            Ok(Response::new(http_body_util::Empty::new()))
        }
    }
}
