use tokio::net::{TcpListener, ToSocketAddrs};

use crate::{error::ErrorKind, upstream::Upstream, ProxyError};

use super::{logger::Logger, Proxy};

#[derive(Default)]
pub struct ProxyBuilder<L>
where
    L: Logger,
{
    logger: Option<L>,
}

impl<L: Logger> ProxyBuilder<L> {
    pub fn new() -> ProxyBuilder<L> {
        ProxyBuilder { logger: None }
    }

    pub fn logger(self, logger: L) -> ProxyBuilder<L> {
        ProxyBuilder {
            logger: Some(logger),
        }
    }

    pub async fn bind<A, B>(self, listener_addr: A, upstream_addr: B) -> crate::Result<Proxy<L>>
    where
        A: ToSocketAddrs,
        B: ToSocketAddrs,
    {
        let Some(logger) = self.logger else {
            return Err(ProxyError::new(ErrorKind::ProxyBuilderError));
        };

        let listener = TcpListener::bind(listener_addr).await.map_err(|src| {
            let mut err = ProxyError::new(ErrorKind::ListenerSocketError);
            err.source(Box::new(src));
            err
        })?;

        let upstream = Upstream::bind(upstream_addr).await?;

        Ok(Proxy {
            listener,
            upstream,
            _logger: logger,
        })
    }
}
