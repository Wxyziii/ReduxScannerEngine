pub mod compat_probe;
pub mod detect;
pub mod dry_replace;
pub mod execution_gate;
pub mod manual_harness;
pub mod model;
pub mod post_write_verify;
pub mod readiness;
pub mod replace_apply;
pub mod rollback_restore;
pub mod search;
pub mod test_run;
pub mod test_summary;

#[cfg(test)]
mod compat_probe_tests;
#[cfg(test)]
mod dry_replace_tests;
#[cfg(test)]
mod execution_gate_tests;
#[cfg(test)]
mod manual_harness_tests;
#[cfg(test)]
mod post_write_verify_tests;
#[cfg(test)]
mod readiness_tests;
#[cfg(test)]
mod replace_apply_tests;
#[cfg(test)]
mod rollback_restore_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod test_run_tests;
#[cfg(test)]
mod test_summary_tests;
#[cfg(test)]
mod tests;
