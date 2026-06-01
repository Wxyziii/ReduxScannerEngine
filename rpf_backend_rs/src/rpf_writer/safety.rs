//! Safety-gate construction for the (future) RPF writer.
//!
//! Gates are descriptive only. Even when every input gate passes, the
//! `real_rpf_writer_not_implemented` blocking gate keeps `safe_to_write` at
//! `false` for this milestone — there is no real archive-writing code.

use super::model::{GateSeverity, RpfWriteSafetyGate};

/// Construct a gate.
pub fn gate(name: &str, passed: bool, severity: GateSeverity, message: &str) -> RpfWriteSafetyGate {
    RpfWriteSafetyGate {
        gate: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// The terminal gate that always blocks writing in this milestone.
pub fn real_writer_not_implemented_gate() -> RpfWriteSafetyGate {
    gate(
        "real_rpf_writer_not_implemented",
        false,
        GateSeverity::Blocking,
        "Real RPF archive writing is intentionally not implemented. This command \
         only plans and validates safety gates; it never opens or modifies an archive.",
    )
}
