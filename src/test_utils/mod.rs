use hyper::Method;

pub fn setup_proxied_server(response: &str) -> httpmock::MockServer {
    let test_server = httpmock::MockServer::start();

    test_server.mock(|when, then| {
        when.method(Method::GET.as_str());
        then.status(200).body(response);
    });

    test_server
}
