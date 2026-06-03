use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineType {
    Context,
    Add,
    Remove,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Changed,
    Unchanged,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub old_line_number: Option<usize>,
    pub new_line_number: Option<usize>,
    pub content: String,
}

/// A single contiguous block of changed + context lines.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    /// Approximate unified-diff range label, e.g. `@@ -4,3 +4,3 @@`.
    pub context_label: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub relative_path: String,
    pub orig_abs: String,
    pub staged_abs: String,
    pub changed: bool,
    pub orig_line_count: usize,
    pub staged_line_count: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub hunks: Vec<DiffHunk>,
    /// `true` when the hunk preview was capped at MAX_PREVIEW_CHANGED_LINES.
    pub preview_truncated: bool,
    pub status: DiffStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub total_files: usize,
    pub changed_count: usize,
    pub unchanged_count: usize,
    pub blocked_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    /// `true` when every staged file was successfully diff'd (no blocked items).
    pub diffed_clean: bool,
    pub files: Vec<DiffFile>,
    pub blocked: Vec<DiffBlockedItem>,
    pub summary: DiffSummary,
}
