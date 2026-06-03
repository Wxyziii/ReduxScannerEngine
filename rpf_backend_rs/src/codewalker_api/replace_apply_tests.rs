#[cfg(test)]
mod replace_apply_tests {
    use crate::codewalker_api::model::{
        CodeWalkerReplaceApplyStatus, CodeWalkerReplaceTargetHashChange,
    };
    use crate::codewalker_api::replace_apply::{
        apply_codewalker_replace_on_test_archive, CONFIRMATION_PHRASE,
    };
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// One captured HTTP request: method, path, and raw body.
    #[derive(Clone)]
    struct Captured {
        method: String,
        path: String,
        body: String,
    }

    /// A mock HTTP server that records method/path/body for every request so
    /// tests can prove exactly which endpoint was hit and what payload was sent.
    struct MockServer {
        base_url: String,
        requests: Arc<Mutex<Vec<Captured>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        fn start(connections: usize, status: u16, body: &'static str) -> MockServer {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_thread = Arc::clone(&requests);

            let handle = std::thread::spawn(move || {
                for _ in 0..connections {
                    let (stream, _) = match listener.accept() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    handle_conn(stream, &requests_thread, status, body);
                }
            });

            MockServer {
                base_url: format!("http://{addr}"),
                requests,
                handle: Some(handle),
            }
        }

        fn captured(&self) -> Vec<Captured> {
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
        requests: &Arc<Mutex<Vec<Captured>>>,
        status: u16,
        body: &str,
    ) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("/").to_string();

        // Read headers; find Content-Length.
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                break;
            }
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("content-length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            }
        }
        let mut body_buf = vec![0u8; content_length];
        if content_length > 0 {
            let _ = reader.read_exact(&mut body_buf);
        }
        let req_body = String::from_utf8_lossy(&body_buf).to_string();

        requests.lock().unwrap().push(Captured {
            method,
            path,
            body: req_body,
        });

        let resp = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.as_bytes().len()
        );
        let mut stream = stream;
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
    }

    // ── Fixture/report builders ─────────────────────────────────────────────

    const RPF_CONTENT: &[u8] = b"FAKE-RPF copied test archive fixture\n";

    fn write_target(dir: &Path) -> PathBuf {
        let p = dir.join("test_copies/fake_update.rpf");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, RPF_CONTENT).unwrap();
        p
    }

    fn write_json(dir: &Path, name: &str, v: &Value) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
        p
    }

    fn gate_report(dir: &Path, target: &Path, eligible: bool, classification: &str) -> PathBuf {
        write_json(
            dir,
            "codewalker_execution_gate.json",
            &json!({
                "status": if eligible { "eligible" } else { "blocked" },
                "codewalkerExecutionEligible": eligible,
                "codewalkerExecutionPerformed": false,
                "targetArchiveClassification": classification,
                "targetRpf": target.display().to_string(),
                "targetRpfExists": true,
                "writerAllowed": false
            }),
        )
    }

    fn dry_plan(dir: &Path, with_requests: bool) -> PathBuf {
        let requests = if with_requests {
            json!([{
                "endpoint": "/api/replace-file",
                "method": "POST",
                "rpfPath": "update/common/data/x.dat",
                "archivePath": "update/common/data/x.dat",
                "sourceFilePath": "/bundle/files/common/data/x.dat",
                "archiveRelativePath": "common/data/x.dat",
                "dryRunOnly": true
            }])
        } else {
            json!([])
        };
        write_json(
            dir,
            "codewalker_dry_replace_plan.json",
            &json!({
                "status": if with_requests { "planned" } else { "blocked" },
                "dryRunOnly": true,
                "readyForExecution": false,
                "plannedRequests": requests
            }),
        )
    }

    /// Full eligible setup: copied test archive gate + planned dry plan.
    struct Setup {
        target: PathBuf,
        gate: PathBuf,
        plan: PathBuf,
    }

    fn eligible_setup(dir: &Path) -> Setup {
        let target = write_target(dir);
        let gate = gate_report(dir, &target, true, "copied_test_archive");
        let plan = dry_plan(dir, true);
        Setup { target, gate, plan }
    }

    // ── Blocked cases (no HTTP) ─────────────────────────────────────────────

    #[test]
    fn codewalker_replace_apply_blocks_without_execute_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &s.gate,
            &s.plan,
            false,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::Blocked);
        assert!(!r.replace_requests_sent);
        assert_eq!(r.replace_request_count, 0);
    }

    #[test]
    fn codewalker_replace_apply_blocks_missing_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &s.gate,
            &s.plan,
            true,
            None,
        )
        .unwrap();
        assert!(!r.confirmation_phrase_provided);
        assert!(!r.replace_requests_sent);
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::Blocked);
    }

    #[test]
    fn codewalker_replace_apply_blocks_wrong_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &s.gate,
            &s.plan,
            true,
            Some("nope"),
        )
        .unwrap();
        assert!(r.confirmation_phrase_provided);
        assert!(!r.confirmation_phrase_matched);
        assert!(!r.replace_requests_sent);
    }

    #[test]
    fn codewalker_replace_apply_blocks_ineligible_execution_gate() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = write_target(dir.path());
        let gate = gate_report(dir.path(), &target, false, "unknown_archive");
        let plan = dry_plan(dir.path(), true);
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &gate,
            &plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.execution_gate_eligible);
        assert!(!r.replace_requests_sent);
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::Blocked);
    }

    #[test]
    fn codewalker_replace_apply_blocks_missing_dry_plan() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = write_target(dir.path());
        let gate = gate_report(dir.path(), &target, true, "copied_test_archive");
        let plan = dir.path().join("missing_plan.json");
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &gate,
            &plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.replace_requests_sent);
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::InvalidInput);
    }

    #[test]
    fn codewalker_replace_apply_blocks_empty_planned_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = write_target(dir.path());
        let gate = gate_report(dir.path(), &target, true, "copied_test_archive");
        let plan = dry_plan(dir.path(), false);
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &gate,
            &plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.replace_requests_sent);
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::Blocked);
    }

    // ── Executed cases (mock HTTP) ──────────────────────────────────────────

    #[test]
    fn codewalker_replace_apply_sends_post_to_replace_endpoint_when_all_gates_pass() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(r.replace_requests_sent);
        assert_eq!(r.replace_request_count, 1);
        let caps = server.captured();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].method, "POST");
        assert_eq!(caps[0].path, "/api/replace-file");
        let _ = s.target;
    }

    #[test]
    fn codewalker_replace_apply_payload_contains_resolved_path_and_source_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let _ = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        let caps = server.captured();
        let body: Value = serde_json::from_str(&caps[0].body).unwrap();
        assert_eq!(body["rpfPath"], "update/common/data/x.dat");
        assert_eq!(body["sourceFilePath"], "/bundle/files/common/data/x.dat");
        assert_eq!(body["archiveRelativePath"], "common/data/x.dat");
        assert_eq!(body["dryRunOnly"], false);
        assert_eq!(body["execute"], true);
    }

    #[test]
    fn codewalker_replace_apply_records_success_response() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::Executed);
        assert_eq!(r.successful_replace_count, 1);
        assert_eq!(r.failed_replace_count, 0);
        assert!(r.item_results[0].response.succeeded);
        assert_eq!(r.item_results[0].response.http_status, Some(200));
        assert!(r.modifies_archive);
        assert!(r.codewalker_execution_performed);
    }

    #[test]
    fn codewalker_replace_apply_records_failure_response() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 500, r#"{"error":"boom"}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(r.status, CodeWalkerReplaceApplyStatus::Failed);
        assert_eq!(r.failed_replace_count, 1);
        assert_eq!(r.successful_replace_count, 0);
        assert!(!r.item_results[0].response.succeeded);
        assert_eq!(r.item_results[0].response.http_status, Some(500));
        assert!(!r.modifies_archive);
    }

    #[test]
    fn codewalker_replace_apply_does_not_call_import_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.import_endpoint_called);
        let caps = server.captured();
        assert!(caps.iter().all(|c| c.path != "/api/import"));
    }

    #[test]
    fn codewalker_replace_apply_does_not_call_reload_services() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.reload_services_called);
        let caps = server.captured();
        assert!(caps.iter().all(|c| c.path != "/api/reload-services"));
    }

    #[test]
    fn codewalker_replace_apply_does_not_call_set_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.set_config_called);
        let caps = server.captured();
        assert!(caps.iter().all(|c| c.path != "/api/set-config"));
    }

    #[test]
    fn codewalker_replace_apply_does_not_call_search_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.search_endpoint_called);
        let caps = server.captured();
        assert!(caps.iter().all(|c| !c.path.starts_with("/api/search-file")));
    }

    #[test]
    fn codewalker_replace_apply_does_not_execute_external_tool() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.external_tool_executed);
    }

    #[test]
    fn codewalker_replace_apply_null_adapter_still_active() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        assert!(r.null_adapter_active);
    }

    #[test]
    fn codewalker_replace_apply_writer_allowed_global_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
        assert!(!r.native_writer_used);
        assert!(!r.native_parser_used);
    }

    #[test]
    fn codewalker_replace_apply_records_target_hash_before_and_after() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        // Mock does not touch the local fixture, so the hash is unchanged.
        assert!(r.original_target_sha256.is_some());
        assert!(r.post_execution_target_sha256.is_some());
        assert_eq!(
            r.target_hash_changed,
            CodeWalkerReplaceTargetHashChange::Unchanged
        );
        // Fixture content must be byte-identical.
        assert_eq!(std::fs::read(&s.target).unwrap(), RPF_CONTENT);
    }

    #[test]
    fn codewalker_replace_apply_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = apply_codewalker_replace_on_test_archive(
            Some(&server.base_url),
            &s.gate,
            &s.plan,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        let out = dir.path().join("codewalker_replace_apply.json");
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["replaceEndpoint"], "/api/replace-file");
        assert_eq!(v["replaceRequestsSent"], true);
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
        assert_eq!(v["importEndpointCalled"], false);
        assert_eq!(v["reloadServicesCalled"], false);
        assert_eq!(v["setConfigCalled"], false);
        assert_eq!(v["searchEndpointCalled"], false);
    }

    #[test]
    fn codewalker_replace_apply_does_not_modify_files_when_blocked() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let before = std::fs::read(&s.target).unwrap();
        let count_before = std::fs::read_dir(dir.path()).unwrap().count();
        // Blocked: no --execute.
        let r = apply_codewalker_replace_on_test_archive(
            Some("http://127.0.0.1:1"),
            &s.gate,
            &s.plan,
            false,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.replace_requests_sent);
        assert_eq!(std::fs::read(&s.target).unwrap(), before);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), count_before);
    }
}
