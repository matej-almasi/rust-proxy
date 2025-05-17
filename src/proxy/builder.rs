use std::{net::SocketAddr, time};

use anyhow::anyhow;
use hyper::Response;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use tokio::net::{lookup_host, TcpListener, ToSocketAddrs};
use tower::ServiceExt;
use tower_http::trace::TraceLayer;

use super::{redirect_util::redirect_request, Proxy};

#[derive(Default)]
pub struct ProxyBuilder {}

impl ProxyBuilder {
    pub fn new() -> ProxyBuilder {
        ProxyBuilder {}
    }

    pub async fn bind<A, B>(self, listener_addr: A, proxied_addr: B) -> anyhow::Result<Proxy>
    where
        A: ToSocketAddrs,
        B: ToSocketAddrs,
    {
        let listener = TcpListener::bind(listener_addr).await?;

        let proxied_addr = lookup_host(proxied_addr)
            .await?
            .next()
            .ok_or(anyhow!("Failed to lookup proxied host"))?;

        let service = tower::service_fn(move |req| async move {
            let req = redirect_request(req, proxied_addr)?;
            let client = Client::builder(TokioExecutor::new()).build_http();
            let extensions = req.extensions().to_owned();
            client
                .request(req)
                .await
                .map(|mut resp| {
                    *resp.extensions_mut() = extensions;
                    resp
                })
                .map_err(anyhow::Error::from)
        });

        let tracing = TraceLayer::new_for_http().on_response(
            move |req: &Response<_>, _latency: time::Duration, _: &_| {
                let peer_addr = req
                    .extensions()
                    .get::<SocketAddr>()
                    .map(|addr| addr.ip().to_string())
                    .unwrap_or_else(|| {
                        tracing::warn!("Couldn't get peer address from response extension.");
                        String::from("UNKNOWN")
                    });
                tracing::info!("{} -", peer_addr);
            },
        );

        let service = tower::ServiceBuilder::new()
            .layer(tracing)
            .service(service)
            .boxed_clone();

        Ok(Proxy { listener, service })
    }
}
