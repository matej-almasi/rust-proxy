use std::fmt::Display;

pub mod proxy;
pub use proxy::Proxy;

pub mod hyper_client_host;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("HTTP Error.")]
    HTTPError(#[from] http::Error),

    #[error("Proxied Host Error")]
    ProxiedHostError(#[from] hyper_util::client::legacy::Error),

    #[error("Socket Error")]
    SocketError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, crate::Error>;

pub trait ThreadSafeError: std::error::Error + Send + Sync + Display {}

impl ThreadSafeError for hyper::Error {}
impl ThreadSafeError for crate::Error {}
