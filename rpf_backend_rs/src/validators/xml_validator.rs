use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct XmlValidationSummary {
    pub filePath: String,
    pub parseOk: bool,
    pub structurePreserved: bool,
    pub colorLikeOnly: bool,
    pub numericChangesDetected: bool,
    pub nodeDeletionDetected: bool,
}

#[derive(Debug, Serialize)]
pub struct XmlValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub summary: XmlValidationSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlValidationMode {
    ParseOnly,
    ColorLikeOnly,
    StructurePreserved,
    NoNumericChanges,
    DiffAgainstBaseline,
}

pub fn validate_xml(
    file_path: &Path,
    mode: Option<XmlValidationMode>,
    baseline_path: Option<&Path>,
) -> XmlValidationResult {
    let mut result = XmlValidationResult {
        ok: true,
        errors: Vec::new(),
        warnings: Vec::new(),
        notes: Vec::new(),
        summary: XmlValidationSummary {
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

    // 1. XML parse check
    if let Err(e) = check_parse(&content) {
        result.ok = false;
        result.errors.push(format!("XML parse error: {}", e));
        result.summary.parseOk = false;
        return result;
    }
    result.summary.parseOk = true;

    if mode == Some(XmlValidationMode::ParseOnly) {
        return result;
    }

    // Baseline-dependent checks
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

        if let Err(e) = check_structure(&base_content, &content, &mut result) {
            result.ok = false;
            result.errors.push(format!("Structure check failed: {}", e));
        }

        if mode == Some(XmlValidationMode::ColorLikeOnly) {
            if let Err(e) = check_color_like_only(&base_content, &content, &mut result) {
                result.ok = false;
                result
                    .errors
                    .push(format!("Color-like-only check failed: {}", e));
            }
        }
    } else if mode.is_some() && mode != Some(XmlValidationMode::ParseOnly) {
        result
            .warnings
            .push("Baseline required for selected validation mode".to_string());
    }

    result
}

fn check_parse(content: &str) -> Result<()> {
    let mut reader = Reader::from_str(content);
    reader.trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(_) => (),
            Err(e) => return Err(anyhow!(e)),
        }
        buf.clear();
    }
    Ok(())
}

fn check_structure(base: &str, current: &str, result: &mut XmlValidationResult) -> Result<()> {
    let base_tags = get_tag_list(base)?;
    let current_tags = get_tag_list(current)?;

    if base_tags.len() != current_tags.len() {
        result.summary.structurePreserved = false;
        result.warnings.push(format!(
            "Tag count mismatch: base={}, current={}",
            base_tags.len(),
            current_tags.len()
        ));
        if current_tags.len() < base_tags.len() {
            result.summary.nodeDeletionDetected = true;
        }
    } else {
        result.summary.structurePreserved = true;
        for (b, c) in base_tags.iter().zip(current_tags.iter()) {
            if b != c {
                result.summary.structurePreserved = false;
                result
                    .warnings
                    .push(format!("Tag mismatch: expected '{}', found '{}'", b, c));
                break;
            }
        }
    }

    Ok(())
}

fn get_tag_list(content: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(content);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut tags = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                tags.push(String::from_utf8_lossy(e.name().as_ref()).into_owned())
            }
            Ok(Event::Empty(e)) => {
                tags.push(String::from_utf8_lossy(e.name().as_ref()).into_owned())
            }
            Ok(Event::Eof) => break,
            Ok(_) => (),
            Err(e) => return Err(anyhow!(e)),
        }
        buf.clear();
    }
    Ok(tags)
}

fn check_color_like_only(
    base: &str,
    current: &str,
    result: &mut XmlValidationResult,
) -> Result<()> {
    // Basic implementation: check if non-color values changed
    // For T0.3, we'll use a conservative approach:
    // 1. Identify all text nodes and attribute values.
    // 2. Compare them.
    // 3. If they differ, check if they are "color-like".

    let base_values = get_value_map(base)?;
    let current_values = get_value_map(current)?;

    result.summary.colorLikeOnly = true;

    for (path, b_val) in base_values {
        if let Some(c_val) = current_values.get(&path) {
            if b_val != *c_val {
                if !is_color_like(&b_val) || !is_color_like(c_val) {
                    // Check if it's purely numeric change
                    result.ok = false;
                    if is_numeric(&b_val) && is_numeric(c_val) {
                        result.summary.numericChangesDetected = true;
                        result.summary.colorLikeOnly = false;
                        result.errors.push(format!(
                            "Numeric change detected at {}: {} -> {}",
                            path, b_val, c_val
                        ));
                    } else {
                        result.summary.colorLikeOnly = false;
                        result.errors.push(format!(
                            "Non-color change detected at {}: {} -> {}",
                            path, b_val, c_val
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn is_color_like(s: &str) -> bool {
    let s = s.trim();
    // Hex color: #RRGGBB or #RRGGBBAA
    let hex_re = Regex::new(r"^#([A-Fa-f0-9]{6}|[A-Fa-f0-9]{8})$").unwrap();
    if hex_re.is_match(s) {
        return true;
    }

    // RGB/RGBA tuples: 255 255 255 or 1.0, 1.0, 1.0
    let tuple_re = Regex::new(r"^\d+(\.\d+)?([\s,]+\d+(\.\d+)?){2,3}$").unwrap();
    if tuple_re.is_match(s) {
        return true;
    }

    false
}

fn is_numeric(s: &str) -> bool {
    s.trim().parse::<f64>().is_ok()
}

fn get_value_map(content: &str) -> Result<std::collections::HashMap<String, String>> {
    let mut reader = Reader::from_str(content);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut values = std::collections::HashMap::new();
    let mut path = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                path.push(name.clone());
                for attr in e.attributes() {
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let val = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                    values.insert(format!("{}/@{}", path.join("/"), key), val);
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                for attr in e.attributes() {
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let val = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                    values.insert(format!("{}/{}/@{}", path.join("/"), name, key), val);
                }
            }
            Ok(Event::Text(e)) => {
                let val = e.unescape()?.into_owned();
                if !val.trim().is_empty() {
                    values.insert(path.join("/"), val);
                }
            }
            Ok(Event::End(_)) => {
                path.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => (),
            Err(e) => return Err(anyhow!(e)),
        }
        buf.clear();
    }
    Ok(values)
}
