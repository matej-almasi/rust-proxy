use std::net::SocketAddr;

use async_trait::async_trait;
use http::uri;
use http::uri::PathAndQuery;
use http::Uri;
use hyper::body::Body;
use hyper::body::Incoming;
use hyper::Request;
use hyper::Response;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::ThreadSafeError;

pub struct HyperClientHost<B> {
    address: SocketAddr,
    client: Client<HttpConnector, B>,
}

impl<B> HyperClientHost<B>
where
    B: Body + Send,
    B::Data: Send,
    B::Error: ThreadSafeError,
{
    pub fn new(address: SocketAddr) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        Self { address, client }
    }

    fn redirect(&self, mut req: Request<B>) -> crate::Result<Request<B>> {
        let p_and_q = req
            .uri()
            .path_and_query()
            .cloned()
            .unwrap_or(PathAndQuery::from_static("/"));

        let uri = Uri::builder()
            .scheme(uri::Scheme::HTTP)
            .authority(self.address.to_string())
            .path_and_query(p_and_q)
            .build()?;

        *req.uri_mut() = uri;

        Ok(req)
    }
}

impl<B> Clone for HyperClientHost<B> {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            client: self.client.clone(),
        }
    }
}

#[async_trait]
impl crate::proxy::RemoteHost for HyperClientHost<Incoming> {
    type Error = crate::Error;
    type ResponseBody = Incoming;

    async fn pass_request(
        &self,
        req: Request<Incoming>,
    ) -> crate::Result<Response<Self::ResponseBody>> {
        self.client
            .request(self.redirect(req)?)
            .await
            .map_err(crate::Error::from)
    }
}
