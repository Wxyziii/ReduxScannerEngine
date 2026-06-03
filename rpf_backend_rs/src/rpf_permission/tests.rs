#[cfg(test)]
mod tests {
    use crate::rpf_permission::model::RpfWriterPermissionStatus;
    use crate::rpf_permission::permission::{
        build_writer_permission_report, EXPECTED_CONFIRMATION_PHRASE,
    };
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    const FAKE_RPF: &str = "../examples/rpf_fixtures/fake_update.rpf";

    /// An existing, empty bundle directory (permission only checks it exists).
    fn bundle_dir(dir: &Path) -> PathBuf {
        let bundle = dir.join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        bundle
    }

    fn write_json(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    fn write_readiness(dir: &Path) -> PathBuf {
        let path = dir.join("readiness.json");
        write_json(
            &path,
            &serde_json::json!({ "readyToWrite": false, "targetRpf": FAKE_RPF }),
        );
        path
    }

    fn write_entry_manifest(dir: &Path) -> PathBuf {
        let path = dir.join("entry_manifest.json");
        write_json(
            &path,
            &serde_json::json!({ "readyForWrite": false, "targetRpf": FAKE_RPF }),
        );
        path
    }

    fn write_backup(dir: &Path) -> PathBuf {
        let path = dir.join("backup.json");
        write_json(
            &path,
            &serde_json::json!({
                "targetArchivePath": FAKE_RPF,
                "hashVerified": true,
                "safeForFutureWrite": true,
                "status": "backed_up"
            }),
        );
        path
    }

    fn has_block(
        report: &crate::rpf_permission::model::RpfWriterPermissionReport,
        ty: &str,
    ) -> bool {
        report.blocked.iter().any(|b| b.block_type == ty)
    }

    fn gate_failed(
        report: &crate::rpf_permission::model::RpfWriterPermissionReport,
        name: &str,
    ) -> bool {
        report.gates.iter().any(|g| g.name == name && !g.passed)
    }

    #[test]
    fn writer_permission_requires_bundle_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("does_not_exist");
        let report = build_writer_permission_report(
            &missing,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(report.status, RpfWriterPermissionStatus::Blocked);
        assert!(gate_failed(&report, "bundle_dir_present"));
        assert!(has_block(&report, "bundle_dir_missing"));
        assert!(report.permission_token.is_none());
    }

    #[test]
    fn writer_permission_requires_target_rpf() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let missing_target = dir.path().join("missing.rpf");
        let report = build_writer_permission_report(
            &bundle,
            &missing_target,
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(report.status, RpfWriterPermissionStatus::Blocked);
        assert!(gate_failed(&report, "target_rpf_present"));
        assert!(has_block(&report, "target_rpf_missing"));
        assert!(report.permission_token.is_none());
    }

    #[test]
    fn writer_permission_blocks_non_rpf_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let not_rpf = dir.path().join("target.txt");
        std::fs::write(&not_rpf, b"x").unwrap();
        let report = build_writer_permission_report(
            &bundle,
            &not_rpf,
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(report.status, RpfWriterPermissionStatus::Blocked);
        assert!(gate_failed(&report, "target_rpf_extension_valid"));
        assert!(has_block(&report, "target_not_rpf"));
        assert!(report.permission_token.is_none());
    }

    #[test]
    fn writer_permission_blocks_missing_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report =
            build_writer_permission_report(&bundle, Path::new(FAKE_RPF), None, None, None, None)
                .unwrap();
        assert_eq!(report.status, RpfWriterPermissionStatus::Blocked);
        assert!(!report.confirmation_phrase_provided);
        assert!(gate_failed(&report, "confirmation_phrase_provided"));
        assert!(has_block(&report, "confirmation_phrase_missing"));
        assert!(report.permission_token.is_none());
    }

    #[test]
    fn writer_permission_blocks_mismatched_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some("totally wrong phrase"),
        )
        .unwrap();
        assert_eq!(report.status, RpfWriterPermissionStatus::Blocked);
        assert!(report.confirmation_phrase_provided);
        assert!(!report.confirmation_phrase_matched);
        assert!(gate_failed(&report, "confirmation_phrase_matched"));
        assert!(has_block(&report, "confirmation_phrase_mismatch"));
        assert!(report.permission_token.is_none());
    }

    #[test]
    fn writer_permission_accepts_exact_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(report.confirmation_phrase_matched);
        assert_eq!(report.status, RpfWriterPermissionStatus::TokenIssued);
        assert!(report.permission_token.is_some());
    }

    #[test]
    fn writer_permission_reads_valid_backup_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let backup = write_backup(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            Some(&backup),
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!gate_failed(
            &report,
            "backup_report_hash_verified_if_present"
        ));
        assert_eq!(report.status, RpfWriterPermissionStatus::TokenIssued);
        assert!(report.permission_token.is_some());
    }

    #[test]
    fn writer_permission_reads_valid_readiness_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let readiness = write_readiness(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            Some(&readiness),
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!gate_failed(&report, "readiness_report_valid_if_present"));
        assert_eq!(report.status, RpfWriterPermissionStatus::TokenIssued);
    }

    #[test]
    fn writer_permission_reads_valid_entry_manifest_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let manifest = write_entry_manifest(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            Some(&manifest),
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!gate_failed(&report, "entry_manifest_valid_if_present"));
        assert_eq!(report.status, RpfWriterPermissionStatus::TokenIssued);
    }

    #[test]
    fn writer_permission_generates_token_when_inputs_valid_and_phrase_matches() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let readiness = write_readiness(dir.path());
        let manifest = write_entry_manifest(dir.path());
        let backup = write_backup(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            Some(&readiness),
            Some(&manifest),
            Some(&backup),
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert_eq!(report.status, RpfWriterPermissionStatus::TokenIssued);
        let token = report.permission_token.expect("token should be present");
        assert_eq!(token.token_version, "1");
        assert!(!token.writer_allowed);
        assert!(!token.ready_to_write_at_creation);
        assert!(token.confirmed_backup_required);
        assert!(token.confirmed_restore_required);
        assert!(token.confirmed_hash_verification_required);
        assert!(token.confirmed_manual_action);
        assert!(!token.modifies_rpf);
        assert!(!token.external_tool_used);
        assert!(!token.native_writer_used);
    }

    #[test]
    fn writer_permission_writer_allowed_false_by_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!report.writer_allowed);
        assert!(!report.summary.writer_allowed);
        assert!(!report.modifies_target_archive);
        assert!(!report.real_writer_implemented);
        assert!(!report.native_parser_implemented);
        // Even when a token is issued, it never authorizes writing.
        assert!(!report.permission_token.as_ref().unwrap().writer_allowed);
        assert!(gate_failed(&report, "writer_permission_allowed"));
    }

    #[test]
    fn writer_permission_blocks_when_real_writer_not_implemented() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(gate_failed(&report, "real_rpf_writer_implemented"));
        assert!(has_block(&report, "real_rpf_writer_not_implemented"));
    }

    #[test]
    fn writer_permission_blocks_when_native_parser_not_implemented() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(gate_failed(&report, "native_rpf_parser_implemented"));
        assert!(has_block(&report, "native_rpf_parser_not_implemented"));
    }

    #[test]
    fn writer_permission_blocks_when_adapter_cannot_write() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(gate_failed(&report, "adapter_supports_write"));
        assert!(has_block(&report, "active_adapter_cannot_write"));
    }

    #[test]
    fn writer_permission_does_not_modify_target_or_bundle() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        // Put a sentinel file in the bundle dir to detect any mutation.
        let sentinel = bundle.join("sentinel.txt");
        std::fs::write(&sentinel, b"unchanged").unwrap();

        let target_before = std::fs::read(FAKE_RPF).unwrap();
        let sentinel_before = std::fs::read(&sentinel).unwrap();

        let readiness = write_readiness(dir.path());
        let manifest = write_entry_manifest(dir.path());
        let backup = write_backup(dir.path());
        build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            Some(&readiness),
            Some(&manifest),
            Some(&backup),
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();

        assert_eq!(target_before, std::fs::read(FAKE_RPF).unwrap());
        assert_eq!(sentinel_before, std::fs::read(&sentinel).unwrap());
    }

    #[test]
    fn writer_permission_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = bundle_dir(dir.path());
        let report = build_writer_permission_report(
            &bundle,
            Path::new(FAKE_RPF),
            None,
            None,
            None,
            Some(EXPECTED_CONFIRMATION_PHRASE),
        )
        .unwrap();

        let out_path = dir.path().join("writer_permission.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(out_path.is_file());

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["modifiesTargetArchive"], false);
        assert_eq!(v["realWriterImplemented"], false);
        assert_eq!(v["nativeParserImplemented"], false);
        assert_eq!(v["permissionToken"]["writerAllowed"], false);
        assert_eq!(v["permissionToken"]["tokenVersion"], "1");
    }
}
