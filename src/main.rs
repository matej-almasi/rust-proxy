use rust_proxy::{logging::ConsoleLogger, proxy::logger::Severity, Proxy};

#[tokio::main]
async fn main() {
    let logger = Box::new(ConsoleLogger::with_severity(Severity::Debug));

    let _ = Proxy::builder()
        .logger(logger)
        .bind("some", "addr")
        .await
        .expect("Failed to build proxy.");
}
