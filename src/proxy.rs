use std::{convert::Infallible, io, net::SocketAddr};

use hyper::{
    body::Incoming, http::uri, server::conn::http1, service::service_fn, Request, Response, Uri,
};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::{TokioExecutor, TokioIo},
};
use tokio::net::{lookup_host, TcpListener, ToSocketAddrs};

pub struct Proxy {
    listener: TcpListener,
    upstream: Upstream,
}

impl Proxy {
    pub async fn bind<L, U>(listener_addr: L, upstream_addr: U) -> io::Result<Self>
    where
        L: ToSocketAddrs,
        U: ToSocketAddrs,
    {
        let listener = TcpListener::bind(listener_addr).await?;

        let upstream = Upstream::bind(upstream_addr).await.unwrap();

        Ok(Self { listener, upstream })
    }

    pub async fn run(&self) {
        loop {
            let Ok((stream, _)) = self.listener.accept().await else {
                continue;
            };
            let io = TokioIo::new(stream);

            let upstream = self.upstream.clone();

            tokio::spawn(async move {
                http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(|req| async { upstream.send_request(req).await }),
                    )
                    .await
                    .unwrap();
            });
        }
    }
}

#[derive(Clone)]
struct Upstream {
    address: SocketAddr,
    client: Client<HttpConnector, Incoming>,
}

impl Upstream {
    async fn bind<A: ToSocketAddrs>(address: A) -> Result<Self, Infallible> {
        let address = lookup_host(address).await.unwrap().next().unwrap();

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(None)
            .pool_max_idle_per_host(32)
            // .http2_only(true)
            // .http2_keep_alive_interval(Duration::from_secs(20))
            // .http2_keep_alive_while_idle(true)
            .build_http::<Incoming>();

        Ok(Self { address, client })
    }

    async fn send_request(
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
    use http_body_util::{BodyExt, Full};
    use hyper::{body::Bytes, Method, Request};

    use super::*;

    #[tokio::test]
    async fn test_proxy_serves_proxied_content() {
        let test_server = httpmock::MockServer::start();

        test_server.mock(|when, then| {
            when.method(Method::GET.as_str()).path("/test");
            then.status(200).body("TEST RESPONSE");
        });

        let proxy = Proxy::bind("127.0.0.1:2000", test_server.address())
            .await
            .unwrap();

        tokio::spawn(async move {
            proxy.run().await;
        });

        let test_client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();

        let test_request: Request<Full<Bytes>> = Request::builder()
            .method(Method::GET)
            .uri("http://localhost:2000/test")
            .body(Full::from("TEST"))
            .unwrap();

        let response = test_client.request(test_request).await.unwrap();

        let response_bytes = response.collect().await.unwrap().to_bytes();
        let response_text = String::from_utf8(response_bytes.into()).unwrap();

        assert_eq!(response_text, "TEST RESPONSE");
    }
}
