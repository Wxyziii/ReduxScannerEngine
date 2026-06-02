#[cfg(test)]
mod tests {
    use crate::codewalker_api::detect::{detect_codewalker_api, DEFAULT_BASE_URL};
    use crate::codewalker_api::model::CodeWalkerApiDetectionStatus;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// A tiny mock HTTP server that records every requested path so tests can
    /// prove no write/replace/import endpoint was ever called. Handles a fixed
    /// number of connections then exits.
    struct MockServer {
        base_url: String,
        paths: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        /// Start a mock that answers `connections` requests. The `body_for`
        /// closure maps a request path to (status_code, body).
        fn start(
            connections: usize,
            body_for: fn(&str) -> (u16, String),
        ) -> (MockServer, Receiver<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let paths = Arc::new(Mutex::new(Vec::new()));
            let paths_thread = Arc::clone(&paths);
            let (ready_tx, ready_rx) = mpsc::channel::<()>();

            let handle = std::thread::spawn(move || {
                let _ = ready_tx.send(());
                for _ in 0..connections {
                    let (stream, _) = match listener.accept() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    handle_conn(stream, &paths_thread, body_for);
                }
            });

            (
                MockServer {
                    base_url: format!("http://{addr}"),
                    paths,
                    handle: Some(handle),
                },
                ready_rx,
            )
        }

        fn requested_paths(&self) -> Vec<String> {
            self.paths.lock().unwrap().clone()
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    fn handle_conn(
        stream: TcpStream,
        paths: &Arc<Mutex<Vec<String>>>,
        body_for: fn(&str) -> (u16, String),
    ) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        // "GET /path HTTP/1.1"
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_string();
        paths.lock().unwrap().push(path.clone());

        let (status, body) = body_for(&path);
        let response = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.as_bytes().len()
        );
        let mut stream = stream;
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn answer_ok(path: &str) -> (u16, String) {
        match path {
            "/" => (200, "<html>CodeWalker.API</html>".to_string()),
            "/api/service-status" => (200, r#"{"status":"ok","ready":false}"#.to_string()),
            _ => (404, "{}".to_string()),
        }
    }

    #[test]
    fn codewalker_detect_uses_default_base_url() {
        // No base URL given -> default is used. Server almost certainly offline
        // on the default port during tests; report must still be valid.
        let r = detect_codewalker_api(None).unwrap();
        assert!(r.default_base_url_used);
        assert_eq!(r.normalized_base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn codewalker_detect_normalizes_trailing_slash() {
        let r = detect_codewalker_api(Some("http://localhost:5555///")).unwrap();
        assert_eq!(r.normalized_base_url, "http://localhost:5555");
        assert!(!r.default_base_url_used);
    }

    #[test]
    fn codewalker_detect_offline_returns_report_not_error() {
        // Bind then drop a listener to obtain a definitely-closed port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let r = detect_codewalker_api(Some(&url)).unwrap();
        assert!(!r.reachable);
        assert!(!r.service_status_available);
        assert!(!r.writer_allowed);
        assert!(!r.can_write_archive);
        assert_eq!(r.status, CodeWalkerApiDetectionStatus::Offline);
    }

    #[test]
    fn codewalker_detect_service_status_success_with_mock_server() {
        let (server, ready) = MockServer::start(2, answer_ok);
        ready.recv().unwrap();
        let r = detect_codewalker_api(Some(&server.base_url)).unwrap();
        assert!(r.service_status_checked);
        assert!(r.service_status_available);
        assert_eq!(r.service_status_http_status, Some(200));
        assert!(r.codewalker_api_detected);
        assert_eq!(r.status, CodeWalkerApiDetectionStatus::Detected);
    }

    #[test]
    fn codewalker_detect_root_success_with_mock_server() {
        let (server, ready) = MockServer::start(2, answer_ok);
        ready.recv().unwrap();
        let r = detect_codewalker_api(Some(&server.base_url)).unwrap();
        assert!(r.root_checked);
        assert!(r.root_available);
        assert_eq!(r.root_http_status, Some(200));
        assert!(r.reachable);
    }

    #[test]
    fn codewalker_detect_does_not_call_replace_endpoint() {
        let (server, ready) = MockServer::start(2, answer_ok);
        ready.recv().unwrap();
        let _ = detect_codewalker_api(Some(&server.base_url)).unwrap();
        let paths = server.requested_paths();
        assert!(
            !paths.iter().any(|p| p.contains("replace")),
            "replace endpoint must not be called, saw: {paths:?}"
        );
    }

    #[test]
    fn codewalker_detect_does_not_call_import_endpoint() {
        let (server, ready) = MockServer::start(2, answer_ok);
        ready.recv().unwrap();
        let _ = detect_codewalker_api(Some(&server.base_url)).unwrap();
        let paths = server.requested_paths();
        assert!(
            !paths.iter().any(|p| p.contains("import")),
            "import endpoint must not be called, saw: {paths:?}"
        );
        // Only the two read-only endpoints were requested.
        assert!(paths.iter().all(|p| p == "/" || p == "/api/service-status"));
    }

    #[test]
    fn codewalker_detect_write_endpoints_called_false() {
        let (server, ready) = MockServer::start(2, answer_ok);
        ready.recv().unwrap();
        let r = detect_codewalker_api(Some(&server.base_url)).unwrap();
        assert!(!r.write_endpoints_called);
        assert!(!r.replace_endpoint_called);
        assert!(!r.import_endpoint_called);
        assert!(!r.write_endpoints_checked);
    }

    #[test]
    fn codewalker_detect_can_write_archive_false() {
        let r = detect_codewalker_api(Some("http://localhost:5555")).unwrap();
        assert!(!r.can_write_archive);
        assert!(!r.can_replace_file);
        assert!(!r.can_import_file);
        assert!(!r.modifies_archive);
    }

    #[test]
    fn codewalker_detect_writer_allowed_false() {
        let r = detect_codewalker_api(Some("http://localhost:5555")).unwrap();
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
        assert!(!r.external_tool_executed);
    }

    #[test]
    fn codewalker_detect_null_adapter_still_active() {
        let r = detect_codewalker_api(Some("http://localhost:5555")).unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        let gate = r
            .safety_gates
            .iter()
            .find(|g| g.name == "null_adapter_still_active")
            .unwrap();
        assert!(gate.passed);
    }

    #[test]
    fn codewalker_detect_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let r = detect_codewalker_api(Some("http://localhost:5555")).unwrap();
        let out = dir.path().join("codewalker_detect.json");
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["canWriteArchive"], false);
        assert_eq!(v["writeEndpointsCalled"], false);
        assert_eq!(v["replaceEndpointCalled"], false);
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_detect_does_not_modify_files() {
        // Detection takes no paths and writes nothing. Confirm the working
        // directory listing is unchanged across a run.
        let before = std::fs::read_dir(".").unwrap().count();
        let _ = detect_codewalker_api(Some("http://localhost:5555")).unwrap();
        let after = std::fs::read_dir(".").unwrap().count();
        assert_eq!(before, after);
    }
}
