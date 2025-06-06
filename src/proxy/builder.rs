use std::net::SocketAddr;

use tokio::net::TcpListener;

use super::RemoteHost;
use crate::Proxy;

pub struct ProxyBuilder<R> {
    remote_host: R,
}

impl<R: RemoteHost> ProxyBuilder<R> {
    pub fn new(remote_host: R) -> Self {
        Self { remote_host }
    }

    pub async fn bind(self, listener_addr: SocketAddr) -> crate::Result<Proxy<R>> {
        let listener = TcpListener::bind(listener_addr).await?;

        let proxy = Proxy {
            listener,
            host: self.remote_host,
        };

        Ok(proxy)
    }
}
