#[cfg(test)]
mod tests {
    use crate::validators::xml_validator::*;
    use crate::validators::dat_validator::*;
    use crate::validators::scope_validator::*;
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
        let current = Path::new("../examples/validator_fixtures/modified_cloudkeyframes_color_only.xml");
        let result = validate_xml(current, Some(XmlValidationMode::ColorLikeOnly), Some(base));
        assert!(result.ok);
        assert!(result.summary.colorLikeOnly);
    }

    #[test]
    fn test_xml_numeric_change_fail() {
        let base = Path::new("../examples/validator_fixtures/valid_cloudkeyframes.xml");
        let current = Path::new("../examples/validator_fixtures/modified_cloudkeyframes_numeric_change.xml");
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

    #[test]
    fn test_scope_valid() {
        let plan = {
            let abs = PathBuf::from(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
            if abs.exists() {
                abs
            } else {
                Path::new("../examples/validator_fixtures/valid_dark_grey_cloudy_sky_qwen.patch_plan.json").to_path_buf()
            }
        };
        let changed = vec![
            "visualsettings.dat".to_string(),
            "cloudkeyframes.xml".to_string(),
            "timecycle_mods_1.xml".to_string(),
        ];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(result.ok);
        assert_eq!(result.summary.unexpectedFileCount, 0);
        assert_eq!(result.summary.blockedFileChangedCount, 0);
    }

    #[test]
    fn test_scope_invalid_weather() {
        let plan = {
            let abs = PathBuf::from(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
            if abs.exists() {
                abs
            } else {
                Path::new("../examples/validator_fixtures/valid_dark_grey_cloudy_sky_qwen.patch_plan.json").to_path_buf()
            }
        };
        let changed = vec!["weather.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok);
        assert!(result.summary.blockedFileChangedCount > 0);
    }

    #[test]
    fn test_scope_unexpected_file() {
        let plan = {
            let abs = PathBuf::from(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
            if abs.exists() {
                abs
            } else {
                Path::new("../examples/validator_fixtures/valid_dark_grey_cloudy_sky_qwen.patch_plan.json").to_path_buf()
            }
        };
        let changed = vec!["visualsettings.dat".to_string(), "unknown.xml".to_string()];
        let result = validate_scope(plan.as_path(), &changed);
        assert!(!result.ok);
        assert!(result.summary.unexpectedFileCount > 0);
    }

    // T0.3 regression tests using the user's patch plan path (absolute). These verify the
    // seven scenarios from the reported manual tests. These are intended for local/dev
    // verification and require the patch plan to be present at the given absolute path.

    #[test]
    fn regression_t0_3_valid_first_patch_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec![
            "visualsettings.dat".to_string(),
            "cloudkeyframes.xml".to_string(),
            "timecycle_mods_1.xml".to_string(),
        ];
        let result = validate_scope(plan, &changed);
        assert!(result.ok, "Expected PASS; errors: {:?}", result.errors);
        assert_eq!(result.summary.unexpectedFileCount, 0);
        assert_eq!(result.summary.blockedFileChangedCount, 0);
    }

    #[test]
    fn regression_t0_3_phase_1_2_fail_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec!["visualsettings.dat".to_string(), "w_foggy.xml".to_string()];
        let result = validate_scope(plan, &changed);
        assert!(!result.ok, "Expected FAIL for w_foggy.xml; got ok");
        assert!(result.errors.iter().any(|e| e.contains("Blocked/Deferred") || e.contains("w_foggy.xml")), "Errors: {:?}", result.errors);
    }

    #[test]
    fn regression_t0_3_deferred_global_fail_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec!["weather.xml".to_string()];
        let result = validate_scope(plan, &changed);
        assert!(!result.ok, "Expected FAIL for weather.xml; got ok");
        assert!(result.errors.iter().any(|e| e.contains("Blocked/Deferred") || e.contains("weather.xml")), "Errors: {:?}", result.errors);
    }

    #[test]
    fn regression_t0_3_blocked_file_fail_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec!["timecycle_mods_3.xml".to_string()];
        let result = validate_scope(plan, &changed);
        assert!(!result.ok, "Expected FAIL for timecycle_mods_3.xml; got ok");
        assert!(result.errors.iter().any(|e| e.contains("Blocked/Deferred") || e.contains("timecycle_mods_3.xml")), "Errors: {:?}", result.errors);
    }

    #[test]
    fn regression_t0_3_binary_file_fail_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec!["cloudkeyframes.xml".to_string(), "some_texture.ytd".to_string()];
        let result = validate_scope(plan, &changed);
        assert!(!result.ok, "Expected FAIL for some_texture.ytd; got ok");
        assert!(result.errors.iter().any(|e| e.contains("Binary file changed") || e.contains("some_texture.ytd")), "Errors: {:?}", result.errors);
    }

    #[test]
    fn regression_t0_3_rpf_archive_fail_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec!["update.rpf".to_string()];
        let result = validate_scope(plan, &changed);
        assert!(!result.ok, "Expected FAIL for update.rpf; got ok");
        assert!(result.errors.iter().any(|e| e.contains("RPF archive changed") || e.contains("update.rpf")), "Errors: {:?}", result.errors);
    }

    #[test]
    fn regression_t0_3_unrelated_component_fail_absolute_plan() {
        let plan = Path::new(r"C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json");
        assert!(plan.exists(), "Patch plan isn't present at {:?}", plan);
        let changed = vec!["tracer_effect.xml".to_string()];
        let result = validate_scope(plan, &changed);
        assert!(!result.ok, "Expected FAIL for tracer_effect.xml; got ok");
        assert!(result.errors.iter().any(|e| e.contains("Unrelated component") || e.contains("tracer_effect.xml")), "Errors: {:?}", result.errors);
    }
}
