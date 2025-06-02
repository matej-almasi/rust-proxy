use http::Method;
use regex::Regex;
use rust_proxy::{hyper_client_host::HyperClientHost, test_utils, Proxy};
use tracing_test::traced_test;

#[tokio::test]
async fn proxy_serves_proxied_content() {
    let test_answer = "TEST RESPONSE";

    let proxied_server = setup_proxied_server(test_answer);

    let remote_host = HyperClientHost::new(*proxied_server.address());

    let proxy = Proxy::builder(remote_host)
        .bind(([127, 0, 0, 1], 0).into())
        .await
        .unwrap();

    let test_address = proxy.local_addr().unwrap();

    tokio::spawn(async {
        proxy.run().await;
    });

    let response_text = test_utils::make_simple_request(format!("http://{test_address}")).await;

    assert_eq!(response_text, test_answer);
}

#[tokio::test]
#[traced_test]
async fn proxy_logs_are_captured() {
    let proxied_server = setup_proxied_server("TEST RESPONSE");

    let remote_host = HyperClientHost::new(*proxied_server.address());

    let proxy = Proxy::builder(remote_host)
        .bind(([127, 0, 0, 1], 0).into())
        .await
        .unwrap();

    let test_address = proxy.local_addr().unwrap();

    tokio::spawn(async {
        proxy.run().await;
    });

    test_utils::make_simple_request(format!("http://{test_address}")).await;

    let log_pattern = log_regex();

    logs_assert(|lines: &[&str]| {
        let line = lines.iter().find(|line| line.contains("INFO")).unwrap();
        log_pattern.is_match(line).then_some(()).ok_or(format!(
            "Log line didn't match expected pattern. Line: \"{line}\". Pattern: \"{log_pattern}\""
        ))
    });
}

fn setup_proxied_server(response: &str) -> httpmock::MockServer {
    let test_server = httpmock::MockServer::start();

    test_server.mock(|when, then| {
        when.method(Method::GET.as_str());
        then.status(200).body(response);
    });

    test_server
}

fn log_regex() -> Regex {
    let peer = {
        let ipv4_triplet = r"(2(5[0-5]|[0-4]\d)|1\d\d|\d?\d)";
        let ipv4_addr = format!("({ipv4_triplet}\\.){{3}}{ipv4_triplet}");

        let ipv6_quartet = r"([\dA-E]{0, 4})";
        let ipv6_addr = format!("({ipv6_quartet}?:){{1, 7}}{ipv6_quartet}?");

        let port = r"\d{1, 5}";

        format!("({ipv4_addr}|{ipv6_addr}:{port})")
    };

    let response = {
        let method = r"(GET|HEAD|POST|PUT|DELETE|CONNECT|OPTIONS|TRACE|PATCH)";
        let p_and_q = r"/\S*";
        let version = r"HTTP/(1\.[01]|2\.0|3\.0)";
        let status = r"[1-5]\d\d";
        let body_bytes = r"\d+";
        format!("\"{method} {p_and_q} {version}\" {status} {body_bytes}")
    };

    let referer = r"\S+";

    let user_agent = r"S+";

    Regex::new(&format!("{peer} {response} {referer} {user_agent}")).unwrap()
}
