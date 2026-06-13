#[cfg(test)]
mod http_client_tests {
    use crate::codewalker_api::http_client::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// A tiny mock HTTP server that returns caller-provided RAW response bytes
    /// verbatim and captures the raw request bytes. No real CodeWalker, no GTA
    /// files, no archive mutation.
    struct RawMock {
        addr: std::net::SocketAddr,
        requests: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl RawMock {
        /// Bind on 127.0.0.1 (IPv4) and serve `response` for `connections` conns.
        fn start(connections: usize, response: Vec<u8>) -> RawMock {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let req_thread = Arc::clone(&requests);
            let handle = std::thread::spawn(move || {
                for _ in 0..connections {
                    let (mut stream, _) = match listener.accept() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    // Read the request headers (until CRLFCRLF).
                    let mut buf = [0u8; 4096];
                    let mut acc = Vec::new();
                    loop {
                        match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                acc.extend_from_slice(&buf[..n]);
                                if acc.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    req_thread
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&acc).to_string());
                    let _ = stream.write_all(&response);
                    let _ = stream.flush();
                    // Dropping the stream closes the connection (Connection: close).
                }
            });
            RawMock {
                addr,
                requests,
                handle: Some(handle),
            }
        }

        fn ipv4_url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{}", self.addr.port(), path)
        }

        fn localhost_url(&self, path: &str) -> String {
            format!("http://localhost:{}{}", self.addr.port(), path)
        }

        fn captured(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for RawMock {
        fn drop(&mut self) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    fn resp_content_length(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.as_bytes().len(),
            body
        )
        .into_bytes()
    }

    fn resp_chunked(chunks: &[&str]) -> Vec<u8> {
        let mut s = String::from(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        );
        for c in chunks {
            s.push_str(&format!("{:x}\r\n{}\r\n", c.as_bytes().len(), c));
        }
        s.push_str("0\r\n\r\n");
        s.into_bytes()
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[test]
    fn codewalker_http_client_tries_all_resolved_addresses() {
        // Server on IPv4 only; request via "localhost" (which also resolves ::1).
        let m = RawMock::start(1, resp_content_length(r#"{"ok":true}"#));
        let r = http_get(&m.localhost_url("/api/service-status")).unwrap();
        assert_eq!(r.status, 200);
        assert!(r
            .connected_address
            .as_deref()
            .unwrap()
            .contains("127.0.0.1"));
    }

    #[test]
    fn codewalker_http_client_localhost_ipv4_fallback() {
        let m = RawMock::start(1, resp_content_length(r#"{"servicesReady":true}"#));
        let r = http_get(&m.localhost_url("/api/service-status")).unwrap();
        assert_eq!(r.status, 200);
        let (shape, parsed) = classify_shape(&r.body);
        assert!(parsed);
        assert_eq!(shape, "json_object");
    }

    #[test]
    fn codewalker_http_client_decodes_content_length_json() {
        let m = RawMock::start(1, resp_content_length(r#"{"a":1}"#));
        let r = http_get(&m.ipv4_url("/x")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::ContentLength);
        assert_eq!(r.body, r#"{"a":1}"#);
        assert_eq!(r.content_length, Some(7));
    }

    #[test]
    fn codewalker_http_client_decodes_chunked_json_object() {
        let body = r#"{"gtaPath":"X","servicesReady":true}"#;
        let m = RawMock::start(1, resp_chunked(&[body]));
        let r = http_get(&m.ipv4_url("/api/service-status")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::Chunked);
        assert_eq!(r.body, body);
        let (shape, parsed) = classify_shape(&r.body);
        assert!(parsed);
        assert_eq!(shape, "json_object");
    }

    #[test]
    fn codewalker_http_client_decodes_chunked_json_array() {
        let body =
            r#"["a.rpf\\data\\visualsettings.dat","update.rpf\\common\\data\\visualsettings.dat"]"#;
        let m = RawMock::start(1, resp_chunked(&[body]));
        let r = http_get(&m.ipv4_url("/api/search-file?fileName=visualsettings.dat")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::Chunked);
        let (shape, parsed) = classify_shape(&r.body);
        assert!(parsed);
        assert_eq!(shape, "json_array");
    }

    #[test]
    fn codewalker_http_client_decodes_multiple_chunks() {
        // Split a JSON object across several chunks.
        let m = RawMock::start(1, resp_chunked(&[r#"{"ser"#, r#"vicesReady":"#, "true}"]));
        let r = http_get(&m.ipv4_url("/api/service-status")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::Chunked);
        assert_eq!(r.body, r#"{"servicesReady":true}"#);
        let (_, parsed) = classify_shape(&r.body);
        assert!(parsed);
    }

    #[test]
    fn codewalker_http_client_handles_invalid_chunked_body() {
        // Advertise chunked but send garbage framing.
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nZZZZnot-a-chunk".to_vec();
        let m = RawMock::start(1, raw);
        let r = http_get(&m.ipv4_url("/bad")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::DecodeFailed);
        assert!(r.body.is_empty());
        let (_, parsed) = classify_shape(&r.body);
        assert!(!parsed);
    }

    #[test]
    fn codewalker_http_client_header_names_case_insensitive() {
        // Mixed-case header names must still be honoured.
        let body = r#"{"k":"v"}"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nCoNtEnT-tYpE: application/json\r\ncOnTeNt-LeNgTh: {}\r\nCONNECTION: close\r\n\r\n{}",
            body.as_bytes().len(),
            body
        )
        .into_bytes();
        let m = RawMock::start(1, raw);
        let r = http_get(&m.ipv4_url("/h")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::ContentLength);
        assert_eq!(r.content_length, Some(9));
        assert_eq!(r.body, body);
    }

    #[test]
    fn codewalker_http_client_connection_close_body_fallback() {
        // No Content-Length, no chunked — body runs until the socket closes.
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":1}"
                .to_vec();
        let m = RawMock::start(1, raw);
        let r = http_get(&m.ipv4_url("/c")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::ConnectionClose);
        assert_eq!(r.body, r#"{"ok":1}"#);
    }

    #[test]
    fn codewalker_http_client_sends_accept_encoding_identity() {
        let m = RawMock::start(1, resp_content_length(r#"{"ok":1}"#));
        let _ = http_get(&m.ipv4_url("/ae")).unwrap();
        let req = &m.captured()[0];
        assert!(
            req.to_ascii_lowercase()
                .contains("accept-encoding: identity"),
            "request was: {req}"
        );
        // Never requests gzip/deflate.
        assert!(!req.to_ascii_lowercase().contains("gzip"));
    }

    #[test]
    fn codewalker_http_client_body_sample_limit_applies_after_decode() {
        let big = "x".repeat(5000);
        let m = RawMock::start(1, resp_chunked(&[&big]));
        let r = http_get(&m.ipv4_url("/big")).unwrap();
        assert_eq!(r.body_decode_mode, BodyDecodeMode::Chunked);
        assert_eq!(r.body.len(), 5000); // full decode
        let sample = sample_body(&r.body, 2048).unwrap();
        assert_eq!(sample.chars().count(), 2048); // sample limited after decode
    }

    #[test]
    fn codewalker_http_client_get_sends_get_method() {
        let m = RawMock::start(1, resp_content_length(r#"{"ok":1}"#));
        let _ = http_get(&m.ipv4_url("/m")).unwrap();
        assert!(m.captured()[0].starts_with("GET /m "));
    }

    #[test]
    fn codewalker_http_client_options_sends_options_method() {
        let m = RawMock::start(
            1,
            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        );
        let r = http_options(&m.ipv4_url("/api/replace-file")).unwrap();
        assert_eq!(r.status, 405);
        assert!(m.captured()[0].starts_with("OPTIONS /api/replace-file "));
    }
}
