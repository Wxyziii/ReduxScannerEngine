#[cfg(test)]
mod tests {
    use crate::validators::dat_validator::*;
    use crate::validators::scope_validator::*;
    use crate::validators::xml_validator::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_xml_parse() {
        let path = Path::new("../examples/validator_fixtures/valid_cloudkeyframes.xml");
        let result = validate_xml(path, Some(XmlValidationMode::ParseOnly), None);
        assert!(result.ok);
        assert!(result.summary.parseOk);
    }

    #[test]
    fn test_xml_broken() {
        let path = Path::new("../examples/validator_fixtures/invalid_broken.xml");
        let result = validate_xml(path, Some(XmlValidationMode::ParseOnly), None);
        assert!(!result.ok);
        assert!(!result.summary.parseOk);
    }

    #[test]
    fn test_xml_color_only_pass() {
        let base = Path::new("../examples/validator_fixtures/valid_cloudkeyframes.xml");
        let current =
            Path::new("../examples/validator_fixtures/modified_cloudkeyframes_color_only.xml");
        let result = validate_xml(current, Some(XmlValidationMode::ColorLikeOnly), Some(base));
        assert!(result.ok);
        assert!(result.summary.colorLikeOnly);
    }

    #[test]
    fn test_xml_numeric_change_fail() {
        let base = Path::new("../examples/validator_fixtures/valid_cloudkeyframes.xml");
        let current =
            Path::new("../examples/validator_fixtures/modified_cloudkeyframes_numeric_change.xml");
        let result = validate_xml(current, Some(XmlValidationMode::ColorLikeOnly), Some(base));
        assert!(!result.ok);
        assert!(!result.summary.colorLikeOnly);
        assert!(result.summary.numericChangesDetected);
    }

    #[test]
    fn test_dat_parse() {
        let path = Path::new("../examples/validator_fixtures/valid_visualsettings.dat");
        let result = validate_dat(path, Some(DatValidationMode::ParseOnly), None);
        assert!(result.ok);
        assert!(result.summary.parseOk);
        assert_eq!(result.summary.namedKeyCount, 4);
    }

    #[test]
    fn test_dat_allowed_family() {
        let path = Path::new("../examples/validator_fixtures/valid_visualsettings.dat");
        let result = validate_dat(path, Some(DatValidationMode::AllowedFamilyOnly), None);
        assert!(result.ok);
    }

    #[test]
    fn test_dat_blocked_family() {
        let path = Path::new("../examples/validator_fixtures/invalid_visualsettings_broken.dat");
        let result = validate_dat(path, Some(DatValidationMode::AllowedFamilyOnly), None);
        assert!(!result.ok);
        assert!(result.warnings.iter().any(|w| w.contains("adaptivedof")));
        assert!(result.errors.iter().any(|e| e.contains("UnknownFamily")));
    }

    // ── Scope-validator fixtures ────────────────────────────────────────────
    //
    // The scope tests use a small, synthetic PatchPlan written to a temp file
    // inside the test. This keeps them fully self-contained: no absolute local
    // paths, no AI-generated patch plans, no external/AI-tester dependency, no
    // real GTA files. The plan is entirely fake and matches the controlled
    // first-patch scope the validator expects.

    fn write_synthetic_patch_plan() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let unique = format!(
            "rpf_scope_plan_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        dir.push(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("synthetic.patch_plan.json");
        // `filePath` (camelCase) matches the PatchOperation contract. The plan
        // covers every file the scope tests reference so each maps to its own
        // error branch rather than "unexpected file".
        let plan = r#"{
  "schemaVersion": "1.0",
  "operations": [
    { "id": "op1", "filePath": "common/data/visualsettings.dat", "operationType": "dat_edit" },
    { "id": "op2", "filePath": "common/data/timecycle/cloudkeyframes.xml", "operationType": "xml_color" },
    { "id": "op3", "filePath": "common/data/timecycle/timecycle_mods_1.xml", "operationType": "xml_color" }
  ],
  "targetFiles": [
    "visualsettings.dat",
    "cloudkeyframes.xml",
    "timecycle_mods_1.xml",
    "weather.xml",
    "w_foggy.xml",
    "timecycle_mods_3.xml",
    "some_texture.ytd",
    "update.rpf",
    "tracer_effect.xml"
  ]
}"#;
        std::fs::write(&path, plan).unwrap();
        path
    }

    #[test]
    fn test_scope_valid() {
        let plan = write_synthetic_patch_plan();
        let changed = vec![
            "visualsettings.dat".to_string(),
            "cloudkeyframes.xml".to_string(),
            "timecycle_mods_1.xml".to_string(),
        ];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(result.ok, "Expected PASS; errors: {:?}", result.errors);
        assert_eq!(result.summary.unexpectedFileCount, 0);
        assert_eq!(result.summary.blockedFileChangedCount, 0);
    }

    #[test]
    fn test_scope_invalid_weather() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["weather.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok);
        assert!(result.summary.blockedFileChangedCount > 0);
    }

    #[test]
    fn test_scope_unexpected_file() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["visualsettings.dat".to_string(), "unknown.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok);
        assert!(result.summary.unexpectedFileCount > 0);
    }

    // T0.3 regression tests: the seven manual-test scenarios, run against the
    // synthetic in-repo plan above. No machine-specific path is required.

    #[test]
    fn regression_t0_3_valid_first_patch() {
        let plan = write_synthetic_patch_plan();
        let changed = vec![
            "visualsettings.dat".to_string(),
            "cloudkeyframes.xml".to_string(),
            "timecycle_mods_1.xml".to_string(),
        ];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(result.ok, "Expected PASS; errors: {:?}", result.errors);
        assert_eq!(result.summary.unexpectedFileCount, 0);
        assert_eq!(result.summary.blockedFileChangedCount, 0);
    }

    #[test]
    fn regression_t0_3_phase_1_2_fail() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["visualsettings.dat".to_string(), "w_foggy.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok, "Expected FAIL for w_foggy.xml; got ok");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Blocked/Deferred") || e.contains("w_foggy.xml")),
            "Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn regression_t0_3_deferred_global_fail() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["weather.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok, "Expected FAIL for weather.xml; got ok");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Blocked/Deferred") || e.contains("weather.xml")),
            "Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn regression_t0_3_blocked_file_fail() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["timecycle_mods_3.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok, "Expected FAIL for timecycle_mods_3.xml; got ok");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Blocked/Deferred") || e.contains("timecycle_mods_3.xml")),
            "Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn regression_t0_3_binary_file_fail() {
        let plan = write_synthetic_patch_plan();
        let changed = vec![
            "cloudkeyframes.xml".to_string(),
            "some_texture.ytd".to_string(),
        ];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok, "Expected FAIL for some_texture.ytd; got ok");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Binary file changed") || e.contains("some_texture.ytd")),
            "Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn regression_t0_3_rpf_archive_fail() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["update.rpf".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok, "Expected FAIL for update.rpf; got ok");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("RPF archive changed") || e.contains("update.rpf")),
            "Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn regression_t0_3_unrelated_component_fail() {
        let plan = write_synthetic_patch_plan();
        let changed = vec!["tracer_effect.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok, "Expected FAIL for tracer_effect.xml; got ok");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Unrelated component") || e.contains("tracer_effect.xml")),
            "Errors: {:?}",
            result.errors
        );
    }
}
