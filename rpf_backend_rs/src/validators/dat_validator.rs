use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct DatValidationSummary {
    pub filePath: String,
    pub parseOk: bool,
    pub lineCount: usize,
    pub namedKeyCount: usize,
    pub changedKeyCount: usize,
    pub unknownLineCount: usize,
}

#[derive(Debug, Serialize)]
pub struct DatValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub summary: DatValidationSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatValidationMode {
    ParseOnly,
    NamedKeyOnly,
    AllowedFamilyOnly,
    DiffAgainstBaseline,
}

pub fn validate_dat(
    file_path: &Path,
    mode: Option<DatValidationMode>,
    baseline_path: Option<&Path>,
) -> DatValidationResult {
    let mut result = DatValidationResult {
        ok: true,
        errors: Vec::new(),
        warnings: Vec::new(),
        notes: Vec::new(),
        summary: DatValidationSummary {
            filePath: file_path.to_string_lossy().into_owned(),
            ..Default::default()
        },
    };

    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            result.ok = false;
            result.errors.push(format!("Failed to read file: {}", e));
            return result;
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    result.summary.lineCount = lines.len();

    let mut named_keys = std::collections::HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        if let Some((key, val)) = parse_named_key(trimmed) {
            result.summary.namedKeyCount += 1;
            named_keys.insert(key.to_string(), val.to_string());

            // Check families
            if key.to_lowercase().contains("adaptivedof") {
                result
                    .warnings
                    .push(format!("Line {}: 'adaptivedof' key found: {}", i + 1, key));
            } else if !is_allowed_family(key) {
                if mode == Some(DatValidationMode::AllowedFamilyOnly) {
                    result.ok = false;
                    result
                        .errors
                        .push(format!("Line {}: Family not allowed: {}", i + 1, key));
                } else {
                    result
                        .warnings
                        .push(format!("Line {}: Unknown family: {}", i + 1, key));
                }
            }
        } else {
            result.summary.unknownLineCount += 1;
            if mode == Some(DatValidationMode::NamedKeyOnly) {
                result.warnings.push(format!(
                    "Line {}: Not a recognized named key: {}",
                    i + 1,
                    trimmed
                ));
            }
        }
    }
    result.summary.parseOk = true;

    if mode == Some(DatValidationMode::ParseOnly) {
        return result;
    }

    if let Some(base_path) = baseline_path {
        let base_content = match fs::read_to_string(base_path) {
            Ok(c) => c,
            Err(e) => {
                result
                    .warnings
                    .push(format!("Failed to read baseline file: {}", e));
                return result;
            }
        };

        let base_keys = get_named_keys(&base_content);
        for (key, val) in &named_keys {
            if let Some(base_val) = base_keys.get(key) {
                if base_val != val {
                    result.summary.changedKeyCount += 1;
                    result
                        .notes
                        .push(format!("Changed key: {} ({} -> {})", key, base_val, val));
                }
            } else {
                result.summary.changedKeyCount += 1;
                result.notes.push(format!("Added key: {}", key));
            }
        }

        // Check for removed keys
        for key in base_keys.keys() {
            if !named_keys.contains_key(key) {
                result.summary.changedKeyCount += 1;
                result.notes.push(format!("Removed key: {}", key));
            }
        }

        if mode == Some(DatValidationMode::DiffAgainstBaseline)
            && result.summary.changedKeyCount > 100
        {
            result
                .warnings
                .push("Large number of key changes detected (>100)".to_string());
        }
    }

    result
}

fn parse_named_key(line: &str) -> Option<(&str, &str)> {
    // visualsettings.dat typically uses "key value" or "key\tvalue"
    // Sometimes keys are like "Adaptation.MaxLum"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        // Simple case: key value
        Some((parts[0], parts[1]))
    } else if line.contains('=') {
        // Maybe key=value?
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() == 2 {
            Some((parts[0].trim(), parts[1].trim()))
        } else {
            None
        }
    } else {
        None
    }
}

fn is_allowed_family(key: &str) -> bool {
    let k = key.to_lowercase();
    k.contains("adaptation") || k.contains("tonemapping")
}

fn get_named_keys(content: &str) -> std::collections::HashMap<String, String> {
    let mut keys = std::collections::HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if let Some((key, val)) = parse_named_key(trimmed) {
            keys.insert(key.to_string(), val.to_string());
        }
    }
    keys
}
