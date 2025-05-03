use std::{io, net::SocketAddr, time::Duration};

use http_body_util::BodyExt;
use hyper::{body::Incoming, server::conn::http1, Request};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::upstream::Upstream;

pub mod builder;

use builder::ProxyBuilder;

pub struct Proxy {
    listener: TcpListener,
    upstream: Upstream,
}

impl Proxy {
    pub fn builder() -> builder::ProxyBuilder {
        ProxyBuilder::new()
    }

    pub async fn run(self) {
        loop {
            let Ok((stream, client)) = self.listener.accept().await else {
                continue;
            };
            let io = TokioIo::new(stream);

            let upstream = self.upstream.clone();

            tokio::spawn(async move {
                let svc = tower::service_fn(|req: Request<Incoming>| async {
                    let (parts, body) = req.into_parts();
                    let req = Request::from_parts(parts, body.boxed());
                    upstream.send_request(req).await
                });

                let tracing = TraceLayer::new_for_http().on_request(()).on_response(
                    |_: &_, _latency: Duration, _: &_| tracing::info!("{} -", client.ip()),
                );

                let svc = ServiceBuilder::new().layer(tracing).service(svc);

                let svc = TowerToHyperService::new(svc);

                if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                    eprintln!("Failed serving connection: {e}");
                };
            });
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    // Unit tests for the Proxy::run function are omitted as they are essentially
    // identical to acceptance tests found in the `tests/acceptance.rs` file.

    #[tokio::test]
    async fn local_addr_is_correct() {
        let listener = TcpListener::bind("localhost:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        let upstream = Upstream::bind("localhost:0").await.unwrap();

        let proxy = Proxy { listener, upstream };

        assert_eq!(listener_addr, proxy.local_addr().unwrap());
    }
}
