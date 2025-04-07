use crate::{error::ErrorKind, upstream::Upstream};
use http_body_util::BodyExt;
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, ToSocketAddrs};

use crate::ProxyError;

pub struct Proxy {
    listener: TcpListener,
    upstream: Upstream,
}

impl Proxy {
    pub async fn bind<L, U>(listener_addr: L, upstream_addr: U) -> crate::Result<Self>
    where
        L: ToSocketAddrs,
        U: ToSocketAddrs,
    {
        let listener = TcpListener::bind(listener_addr).await.map_err(|src| {
            let mut err = ProxyError::new(ErrorKind::ListenerSocketError);
            err.source(Box::new(src));
            err
        })?;

        let upstream = Upstream::bind(upstream_addr).await.unwrap();

        Ok(Self { listener, upstream })
    }

    pub async fn run(&self) {
        loop {
            let Ok((stream, _)) = self.listener.accept().await else {
                continue;
            };
            let io = TokioIo::new(stream);

            let upstream = self.upstream.clone();

            tokio::spawn(async move {
                http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(|req: Request<Incoming>| async {
                            let (parts, body) = req.into_parts();
                            let req = Request::from_parts(parts, body.boxed());
                            upstream.send_request(req).await
                        }),
                    )
                    .await
                    .unwrap();
            });
        }
    }
}

#[cfg(test)]
mod test {
    use http_body_util::{BodyExt, Full};
    use hyper::{body::Bytes, http, Method, Request, Uri};
    use hyper_util::{client::legacy::Client, rt::TokioExecutor};

    use crate::test_utils::setup_proxied_server;

    use super::*;

    #[tokio::test]
    async fn proxy_serves_proxied_content() {
        let test_address = "127.0.0.1:2000";
        let test_answer = "TEST RESPONSE";

        let proxied_server = setup_proxied_server(test_answer);

        let proxy = Proxy::bind(test_address, proxied_server.address())
            .await
            .unwrap();

        tokio::spawn(async move {
            proxy.run().await;
        });

        let response_text = make_simple_request(format!("http://{test_address}")).await;

        assert_eq!(response_text, test_answer);
    }

    async fn make_simple_request<T>(uri: T) -> String
    where
        T: TryInto<Uri>,
        T::Error: Into<http::Error>,
    {
        let test_client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();

        let test_request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Full::default())
            .unwrap();

        let response = test_client.request(test_request).await.unwrap();

        let response_bytes = response.collect().await.unwrap().to_bytes();
        String::from_utf8(response_bytes.into()).unwrap()
    }
}
