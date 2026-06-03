#[cfg(test)]
mod tests {
    use crate::rpf_readiness::model::{RpfReadinessSeverity, RpfWriteReadinessStatus};
    use crate::rpf_readiness::readiness::build_write_readiness_report;
    use std::path::{Path, PathBuf};

    const FAKE_RPF: &str = "../examples/rpf_fixtures/fake_update.rpf";

    /// Write a minimal, valid patch bundle into `dir` and return the bundle path.
    fn write_valid_bundle(dir: &Path) -> PathBuf {
        let bundle = dir.join("bundle");
        let files = bundle.join("files");
        std::fs::create_dir_all(&files).unwrap();

        let manifest = serde_json::json!({
            "bundleFormat": "redux_patch_bundle",
            "modifiesRpf": false,
            "modifiesSourceWorkspace": false,
            "exportedFromStageOnly": true,
            "files": [
                {
                    "relativePath": "common/data/foo.meta",
                    "exportedPath": "files/common/data/foo.meta",
                    "sizeBytes": 4,
                    "extension": "meta",
                    "sha256": null
                }
            ]
        });
        std::fs::write(
            bundle.join("bundle_manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(bundle.join("patch_plan.json"), "{}").unwrap();
        std::fs::write(bundle.join("diff_report.json"), "{}").unwrap();
        let nested = files.join("common").join("data");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("foo.meta"), b"data").unwrap();

        bundle
    }

    fn write_backup_report(dir: &Path, verified: bool, target: &str) -> PathBuf {
        let path = dir.join("backup_report.json");
        let report = serde_json::json!({
            "status": if verified { "backed_up" } else { "blocked" },
            "targetArchivePath": target,
            "hashVerified": verified,
            "safeForFutureWrite": verified
        });
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    #[test]
    fn write_readiness_reads_bundle_manifest() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report.components.bundle.present);
        assert!(report.components.bundle.ok);
    }

    #[test]
    fn write_readiness_builds_write_plan_component() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report.components.write_plan.present);
        assert!(report.components.write_plan.ok);
        // The embedded plan never permits writing.
        assert!(!report.write_plan.safe_to_write);
    }

    #[test]
    fn write_readiness_reads_valid_backup_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let backup = write_backup_report(dir.path(), true, FAKE_RPF);
        let report =
            build_write_readiness_report(&bundle, Path::new(FAKE_RPF), Some(&backup)).unwrap();
        assert!(report.components.backup.present);
        assert!(report.components.backup.ok);
        assert!(report
            .gates
            .iter()
            .any(|g| g.name == "backup_hash_verified" && g.passed));
    }

    #[test]
    fn write_readiness_marks_missing_backup_report_blocking_or_warning() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(!report.components.backup.present);
        let g = report
            .gates
            .iter()
            .find(|g| g.name == "backup_hash_verified")
            .unwrap();
        assert!(!g.passed);
        assert!(matches!(
            g.severity,
            RpfReadinessSeverity::Warning | RpfReadinessSeverity::Blocking
        ));
    }

    #[test]
    fn write_readiness_runs_probe_component() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report.components.probe.present);
        assert!(report.components.probe.ok);
        assert!(report.probe.is_some());
    }

    #[test]
    fn write_readiness_includes_adapter_component() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert_eq!(report.adapter_info.adapter_name, "null_rpf_adapter");
        assert!(report.components.adapter.ok);
        assert!(!report.adapter_info.capabilities.can_write_archive);
    }

    #[test]
    fn write_readiness_includes_external_tool_component() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report.components.external_tools.ok);
        assert!(report.external_tool_plan.safe_mode_only);
        assert!(
            !report
                .external_tool_plan
                .can_use_external_tools_automatically
        );
    }

    #[test]
    fn write_readiness_ready_to_write_false_by_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let backup = write_backup_report(dir.path(), true, FAKE_RPF);
        let report =
            build_write_readiness_report(&bundle, Path::new(FAKE_RPF), Some(&backup)).unwrap();
        assert!(!report.ready_to_write);
        assert!(!report.summary.ready_to_write);
        assert_eq!(report.status, RpfWriteReadinessStatus::NotReady);
    }

    #[test]
    fn write_readiness_blocks_when_adapter_cannot_write() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "active_adapter_cannot_write"));
    }

    #[test]
    fn write_readiness_blocks_when_real_writer_not_implemented() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "real_rpf_writer_not_implemented"));
    }

    #[test]
    fn write_readiness_blocks_when_native_parser_not_implemented() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "native_rpf_parser_not_implemented"));
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "external_archive_mutation_not_allowed"));
    }

    #[test]
    fn write_readiness_does_not_modify_target_or_bundle() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());

        let target_before = std::fs::read(FAKE_RPF).unwrap();
        let manifest_path = bundle.join("bundle_manifest.json");
        let manifest_before = std::fs::read(&manifest_path).unwrap();

        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();
        assert!(!report.modifies_target_archive);
        assert!(!report.real_writer_implemented);
        assert!(!report.native_parser_implemented);

        assert_eq!(target_before, std::fs::read(FAKE_RPF).unwrap());
        assert_eq!(manifest_before, std::fs::read(&manifest_path).unwrap());
    }

    #[test]
    fn write_readiness_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = write_valid_bundle(dir.path());
        let report = build_write_readiness_report(&bundle, Path::new(FAKE_RPF), None).unwrap();

        let out_path = dir.path().join("write_readiness_report.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(out_path.is_file());

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["readyToWrite"], false);
        assert_eq!(v["realWriterImplemented"], false);
        assert_eq!(v["nativeParserImplemented"], false);
        assert_eq!(v["modifiesTargetArchive"], false);
        assert_eq!(v["adapterInfo"]["adapterName"], "null_rpf_adapter");
    }
}
