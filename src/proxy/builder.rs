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

    /// Build a proxy, binding it to an address on which it will listen to
    /// incoming requests.
    ///
    /// # Errors
    /// Returns an error if the provided address can't be bound to (typically
    /// because it is already used by another process on the machine).
    pub async fn bind(self, listener_addr: SocketAddr) -> crate::Result<Proxy<R>> {
        let listener = TcpListener::bind(listener_addr).await?;

        let proxy = Proxy {
            listener,
            host: self.remote_host,
        };

        Ok(proxy)
    }
}
