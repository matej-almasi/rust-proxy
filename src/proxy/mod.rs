use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tower::ServiceBuilder;

use crate::upstream::Upstream;

pub mod builder;
mod log_service;
pub mod logger;

use builder::ProxyBuilder;
use log_service::LogService;
use logger::Logger;

type BoxSendLogger = Box<dyn Logger + Send + Sync>;

pub struct Proxy {
    listener: TcpListener,
    upstream: Upstream,
    logger: BoxSendLogger,
}

impl Proxy {
    pub fn builder() -> builder::ProxyBuilder {
        ProxyBuilder::new()
    }

    pub async fn run(self) {
        let logger: Arc<dyn Logger + Send + Sync> = Arc::from(self.logger);

        loop {
            let Ok((stream, _)) = self.listener.accept().await else {
                continue;
            };
            let io = TokioIo::new(stream);

            let upstream = self.upstream.clone();

            let logger = logger.clone();

            tokio::spawn(async move {
                let svc = service_fn(|req: Request<Incoming>| async {
                    let (parts, body) = req.into_parts();
                    let req = Request::from_parts(parts, body.boxed());
                    upstream.send_request(req).await
                });

                let svc = ServiceBuilder::new()
                    .layer_fn(|service| LogService::new(service, logger.clone()))
                    .service(svc);

                if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                    logger.critical(&format!("Failed serving connection: {e}"));
                };
            });
        }
    }
}

// Unit tests for the Proxy::run function are omitted as they are essentially
// identical to acceptance tests found in the `tests/acceptance.rs` file.
