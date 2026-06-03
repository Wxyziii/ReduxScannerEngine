#[cfg(test)]
mod manual_harness_tests {
    use crate::codewalker_api::manual_harness::{
        build_codewalker_manual_test_harness, CONFIRMATION_PHRASE,
    };
    use crate::codewalker_api::model::{
        CodeWalkerManualHarnessStatus, CodeWalkerTargetArchiveClassification,
    };
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};

    const FAKE_RPF: &[u8] = b"FAKE-RPF tiny copied test archive content\n";

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        format!("{:x}", h.finalize())
    }

    fn write_file(p: &Path, bytes: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    /// A safe copied test archive under a test-copy directory.
    fn test_copy_rpf(dir: &Path) -> PathBuf {
        let p = dir.join("test-copy/update.rpf");
        write_file(&p, FAKE_RPF);
        p
    }

    #[test]
    fn codewalker_manual_harness_requires_target_rpf() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("test-copy/nope.rpf");
        let r = build_codewalker_manual_test_harness(
            &missing, None, true, None, None, false, false, None,
        )
        .unwrap();
        assert!(!r.target_rpf_exists);
        assert_eq!(r.status, CodeWalkerManualHarnessStatus::InvalidInput);
    }

    #[test]
    fn codewalker_manual_harness_blocks_non_rpf_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("test-copy/update.txt");
        write_file(&p, FAKE_RPF);
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        assert!(!r.target_rpf_extension_valid);
        assert_eq!(r.status, CodeWalkerManualHarnessStatus::InvalidInput);
        assert!(r
            .gates
            .iter()
            .any(|g| g.name == "target_rpf_extension_valid" && !g.passed));
    }

    #[test]
    fn codewalker_manual_harness_requires_test_copy_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let r =
            build_codewalker_manual_test_harness(&p, None, false, None, None, false, false, None)
                .unwrap();
        assert_eq!(r.status, CodeWalkerManualHarnessStatus::Blocked);
        assert!(r
            .gates
            .iter()
            .any(|g| g.name == "target_marked_as_test_copy" && !g.passed));
        assert!(!r.target_path_allowed_for_test_execution);
    }

    #[test]
    fn codewalker_manual_harness_blocks_original_game_path() {
        // Use a path that matches the original-install heuristic regardless of disk.
        let p =
            PathBuf::from("C:/Program Files/Rockstar Games/Grand Theft Auto V/update/update.rpf");
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        assert!(r.original_game_path_blocked);
        assert_eq!(
            r.target_classification,
            CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected
        );
        assert!(r
            .gates
            .iter()
            .any(|g| g.name == "target_not_original_game_archive" && !g.passed));
        assert!(r.status != CodeWalkerManualHarnessStatus::Planned);
    }

    #[test]
    fn codewalker_manual_harness_accepts_safe_copied_test_archive() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        assert_eq!(r.status, CodeWalkerManualHarnessStatus::Planned);
        assert_eq!(
            r.target_classification,
            CodeWalkerTargetArchiveClassification::CopiedTestArchive
        );
        assert!(r.target_path_allowed_for_test_execution);
        assert!(!r.original_game_path_blocked);
        assert_eq!(r.base_url, "http://localhost:5555");
    }

    #[test]
    fn codewalker_manual_harness_generates_step_checklist() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        assert_eq!(r.planned_steps.len(), 12);
        assert_eq!(r.planned_steps[0].command_name, "probe-rpf");
        // Steps are indexed 1..=12.
        for (i, s) in r.planned_steps.iter().enumerate() {
            assert_eq!(s.index, i + 1);
        }
    }

    #[test]
    fn codewalker_manual_harness_includes_required_pipeline_commands() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        let names: Vec<&str> = r
            .planned_steps
            .iter()
            .map(|s| s.command_name.as_str())
            .collect();
        for expected in [
            "probe-rpf",
            "backup-rpf",
            "codewalker-detect",
            "codewalker-readiness",
            "rpf-entry-manifest",
            "codewalker-resolve-targets",
            "codewalker-dry-replace-plan",
            "writer-permission",
            "codewalker-execution-gate",
            "codewalker-replace-apply",
            "codewalker-post-write-verify",
            "codewalker-rollback-restore",
        ] {
            assert!(names.contains(&expected), "missing step {}", expected);
        }
        // Mutating commands are commented out in the generated checklist.
        let replace_cmd = r
            .generated_commands
            .iter()
            .find(|c| c.contains("codewalker-replace-apply"))
            .unwrap();
        assert!(replace_cmd.trim_start().starts_with('#'));
    }

    #[test]
    fn codewalker_manual_harness_generate_script_writes_file_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let proj = dir.path().join("proj");
        let r = build_codewalker_manual_test_harness(
            &p,
            None,
            true,
            Some(&proj),
            None,
            true,
            false,
            None,
        )
        .unwrap();
        assert_eq!(r.status, CodeWalkerManualHarnessStatus::ScriptGenerated);
        let script = r.generated_script_path.clone().unwrap();
        assert!(Path::new(&script).is_file());
        let body = fs::read_to_string(&script).unwrap();
        // Mutating commands stay commented in the script.
        assert!(body.contains("codewalker-replace-apply"));
        assert!(body.contains("COPIED TEST ARCHIVES ONLY"));
        assert!(r.summary.script_generated);
    }

    #[test]
    fn codewalker_manual_harness_plan_mode_sends_no_http_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        assert!(!r.http_requests_sent);
        assert!(!r.post_requests_sent);
        assert!(!r.codewalker_called);
        assert!(!r.external_tool_executed);
        assert!(!r.native_parser_used);
        assert!(!r.writer_allowed);
    }

    #[test]
    fn codewalker_manual_harness_plan_mode_does_not_modify_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let before = sha256_hex(&fs::read(&p).unwrap());
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        let after = sha256_hex(&fs::read(&p).unwrap());
        assert_eq!(before, after);
        assert_eq!(r.target_sha256_before, r.target_sha256_after);
        assert!(!r.modifies_archive);
        assert!(r
            .gates
            .iter()
            .any(|g| g.name == "archive_not_modified_in_plan_mode" && g.passed));
    }

    #[test]
    fn codewalker_manual_harness_execute_requires_confirmation() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        // execute without confirm.
        let r = build_codewalker_manual_test_harness(&p, None, true, None, None, false, true, None)
            .unwrap();
        assert!(r.execute_requested);
        assert!(!r.confirmation_phrase_matched);
        assert!(!r.execution_performed);
        assert!(r
            .blocked_items
            .iter()
            .any(|b| b.block_type == "confirmation_phrase_required"));

        // execute with the exact phrase: still NOT performed this milestone.
        let r2 = build_codewalker_manual_test_harness(
            &p,
            None,
            true,
            None,
            None,
            false,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(r2.confirmation_phrase_matched);
        assert!(!r2.execution_performed);
        assert_eq!(
            r2.status,
            CodeWalkerManualHarnessStatus::ExecuteRequestedNotPerformed
        );
    }

    #[test]
    fn codewalker_manual_harness_execute_without_full_inputs_does_not_run_replace() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        // No bundle/project inputs at all; confirmed execute.
        let r = build_codewalker_manual_test_harness(
            &p,
            None,
            true,
            None,
            None,
            false,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.execution_performed);
        assert!(!r.codewalker_called);
        assert!(!r.http_requests_sent);
        assert!(!r.modifies_archive);
        let before = sha256_hex(&fs::read(&p).unwrap());
        assert_eq!(r.target_sha256_after.as_deref(), Some(before.as_str()));
    }

    #[test]
    fn codewalker_manual_harness_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = test_copy_rpf(dir.path());
        let r =
            build_codewalker_manual_test_harness(&p, None, true, None, None, false, false, None)
                .unwrap();
        let out = dir.path().join("codewalker_manual_harness.json");
        fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["status"], "planned");
        assert_eq!(v["mode"], "plan_only");
        assert_eq!(v["targetClassification"], "copied_test_archive");
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["httpRequestsSent"], false);
        assert_eq!(v["codewalkerCalled"], false);
        assert_eq!(v["nativeParserUsed"], false);
        assert_eq!(v["writerAllowed"], false);
        assert!(v["plannedSteps"].as_array().unwrap().len() == 12);
    }
}
