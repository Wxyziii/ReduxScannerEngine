//! Informational external-tool detection + theoretical capability descriptors.
//!
//! This module only *looks up* whether well-known tools appear on PATH. It NEVER
//! executes any tool, and NEVER drives a tool against a real archive. A
//! "not found" result is informational, never an error.

use super::model::*;

/// Block type used for any operation that could mutate an archive.
pub const EXTERNAL_WRITE_BLOCK: &str = "external_archive_mutation_not_allowed";
/// Block type used for automatic (non-interactive) tool execution.
pub const AUTO_EXEC_BLOCK: &str = "automatic_external_tool_execution_not_allowed";

/// The known tools we model, in a stable order.
pub const KNOWN_TOOLS: &[ExternalToolKind] = &[
    ExternalToolKind::OpenIv,
    ExternalToolKind::CodeWalker,
    ExternalToolKind::SevenZip,
    ExternalToolKind::PowerShell,
    ExternalToolKind::Cmd,
];

/// On Windows, executable lookups should consider these suffixes.
#[cfg(windows)]
const EXE_SUFFIXES: &[&str] = &["", ".exe", ".bat", ".cmd", ".com"];
#[cfg(not(windows))]
const EXE_SUFFIXES: &[&str] = &[""];

impl ExternalToolKind {
    /// The PATH lookup name for this tool.
    pub fn lookup_name(&self) -> &'static str {
        match self {
            ExternalToolKind::OpenIv => "OpenIV",
            ExternalToolKind::CodeWalker => "CodeWalker",
            ExternalToolKind::SevenZip => "7z",
            ExternalToolKind::PowerShell => "powershell",
            ExternalToolKind::Cmd => "cmd",
        }
    }

    fn trust_level(&self) -> ExternalToolTrustLevel {
        // No tool is trusted for archive operations in this milestone.
        ExternalToolTrustLevel::Untrusted
    }

    fn risk_level(&self) -> ExternalToolRiskLevel {
        match self {
            // GUI archive editors that could rewrite RPF internals.
            ExternalToolKind::OpenIv | ExternalToolKind::CodeWalker => ExternalToolRiskLevel::High,
            // Generic shells can run arbitrary commands.
            ExternalToolKind::PowerShell | ExternalToolKind::Cmd => ExternalToolRiskLevel::High,
            // Generic archiver; cannot meaningfully write RPF7.
            ExternalToolKind::SevenZip => ExternalToolRiskLevel::Medium,
        }
    }

    /// Theoretical capabilities a tool *might* offer in a future trusted
    /// integration. `allowed_now` is always false for anything that mutates an
    /// archive or runs automatically.
    fn theoretical_capabilities(&self) -> Vec<ExternalToolCapability> {
        let mutate = |name: &str, detail: &str| ExternalToolCapability {
            name: name.to_string(),
            theoretical: true,
            allowed_now: false,
            detail: detail.to_string(),
        };
        match self {
            ExternalToolKind::OpenIv | ExternalToolKind::CodeWalker => vec![
                mutate(
                    "read_internals",
                    "Could theoretically read RPF internals; not used now (no native parser).",
                ),
                mutate(
                    "extract_files",
                    "Could theoretically extract entries; not allowed now.",
                ),
                mutate(
                    "replace_files",
                    "Could theoretically replace entries; mutation is not allowed now.",
                ),
                mutate(
                    "write_archive",
                    "Could theoretically rewrite the archive; writing is not allowed now.",
                ),
            ],
            ExternalToolKind::SevenZip => vec![mutate(
                "generic_archive_ops",
                "Generic archiver; cannot reliably handle RPF7. No archive mutation allowed now.",
            )],
            ExternalToolKind::PowerShell | ExternalToolKind::Cmd => vec![mutate(
                "script_host",
                "Generic command host; could run arbitrary commands. Never auto-executed.",
            )],
        }
    }
}

/// Pure filesystem PATH check — nothing is executed.
fn exists_on_path(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for suffix in EXE_SUFFIXES {
            let candidate = if suffix.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{}{}", name, suffix))
            };
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

fn detect(kind: ExternalToolKind) -> ExternalToolDetection {
    let name = kind.lookup_name();
    let found = exists_on_path(name);
    let detail = if found {
        format!("'{}' found on PATH (not executed).", name)
    } else {
        format!("'{}' not found on PATH (informational only).", name)
    };
    ExternalToolDetection {
        tool: name.to_string(),
        kind,
        found,
        method: "path_lookup".to_string(),
        detail,
    }
}

/// Build the plan entry for a single tool: detection + theoretical capabilities,
/// with all mutation/auto-exec paths blocked.
fn plan_entry(kind: ExternalToolKind) -> ExternalToolPlanEntry {
    let name = kind.lookup_name();
    let detection = detect(kind);
    let capabilities = kind.theoretical_capabilities();

    let mut blocked = vec![
        ExternalToolBlockedItem {
            tool: name.to_string(),
            operation: "write_archive".to_string(),
            reason: "External archive mutation is not allowed; no write path exists.".to_string(),
            block_type: EXTERNAL_WRITE_BLOCK.to_string(),
        },
        ExternalToolBlockedItem {
            tool: name.to_string(),
            operation: "auto_execute".to_string(),
            reason: "Automatic external tool execution is not allowed; manual user action \
                     would be required for any future use."
                .to_string(),
            block_type: AUTO_EXEC_BLOCK.to_string(),
        },
    ];
    // Editors additionally have extract/replace blocked explicitly.
    if matches!(
        kind,
        ExternalToolKind::OpenIv | ExternalToolKind::CodeWalker
    ) {
        blocked.push(ExternalToolBlockedItem {
            tool: name.to_string(),
            operation: "replace_files".to_string(),
            reason: "Entry replacement via external editor is not allowed.".to_string(),
            block_type: EXTERNAL_WRITE_BLOCK.to_string(),
        });
    }

    ExternalToolPlanEntry {
        tool: name.to_string(),
        kind,
        detection,
        trust_level: kind.trust_level(),
        risk_level: kind.risk_level(),
        capabilities,
        allowed_now: false,
        manual_user_action_required: true,
        blocked,
    }
}

/// Detect + plan all known tools. Missing tools never cause failure.
pub fn plan_known_tools() -> Vec<ExternalToolPlanEntry> {
    KNOWN_TOOLS.iter().map(|k| plan_entry(*k)).collect()
}

/// Test-only helper: PATH membership of an arbitrary name.
#[cfg(test)]
pub(crate) fn exists_on_path_for_test(name: &str) -> bool {
    exists_on_path(name)
}
