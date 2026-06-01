use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct ScopeValidationSummary {
    pub plannedFileCount: usize,
    pub changedFileCount: usize,
    pub unexpectedFileCount: usize,
    pub blockedFileChangedCount: usize,
}

#[derive(Debug, Serialize)]
pub struct ScopeValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub summary: ScopeValidationSummary,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchPlan {
    #[serde(rename = "schemaVersion")]
    schema_version: Option<String>,
    #[serde(default)]
    operations: Vec<PatchOperation>,
    #[serde(default)]
    target_files: Vec<TargetFileSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TargetFileSpec {
    Path(String),
    Object(TargetFile),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetFile {
    path: String,
    phase: Option<String>,
    role: Option<String>,
    confidence: Option<String>,
    risk: Option<String>,
    evidence: Option<String>,
    allowed_operations: Option<Vec<String>>,
    blocked_operations: Option<Vec<String>>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchOperation {
    id: Option<String>,
    phase: Option<String>,
    file_path: String,
    tool: Option<String>,
    operation_type: Option<String>,
}

pub fn validate_scope(patch_plan_path: &Path, changed_files: &[String]) -> ScopeValidationResult {
    let mut result = ScopeValidationResult {
        ok: true,
        errors: Vec::new(),
        warnings: Vec::new(),
        notes: Vec::new(),
        summary: ScopeValidationSummary::default(),
    };

    let plan_content = match fs::read_to_string(patch_plan_path) {
        Ok(c) => c,
        Err(e) => {
            result.ok = false;
            result
                .errors
                .push(format!("Failed to read patch plan: {}", e));
            return result;
        }
    };

    let plan: PatchPlan = match serde_json::from_str(&plan_content) {
        Ok(p) => p,
        Err(e) => {
            result.ok = false;
            result
                .errors
                .push(format!("Failed to parse patch plan JSON: {}", e));
            return result;
        }
    };

    let allowed_first_patch = vec![
        "visualsettings.dat",
        "cloudkeyframes.xml",
        "timecycle_mods_1.xml",
    ];

    let blocked_or_deferred = vec![
        "weather.xml",
        "timecycle_mods_3.xml",
        "timecycle_mods_4.xml",
        "w_foggy.xml",
        "w_clouds.xml",
    ];

    let mut planned_files = std::collections::HashSet::new();
    for op in &plan.operations {
        planned_files.insert(get_file_name(&op.file_path));
    }
    // Also include any target_files entries (which may be strings or detailed objects)
    for tf in &plan.target_files {
        match tf {
            TargetFileSpec::Path(p) => {
                planned_files.insert(get_file_name(p));
            }
            TargetFileSpec::Object(o) => {
                planned_files.insert(get_file_name(&o.path));
            }
        }
    }
    result.summary.plannedFileCount = planned_files.len();
    result.summary.changedFileCount = changed_files.len();

    for file in changed_files {
        let file_name = get_file_name(file);

        // 1. Check if it was planned
        if !planned_files.contains(&file_name) {
            result.summary.unexpectedFileCount += 1;
            result
                .errors
                .push(format!("File changed but not in plan operations: {}", file));
        }

        // 2. Check if it's allowed in first_controlled_patch
        let mut is_allowed = false;
        for allowed in &allowed_first_patch {
            if file_name == *allowed {
                is_allowed = true;
                break;
            }
        }

        if !is_allowed {
            result.ok = false;
            result.summary.blockedFileChangedCount += 1;

            let mut is_blocked = false;
            for blocked in &blocked_or_deferred {
                if file_name == *blocked {
                    is_blocked = true;
                    break;
                }
            }

            if is_blocked {
                result
                    .errors
                    .push(format!("Blocked/Deferred file changed: {}", file));
            } else if is_binary(file) {
                result.errors.push(format!("Binary file changed: {}", file));
            } else if is_rpf(file) {
                result.errors.push(format!("RPF archive changed: {}", file));
            } else if is_unrelated_component(file) {
                result
                    .errors
                    .push(format!("Unrelated component file changed: {}", file));
            } else {
                result
                    .errors
                    .push(format!("File not in allowed first_patch list: {}", file));
            }
        }
    }

    if result.summary.unexpectedFileCount > 0 || result.summary.blockedFileChangedCount > 0 {
        result.ok = false;
    }

    result
}

fn get_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn is_binary(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "ytd" | "ypt" | "ysc" | "gfx" | "fxc" | "dll" | "exe"
    )
}

fn is_rpf(path: &str) -> bool {
    path.to_lowercase().ends_with(".rpf")
}

fn is_unrelated_component(path: &str) -> bool {
    let p = path.to_lowercase();
    p.contains("tracer")
        || p.contains("hit_effect")
        || p.contains("kill_effect")
        || p.contains("minimap_hud")
}
