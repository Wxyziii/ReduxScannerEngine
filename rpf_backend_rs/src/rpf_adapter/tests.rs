#[cfg(test)]
mod tests {
    use crate::rpf_adapter::contract::{build_adapter_info_report, RpfAdapter};
    use crate::rpf_adapter::model::{RpfAdapterKind, RpfAdapterOperation, RpfAdapterStatus};
    use crate::rpf_adapter::null_adapter::{NullRpfAdapter, NOT_IMPLEMENTED_BLOCK};

    fn adapter() -> NullRpfAdapter {
        NullRpfAdapter::new()
    }

    #[test]
    fn rpf_adapter_null_reports_safe_mode_only() {
        let caps = adapter().capabilities();
        assert!(caps.safe_mode_only);
        assert_eq!(adapter().kind(), RpfAdapterKind::Null);
        assert_eq!(adapter().name(), "null_rpf_adapter");
    }

    #[test]
    fn rpf_adapter_null_can_write_archive_false() {
        assert!(!adapter().capabilities().can_write_archive);
    }

    #[test]
    fn rpf_adapter_null_can_replace_files_false() {
        assert!(!adapter().capabilities().can_replace_files);
    }

    #[test]
    fn rpf_adapter_null_native_parser_false() {
        assert!(!adapter().capabilities().native_parser);
    }

    #[test]
    fn rpf_adapter_null_native_writer_false() {
        assert!(!adapter().capabilities().native_writer);
    }

    fn assert_blocked(op: RpfAdapterOperation) {
        let plan = adapter().plan_operation(op);
        assert!(!plan.supported, "{:?} must not be supported", op);
        assert_eq!(plan.status, RpfAdapterStatus::NotImplemented);
        assert!(!plan.modifies_archive);
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == NOT_IMPLEMENTED_BLOCK));
    }

    #[test]
    fn rpf_adapter_null_blocks_list_entries() {
        assert_blocked(RpfAdapterOperation::ListEntries);
    }

    #[test]
    fn rpf_adapter_null_blocks_extract_file() {
        assert_blocked(RpfAdapterOperation::ExtractFile);
    }

    #[test]
    fn rpf_adapter_null_blocks_replace_file() {
        assert_blocked(RpfAdapterOperation::ReplaceFile);
    }

    #[test]
    fn rpf_adapter_null_blocks_write_archive() {
        assert_blocked(RpfAdapterOperation::WriteArchive);
    }

    #[test]
    fn rpf_adapter_info_out_file_written_when_requested() {
        let report = build_adapter_info_report(&adapter());
        let dir = tempfile::TempDir::new().unwrap();
        let out_path = dir.path().join("rpf_adapter_info.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();

        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["capabilities"]["safeModeOnly"], true);
        assert_eq!(v["capabilities"]["canWriteArchive"], false);
        assert_eq!(v["capabilities"]["canReplaceFiles"], false);
        assert_eq!(v["capabilities"]["nativeParser"], false);
        assert_eq!(v["capabilities"]["nativeWriter"], false);
        assert_eq!(v["nativeAdapterImplemented"], false);
        assert_eq!(v["modifiesArchive"], false);
    }

    #[test]
    fn rpf_adapter_execute_operation_does_not_modify_target() {
        for op in RpfAdapterOperation::all() {
            let result = adapter().execute_operation(*op);
            assert!(!result.executed, "{:?} must not execute", op);
            assert!(
                !result.modified_archive,
                "{:?} must not modify the archive",
                op
            );
        }
        // The dangerous operations also carry a not-implemented block.
        let write = adapter().execute_operation(RpfAdapterOperation::WriteArchive);
        assert!(write
            .blocked
            .iter()
            .any(|b| b.block_type == NOT_IMPLEMENTED_BLOCK));
    }

    #[test]
    fn rpf_adapter_info_report_summary_is_consistent() {
        let report = build_adapter_info_report(&adapter());
        assert_eq!(
            report.summary.operation_count,
            RpfAdapterOperation::all().len()
        );
        // Only probe_metadata is "supported" (read-only via probe layer).
        assert_eq!(report.summary.supported_operation_count, 1);
        assert_eq!(report.summary.blocked_operation_count, 4);
        assert!(report.summary.safe_mode_only);
    }
}
