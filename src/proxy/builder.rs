use anyhow::anyhow;
use tokio::net::{TcpListener, ToSocketAddrs};

use crate::upstream::Upstream;

use super::{BoxSendLogger, Proxy};

#[derive(Default)]
pub struct ProxyBuilder {
    logger: Option<BoxSendLogger>,
}

impl ProxyBuilder {
    pub fn new() -> ProxyBuilder {
        ProxyBuilder { logger: None }
    }

    pub fn logger(self, logger: BoxSendLogger) -> ProxyBuilder {
        ProxyBuilder {
            logger: Some(logger),
        }
    }

    pub async fn bind<A, B>(self, listener_addr: A, upstream_addr: B) -> anyhow::Result<Proxy>
    where
        A: ToSocketAddrs,
        B: ToSocketAddrs,
    {
        let listener = TcpListener::bind(listener_addr).await?;

        let upstream = Upstream::bind(upstream_addr).await?;

        let Some(logger) = self.logger else {
            return Err(anyhow!("Missing logger for Proxy."));
        };

        Ok(Proxy {
            listener,
            upstream,
            logger,
        })
    }
}
