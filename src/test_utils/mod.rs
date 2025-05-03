use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, http, Method, Request, Uri};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

pub fn setup_proxied_server(response: &str) -> httpmock::MockServer {
    let test_server = httpmock::MockServer::start();

    test_server.mock(|when, then| {
        when.method(Method::GET.as_str());
        then.status(200).body(response);
    });

    test_server
}

pub async fn make_simple_request<T>(uri: T) -> String
where
    T: TryInto<Uri>,
    T::Error: Into<http::Error>,
{
    let test_client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();

    let test_request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Full::default())
        .unwrap();

    let response = test_client.request(test_request).await.unwrap();

    let response_bytes = response.collect().await.unwrap().to_bytes();
    String::from_utf8(response_bytes.into()).unwrap()
}
