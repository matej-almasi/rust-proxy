use anyhow::anyhow;
use tokio::net::{lookup_host, TcpListener, ToSocketAddrs};

use super::Proxy;

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

        Ok(Proxy {
            listener,
            proxied_addr,
        })
    }
}
