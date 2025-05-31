use std::net::SocketAddr;

use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use tokio::net::TcpListener;

use super::Proxy;

#[derive(Default)]
pub struct ProxyBuilder {
    proxied_addr: Option<SocketAddr>,
}

impl ProxyBuilder {
    pub fn proxied_addr(mut self, addr: SocketAddr) -> Self {
        self.proxied_addr = Some(addr);
        self
    }

    pub async fn bind(self, listener_addr: SocketAddr) -> Proxy {
        let listener = TcpListener::bind(listener_addr).await.unwrap();

        let client = Client::builder(TokioExecutor::new()).build_http();

        let proxied_addr = self.proxied_addr.unwrap();

        Proxy {
            listener,
            proxied_addr,
            client,
        }
    }
}
