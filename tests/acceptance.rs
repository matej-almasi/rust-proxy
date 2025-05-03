use rust_proxy::{
    proxy::builder::ProxyBuilder,
    test_utils::{make_simple_request, setup_proxied_server},
};

#[tokio::test]
async fn proxy_serves_proxied_content() {
    let test_address = "127.0.0.1:2000";
    let test_answer = "TEST RESPONSE";

    let proxied_server = setup_proxied_server(test_answer);

    let proxy = ProxyBuilder::new()
        .bind(test_address, proxied_server.address())
        .await
        .unwrap();

    tokio::spawn(async move {
        proxy.run().await;
    });

    let response_text = make_simple_request(format!("http://{test_address}")).await;

    assert_eq!(response_text, test_answer);
}
