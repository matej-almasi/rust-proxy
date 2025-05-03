use rust_proxy::Proxy;

#[tokio::main]
async fn main() {
    let file_appender = tracing_appender::rolling::never("/logs", "proxy.log");
    let (writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .compact()
        .with_writer(writer)
        .init();

    let _ = Proxy::builder()
        .bind("some", "addr")
        .await
        .expect("Failed to build proxy.");
}
