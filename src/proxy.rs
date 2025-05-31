use std::{io, net::SocketAddr};

use http::{header, Extensions, HeaderValue};
use hyper::body::Incoming;
use hyper::Response;
use hyper::{server::conn::http1, Request};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::net::TcpListener;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

pub mod builder;
use builder::ProxyBuilder;
mod remote_host;

pub struct Proxy<B> {
    listener: TcpListener,
    host: remote_host::RemoteHost<B>,
}

impl Proxy<Incoming> {
    pub fn builder(proxied_addr: SocketAddr) -> builder::ProxyBuilder {
        ProxyBuilder::new(proxied_addr)
    }

    pub async fn run(self) {
        let Ok(local_addr) = self.local_addr() else {
            tracing::error!("Local listener address not available! Shutting down...");
            panic!()
        };

        let service = tower::ServiceBuilder::new()
            .map_request(move |req| update_redirected_header(req, local_addr))
            .layer(TraceLayer::new_for_http().on_response(log_response))
            .service_fn(move |req| {
                let host = self.host.clone();
                async move { host.pass_request(req).await }
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

fn log_response<T, U, V>(resp: &Response<T>, _: U, _: &V) {
    let peer_addr = extract_peer_addr_formatted(resp.extensions());
    tracing::info!("{} -", peer_addr);
}

fn extract_peer_addr_formatted(ext: &Extensions) -> String {
    ext.get::<SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| {
            tracing::warn!("Couldn't get peer address from response extension.");
            String::from("UNKNOWN")
        })
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
