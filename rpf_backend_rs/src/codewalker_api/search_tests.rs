#[cfg(test)]
mod search_tests {
    use crate::codewalker_api::model::{CodeWalkerSearchConfidence, CodeWalkerSearchResolveStatus};
    use crate::codewalker_api::search::build_codewalker_search_resolve_report;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// Mock HTTP server recording "METHOD PATH" (path includes query string).
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

    const READY_STATUS: &str = r#"{"status":"ready","ready":true,"servicesReady":true}"#;

    /// Number of connections the probe makes before any search: detect does
    /// GET / and GET /api/service-status; readiness does one more status GET.
    const PROBE_CONNS: usize = 3;

    fn write_manifest(dir: &Path, entries: &[&str]) -> PathBuf {
        let entries_json: Vec<Value> = entries
            .iter()
            .map(|p| serde_json::json!({ "archiveRelativePath": p }))
            .collect();
        let report = serde_json::json!({
            "status": "built",
            "manifest": { "entries": entries_json }
        });
        let path = dir.join("rpf_entry_manifest_report.json");
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    // ── body_for variants ──────────────────────────────────────────────────

    fn base(path: &str, search_body: &str) -> (u16, String) {
        if path == "/" {
            (200, "<html>CodeWalker.API</html>".to_string())
        } else if path == "/api/service-status" {
            (200, READY_STATUS.to_string())
        } else if path.starts_with("/api/search-file") {
            (200, search_body.to_string())
        } else {
            (404, "{}".to_string())
        }
    }

    fn answer_exact(path: &str) -> (u16, String) {
        base(path, r#"["common/data/file.ymt"]"#)
    }
    fn answer_suffix(path: &str) -> (u16, String) {
        base(path, r#"["update/common/data/file.ymt"]"#)
    }
    fn answer_filename_only(path: &str) -> (u16, String) {
        base(path, r#"["other/place/file.ymt"]"#)
    }
    fn answer_ambiguous(path: &str) -> (u16, String) {
        base(
            path,
            r#"["a/common/data/file.ymt","b/common/data/file.ymt"]"#,
        )
    }
    fn answer_no_match(path: &str) -> (u16, String) {
        base(path, r#"["zzz/unrelated.dat"]"#)
    }

    const ARP: &str = "common/data/file.ymt";

    #[test]
    fn codewalker_resolve_reads_entry_manifest_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        // Offline base URL: closed port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let r = build_codewalker_search_resolve_report(&manifest, Some(&url), None).unwrap();
        assert_eq!(r.targets.len(), 1);
        assert_eq!(r.targets[0].archive_relative_path, ARP);
    }

    #[test]
    fn codewalker_resolve_offline_returns_unresolved_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let r = build_codewalker_search_resolve_report(&manifest, Some(&url), None).unwrap();
        assert!(!r.codewalker_api_reachable);
        assert!(r.resolved_targets.is_empty());
        assert_eq!(r.unresolved_targets.len(), 1);
        assert!(!r.writer_allowed);
        assert!(!r.can_write_archive);
        assert_eq!(r.status, CodeWalkerSearchResolveStatus::Offline);
        assert!(r
            .blocked_items
            .iter()
            .any(|b| b.block_type == "codewalker_api_offline"));
    }

    #[test]
    fn codewalker_resolve_calls_search_file_get_for_each_entry() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP, "common/data/two.ymt"]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 2, answer_exact);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert_eq!(r.search_requests.len(), 2);
        let reqs = server.requests();
        let search_calls = reqs
            .iter()
            .filter(|s| s.contains("/api/search-file"))
            .count();
        assert_eq!(search_calls, 2);
    }

    #[test]
    fn codewalker_resolve_url_encodes_filename() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &["common/data/my file.ymt"]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_no_match);
        ready.recv().unwrap();
        let _ = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        let reqs = server.requests();
        assert!(
            reqs.iter().any(|s| s.contains("fileName=my%20file.ymt")),
            "encoded filename not seen: {reqs:?}"
        );
    }

    #[test]
    fn codewalker_resolve_parses_json_array_results() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_exact);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert_eq!(r.targets[0].candidates.len(), 1);
        assert_eq!(r.targets[0].candidates[0].normalized_path, ARP);
    }

    #[test]
    fn codewalker_resolve_exact_match_resolves_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_exact);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert_eq!(r.resolved_targets.len(), 1);
        assert_eq!(
            r.resolved_targets[0].match_type,
            CodeWalkerSearchConfidence::Exact
        );
        assert_eq!(r.resolved_targets[0].selected_candidate, ARP);
        assert!(r.targets[0].resolved);
        assert!(r.targets[0].exact_match_found);
    }

    #[test]
    fn codewalker_resolve_suffix_match_resolves_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_suffix);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert_eq!(r.resolved_targets.len(), 1);
        assert_eq!(
            r.resolved_targets[0].match_type,
            CodeWalkerSearchConfidence::Suffix
        );
        assert_eq!(
            r.resolved_targets[0].selected_candidate,
            "update/common/data/file.ymt"
        );
        assert!(r.targets[0].suffix_match_found);
    }

    #[test]
    fn codewalker_resolve_filename_only_match_does_not_resolve() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_filename_only);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert!(r.resolved_targets.is_empty());
        assert_eq!(r.unresolved_targets.len(), 1);
        assert_eq!(
            r.targets[0].candidates[0].confidence,
            CodeWalkerSearchConfidence::FilenameOnly
        );
        assert!(!r.targets[0].resolved);
    }

    #[test]
    fn codewalker_resolve_ambiguous_matches_unresolved() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_ambiguous);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert!(r.resolved_targets.is_empty());
        assert_eq!(r.ambiguous_targets.len(), 1);
        assert!(r.targets[0].ambiguous);
    }

    #[test]
    fn codewalker_resolve_no_match_unresolved() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, answer_no_match);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert!(r.resolved_targets.is_empty());
        assert_eq!(r.unresolved_targets.len(), 1);
        assert!(!r.targets[0].exact_match_found);
        assert!(!r.targets[0].suffix_match_found);
    }

    fn run_for_request_audit(body_for: fn(&str) -> (u16, String)) -> Vec<String> {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = MockServer::start(PROBE_CONNS + 1, body_for);
        ready.recv().unwrap();
        let _ = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        server.requests()
    }

    #[test]
    fn codewalker_resolve_does_not_call_replace_endpoint() {
        let reqs = run_for_request_audit(answer_exact);
        assert!(!reqs.iter().any(|r| r.contains("replace")));
    }

    #[test]
    fn codewalker_resolve_does_not_call_import_endpoint() {
        let reqs = run_for_request_audit(answer_exact);
        assert!(!reqs.iter().any(|r| r.contains("import")));
    }

    #[test]
    fn codewalker_resolve_does_not_call_reload_services() {
        let reqs = run_for_request_audit(answer_exact);
        assert!(!reqs.iter().any(|r| r.contains("reload-services")));
    }

    #[test]
    fn codewalker_resolve_does_not_call_set_config() {
        let reqs = run_for_request_audit(answer_exact);
        assert!(!reqs.iter().any(|r| r.contains("set-config")));
    }

    #[test]
    fn codewalker_resolve_uses_get_only() {
        let reqs = run_for_request_audit(answer_exact);
        assert!(!reqs.is_empty());
        assert!(reqs.iter().all(|r| r.starts_with("GET ")), "{reqs:?}");
    }

    #[test]
    fn codewalker_resolve_can_write_archive_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let r =
            build_codewalker_search_resolve_report(&manifest, Some("http://localhost:5555"), None)
                .unwrap();
        assert!(!r.can_write_archive);
        assert!(!r.mutation_endpoints_called);
        assert!(!r.modifies_archive);
    }

    #[test]
    fn codewalker_resolve_writer_allowed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let r =
            build_codewalker_search_resolve_report(&manifest, Some("http://localhost:5555"), None)
                .unwrap();
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
        assert!(!r.post_requests_used);
        assert!(!r.external_tool_executed);
    }

    #[test]
    fn codewalker_resolve_null_adapter_still_active() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let r =
            build_codewalker_search_resolve_report(&manifest, Some("http://localhost:5555"), None)
                .unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        let gate = r
            .safety_gates
            .iter()
            .find(|g| g.name == "null_adapter_still_active")
            .unwrap();
        assert!(gate.passed);
    }

    #[test]
    fn codewalker_resolve_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let r =
            build_codewalker_search_resolve_report(&manifest, Some("http://localhost:5555"), None)
                .unwrap();
        let out = dir.path().join("resolve.json");
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
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_resolve_does_not_modify_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let before = std::fs::read_dir(dir.path()).unwrap().count();
        let _ =
            build_codewalker_search_resolve_report(&manifest, Some("http://localhost:5555"), None)
                .unwrap();
        let after = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(before, after);
    }

    // ── Chunked-encoding mock (T0.6.12) ─────────────────────────────────────

    /// Mock that replies with Transfer-Encoding: chunked, like the real
    /// CodeWalker.API. Returns the exact-match array for search-file.
    fn start_chunked(connections: usize) -> (MockServer, Receiver<()>) {
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
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("?").to_string();
                let path = parts.next().unwrap_or("/").to_string();
                requests_thread
                    .lock()
                    .unwrap()
                    .push(format!("{method} {path}"));
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                        break;
                    }
                }
                let (status, body) = answer_exact(&path);
                let chunked = format!("{:x}\r\n{}\r\n0\r\n\r\n", body.as_bytes().len(), body);
                let resp = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{chunked}"
                );
                let mut stream = stream;
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
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

    #[test]
    fn codewalker_resolve_targets_parses_chunked_search_array() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = write_manifest(&dir.path(), &[ARP]);
        let (server, ready) = start_chunked(PROBE_CONNS + 1);
        ready.recv().unwrap();
        let r = build_codewalker_search_resolve_report(&manifest, Some(&server.base_url), None)
            .unwrap();
        assert_eq!(r.status, CodeWalkerSearchResolveStatus::Completed);
        assert_eq!(r.resolved_targets.len(), 1);
        assert_eq!(r.resolved_targets[0].selected_candidate, ARP);
        // The search used the real `fileName` query parameter.
        let reqs = server.requests();
        assert!(reqs.iter().any(|s| s.contains("fileName=")));
    }
}
