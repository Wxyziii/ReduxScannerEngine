#[cfg(test)]
mod tests {
    use crate::editors::dry_run::{build_dry_run_report, execute_dry_run};
    use std::path::Path;

    // ── Legacy execute_dry_run tests (T0.4.0) ────────────────────────────────

    #[test]
    fn test_valid_dry_run_all() {
        let path = Path::new("../examples/editor_fixtures/valid_editor_dry_run_patch_plan.json");
        let result = execute_dry_run(path, None).unwrap();
        assert!(result.ok);
        assert_eq!(result.results.len(), 3);
        for res in result.results {
            assert!(res.ok);
            assert_eq!(res.mode, "dry_run");
            assert!(res.would_create_backup);
            assert!(!res.validators_planned.is_empty());
        }
    }

    #[test]
    fn test_valid_dry_run_single() {
        let path = Path::new("../examples/editor_fixtures/valid_editor_dry_run_patch_plan.json");
        let result = execute_dry_run(path, Some("op_001")).unwrap();
        assert!(result.ok);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].operation_id, "op_001");
    }

    #[test]
    fn test_invalid_phase() {
        let path = Path::new("../examples/editor_fixtures/invalid_editor_phase_1_2_operation.json");
        let result = execute_dry_run(path, None).unwrap();
        assert!(!result.ok);
        assert!(!result.results[0].ok);
        assert!(result.results[0].errors.iter().any(|e| e.contains("phase")));
    }

    #[test]
    fn test_invalid_binary() {
        let path = Path::new("../examples/editor_fixtures/invalid_editor_binary_operation.json");
        let result = execute_dry_run(path, None).unwrap();
        assert!(!result.ok);
        assert!(result.results[0]
            .errors
            .iter()
            .any(|e| e.contains("Binary")));
    }

    #[test]
    fn test_invalid_weather() {
        let path =
            Path::new("../examples/editor_fixtures/invalid_editor_weather_xml_operation.json");
        let result = execute_dry_run(path, None).unwrap();
        assert!(!result.ok);
        assert!(result.results[0]
            .errors
            .iter()
            .any(|e| e.contains("Blocked/Deferred")));
    }

    #[test]
    fn test_invalid_unknown_tool() {
        let path = Path::new("../examples/editor_fixtures/invalid_editor_unknown_tool.json");
        let result = execute_dry_run(path, None).unwrap();
        assert!(!result.ok);
        assert!(result.results[0]
            .errors
            .iter()
            .any(|e| e.contains("Unknown tool")));
    }

    #[test]
    fn test_invalid_missing_validation() {
        let path = Path::new(
            "../examples/editor_fixtures/invalid_editor_missing_validation_required.json",
        );
        let result = execute_dry_run(path, None).unwrap();
        assert!(!result.ok);
        assert!(result.results[0]
            .errors
            .iter()
            .any(|e| e.contains("validationRequired")));
    }

    #[test]
    fn test_invalid_intent() {
        let path =
            Path::new("../examples/editor_fixtures/invalid_editor_non_hypothesis_intent.json");
        let result = execute_dry_run(path, None).unwrap();
        assert!(!result.ok);
        assert!(result.results[0]
            .errors
            .iter()
            .any(|e| e.contains("hypothesis")));
    }

    // ── DryRunReport tests (T0.4.1) ──────────────────────────────────────────

    #[test]
    fn dry_run_valid_patch_plan_ok() {
        let path = Path::new("../examples/editor_fixtures/valid_editor_dry_run_patch_plan.json");
        let report = build_dry_run_report(path, None).unwrap();
        assert!(report.safe_to_apply);
        assert!(report.blocked.is_empty());
        assert_eq!(report.summary.total_operations, 3);
        assert_eq!(report.summary.allowed_operations, 3);
        assert_eq!(report.summary.blocked_operations, 0);
    }

    #[test]
    fn dry_run_blocked_path_fails() {
        let path =
            Path::new("../examples/editor_fixtures/invalid_editor_weather_xml_operation.json");
        let report = build_dry_run_report(path, None).unwrap();
        assert!(!report.safe_to_apply);
        assert!(!report.blocked.is_empty());
        assert_eq!(report.blocked[0].block_type, "blocked_deferred");
    }

    #[test]
    fn dry_run_binary_file_fails() {
        let path = Path::new("../examples/editor_fixtures/invalid_editor_binary_operation.json");
        let report = build_dry_run_report(path, None).unwrap();
        assert!(!report.safe_to_apply);
        assert!(!report.blocked.is_empty());
        assert_eq!(report.blocked[0].block_type, "binary_file");
    }

    #[test]
    fn dry_run_unrelated_component_fails() {
        let path = Path::new("../examples/editor_fixtures/invalid_editor_unrelated_component.json");
        let report = build_dry_run_report(path, None).unwrap();
        assert!(!report.safe_to_apply);
        assert!(!report.blocked.is_empty());
        assert_eq!(report.blocked[0].block_type, "unrelated_component");
    }

    #[test]
    fn dry_run_report_safe_to_apply_false_when_blocked() {
        let path =
            Path::new("../examples/editor_fixtures/invalid_editor_weather_xml_operation.json");
        let report = build_dry_run_report(path, None).unwrap();
        assert!(!report.safe_to_apply);
        assert_eq!(report.summary.blocked_operations, 1);
    }

    #[test]
    fn dry_run_report_safe_to_apply_true_when_valid() {
        let path = Path::new("../examples/editor_fixtures/valid_editor_dry_run_patch_plan.json");
        let report = build_dry_run_report(path, None).unwrap();
        assert!(report.safe_to_apply);
        assert_eq!(report.targets.len(), 3);
        for target in &report.targets {
            assert!(target.would_change);
            assert!(!target.validators_planned.is_empty());
        }
    }

    #[test]
    fn dry_run_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let out_path = dir.path().join("report.json");
        let plan_path =
            Path::new("../examples/editor_fixtures/valid_editor_dry_run_patch_plan.json");
        let report = build_dry_run_report(plan_path, None).unwrap();
        let json = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write(&out_path, &json).unwrap();
        assert!(out_path.exists());
        let content = std::fs::read_to_string(&out_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.get("safeToApply").is_some());
        assert_eq!(parsed["safeToApply"], true);
    }
}
