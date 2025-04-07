use std::net::SocketAddr;

use http_body_util::combinators::BoxBody;
use hyper::{
    body::{Bytes, Incoming},
    http::uri,
    Request, Response, Uri,
};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use tokio::net::{lookup_host, ToSocketAddrs};

use crate::error::{ErrorKind, ProxyError};

type RequestBody = BoxBody<Bytes, hyper::Error>;

#[derive(Clone)]
pub struct Upstream {
    address: SocketAddr,
    client: Client<HttpConnector, RequestBody>,
}

impl Upstream {
    pub async fn bind<A: ToSocketAddrs>(address: A) -> crate::Result<Self> {
        let mut found_sockets = lookup_host(address).await.map_err(|src| {
            let mut error = ProxyError::new(ErrorKind::UpstreamHostDNSResolutionError);
            error.source(Box::new(src));
            error
        })?;

        let Some(address) = found_sockets.next() else {
            return Err(ProxyError::new(ErrorKind::UpstreamHostNotFound));
        };

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(None)
            .pool_max_idle_per_host(32)
            // .http2_only(true)
            // .http2_keep_alive_interval(Duration::from_secs(20))
            // .http2_keep_alive_while_idle(true)
            .build_http::<RequestBody>();

        Ok(Self { address, client })
    }

    pub async fn send_request(
        &self,
        mut request: Request<RequestBody>,
    ) -> crate::Result<Response<Incoming>> {
        request = self.update_uri(request);

        self.client
            .request(request)
            .await
            .map_err(|_| ProxyError::new(ErrorKind::UpstreamRequestFail))
    }

    fn update_uri<B>(&self, mut request: Request<B>) -> Request<B> {
        let mut uri_parts = request.uri().clone().into_parts();

        let authority = uri_parts
            .authority
            .take()
            .map(|auth| auth.to_string())
            .unwrap_or_default();

        let userinfo = authority
            .rsplit_once('@')
            .map(|(userinfo, _)| userinfo.to_owned())
            .map(|userinfo| format!("{userinfo}@"))
            .unwrap_or_default();

        let new_authority = format!("{userinfo}{}:{}", self.address.ip(), self.address.port());

        let new_authority = new_authority.parse().unwrap();

        uri_parts.scheme.replace(uri::Scheme::HTTP);
        uri_parts.authority.replace(new_authority);

        let updated_uri = Uri::from_parts(uri_parts).unwrap();

        *request.uri_mut() = updated_uri;

        request
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use http_body_util::{BodyExt, Empty};
    use hyper::Method;

    use crate::test_utils::setup_proxied_server;

    use super::*;

    #[tokio::test]
    async fn bind_to_valid_address_works() {
        let address = "localhost:1500";
        Upstream::bind(address).await.unwrap();
    }

    #[tokio::test]
    async fn bind_to_garbage_address_fails() {
        let address = "garbage";
        let upstream = Upstream::bind(address).await;
        assert!(upstream.is_err())
    }

    #[tokio::test]
    async fn send_request_works() -> Result<(), Box<dyn Error + Send + Sync>> {
        let expected_response = "TEST RESPONSE";

        let mock_server = setup_proxied_server(expected_response);

        let upstream = Upstream::bind(mock_server.address()).await.unwrap();

        let test_body = Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed();

        let test_request = Request::builder()
            .method(Method::GET)
            .body(test_body)
            .unwrap();

        let response = upstream
            .send_request(test_request)
            .await?
            .collect()
            .await?
            .to_bytes();

        let response = String::from_utf8(response.to_vec())?;

        assert_eq!(response, expected_response);

        Ok(())
    }
}
