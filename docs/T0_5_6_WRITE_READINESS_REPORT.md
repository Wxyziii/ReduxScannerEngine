# T0.5.6 — Write Readiness Report

This milestone adds a unified, **read-only** readiness report that combines the
previous preflight layers into one final decision object. It answers a single
question:

> *Is this patch bundle ready to be written into this target RPF?*

In this milestone the answer is always **`readyToWrite: false`**, because real
RPF writing is not implemented, `NullRpfAdapter` cannot write, native
parsing/writing is not implemented, and external tool mutation is not allowed.

> **Read-only.** `write-readiness` never opens or modifies the target archive,
> never modifies the bundle, never creates backups, and never executes external
> tools. It only reads existing reports/files and produces a report.

## What `write-readiness` combines

| Component | Source | Notes |
|---|---|---|
| `bundle` | `plan-rpf-write` gates | manifest present + safety flags valid |
| `write_plan` | `build_rpf_write_plan` | planning only; `safeToWrite` always false |
| `backup` | optional `--backup-report` | verified only if `hashVerified` + `safeForFutureWrite` |
| `probe` | `probe-rpf` (if target exists) | read-only metadata/hash; internals never parsed |
| `adapter` | `rpf-adapter-info` | `NullRpfAdapter` active, `canWriteArchive=false` |
| `external_tools` | `rpf-external-tools` | `safeModeOnly=true`, auto-execution false |

The full source reports are embedded under `writePlan`, `probe`, `adapterInfo`,
and `externalToolPlan` for detail, alongside the compact `components` summaries.

## Gates

`bundle_manifest_present`, `bundle_safety_flags_valid`, `write_plan_built`,
`backup_report_present_or_missing`, `backup_hash_verified`,
`target_probe_successful`, `adapter_info_loaded`, `adapter_supports_write`,
`external_tool_plan_loaded`, `external_archive_mutation_allowed`,
`real_rpf_writer_implemented`, `native_rpf_parser_implemented`,
`manual_confirmation_required`.

Each gate has a `name`, `passed`, `severity` (`info`/`warning`/`blocking`), and a
`message`.

## Blocking items (always present this milestone)

- `real_rpf_writer_not_implemented`
- `active_adapter_cannot_write`
- `native_rpf_parser_not_implemented`
- `external_archive_mutation_not_allowed`

## Usage

```
write-readiness --bundle-dir <path> --target-rpf <path> [--backup-report <path>] [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- write-readiness \
  --bundle-dir .tmp/redux_patch_bundle_readiness_test \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --backup-report .tmp/rpf_backup_readiness_report.json \
  --out .tmp/write_readiness_report.json
```

Expected:

- the command exits `0` (it is a reporting command),
- `readyToWrite` is `false`,
- the backup component reports verified when a valid backup report is supplied,
- the adapter component shows `NullRpfAdapter` active,
- the external-tools component shows `safeModeOnly=true` / auto-execution false,
- `realWriterImplemented`, `nativeParserImplemented`, and
  `modifiesTargetArchive` are all `false`.

Run `backup-rpf` **before** this command to provide a hash-verified backup
report. `write-readiness` itself never creates a backup.

## Intended role

This command is intended to become the final UI/API gate before any future write
operation: a single object a UI can inspect to decide whether to even offer a
write. Until the writer, parser, and a trusted writing adapter exist, it will
always report not ready.

## Test safety

All tests build tiny fake bundles in temp directories and use the fake
`.rpf` fixture as the target. No real GTA V files or copyrighted archive content
are used, no archive is opened or modified, and no external tool is executed.
