use std::{io, net::SocketAddr, time::Duration};

use anyhow::anyhow;
use hyper::http::uri;
use hyper::Uri;
use hyper::{server::conn::http1, Request};
use hyper_util::{
    client::legacy::Client,
    rt::{TokioExecutor, TokioIo},
    service::TowerToHyperService,
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

pub mod builder;

use builder::ProxyBuilder;

pub struct Proxy {
    listener: TcpListener,
    proxied_addr: SocketAddr,
}

impl Proxy {
    pub fn builder() -> builder::ProxyBuilder {
        ProxyBuilder::new()
    }

    pub async fn run(self) {
        loop {
            let Ok((stream, client)) = self.listener.accept().await else {
                continue;
            };

            let io = TokioIo::new(stream);

            let proxied_host = self.proxied_addr.clone();

            tokio::spawn(async move {
                let svc = tower::service_fn(|req| async {
                    let req = redirect_request(req, proxied_host)?;
                    let client = Client::builder(TokioExecutor::new()).build_http();
                    client
                        .request(req)
                        .await
                        .map_err(|e| anyhow::Error::from(e))
                });

                let tracing = TraceLayer::new_for_http().on_request(()).on_response(
                    |_: &_, _latency: Duration, _: &_| tracing::info!("{} -", client.ip()),
                );

                let svc = ServiceBuilder::new().layer(tracing).service(svc);

                let svc = TowerToHyperService::new(svc);

                if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                    eprintln!("Failed serving connection: {e}");
                };
            });
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

fn redirect_request<B>(
    mut request: Request<B>,
    new_host: SocketAddr,
) -> anyhow::Result<Request<B>> {
    let mut uri_parts = request.uri().clone().into_parts();

    let authority = uri_parts
        .authority
        .take()
        .map(|auth| auth.to_string())
        .unwrap_or_default();

    let userinfo = authority
        .rsplit_once('@')
        .map(|(userinfo, _)| format!("{}@", userinfo.to_owned()))
        .unwrap_or_default();

    let new_authority = format!("{userinfo}{}", new_host);

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

#[cfg(test)]
mod test {
    use super::*;
    // Unit tests for the Proxy::run function are omitted as they are essentially
    // identical to acceptance tests found in the `tests/acceptance.rs` file.

    #[tokio::test]
    async fn local_addr_is_correct() {
        let listener = TcpListener::bind("localhost:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        let proxied_addr = SocketAddr::from(([127, 0, 0, 1], 2000));

        let proxy = Proxy {
            listener,
            proxied_addr,
        };

        assert_eq!(listener_addr, proxy.local_addr().unwrap());
    }
}
