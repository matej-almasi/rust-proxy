pub mod proxy;
pub use proxy::Proxy;

pub mod upstream;

pub mod error;
pub use error::ProxyError;

pub type Result<T> = std::result::Result<T, ProxyError>;

pub mod logging;

#[cfg(test)]
mod test_utils;
