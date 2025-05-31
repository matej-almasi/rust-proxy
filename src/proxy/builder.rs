use std::net::SocketAddr;

use hyper::body::Body;
use tokio::net::TcpListener;

use super::{remote_host, Proxy};

pub struct ProxyBuilder {
    proxied_addr: SocketAddr,
}

impl ProxyBuilder {
    pub fn new(proxied_addr: SocketAddr) -> Self {
        Self { proxied_addr }
    }

    pub async fn bind<B>(self, listener_addr: SocketAddr) -> crate::Result<Proxy<B>>
    where
        B: Body + Send + 'static + Unpin,
        B::Data: Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let listener = TcpListener::bind(listener_addr).await?;

        let proxy = Proxy {
            listener,
            host: remote_host::RemoteHost::new(self.proxied_addr),
        };

        Ok(proxy)
    }
}
