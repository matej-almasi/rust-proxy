use rust_proxy::{logging, proxy::logger::Severity, Proxy};

#[tokio::main]
async fn main() {
    let _ = Proxy::builder()
        .logger(logging::ConsoleLogger::with_severity(Severity::Debug))
        .bind("some", "addr")
        .await
        .unwrap();
}
