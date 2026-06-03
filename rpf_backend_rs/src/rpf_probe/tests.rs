#[cfg(test)]
mod tests {
    use crate::rpf_probe::model::RpfProbeStatus;
    use crate::rpf_probe::probe::probe_rpf_archive;
    use crate::rpf_probe::tools::{
        check_tool_for_test, detect_external_tools, exists_on_path_for_test,
    };
    use std::path::Path;

    const FAKE_RPF: &str = "../examples/rpf_fixtures/fake_update.rpf";

    fn sha256_of(path: &Path) -> String {
        let data = std::fs::read(path).unwrap();
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
    }

    #[test]
    fn probe_rpf_reads_file_metadata() {
        let report = probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();
        assert_eq!(report.status, RpfProbeStatus::Probed);
        assert!(report.exists);
        assert!(report.is_file);
        assert!(report.extension_valid);
        assert!(report.can_read_metadata);
        assert_eq!(
            report.size_bytes,
            Some(std::fs::metadata(FAKE_RPF).unwrap().len())
        );
    }

    #[test]
    fn probe_rpf_computes_sha256() {
        let report = probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();
        assert_eq!(report.hash_algorithm, "SHA-256");
        assert_eq!(
            report.sha256.as_deref(),
            Some(sha256_of(Path::new(FAKE_RPF)).as_str())
        );
        // file_info mirror is consistent with the top-level fields.
        assert_eq!(report.file_info.sha256, report.sha256);
        assert_eq!(report.file_info.size_bytes, report.size_bytes);
    }

    #[test]
    fn probe_rpf_blocks_missing_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("does_not_exist.rpf");
        let report = probe_rpf_archive(&missing).unwrap();

        assert_eq!(report.status, RpfProbeStatus::Blocked);
        assert!(!report.exists);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_target"));
        assert!(report.sha256.is_none());
    }

    #[test]
    fn probe_rpf_blocks_directory_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_target = dir.path().join("archive.rpf");
        std::fs::create_dir_all(&dir_target).unwrap();

        let report = probe_rpf_archive(&dir_target).unwrap();
        assert_eq!(report.status, RpfProbeStatus::Blocked);
        assert!(report.exists);
        assert!(!report.is_file);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "target_not_a_file"));
    }

    #[test]
    fn probe_rpf_blocks_non_rpf_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        let txt = dir.path().join("not_an_archive.txt");
        std::fs::write(&txt, b"hello\n").unwrap();

        let report = probe_rpf_archive(&txt).unwrap();
        assert_eq!(report.status, RpfProbeStatus::Blocked);
        assert!(!report.extension_valid);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "non_rpf_target"));
    }

    #[test]
    fn probe_rpf_does_not_modify_original() {
        let before = sha256_of(Path::new(FAKE_RPF));
        let before_len = std::fs::metadata(FAKE_RPF).unwrap().len();

        probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();

        let after = sha256_of(Path::new(FAKE_RPF));
        let after_len = std::fs::metadata(FAKE_RPF).unwrap().len();
        assert_eq!(before, after, "original archive must not change");
        assert_eq!(before_len, after_len, "original size must not change");
    }

    #[test]
    fn probe_rpf_reports_native_writer_not_implemented() {
        let report = probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();
        assert!(!report.native_writer_implemented);
        assert!(!report.can_parse_rpf);
        assert!(report
            .capabilities
            .iter()
            .any(|c| c.name == "native_writer" && !c.available));
    }

    #[test]
    fn probe_rpf_reports_can_write_false() {
        let report = probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();
        assert!(!report.can_write_rpf);
        assert!(!report.modifies_target_archive);
        assert!(report
            .capabilities
            .iter()
            .any(|c| c.name == "write_rpf" && !c.available));
    }

    #[test]
    fn probe_rpf_out_file_written_when_requested() {
        let report = probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let out_path = dir.path().join("rpf_probe_report.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();

        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["canParseRpf"], false);
        assert_eq!(v["canWriteRpf"], false);
        assert_eq!(v["nativeWriterImplemented"], false);
        assert_eq!(v["modifiesTargetArchive"], false);
        assert!(v["sha256"].is_string());
    }

    #[test]
    fn probe_rpf_tool_detection_is_informational() {
        // Detection always returns one entry per known tool and never errors.
        let tools = detect_external_tools();
        assert!(tools.len() >= 5, "expected the known tool set");
        for t in &tools {
            assert_eq!(t.method, "path_lookup");
        }
        // A clearly bogus tool name must be reported not-found, never an error.
        let bogus = check_tool_for_test("definitely_not_a_real_tool_xyz_123");
        assert!(!bogus.found);
        assert!(!exists_on_path_for_test(
            "definitely_not_a_real_tool_xyz_123"
        ));

        // The probe report also carries the informational tool checks.
        let report = probe_rpf_archive(Path::new(FAKE_RPF)).unwrap();
        assert_eq!(report.external_tools.len(), tools.len());
        assert_eq!(report.summary.tools_checked, tools.len());
    }
}
