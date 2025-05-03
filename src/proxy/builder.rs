use tokio::net::{TcpListener, ToSocketAddrs};

use super::Proxy;
use crate::upstream::Upstream;

#[derive(Default)]
pub struct ProxyBuilder {}

impl ProxyBuilder {
    pub fn new() -> ProxyBuilder {
        ProxyBuilder {}
    }

    pub async fn bind<A, B>(self, listener_addr: A, upstream_addr: B) -> anyhow::Result<Proxy>
    where
        A: ToSocketAddrs,
        B: ToSocketAddrs,
    {
        let listener = TcpListener::bind(listener_addr).await?;

        let upstream = Upstream::bind(upstream_addr).await?;

        Ok(Proxy { listener, upstream })
    }
}
