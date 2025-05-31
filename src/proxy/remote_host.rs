use std::net::SocketAddr;

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

pub(super) struct RemoteHost<B> {
    address: SocketAddr,
    client: Client<HttpConnector, B>,
}

impl<B> RemoteHost<B>
where
    B: Body + Send + 'static + Unpin,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub fn new(address: SocketAddr) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        Self { address, client }
    }

    pub async fn pass_request(&self, req: Request<B>) -> crate::Result<Response<Incoming>> {
        let req = redirect(req, self.address)?;

        // Extensions don't automatically transfer from Req to Resp,
        // we have to do it manually.
        let extensions = req.extensions().to_owned();

        let resp = self.client.request(req).await.map(|mut resp| {
            *resp.extensions_mut() = extensions;
            resp
        });

        Ok(resp?)
    }
}

impl<B> Clone for RemoteHost<B> {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            client: self.client.clone(),
        }
    }
}

fn redirect<B>(mut req: Request<B>, new_host: SocketAddr) -> crate::Result<Request<B>> {
    let p_and_q = req
        .uri()
        .path_and_query()
        .cloned()
        .unwrap_or(PathAndQuery::from_static("/"));

    let uri = Uri::builder()
        .scheme(uri::Scheme::HTTP)
        .authority(new_host.to_string())
        .path_and_query(p_and_q)
        .build()?;

    *req.uri_mut() = uri;

    Ok(req)
}
