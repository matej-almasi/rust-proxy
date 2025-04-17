use std::net::SocketAddr;

use anyhow::{anyhow, Context};
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

type RequestBody = BoxBody<Bytes, hyper::Error>;

#[derive(Clone)]
pub struct Upstream {
    address: SocketAddr,
    client: Client<HttpConnector, RequestBody>,
}

impl Upstream {
    pub async fn bind<A: ToSocketAddrs>(address: A) -> anyhow::Result<Self> {
        let mut found_addresses = lookup_host(address)
            .await
            .context("Failed to DNS resolve upstream server.")?;

        let Some(address) = found_addresses.next() else {
            return Err(anyhow!("Upstream server not found."));
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
    ) -> anyhow::Result<Response<Incoming>> {
        request = self
            .update_uri(request)
            .context("Failed constructing URI for request.")?;

        self.client
            .request(request)
            .await
            .context("Failed passing request to upstream.")
    }

    fn update_uri<B>(&self, mut request: Request<B>) -> anyhow::Result<Request<B>> {
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

        // since we have full control over the new authority and we know all
        // the parts to be correct, this should be always Ok(...), but in case
        // it isn't, we don't want to expose `userinfo` to the caller as it may
        // contain sensitive data
        let new_authority = new_authority
            .parse()
            .map_err(|_| anyhow!("Failed parsing new authority."))?;

        uri_parts.scheme.replace(uri::Scheme::HTTP);
        uri_parts.authority.replace(new_authority);

        // the same note as with `new_authority` above applies here too
        let updated_uri =
            Uri::from_parts(uri_parts).map_err(|_| anyhow!("Failed constructing new URI."))?;

        *request.uri_mut() = updated_uri;

        Ok(request)
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
