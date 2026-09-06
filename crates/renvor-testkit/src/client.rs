//! A blocking loopback HTTP/1.1 client for tests that spawn a real binary (Phase 011).
//!
//! The generated starter's `tests/starter.rs` starts its own executable on a free port and talks
//! to it over the socket, because what it proves — the start-up order, signal handling, and clean
//! shutdown — only a process has. This is the client it talks with. One call, one reply; no
//! connection reuse, no TLS, no redirects followed. It is [`minreq`] underneath (package research
//! §3: one ISC crate with no dependencies, chunked responses and timeouts handled), and it is
//! **not** a general HTTP client — `renvor-http`'s own suites dispatch through
//! `renvor_testkit::app::TestApplication` (behind `http`) without a socket.
//!
//! Only behind the `client` feature, so a crate that does not spawn binaries pulls nothing.

/// How long one exchange may take before the test fails instead of hanging.
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// One HTTP reply, fully read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The status code, e.g. `401`.
    pub status: u16,
    /// Every header as sent, in order. Look one up with [`Reply::header`].
    pub headers: Vec<(String, String)>,
    /// The body, chunked encoding already removed, as UTF-8.
    pub body: String,
}

impl Reply {
    /// The first header named `name`, compared case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Sends one request to `address` (`host:port`) and reads the whole reply.
///
/// `extra` headers are sent as given. A non-empty `body` is sent as `application/json`; an empty
/// one is still sent with `Content-Length: 0`, so a write route sees a complete request. The
/// `Host` header is `address` as given, port included (minreq writes it from the URL); the
/// generated starter's host policy validates the port and then ignores it.
///
/// # Panics
///
/// When the connection is refused, the exchange exceeds [`TIMEOUT`], or the reply is not UTF-8:
/// each is a test failure with the reason in the message, never a `Result` a test would unwrap.
#[must_use]
pub fn http(address: &str, method: &str, path: &str, extra: &[(&str, &str)], body: &str) -> Reply {
    let method = match method {
        "GET" => minreq::Method::Get,
        "HEAD" => minreq::Method::Head,
        "POST" => minreq::Method::Post,
        "PUT" => minreq::Method::Put,
        "DELETE" => minreq::Method::Delete,
        "PATCH" => minreq::Method::Patch,
        "OPTIONS" => minreq::Method::Options,
        other => minreq::Method::Custom(other.to_owned()),
    };
    let mut request = minreq::Request::new(method, format!("http://{address}{path}"))
        .with_header("Connection", "close")
        .with_timeout(TIMEOUT.as_secs());
    if !body.is_empty() {
        request = request.with_header("Content-Type", "application/json");
    }
    for (name, value) in extra {
        request = request.with_header(*name, *value);
    }
    let response = request
        .with_body(body)
        .send()
        .unwrap_or_else(|why| panic!("{path} against {address} failed: {why}"));
    let body = response
        .as_str()
        .unwrap_or_else(|why| panic!("{path}: the reply body is not UTF-8: {why}"))
        .to_owned();
    Reply {
        status: response.status_code,
        headers: response.headers,
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A one-shot server: records the raw request it received and answers with `reply`.
    fn serve(reply: &'static str) -> (String, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let address = listener.local_addr().expect("an address").to_string();
        let seen = Arc::new(Mutex::new(String::new()));
        let recorded = Arc::clone(&seen);
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one connection");
            let mut raw = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let n = stream.read(&mut buffer).expect("request bytes");
                raw.extend_from_slice(&buffer[..n]);
                let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let head = String::from_utf8_lossy(&raw[..end]).to_string();
                let length = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if raw.len() >= end + 4 + length {
                    break;
                }
            }
            *recorded.lock().expect("the record") = String::from_utf8_lossy(&raw).to_string();
            stream
                .write_all(reply.as_bytes())
                .expect("the reply is written");
        });
        (address, seen)
    }

    #[test]
    fn a_chunked_reply_is_reassembled_and_headers_are_found_case_insensitively() {
        let (address, seen) = serve(
            "HTTP/1.1 200 OK\r\nSet-Cookie: __Host-rv_session=abc; Path=/\r\n\
             Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
             5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        );
        let reply = http(
            &address,
            "POST",
            "/auth/login",
            &[("Cookie", "x=y")],
            r#"{"a":1}"#,
        );
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, "hello world", "chunked framing must be removed");
        assert_eq!(
            reply.header("set-cookie"),
            Some("__Host-rv_session=abc; Path=/"),
            "a lower-case lookup finds a mixed-case header"
        );
        assert_eq!(reply.header("x-missing"), None);

        let request = seen.lock().expect("the record").clone();
        let lower = request.to_ascii_lowercase();
        assert!(
            request.starts_with("POST /auth/login HTTP/1.1\r\n"),
            "{request}"
        );
        let host = format!("\r\nhost: {address}\r\n");
        assert!(
            lower.contains(&host),
            "the host is the address, port included: {request}"
        );
        assert!(lower.contains("\r\ncookie: x=y\r\n"), "{request}");
        assert!(
            lower.contains("\r\ncontent-type: application/json\r\n"),
            "{request}"
        );
        assert!(lower.contains("\r\ncontent-length: 7\r\n"), "{request}");
        assert!(request.ends_with(r#"{"a":1}"#), "{request}");
    }

    #[test]
    fn an_empty_body_is_sent_with_a_zero_length_and_no_content_type() {
        let (address, seen) =
            serve("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let reply = http(&address, "DELETE", "/items/1", &[], "");
        assert_eq!(reply.status, 204);
        assert_eq!(reply.body, "");
        let request = seen.lock().expect("the record").to_ascii_lowercase();
        assert!(
            request.starts_with("delete /items/1 http/1.1\r\n"),
            "{request}"
        );
        assert!(request.contains("\r\ncontent-length: 0\r\n"), "{request}");
        assert!(!request.contains("content-type"), "{request}");
    }

    #[test]
    fn a_content_length_body_is_read_to_its_length() {
        let (address, _) = serve(
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 13\r\nContent-Type: application/json\r\n\r\n{\"error\":\"x\"}",
        );
        let reply = http(&address, "GET", "/auth/me", &[], "");
        assert_eq!(reply.status, 401);
        assert_eq!(reply.body, r#"{"error":"x"}"#);
        assert_eq!(reply.header("Content-Type"), Some("application/json"));
    }
}
