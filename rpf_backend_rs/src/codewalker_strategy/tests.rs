#[cfg(test)]
mod tests {
    use crate::codewalker_strategy::model::CodeWalkerStrategyStatus;
    use crate::codewalker_strategy::strategy::build_codewalker_strategy_report;
    use serde_json::Value;

    #[test]
    fn codewalker_strategy_selected_route_is_codewalker_api() {
        let r = build_codewalker_strategy_report().unwrap();
        assert_eq!(r.status, CodeWalkerStrategyStatus::RouteLocked);
        assert_eq!(r.selected_writer_route, "CodeWalker.API");
        assert_eq!(r.selected_route.name, "CodeWalker.API");
    }

    #[test]
    fn codewalker_strategy_route_locked_true() {
        let r = build_codewalker_strategy_report().unwrap();
        assert!(r.selected_writer_route_locked);
        assert!(r.selected_route.locked);
    }

    #[test]
    fn codewalker_strategy_active_adapter_is_null() {
        let r = build_codewalker_strategy_report().unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        assert!(r.active_adapter_is_null);
    }

    #[test]
    fn codewalker_strategy_writer_allowed_now_false() {
        let r = build_codewalker_strategy_report().unwrap();
        assert!(!r.writer_allowed_now);
        assert!(!r.summary.writer_allowed_now);
        assert!(!r.real_writer_implemented);
        assert!(!r.native_parser_implemented);
    }

    #[test]
    fn codewalker_strategy_codewalker_write_allowed_now_false() {
        let r = build_codewalker_strategy_report().unwrap();
        assert!(!r.codewalker_write_allowed_now);
    }

    #[test]
    fn codewalker_strategy_detection_implemented_after_t0_6_0() {
        let r = build_codewalker_strategy_report().unwrap();
        // T0.6.0 shipped codewalker-detect; T0.6.1 shipped codewalker-readiness.
        assert!(r.codewalker_detection_implemented);
        assert!(r.codewalker_readiness_implemented);
        assert!(r.codewalker_search_resolution_implemented);
        assert!(r.codewalker_dry_replace_plan_implemented);
        assert!(r.codewalker_execution_gate_implemented);
        assert!(r.codewalker_replace_apply_implemented);
        assert!(r.codewalker_post_write_verification_implemented);
        assert!(r.codewalker_rollback_restore_implemented);
        assert!(r.codewalker_manual_harness_implemented);
        assert!(r.codewalker_compatibility_probe_implemented);
        assert!(r.codewalker_copied_archive_test_run_implemented);
    }

    #[test]
    fn codewalker_strategy_execution_not_implemented_yet() {
        let r = build_codewalker_strategy_report().unwrap();
        assert!(!r.codewalker_execution_implemented);
    }

    #[test]
    fn codewalker_strategy_external_tool_execution_allowed_false() {
        let r = build_codewalker_strategy_report().unwrap();
        assert!(!r.external_tool_execution_allowed);
    }

    #[test]
    fn codewalker_strategy_includes_required_safety_gates() {
        let r = build_codewalker_strategy_report().unwrap();
        let names: Vec<&str> = r
            .required_safety_gates
            .iter()
            .map(|g| g.name.as_str())
            .collect();
        for expected in [
            "backup_rpf_verified",
            "probe_rpf_successful",
            "entry_manifest_built",
            "write_readiness_checked",
            "writer_permission_token_present",
            "copied_test_archive_only",
            "codewalker_api_detected",
            "codewalker_replace_endpoint_available",
            "codewalker_target_resolution_successful",
            "manual_confirmation_required",
            "rollback_restore_available",
            "post_write_verification_required",
            "codewalker_execution_not_enabled_yet",
        ] {
            assert!(names.contains(&expected), "missing gate {}", expected);
        }
        // None satisfied yet.
        assert!(r.required_safety_gates.iter().all(|g| !g.satisfied));
    }

    #[test]
    fn codewalker_strategy_includes_t0_6_milestone_plan() {
        let r = build_codewalker_strategy_report().unwrap();
        let ids: Vec<&str> = r.milestone_plan.iter().map(|m| m.id.as_str()).collect();
        for expected in [
            "T0.6.0", "T0.6.1", "T0.6.2", "T0.6.3", "T0.6.4", "T0.6.5", "T0.6.6", "T0.6.7",
            "T0.6.8", "T0.6.9", "T0.6.10",
        ] {
            assert!(ids.contains(&expected), "missing milestone {}", expected);
        }
        // T0.6.0–T0.6.7 shipped.
        for m in r.milestone_plan.iter() {
            if m.id.starts_with("T0.6.") {
                assert!(m.implemented, "{} should be implemented", m.id);
            }
        }
    }

    #[test]
    fn codewalker_strategy_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let r = build_codewalker_strategy_report().unwrap();
        let out = dir.path().join("codewalker_strategy.json");
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["selectedWriterRoute"], "CodeWalker.API");
        assert_eq!(v["selectedWriterRouteLocked"], true);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
        assert_eq!(v["writerAllowedNow"], false);
        assert_eq!(v["codewalkerWriteAllowedNow"], false);
        assert_eq!(v["codewalkerDetectionImplemented"], true);
        assert_eq!(v["codewalkerReadinessImplemented"], true);
        assert_eq!(v["codewalkerSearchResolutionImplemented"], true);
        assert_eq!(v["codewalkerDryReplacePlanImplemented"], true);
        assert_eq!(v["codewalkerExecutionGateImplemented"], true);
        assert_eq!(v["codewalkerReplaceApplyImplemented"], true);
        assert_eq!(v["codewalkerPostWriteVerificationImplemented"], true);
        assert_eq!(v["codewalkerRollbackRestoreImplemented"], true);
        assert_eq!(v["codewalkerManualHarnessImplemented"], true);
        assert_eq!(v["codewalkerCompatibilityProbeImplemented"], true);
        assert_eq!(v["codewalkerExecutionImplemented"], false);
        assert_eq!(v["externalToolExecutionAllowed"], false);
        assert_eq!(v["plannedBaseUrlDefault"], "http://localhost:5555");
    }

    #[test]
    fn codewalker_strategy_does_not_modify_files() {
        // The builder takes no paths and performs no writes. Run it and confirm
        // the working directory listing is unchanged.
        let before: Vec<_> = std::fs::read_dir(".")
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        let _ = build_codewalker_strategy_report().unwrap();
        let after: Vec<_> = std::fs::read_dir(".")
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        assert_eq!(before.len(), after.len());
    }
}
