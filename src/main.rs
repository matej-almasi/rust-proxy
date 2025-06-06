use std::{env, net::SocketAddr, str::FromStr};

use rust_proxy::{hyper_client_host::HyperClientHost, Proxy};
use tracing_appender::non_blocking::WorkerGuard;

#[tokio::main]
async fn main() {
    let _guard = setup_tracing_to_log_file();

    let args = env::args().collect::<Vec<_>>();
    let host_address = SocketAddr::from_str(&args[1]).unwrap();

    let remote_host = HyperClientHost::new(host_address);

    let proxy = Proxy::builder(remote_host)
        .bind(([127, 0, 0, 1], 0).into())
        .await
        .unwrap();

    let test_address = proxy.local_addr().unwrap();

    println!("Listening on {test_address}");

    proxy.run().await;
}

fn setup_tracing_to_log_file() -> WorkerGuard {
    let file_appender = tracing_appender::rolling::never("./logs", "proxy.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_ansi(false)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_writer(writer)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    guard
}
