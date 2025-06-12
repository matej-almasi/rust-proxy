use std::net::SocketAddr;

use http::header;
use http::uri::PathAndQuery;
use http::Extensions;
use http::HeaderName;
use hyper::body::Body;
use hyper::Response;

pub(super) fn log_response<T, U, V>(resp: &Response<T>, _: U, _: &V)
where
    T: Body,
{
    let peer_addr = extract_socket_addr_formatted(resp.extensions());
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

pub fn extract_socket_addr_formatted(ext: &Extensions) -> String {
    ext.get::<SocketAddr>().map_or_else(
        || {
            tracing::warn!("Couldn't get peer address from response extension.");
            String::from("UNKNOWN")
        },
        ToString::to_string,
    )
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

pub fn extract_p_and_q_formatted(ext: &Extensions) -> String {
    ext.get::<Option<PathAndQuery>>().map_or_else(
        || {
            tracing::warn!("Couldn't get http path and query for request.");
            String::from("UNKNOWN")
        },
        |maybe_pq| maybe_pq.as_ref().map_or("-".into(), ToString::to_string),
    )
}

pub fn extract_header_formatted(ext: &Extensions, name: &HeaderName) -> String {
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
    use http::{HeaderMap, HeaderValue};
    use tracing_test::traced_test;

    use super::*;

    #[test]
    fn test_extract_existing_socket_address() {
        let addr = SocketAddr::from(([111, 222, 133, 144], 155));

        let mut ext = Extensions::new();
        ext.insert(addr);

        assert_eq!(extract_socket_addr_formatted(&ext), addr.to_string());
    }

    #[test]
    fn test_extract_nonexistent_socket_address() {
        assert_eq!(
            extract_socket_addr_formatted(&Extensions::default()),
            "UNKNOWN"
        );
    }

    #[test]
    fn test_extract_existing_method_formatted() {
        let method = http::Method::PATCH;

        let mut ext = Extensions::new();
        ext.insert(method.clone());

        assert_eq!(extract_method_formatted(&ext), method.to_string());
    }

    #[test]
    fn test_extract_nonexistent_method() {
        assert_eq!(extract_method_formatted(&Extensions::default()), "UNKNOWN");
    }

    #[test]
    fn test_extract_existing_p_and_q_formatted() {
        let p_and_q = PathAndQuery::from_static("path/and?query=val");

        let mut ext = Extensions::new();
        ext.insert(Some(p_and_q.clone()));

        assert_eq!(extract_p_and_q_formatted(&ext), p_and_q.to_string());
    }

    #[test]
    fn test_extract_empty_p_and_q_formatted() {
        let mut ext = Extensions::new();
        ext.insert(None::<PathAndQuery>);

        assert_eq!(extract_p_and_q_formatted(&ext), "-");
    }

    #[test]
    fn test_extract_nonexistent_p_and_q() {
        assert_eq!(extract_p_and_q_formatted(&Extensions::default()), "UNKNOWN");
    }

    #[test]
    fn test_extract_existing_header_formatted() {
        let header_name = header::CONTENT_LANGUAGE;
        let header_val = "SK-sk";

        let mut headers = HeaderMap::new();
        headers.insert(&header_name, HeaderValue::from_static(header_val));

        let mut ext = Extensions::new();
        ext.insert(headers);

        assert_eq!(
            extract_header_formatted(&ext, &header_name),
            header_val.to_string()
        );
    }

    #[test]
    fn test_extract_empty_header_formatted() {
        let mut ext = Extensions::new();
        ext.insert(HeaderMap::new());

        assert_eq!(extract_header_formatted(&ext, &header::COOKIE), "-");
    }

    #[test]
    fn test_extract_nonexistent_headers() {
        assert_eq!(
            extract_header_formatted(&Extensions::default(), &header::ALLOW),
            "UNKNOWN"
        );
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_peer_address() -> anyhow::Result<()> {
        logs_assert(|lines| {
            check_log_contains(lines, "127.0.0.1:")?
                .then_some(())
                .ok_or(String::from("Peer address not found in log entry."))
        });

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_method() -> anyhow::Result<()> {
        assert!(logs_contain("GET"));

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_path_and_query() -> anyhow::Result<()> {
        let test_p_and_q = "/some/path?johnny=12";
        assert!(logs_contain(test_p_and_q));
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_http_version() -> anyhow::Result<()> {
        assert!(logs_contain("HTTP/2.0"));
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_status() -> anyhow::Result<()> {
        assert!(logs_contain("200"));
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_response_size() -> anyhow::Result<()> {
        let response_size = 1_234_567;
        assert!(logs_contain(&response_size.to_string()));
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_contain_referrer() -> anyhow::Result<()> {
        let referrer = "acmecompany.com/referrer";
        assert!(logs_contain(referrer));
        Ok(())
    }
    #[tokio::test]
    #[traced_test]
    async fn logs_contain_user_agent() -> anyhow::Result<()> {
        let user_agent = "firefox/1.0";
        assert!(logs_contain(user_agent));
        Ok(())
    }

    fn check_log_contains(lines: &[&str], val: &str) -> Result<bool, String> {
        Ok(lines
            .iter()
            .find(|line| line.contains("INFO"))
            .ok_or(String::from("No proxy logging line found in logs."))?
            .contains(val))
    }
}
