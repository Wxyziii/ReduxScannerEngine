#[cfg(test)]
mod tests {
    use crate::rpf_external::build_external_tool_adapter_plan;
    use crate::rpf_external::tools::{
        exists_on_path_for_test, AUTO_EXEC_BLOCK, EXTERNAL_WRITE_BLOCK, KNOWN_TOOLS,
    };

    #[test]
    fn external_tool_plan_is_safe_mode_only() {
        let plan = build_external_tool_adapter_plan().unwrap();
        assert!(plan.safe_mode_only);
        assert!(plan.summary.safe_mode_only);
    }

    #[test]
    fn external_tool_plan_can_write_archive_false() {
        let plan = build_external_tool_adapter_plan().unwrap();
        assert!(!plan.can_write_archive);
        assert!(!plan.can_modify_archive);
        assert!(!plan.summary.can_write_archive);
    }

    #[test]
    fn external_tool_plan_auto_execution_false() {
        let plan = build_external_tool_adapter_plan().unwrap();
        assert!(!plan.can_use_external_tools_automatically);
        assert!(!plan.summary.can_use_external_tools_automatically);
        assert!(plan.manual_user_action_required);
    }

    #[test]
    fn external_tool_plan_detection_is_informational() {
        let plan = build_external_tool_adapter_plan().unwrap();
        for t in &plan.tools {
            assert_eq!(t.detection.method, "path_lookup");
            // Detection never enables running the tool.
            assert!(!t.allowed_now);
        }
        // A clearly bogus tool name must be reported not-found, never an error.
        assert!(!exists_on_path_for_test(
            "definitely_not_a_real_tool_xyz_123"
        ));
    }

    #[test]
    fn external_tool_plan_has_known_tool_entries() {
        let plan = build_external_tool_adapter_plan().unwrap();
        assert_eq!(plan.tools.len(), KNOWN_TOOLS.len());
        let names: Vec<&str> = plan.tools.iter().map(|t| t.tool.as_str()).collect();
        for expected in ["OpenIV", "CodeWalker", "7z", "powershell", "cmd"] {
            assert!(
                names.contains(&expected),
                "missing tool entry: {}",
                expected
            );
        }
    }

    #[test]
    fn external_tool_plan_missing_tools_do_not_fail() {
        // The plan always succeeds and yields one entry per known tool regardless
        // of whether any are actually installed.
        let plan = build_external_tool_adapter_plan().unwrap();
        assert_eq!(plan.summary.tools_checked, KNOWN_TOOLS.len());
        assert!(plan.summary.tools_found <= plan.summary.tools_checked);
    }

    #[test]
    fn external_tool_plan_marks_write_paths_blocked() {
        let plan = build_external_tool_adapter_plan().unwrap();
        // Every tool blocks archive mutation and automatic execution.
        for t in &plan.tools {
            assert!(t
                .blocked
                .iter()
                .any(|b| b.block_type == EXTERNAL_WRITE_BLOCK));
            assert!(t.blocked.iter().any(|b| b.block_type == AUTO_EXEC_BLOCK));
        }
        // And the aggregate list carries them too.
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == EXTERNAL_WRITE_BLOCK));
    }

    #[test]
    fn rpf_adapter_info_still_reports_null_adapter_active() {
        use crate::rpf_adapter::contract::build_adapter_info_report;
        use crate::rpf_adapter::null_adapter::NullRpfAdapter;

        let report = build_adapter_info_report(&NullRpfAdapter::new());
        assert_eq!(report.adapter_name, "null_rpf_adapter");
        assert!(report.capabilities.safe_mode_only);
        assert!(!report.capabilities.can_write_archive);
        assert!(!report.capabilities.can_replace_files);
        assert!(!report.capabilities.native_parser);
        assert!(!report.capabilities.native_writer);
        // The external-tool plan is included but does not change the active adapter.
        assert!(!report.external_tool_plan.can_write_archive);
        assert!(report.external_tool_plan.safe_mode_only);
    }

    #[test]
    fn rpf_external_tools_out_file_written_when_requested() {
        let plan = build_external_tool_adapter_plan().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let out_path = dir.path().join("rpf_external_tools.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&plan).unwrap()).unwrap();

        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["safeModeOnly"], true);
        assert_eq!(v["canWriteArchive"], false);
        assert_eq!(v["canUseExternalToolsAutomatically"], false);
        assert_eq!(v["modifiesFiles"], false);
    }

    #[test]
    fn external_tool_plan_does_not_modify_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let marker = dir.path().join("marker.txt");
        std::fs::write(&marker, b"untouched\n").unwrap();
        let before = std::fs::read(&marker).unwrap();

        let plan = build_external_tool_adapter_plan().unwrap();
        assert!(!plan.modifies_files);

        // No file in the temp dir is touched by planning.
        let after = std::fs::read(&marker).unwrap();
        assert_eq!(before, after);
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1, "planning must not create files");
    }
}
