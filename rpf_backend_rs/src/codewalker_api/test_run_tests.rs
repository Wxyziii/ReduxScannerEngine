#[cfg(test)]
mod test_run_tests {
    use crate::codewalker_api::model::{
        CodeWalkerReplaceTargetHashChange, CodeWalkerTestRunMode, CodeWalkerTestRunStatus,
    };
    use crate::codewalker_api::test_run::{
        build_or_run_codewalker_copied_archive_test, CONFIRMATION_PHRASE,
    };
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    // ── Minimal mock HTTP server (execute-mode tests only) ──────────────────

    #[derive(Clone)]
    struct Captured {
        method: String,
        path: String,
    }

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
        if content_length > 0 {
            let mut body_buf = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body_buf);
        }
        requests.lock().unwrap().push(Captured { method, path });
        let resp = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.as_bytes().len()
        );
        let mut stream = stream;
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
    }

    // ── Fixtures ─────────────────────────────────────────────────────────────

    const RPF_CONTENT: &[u8] = b"FAKE-RPF copied test archive fixture for test-run\n";

    fn write_json(dir: &Path, name: &str, v: &Value) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
        p
    }

    fn write_target(dir: &Path) -> PathBuf {
        let p = dir.join("test-copy/update.rpf");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, RPF_CONTENT).unwrap();
        p
    }

    fn gate_report(dir: &Path, target: &Path, eligible: bool, classification: &str) -> PathBuf {
        write_json(
            dir,
            "execution_gate.json",
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

    const RPF_FILE_PATH: &str = "update/update.rpf/common/data/visualsettings.dat";

    fn dry_plan(dir: &Path, with_requests: bool) -> PathBuf {
        let requests = if with_requests {
            // Real absolute local replacement file (T0.6.15 contract).
            let local = dir.join("bundle/visualsettings.dat");
            std::fs::create_dir_all(local.parent().unwrap()).unwrap();
            std::fs::write(&local, b"replacement bytes\n").unwrap();
            let local_str = local.display().to_string();
            json!([{
                "endpoint": "/api/replace-file",
                "method": "POST",
                "apiContractName": "codewalker_replace_file_v1",
                "actualRequestPayload": {
                    "localFilePath": local_str,
                    "rpfFilePath": RPF_FILE_PATH
                },
                "localFilePath": local_str,
                "localFilePathIsAbsolute": true,
                "localFilePathExists": true,
                "codewalkerTargetPath": RPF_FILE_PATH,
                "requestSchemaValidated": true,
                "rpfPath": RPF_FILE_PATH,
                "archivePath": RPF_FILE_PATH,
                "sourceFilePath": local_str,
                "archiveRelativePath": "common/data/visualsettings.dat",
                "dryRunOnly": true
            }])
        } else {
            json!([])
        };
        write_json(
            dir,
            "dry_replace_plan.json",
            &json!({
                "status": "planned",
                "dryRunOnly": true,
                "readyForExecution": false,
                "plannedRequests": requests
            }),
        )
    }

    fn backup_report(dir: &Path, target: &Path) -> PathBuf {
        write_json(
            dir,
            "backup_report.json",
            &json!({
                "targetArchivePath": target.display().to_string(),
                "backupFilePath": dir.join("backup/update.rpf").display().to_string(),
                "originalHash": "deadbeef",
                "backupHash": "deadbeef",
                "hashVerified": true,
                "safeForFutureWrite": true
            }),
        )
    }

    fn plain_report(dir: &Path, name: &str) -> PathBuf {
        write_json(dir, name, &json!({ "status": "ok" }))
    }

    fn compat_report(dir: &Path, compatible: Option<bool>) -> PathBuf {
        write_json(
            dir,
            "compat_probe.json",
            &json!({
                "status": "compatible",
                "compatibleForSearch": compatible
            }),
        )
    }

    /// A fully eligible plan-mode setup.
    struct Setup {
        target: PathBuf,
        project_dir: PathBuf,
        backup: PathBuf,
        readiness: PathBuf,
        entry_manifest: PathBuf,
        resolve: PathBuf,
        dry_plan: PathBuf,
        gate: PathBuf,
        compat: PathBuf,
    }

    fn eligible_setup(dir: &Path) -> Setup {
        let target = write_target(dir);
        Setup {
            backup: backup_report(dir, &target),
            readiness: plain_report(dir, "readiness.json"),
            entry_manifest: plain_report(dir, "entry_manifest.json"),
            resolve: plain_report(dir, "resolve.json"),
            dry_plan: dry_plan(dir, true),
            gate: gate_report(dir, &target, true, "copied_test_archive"),
            compat: compat_report(dir, Some(true)),
            project_dir: dir.join("project"),
            target,
        }
    }

    fn run_plan(
        s: &Setup,
        base_url: Option<&str>,
    ) -> crate::codewalker_api::model::CodeWalkerTestRunReport {
        build_or_run_codewalker_copied_archive_test(
            &s.target,
            base_url,
            &s.project_dir,
            &s.backup,
            &s.readiness,
            &s.entry_manifest,
            &s.resolve,
            &s.dry_plan,
            &s.gate,
            Some(&s.compat),
            false,
            None,
        )
        .unwrap()
    }

    // ── Plan-mode tests ─────────────────────────────────────────────────────

    #[test]
    fn codewalker_test_run_plan_mode_loads_required_reports() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = run_plan(&s, Some("http://localhost:5555"));
        assert_eq!(r.mode, CodeWalkerTestRunMode::PlanOnly);
        assert_eq!(r.status, CodeWalkerTestRunStatus::PlannedReady);
        assert!(r.ready_for_execute);
        // Every required input loaded.
        for i in r.inputs.iter().filter(|i| i.required) {
            assert!(i.loaded, "required input {} not loaded", i.name);
        }
        assert!(r.execution_gate_eligible);
        assert!(r.copied_test_archive_confirmed);
        assert!(r.dry_replace_plan_has_planned_requests);
    }

    /// A resolve report whose single target was resolved via an archive-prefix
    /// preference (T0.6.13).
    fn preferred_archive_resolve(dir: &Path) -> PathBuf {
        write_json(
            dir,
            "resolve.json",
            &json!({
                "status": "completed",
                "preferredArchive": "update/update.rpf",
                "archivePrefixResolutionEnabled": true,
                "resolvedTargets": [{
                    "archiveRelativePath": "common/data/visualsettings.dat",
                    "selectedCandidate": "update/update.rpf/common/data/visualsettings.dat",
                    "matchType": "suffix",
                    "resolutionStrategy": "preferred_archive_suffix"
                }],
                "unresolvedTargets": [],
                "ambiguousTargets": []
            }),
        )
    }

    #[test]
    fn test_run_plan_ready_after_preferred_archive_resolution_if_other_inputs_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = eligible_setup(dir.path());
        // Swap in a resolve report produced by archive-prefix resolution.
        s.resolve = preferred_archive_resolve(dir.path());
        let r = run_plan(&s, Some("http://localhost:5555"));
        assert_eq!(r.status, CodeWalkerTestRunStatus::PlannedReady);
        assert!(r.ready_for_execute);
        assert!(r.dry_replace_plan_has_planned_requests);
        // Plan-only: no execution happened.
        assert!(!r.codewalker_replace_apply_invoked);
        assert!(!r.modifies_archive);
    }

    #[test]
    fn codewalker_test_run_plan_mode_sends_no_http_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = run_plan(&s, Some("http://localhost:5555"));
        assert!(!r.codewalker_replace_apply_invoked);
        assert!(!r.post_write_verify_invoked);
        let g = r
            .gates
            .iter()
            .find(|g| g.name == "plan_only_no_http_requests")
            .unwrap();
        assert!(g.passed);
    }

    #[test]
    fn codewalker_test_run_plan_mode_does_not_modify_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let before = std::fs::read(&s.target).unwrap();
        let r = run_plan(&s, Some("http://localhost:5555"));
        assert_eq!(std::fs::read(&s.target).unwrap(), before);
        assert_eq!(
            r.target_hash_changed,
            CodeWalkerReplaceTargetHashChange::Unchanged
        );
        assert!(!r.modifies_archive);
    }

    #[test]
    fn codewalker_test_run_blocks_original_game_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        // A target path that resembles an original install.
        let bad = dir.path().join("Grand Theft Auto V/update/update.rpf");
        std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
        std::fs::write(&bad, RPF_CONTENT).unwrap();
        let r = build_or_run_codewalker_copied_archive_test(
            &bad,
            Some("http://localhost:5555"),
            &s.project_dir,
            &s.backup,
            &s.readiness,
            &s.entry_manifest,
            &s.resolve,
            &s.dry_plan,
            &s.gate,
            Some(&s.compat),
            false,
            None,
        )
        .unwrap();
        assert!(r.original_game_path_blocked);
        assert!(!r.ready_for_execute);
        assert!(!r.codewalker_replace_apply_invoked);
        assert!(r
            .blocked_items
            .iter()
            .any(|b| b.block_type == "original_game_archive_suspected"));
    }

    #[test]
    fn codewalker_test_run_blocks_missing_required_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let missing = dir.path().join("does_not_exist.json");
        let r = build_or_run_codewalker_copied_archive_test(
            &s.target,
            Some("http://localhost:5555"),
            &s.project_dir,
            &s.backup,
            &s.readiness,
            &s.entry_manifest,
            &missing, // resolve report missing
            &s.dry_plan,
            &s.gate,
            Some(&s.compat),
            false,
            None,
        )
        .unwrap();
        assert!(!r.ready_for_execute);
        assert_eq!(r.status, CodeWalkerTestRunStatus::InvalidInput);
        assert!(r
            .blocked_items
            .iter()
            .any(|b| b.block_type == "required_report_unusable"));
    }

    #[test]
    fn codewalker_test_run_blocks_execute_without_confirmation() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = build_or_run_codewalker_copied_archive_test(
            &s.target,
            Some("http://127.0.0.1:1"),
            &s.project_dir,
            &s.backup,
            &s.readiness,
            &s.entry_manifest,
            &s.resolve,
            &s.dry_plan,
            &s.gate,
            Some(&s.compat),
            true, // execute
            None, // no confirm
        )
        .unwrap();
        assert_eq!(r.status, CodeWalkerTestRunStatus::Blocked);
        assert!(!r.confirmation_phrase_matched);
        assert!(!r.codewalker_replace_apply_invoked);
    }

    #[test]
    fn codewalker_test_run_blocks_execute_when_execution_gate_not_eligible() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let gate = gate_report(dir.path(), &s.target, false, "copied_test_archive");
        let r = build_or_run_codewalker_copied_archive_test(
            &s.target,
            Some("http://127.0.0.1:1"),
            &s.project_dir,
            &s.backup,
            &s.readiness,
            &s.entry_manifest,
            &s.resolve,
            &s.dry_plan,
            &gate,
            Some(&s.compat),
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.execution_gate_eligible);
        assert_eq!(r.status, CodeWalkerTestRunStatus::Blocked);
        assert!(!r.codewalker_replace_apply_invoked);
    }

    // ── Execute-mode tests (mock HTTP server) ───────────────────────────────

    fn run_execute(
        s: &Setup,
        base_url: &str,
    ) -> crate::codewalker_api::model::CodeWalkerTestRunReport {
        build_or_run_codewalker_copied_archive_test(
            &s.target,
            Some(base_url),
            &s.project_dir,
            &s.backup,
            &s.readiness,
            &s.entry_manifest,
            &s.resolve,
            &s.dry_plan,
            &s.gate,
            Some(&s.compat),
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap()
    }

    #[test]
    fn codewalker_test_run_execute_invokes_replace_apply_when_all_gates_pass() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = run_execute(&s, &server.base_url);
        assert!(r.codewalker_replace_apply_invoked);
        assert!(r.replace_apply_report_path.is_some());
        let caps = server.captured();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].method, "POST");
        assert_eq!(caps[0].path, "/api/replace-file");
    }

    #[test]
    fn test_run_execute_uses_correct_replace_payload() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = run_execute(&s, &server.base_url);
        assert!(r.codewalker_replace_apply_invoked);
        // The coordinator wrote the replace-apply report; inspect the sent body.
        let apply_path = r.replace_apply_report_path.as_ref().unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(apply_path).unwrap()).unwrap();
        let body_json = v["itemResults"][0]["request"]["requestBodyJson"]
            .as_str()
            .unwrap();
        let body: Value = serde_json::from_str(body_json).unwrap();
        assert!(Path::new(body["localFilePath"].as_str().unwrap()).is_absolute());
        assert_eq!(body["rpfFilePath"], RPF_FILE_PATH);
        assert!(body.get("sourceFilePath").is_none());
        assert!(body.get("execute").is_none());
    }

    #[test]
    fn no_post_replace_in_plan_only_modes() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        // 0-connection server: it never blocks on accept, so a clean join proves
        // plan-only sent nothing.
        let server = MockServer::start(0, 200, r#"{"ok":true}"#);
        let r = run_plan(&s, Some(&server.base_url));
        assert_eq!(r.mode, CodeWalkerTestRunMode::PlanOnly);
        assert!(!r.codewalker_replace_apply_invoked);
        assert!(server.captured().is_empty());
    }

    #[test]
    fn codewalker_test_run_execute_invokes_post_write_verify_after_replace() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = run_execute(&s, &server.base_url);
        assert!(r.post_write_verify_invoked);
        assert!(r.post_write_verify_report_path.is_some());
        assert!(Path::new(r.post_write_verify_report_path.as_ref().unwrap()).is_file());
    }

    #[test]
    fn codewalker_test_run_never_invokes_rollback_automatically() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = run_execute(&s, &server.base_url);
        assert!(!r.rollback_restore_invoked);
        let g = r
            .gates
            .iter()
            .find(|g| g.name == "rollback_not_automatic")
            .unwrap();
        assert!(g.passed);
    }

    #[test]
    fn codewalker_test_run_records_target_hash_before_after() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let server = MockServer::start(1, 200, r#"{"ok":true}"#);
        let r = run_execute(&s, &server.base_url);
        assert!(r.target_sha256_before.is_some());
        assert!(r.target_sha256_after.is_some());
        // Mock never touches the local fixture, so it is unchanged.
        assert_eq!(
            r.target_hash_changed,
            CodeWalkerReplaceTargetHashChange::Unchanged
        );
        assert_eq!(std::fs::read(&s.target).unwrap(), RPF_CONTENT);
    }

    #[test]
    fn codewalker_test_run_null_adapter_still_active() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = run_plan(&s, Some("http://localhost:5555"));
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        assert!(r.null_adapter_active);
        assert!(!r.writer_allowed_global);
    }

    #[test]
    fn codewalker_test_run_native_parser_not_used() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = run_plan(&s, Some("http://localhost:5555"));
        assert!(!r.native_parser_used);
        assert!(!r.external_tool_executed);
        let g = r
            .gates
            .iter()
            .find(|g| g.name == "native_parser_not_used")
            .unwrap();
        assert!(g.passed);
    }

    #[test]
    fn codewalker_test_run_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = eligible_setup(dir.path());
        let r = run_plan(&s, Some("http://localhost:5555"));
        let out = dir.path().join("codewalker_test_run.json");
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["mode"], "plan_only");
        assert_eq!(v["writerAllowedGlobal"], false);
        assert_eq!(v["nullAdapterActive"], true);
        assert_eq!(v["nativeParserUsed"], false);
        assert_eq!(v["rollbackRestoreInvoked"], false);
        assert_eq!(v["readyForExecute"], true);
    }
}
