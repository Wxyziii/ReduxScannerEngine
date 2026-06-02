#[cfg(test)]
mod readiness_tests {
    use crate::codewalker_api::model::CodeWalkerApiReadinessStatus;
    use crate::codewalker_api::readiness::probe_codewalker_api_readiness;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// Mock HTTP server recording "METHOD PATH" for every request so tests can
    /// prove only GET was used and no mutation endpoint was called.
    struct MockServer {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        fn start(
            connections: usize,
            body_for: fn(&str) -> (u16, String),
        ) -> (MockServer, Receiver<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_thread = Arc::clone(&requests);
            let (ready_tx, ready_rx) = mpsc::channel::<()>();

            let handle = std::thread::spawn(move || {
                let _ = ready_tx.send(());
                for _ in 0..connections {
                    let (stream, _) = match listener.accept() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    handle_conn(stream, &requests_thread, body_for);
                }
            });

            (
                MockServer {
                    base_url: format!("http://{addr}"),
                    requests,
                    handle: Some(handle),
                },
                ready_rx,
            )
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
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
        requests: &Arc<Mutex<Vec<String>>>,
        body_for: fn(&str) -> (u16, String),
    ) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("?").to_string();
        let path = parts.next().unwrap_or("/").to_string();
        requests.lock().unwrap().push(format!("{method} {path}"));

        let (status, body) = body_for(&path);
        let response = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.as_bytes().len()
        );
        let mut stream = stream;
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn answer_ready(path: &str) -> (u16, String) {
        match path {
            "/" => (200, "<html>CodeWalker.API</html>".to_string()),
            "/api/service-status" => (
                200,
                r#"{"status":"ready","ready":true,"servicesReady":true,"gtaPath":"C:/Games/GTAV","version":"1.0"}"#
                    .to_string(),
            ),
            _ => (404, "{}".to_string()),
        }
    }

    fn answer_unexpected_json(path: &str) -> (u16, String) {
        match path {
            "/" => (200, "{}".to_string()),
            "/api/service-status" => (200, r#"{"foo":"bar","count":3}"#.to_string()),
            _ => (404, "{}".to_string()),
        }
    }

    fn answer_non_json(path: &str) -> (u16, String) {
        match path {
            "/" => (200, "hello".to_string()),
            "/api/service-status" => (200, "not json at all".to_string()),
            _ => (404, "nope".to_string()),
        }
    }

    #[test]
    fn codewalker_readiness_offline_returns_not_ready_report() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let r = probe_codewalker_api_readiness(Some(&url)).unwrap();
        assert!(!r.codewalker_api_reachable);
        assert!(!r.codewalker_api_ready_for_search);
        assert!(!r.codewalker_api_ready_for_replace);
        assert!(!r.can_write_archive);
        assert!(!r.writer_allowed);
        assert_eq!(r.status, CodeWalkerApiReadinessStatus::Offline);
    }

    #[test]
    fn codewalker_readiness_uses_default_base_url() {
        let r = probe_codewalker_api_readiness(None).unwrap();
        assert_eq!(r.normalized_base_url, "http://localhost:5555");
    }

    #[test]
    fn codewalker_readiness_parses_ready_service_status_mock() {
        let (server, ready) = MockServer::start(3, answer_ready);
        ready.recv().unwrap();
        let r = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(r.codewalker_api_reachable);
        assert_eq!(r.service_status_http_status, Some(200));
        assert!(r.service_status_json_parse_success);
        assert!(r.codewalker_api_ready_for_search);
        assert!(r.can_call_search_later);
        assert!(!r.codewalker_api_ready_for_replace);
        assert_eq!(r.status, CodeWalkerApiReadinessStatus::Ready);
        assert!(r.gta_path_detected);
        assert_eq!(r.gta_path.as_deref(), Some("C:/Games/GTAV"));
        assert_eq!(r.reload_version.as_deref(), Some("1.0"));
    }

    #[test]
    fn codewalker_readiness_handles_unexpected_json_shape() {
        let (server, ready) = MockServer::start(3, answer_unexpected_json);
        ready.recv().unwrap();
        let r = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(r.codewalker_api_reachable);
        assert!(r.service_status_json_parse_success); // valid JSON, just unknown shape
        assert!(!r.codewalker_api_ready_for_search);
        assert!(!r.gta_path_detected);
        assert_eq!(r.status, CodeWalkerApiReadinessStatus::ReachableNotReady);
    }

    #[test]
    fn codewalker_readiness_handles_non_json_service_status() {
        let (server, ready) = MockServer::start(3, answer_non_json);
        ready.recv().unwrap();
        let r = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(r.codewalker_api_reachable);
        assert!(!r.service_status_json_parse_success);
        assert!(!r.codewalker_api_ready_for_search);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.code == "service_status_not_json"));
    }

    #[test]
    fn codewalker_readiness_does_not_call_replace_endpoint() {
        let (server, ready) = MockServer::start(3, answer_ready);
        ready.recv().unwrap();
        let _ = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(!server.requests().iter().any(|r| r.contains("replace")));
    }

    #[test]
    fn codewalker_readiness_does_not_call_import_endpoint() {
        let (server, ready) = MockServer::start(3, answer_ready);
        ready.recv().unwrap();
        let _ = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(!server.requests().iter().any(|r| r.contains("import")));
    }

    #[test]
    fn codewalker_readiness_does_not_call_reload_services() {
        let (server, ready) = MockServer::start(3, answer_ready);
        ready.recv().unwrap();
        let _ = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(!server
            .requests()
            .iter()
            .any(|r| r.contains("reload-services")));
    }

    #[test]
    fn codewalker_readiness_does_not_call_set_config() {
        let (server, ready) = MockServer::start(3, answer_ready);
        ready.recv().unwrap();
        let _ = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        assert!(!server.requests().iter().any(|r| r.contains("set-config")));
    }

    #[test]
    fn codewalker_readiness_uses_get_only() {
        let (server, ready) = MockServer::start(3, answer_ready);
        ready.recv().unwrap();
        let _ = probe_codewalker_api_readiness(Some(&server.base_url)).unwrap();
        let reqs = server.requests();
        assert!(!reqs.is_empty());
        assert!(
            reqs.iter().all(|r| r.starts_with("GET ")),
            "non-GET request seen: {reqs:?}"
        );
        // Only root and service-status were ever requested.
        assert!(reqs
            .iter()
            .all(|r| r.ends_with(" /") || r.ends_with(" /api/service-status")));
    }

    #[test]
    fn codewalker_readiness_can_write_archive_false() {
        let r = probe_codewalker_api_readiness(Some("http://localhost:5555")).unwrap();
        assert!(!r.can_write_archive);
        assert!(!r.can_call_replace_later);
        assert!(!r.modifies_archive);
        assert!(!r.mutation_endpoints_called);
    }

    #[test]
    fn codewalker_readiness_writer_allowed_false() {
        let r = probe_codewalker_api_readiness(Some("http://localhost:5555")).unwrap();
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
        assert!(!r.external_tool_executed);
        assert!(!r.post_requests_used);
    }

    #[test]
    fn codewalker_readiness_null_adapter_still_active() {
        let r = probe_codewalker_api_readiness(Some("http://localhost:5555")).unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        let gate = r
            .gates
            .iter()
            .find(|g| g.name == "null_adapter_still_active")
            .unwrap();
        assert!(gate.passed);
    }

    #[test]
    fn codewalker_readiness_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let r = probe_codewalker_api_readiness(Some("http://localhost:5555")).unwrap();
        let out = dir.path().join("codewalker_readiness.json");
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["canWriteArchive"], false);
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["replaceEndpointCalled"], false);
        assert_eq!(v["importEndpointCalled"], false);
        assert_eq!(v["reloadServicesCalled"], false);
        assert_eq!(v["setConfigCalled"], false);
        assert_eq!(v["mutationEndpointsCalled"], false);
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_readiness_does_not_modify_files() {
        let before = std::fs::read_dir(".").unwrap().count();
        let _ = probe_codewalker_api_readiness(Some("http://localhost:5555")).unwrap();
        let after = std::fs::read_dir(".").unwrap().count();
        assert_eq!(before, after);
    }
}
