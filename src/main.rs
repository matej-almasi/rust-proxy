use std::net::SocketAddr;

use rust_proxy::Proxy;

#[tokio::main]
async fn main() {
    let file_appender = tracing_appender::rolling::never("/logs", "proxy.log");
    let (writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .compact()
        .with_writer(writer)
        .init();

    let self_address = SocketAddr::from(([127, 0, 0, 1], 5609));
    let host_address = SocketAddr::from(([127, 0, 0, 1], 3456));

    let _ = Proxy::builder()
        .bind(self_address, host_address)
        .await
        .expect("Failed to build proxy.");
}
