use std::env;

use rust_proxy::Proxy;
use tracing_appender::non_blocking::WorkerGuard;

#[tokio::main]
async fn main() {
    let args = env::args().collect::<Vec<_>>();
    let host_address = &args[1];

    let _guard = setup_tracing_to_log_file();

    let proxy = Proxy::builder()
        .bind("127.0.0.1:0", host_address)
        .await
        .unwrap_or_else(|e| panic!("Failed to create proxy: {e}"));

    let test_address = proxy
        .local_addr()
        .unwrap_or_else(|e| panic!("Failed to get own address: {e}"));

    println!("Listening on {test_address}");

    proxy.run().await;
}

fn setup_tracing_to_log_file() -> WorkerGuard {
    let file_appender = tracing_appender::rolling::never("./logs", "proxy.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_ansi(false)
        .with_writer(writer)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    guard
}
