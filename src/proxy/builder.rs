use std::net::SocketAddr;

use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use tokio::net::TcpListener;

use super::Proxy;

pub struct ProxyBuilder {
    proxied_addr: SocketAddr,
}

impl ProxyBuilder {
    pub fn new(proxied_addr: SocketAddr) -> Self {
        Self { proxied_addr }
    }

    pub async fn bind(self, listener_addr: SocketAddr) -> crate::Result<Proxy> {
        let listener = TcpListener::bind(listener_addr).await?;

        let client = Client::builder(TokioExecutor::new()).build_http();

        let proxied_addr = self.proxied_addr;

        let proxy = Proxy {
            listener,
            proxied_addr,
            client,
        };

        Ok(proxy)
    }
}
