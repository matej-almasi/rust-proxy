use http::header;
use http::uri::PathAndQuery;
use http::Extensions;
use http::HeaderName;
use hyper::body::Body;
use hyper::Response;

pub(super) fn log_response<T>(resp: &Response<T>)
where
    T: Body,
{
    let peer_addr = super::extract_socket_addr_formatted(resp.extensions());
    let method = extract_method_formatted(resp.extensions());
    let p_and_q = extract_p_and_q_formatted(resp.extensions());
    let version = "HTTP/2.0";
    let status = resp.status().as_u16().to_string();

    let size = resp
        .body()
        .size_hint()
        .exact()
        .map_or("UNKNOWN".into(), |s| s.to_string());

    let referrer = extract_header_formatted(resp.extensions(), &header::REFERER);
    let user_agent = extract_header_formatted(resp.extensions(), &header::USER_AGENT);

    tracing::info!(
        "{peer_addr} {method} {p_and_q} {version} {status} {size} {referrer} {user_agent}"
    );
}

pub fn extract_method_formatted(ext: &Extensions) -> String {
    ext.get::<http::Method>().map_or_else(
        || {
            tracing::warn!("Couldn't get http method for request.");
            String::from("UNKNOWN")
        },
        ToString::to_string,
    )
}

fn extract_p_and_q_formatted(ext: &Extensions) -> String {
    ext.get::<Option<PathAndQuery>>().map_or_else(
        || {
            tracing::warn!("Couldn't get http path and query for request.");
            String::from("UNKNOWN")
        },
        |maybe_pq| maybe_pq.as_ref().map_or("-".into(), ToString::to_string),
    )
}

fn extract_header_formatted(ext: &Extensions, name: &HeaderName) -> String {
    let Some(headers) = ext.get::<http::HeaderMap>() else {
        tracing::warn!("Couldn't get headers for request.");
        return String::from("UNKNOWN");
    };

    let Some(header) = headers.get(name) else {
        return String::from("-");
    };

    let Ok(value) = header.to_str() else {
        tracing::warn!("Couldn't parse {name} for request.");
        return String::from("UNKNOWN");
    };

    String::from(value)
}

#[cfg(test)]
mod test {
    use std::net::SocketAddr;

    use bytes::Bytes;
    use http::{status, HeaderMap, HeaderValue};
    use http_body_util::{Empty, Full};
    use tracing_test::traced_test;

    use super::*;

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_peer_address() {
        let peer = SocketAddr::new([123, 0, 255, 145].into(), 123);

        let resp = Response::builder()
            .extension(peer)
            .body(Empty::<Bytes>::new())
            .unwrap();

        log_response(&resp);

        logs_assert(|lines| {
            check_log_contains(lines, &peer.to_string())?
                .then_some(())
                .ok_or(String::from("Peer address not found in log entry."))
        });
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_method() {
        let method = http::Method::OPTIONS;

        let resp = Response::builder()
            .extension(method.clone())
            .body(Empty::<Bytes>::new())
            .unwrap();

        log_response(&resp);

        assert!(logs_contain(method.as_str()));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_path_and_query() {
        let test_p_and_q = PathAndQuery::from_static("/some/path?johnny=12");

        let resp = Response::builder()
            .extension(Some(test_p_and_q.clone()))
            .body(Empty::<Bytes>::new())
            .unwrap();

        log_response(&resp);

        assert!(logs_contain(test_p_and_q.as_str()));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_http_version() {
        log_response(&Response::new(Empty::<Bytes>::new()));
        assert!(logs_contain("HTTP/2.0"));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_status() {
        let status = status::StatusCode::PERMANENT_REDIRECT;

        let resp = Response::builder()
            .status(status)
            .body(Empty::<Bytes>::new())
            .unwrap();

        log_response(&resp);

        assert!(logs_contain(status.as_str()));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_response_size() {
        let size = 256;

        let resp = Response::new(Full::new(Bytes::from_owner(vec![0; size])));

        log_response(&resp);

        assert!(logs_contain(&size.to_string()));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_referrer() {
        let referrer = "acmecompany.com/referrer";

        let mut headers = HeaderMap::new();
        headers.insert(header::REFERER, HeaderValue::from_static(referrer));

        let resp = Response::builder()
            .extension(headers)
            .body(Empty::<Bytes>::new())
            .unwrap();

        log_response(&resp);

        assert!(logs_contain(referrer));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_user_agent() {
        let user_agent = "firefox/1.0";

        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, HeaderValue::from_static(user_agent));

        let resp = Response::builder()
            .extension(headers)
            .body(Empty::<Bytes>::new())
            .unwrap();

        log_response(&resp);

        assert!(logs_contain(user_agent));
    }

    fn check_log_contains(lines: &[&str], val: &str) -> Result<bool, String> {
        Ok(lines
            .iter()
            .find(|line| line.contains("INFO"))
            .ok_or(String::from("No proxy logging line found in logs."))?
            .contains(val))
    }
}
