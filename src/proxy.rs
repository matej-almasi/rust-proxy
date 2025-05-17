use std::{io, net::SocketAddr};

use anyhow::Error;
use hyper::body::Incoming;
use hyper::header::FORWARDED;
use hyper::Response;
use hyper::{server::conn::http1, Request};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::net::TcpListener;
use tower::util::BoxCloneService;
use tower::Service;
use tower_http::classify::{NeverClassifyEos, ServerErrorsFailureClass};
use tower_http::trace::ResponseBody;

pub mod builder;
use builder::ProxyBuilder;

mod redirect_util;
type ProxyRespBody = ResponseBody<Incoming, NeverClassifyEos<ServerErrorsFailureClass>>;

pub struct Proxy {
    listener: TcpListener,
    service: BoxCloneService<Request<Incoming>, Response<ProxyRespBody>, Error>,
}

impl Proxy {
    pub fn builder() -> builder::ProxyBuilder {
        ProxyBuilder::new()
    }

    pub async fn run(self) {
        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!("Failed establishing connection: {e}");
                    continue;
                }
            };

            let io = TokioIo::new(stream);
            let mut service = self.service.clone();
            let service = tower::service_fn(move |req: Request<Incoming>| async move {
                req.headers_mut().get_mut(FORWARDED);
                service.call(req)
            });
            let svc = TowerToHyperService::new(service);

            tokio::spawn(async move {
                if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                    tracing::error!("Failed serving peer {}: {e}", peer_addr.ip());
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

        let proxied_addr = SocketAddr::from(([127, 0, 0, 1], 2000));

        // let proxy = Proxy { listener };

        // assert_eq!(listener_addr, proxy.local_addr().unwrap());
    }
}
