use std::io::{Read, Seek};

mod utils;

use rust_proxy::proxy::builder::ProxyBuilder;

#[tokio::test]
async fn proxy_serves_proxied_content() {
    let test_answer = "TEST RESPONSE";

    let proxied_server = utils::setup_proxied_server(test_answer);

    let proxy = ProxyBuilder::new(*proxied_server.address())
        .bind(([127, 0, 0, 1], 0).into())
        .await
        .unwrap();

    let test_address = proxy.local_addr().unwrap();

    tokio::spawn(async move {
        proxy.run().await;
    });

    let response_text = utils::make_simple_request(format!("http://{test_address}")).await;

    assert_eq!(response_text, test_answer);
}

#[tokio::test]
async fn proxy_logs_are_captured() {
    let logfile = tempfile::tempfile().unwrap();
    let mut logfile_reader = logfile.try_clone().unwrap();

    {
        let (writer, _guard) = tracing_appender::non_blocking(logfile);

        let subscriber = tracing_subscriber::fmt()
            .compact()
            .with_ansi(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::set_global_default(subscriber).unwrap();

        let proxied_server = utils::setup_proxied_server("TEST RESPONSE");

        let proxy = ProxyBuilder::new(*proxied_server.address())
            .bind(([127, 0, 0, 1], 0).into())
            .await
            .unwrap();

        let test_address = proxy.local_addr().unwrap();

        tokio::spawn(async move {
            proxy.run().await;
        });

        utils::make_simple_request(format!("http://{test_address}")).await;
    }

    logfile_reader.seek(std::io::SeekFrom::Start(0)).unwrap();

    let mut log_content = String::new();
    logfile_reader.read_to_string(&mut log_content).unwrap();

    println!("{log_content}");

    assert!(log_content.contains("127.0.0.1 -"));
}
