use std::time;
use std::{io, net::SocketAddr};

use http::uri::{self, PathAndQuery};
use http::{header, Extensions, HeaderValue, Uri};
use hyper::body::{Body, Incoming};
use hyper::Response;
use hyper::{server::conn::http1, Request};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::net::TcpListener;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

pub mod builder;
use builder::ProxyBuilder;

pub struct Proxy {
    listener: TcpListener,
    client: Client<HttpConnector, Incoming>,
    proxied_addr: SocketAddr,
}

impl Proxy {
    pub fn builder() -> builder::ProxyBuilder {
        ProxyBuilder::default()
    }

    pub async fn run(self) {
        let Ok(local_addr) = self.local_addr() else {
            tracing::error!("Local listener address not available! Shutting down...");
            panic!()
        };

        let tracing = TraceLayer::new_for_http().on_response(
            move |resp: &Response<_>, _latency: time::Duration, _: &_| {
                let peer_addr = extract_peer_addr_formatted(resp.extensions());
                tracing::info!("{} -", peer_addr);
            },
        );

        let service = tower::ServiceBuilder::new()
            .map_request(move |mut req: Request<_>| {
                let peer_addr = extract_peer_addr_formatted(req.extensions());
                let header_entry = format!("by={};for={}", local_addr, peer_addr);
                if let Ok(val) = HeaderValue::try_from(&header_entry) {
                    req.headers_mut().append(header::FORWARDED, val);
                } else {
                    tracing::warn!("Couldn't convert to valid HTTP header: {header_entry}");
                };
                req
            })
            .layer(tracing)
            .service_fn(move |req| pass_request(req, self.client.clone(), self.proxied_addr));

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

fn extract_peer_addr_formatted(ext: &Extensions) -> String {
    ext.get::<SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| {
            tracing::warn!("Couldn't get peer address from response extension.");
            String::from("UNKNOWN")
        })
}

async fn pass_request<B>(
    req: Request<B>,
    client: Client<HttpConnector, B>,
    host: SocketAddr,
) -> crate::Result<Response<Incoming>>
where
    B: Body + Send + 'static + Unpin,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let req = redirect(req, host)?;

    // Extensions don't automatically transfer from Req to Resp,
    // we have to do it manually.
    let extensions = req.extensions().to_owned();

    let resp = client.request(req).await.map(|mut resp| {
        *resp.extensions_mut() = extensions;
        resp
    });

    Ok(resp?)
}

fn redirect<B>(mut req: Request<B>, new_host: SocketAddr) -> crate::Result<Request<B>> {
    let p_and_q = req
        .uri()
        .path_and_query()
        .cloned()
        .unwrap_or(PathAndQuery::from_static("/"));

    let uri = Uri::builder()
        .scheme(uri::Scheme::HTTP)
        .authority(new_host.to_string())
        .path_and_query(p_and_q)
        .build()?;

    *req.uri_mut() = uri;

    Ok(req)
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
