use super::contract::RpfAdapter;
use super::model::*;

/// Block type reported for every unsupported operation.
pub const NOT_IMPLEMENTED_BLOCK: &str = "native_rpf_adapter_not_implemented";

/// The default, safe-mode-only adapter. It never opens, parses, or modifies an
/// archive and never invokes external tools. Every operation that could read
/// internals or write is reported as not implemented.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullRpfAdapter;

impl NullRpfAdapter {
    pub fn new() -> Self {
        NullRpfAdapter
    }

    fn not_implemented_block(op: RpfAdapterOperation) -> RpfAdapterBlockedItem {
        RpfAdapterBlockedItem {
            operation: op.as_str().to_string(),
            reason: format!(
                "Operation `{}` is not implemented: the native RPF adapter does not exist yet.",
                op.as_str()
            ),
            block_type: NOT_IMPLEMENTED_BLOCK.to_string(),
        }
    }
}

impl RpfAdapter for NullRpfAdapter {
    fn name(&self) -> &'static str {
        "null_rpf_adapter"
    }

    fn kind(&self) -> RpfAdapterKind {
        RpfAdapterKind::Null
    }

    fn capabilities(&self) -> RpfAdapterCapabilities {
        RpfAdapterCapabilities {
            // Metadata is only ever read through the existing probe layer
            // (probe-rpf), never by this adapter opening the archive itself.
            can_probe_metadata: true,
            can_list_entries: false,
            can_extract_files: false,
            can_replace_files: false,
            can_write_archive: false,
            requires_external_tool: false,
            native_parser: false,
            native_writer: false,
            safe_mode_only: true,
        }
    }

    fn plan_operation(&self, operation: RpfAdapterOperation) -> RpfAdapterOperationPlan {
        match operation {
            // Metadata probing is "supported" only in the sense that it is
            // delegated to the read-only probe layer; this adapter still does
            // not open the archive.
            RpfAdapterOperation::ProbeMetadata => RpfAdapterOperationPlan {
                operation,
                supported: true,
                status: RpfAdapterStatus::Ready,
                detail: "Metadata/hash is available read-only via the probe layer \
                         (`probe-rpf`). This adapter does not parse archive internals."
                    .to_string(),
                blocked: Vec::new(),
                modifies_archive: false,
            },
            // Everything else is refused as not implemented.
            RpfAdapterOperation::ListEntries
            | RpfAdapterOperation::ExtractFile
            | RpfAdapterOperation::ReplaceFile
            | RpfAdapterOperation::WriteArchive => RpfAdapterOperationPlan {
                operation,
                supported: false,
                status: RpfAdapterStatus::NotImplemented,
                detail: format!(
                    "`{}` is not supported by the null adapter; no native parsing or \
                     writing exists.",
                    operation.as_str()
                ),
                blocked: vec![Self::not_implemented_block(operation)],
                modifies_archive: false,
            },
        }
    }

    fn execute_operation(&self, operation: RpfAdapterOperation) -> RpfAdapterOperationResult {
        // The null adapter NEVER executes anything and NEVER modifies an archive.
        let (status, detail, blocked) = match operation {
            RpfAdapterOperation::ProbeMetadata => (
                RpfAdapterStatus::NotImplemented,
                "Use `probe-rpf` for read-only metadata; the null adapter does not \
                 execute operations."
                    .to_string(),
                Vec::new(),
            ),
            other => (
                RpfAdapterStatus::NotImplemented,
                format!(
                    "`{}` is refused: the null adapter performs no side effects and the \
                     native RPF adapter is not implemented.",
                    other.as_str()
                ),
                vec![Self::not_implemented_block(other)],
            ),
        };

        RpfAdapterOperationResult {
            operation,
            executed: false,
            status,
            detail,
            blocked,
            modified_archive: false,
        }
    }
}
