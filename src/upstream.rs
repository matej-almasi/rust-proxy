use std::{
    convert::Infallible,
    io::{self, ErrorKind},
    net::SocketAddr,
};

use hyper::{body::Incoming, http::uri, Request, Response, Uri};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use tokio::net::{lookup_host, ToSocketAddrs};

#[derive(Clone)]
pub struct Upstream {
    address: SocketAddr,
    client: Client<HttpConnector, Incoming>,
}

impl Upstream {
    pub async fn bind<A: ToSocketAddrs>(address: A) -> io::Result<Self> {
        let Some(address) = lookup_host(address).await?.next() else {
            return Err(io::Error::from(ErrorKind::NotFound));
        };

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(None)
            .pool_max_idle_per_host(32)
            // .http2_only(true)
            // .http2_keep_alive_interval(Duration::from_secs(20))
            // .http2_keep_alive_while_idle(true)
            .build_http::<Incoming>();

        Ok(Self { address, client })
    }

    pub async fn send_request(
        &self,
        mut request: Request<Incoming>,
    ) -> Result<Response<Incoming>, Infallible> {
        request = self.update_uri(request);

        Ok(self.client.request(request).await.unwrap())
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
    use super::*;

    #[tokio::test]
    async fn test_bind_to_valid_address() {
        let address = "localhost:1500";
        Upstream::bind(address).await.unwrap();
    }

    #[tokio::test]
    async fn test_bind_to_garbage_address_fails() {
        let address = "garbage";
        let upstream = Upstream::bind(address).await;
        assert!(upstream.is_err())
    }
}
