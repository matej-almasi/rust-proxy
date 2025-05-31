pub mod proxy;
use hyper_util::client;
pub use proxy::Proxy;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("HTTP Error.")]
    HTTPError(#[from] http::Error),

    #[error("Proxied Host Error")]
    ProxiedHostError(#[from] client::legacy::Error),

    #[error("Socket Error")]
    SocketError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, crate::Error>;
