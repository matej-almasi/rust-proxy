use std::net::SocketAddr;

use anyhow::anyhow;
use hyper::http::uri;
use hyper::{Request, Uri};

pub(super) fn redirect_request<B>(
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
