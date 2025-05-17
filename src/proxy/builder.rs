use std::time;

use anyhow::anyhow;
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
            client.request(req).await.map_err(anyhow::Error::from)
        });

        let tracing = TraceLayer::new_for_http().on_request(()).on_response(
            move |_: &_, _latency: time::Duration, _: &_| tracing::info!("{} -", peer_addr.ip()),
        );

        let service = tower::ServiceBuilder::new()
            .layer(tracing)
            .service(service)
            .boxed_clone();

        Ok(Proxy { listener, service })
    }
}
