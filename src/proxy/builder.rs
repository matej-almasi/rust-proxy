use std::net::SocketAddr;

use hyper::body::Body;
use tokio::net::TcpListener;

use super::{hyper_client_host::HyperClientHost, Proxy};
use crate::ThreadSafeError;

pub struct ProxyBuilder<B> {
    remote_host: HyperClientHost<B>,
}

impl<B> ProxyBuilder<B>
where
    B: Body + Send,
    B::Data: Send,
    B::Error: ThreadSafeError,
{
    pub fn new(proxied_addr: SocketAddr) -> Self {
        let remote_host = HyperClientHost::new(proxied_addr);
        Self { remote_host }
    }

    pub async fn bind(self, listener_addr: SocketAddr) -> crate::Result<Proxy<HyperClientHost<B>>> {
        let listener = TcpListener::bind(listener_addr).await?;

        let proxy = Proxy {
            listener,
            host: self.remote_host,
        };

        Ok(proxy)
    }
}
