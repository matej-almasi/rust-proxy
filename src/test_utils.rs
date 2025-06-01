use hyper::{body::Bytes, http, Method, Request, Uri};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

pub async fn make_simple_request<T>(uri: T) -> String
where
    T: TryInto<Uri>,
    T::Error: Into<http::Error>,
{
    use http_body_util::{BodyExt, Full};
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
