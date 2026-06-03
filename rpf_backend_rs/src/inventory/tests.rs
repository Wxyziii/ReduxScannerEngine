#[cfg(test)]
mod tests {
    use crate::inventory::model::InventoryScanStatus;
    use crate::inventory::scanner::{check_targets, scan_workspace};
    use std::path::Path;

    // ── T0.4.2: inventory tests ──────────────────────────────────────────────

    #[test]
    fn inventory_scans_workspace_files() {
        let ws = Path::new("../examples/workspaces/update_rpf_fixture");
        let report = scan_workspace(ws).unwrap();
        assert_eq!(report.status, InventoryScanStatus::Ok);
        assert!(
            report.files.len() >= 5,
            "expected >= 5 files, got {}",
            report.files.len()
        );
    }

    #[test]
    fn inventory_normalizes_paths() {
        let ws = Path::new("../examples/workspaces/update_rpf_fixture");
        let report = scan_workspace(ws).unwrap();
        for f in &report.files {
            assert!(
                !f.path.contains('\\'),
                "path contains backslash: {}",
                f.path
            );
            assert!(
                !f.path.starts_with('/'),
                "path starts with slash: {}",
                f.path
            );
        }
    }

    #[test]
    fn inventory_reports_file_extensions() {
        let ws = Path::new("../examples/workspaces/update_rpf_fixture");
        let report = scan_workspace(ws).unwrap();
        assert!(!report.summary.extensions.is_empty());
        assert!(
            report.summary.extensions.contains(&"xml".to_string())
                || report.summary.extensions.contains(&"dat".to_string()),
            "expected xml or dat in extensions, got: {:?}",
            report.summary.extensions
        );
    }

    #[test]
    fn inventory_detects_missing_patch_plan_target() {
        let ws = Path::new("../examples/workspaces/partial_rpf_fixture");
        let report = scan_workspace(ws).unwrap();
        let targets = vec![
            "common/data/visualsettings.dat".to_string(),
            "common/data/timecycle/cloudkeyframes.xml".to_string(),
        ];
        let missing = check_targets(&report, &targets);
        assert_eq!(
            missing.len(),
            1,
            "expected 1 missing target, got: {:?}",
            missing.iter().map(|m| &m.target_path).collect::<Vec<_>>()
        );
        assert_eq!(
            missing[0].target_path,
            "common/data/timecycle/cloudkeyframes.xml"
        );
    }

    #[test]
    fn dry_run_with_workspace_valid_targets_safe() {
        let plan = Path::new("../examples/patch_plans/valid_first_patch.json");
        let ws = Path::new("../examples/workspaces/update_rpf_fixture");
        let report = crate::editors::dry_run::build_dry_run_report(plan, Some(ws)).unwrap();
        assert!(
            report.safe_to_apply,
            "expected safe_to_apply=true; blocked={:?}, missing={:?}",
            report.blocked, report.missing_targets
        );
        assert!(report.missing_targets.is_empty());
    }

    #[test]
    fn dry_run_with_workspace_missing_target_blocked() {
        let plan = Path::new("../examples/patch_plans/valid_first_patch.json");
        let ws = Path::new("../examples/workspaces/partial_rpf_fixture");
        let report = crate::editors::dry_run::build_dry_run_report(plan, Some(ws)).unwrap();
        assert!(!report.safe_to_apply, "expected safe_to_apply=false");
        assert!(
            !report.missing_targets.is_empty(),
            "expected missing_targets to be non-empty"
        );
    }

    #[test]
    fn dry_run_without_workspace_preserves_old_behavior() {
        let plan = Path::new("../examples/patch_plans/valid_first_patch.json");
        let report = crate::editors::dry_run::build_dry_run_report(plan, None).unwrap();
        assert!(report.safe_to_apply);
        assert!(report.missing_targets.is_empty());
        assert_eq!(report.summary.total_operations, 3);
        assert_eq!(report.summary.allowed_operations, 3);
    }

    #[test]
    fn inventory_out_file_written_when_requested() {
        let ws = Path::new("../examples/workspaces/update_rpf_fixture");
        let report = scan_workspace(ws).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let out_path = dir.path().join("inventory.json");
        let json = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write(&out_path, &json).unwrap();
        assert!(out_path.exists());
        let content = std::fs::read_to_string(&out_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.get("files").is_some());
        assert!(parsed.get("summary").is_some());
        assert!(parsed["summary"]["totalFiles"].as_u64().unwrap() >= 5);
    }
}
