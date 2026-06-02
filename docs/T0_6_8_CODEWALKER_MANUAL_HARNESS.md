# T0.6.8 — CodeWalker Manual Test Harness

The **first real copied-archive manual test harness**. It prepares, validates, and
documents a real copied-archive test run that a human drives by hand through the
existing CodeWalker pipeline. It is **plan/checklist first**: by default it produces
a structured report and a manual command checklist, and only writes a script when
explicitly asked. It is deliberately hard to point at a real or original game
archive.

## What it does

- Validates the target `.rpf` path for a **copied test archive** (existence +
  `.rpf` extension).
- Classifies the target path conservatively using the **same original-game-path
  rules** as the execution gate and rollback restore — an original game install
  path is **blocked** (even if the file is absent).
- Requires the target to be explicitly confirmed as a test copy
  (`--target-is-test-copy`).
- Normalizes the CodeWalker base URL (default `http://localhost:5555`).
- Records optional bundle/report inputs and flags any that are missing as
  `missing_inputs` warnings (the plan is still generated).
- Generates a structured **plan + command checklist** for the full real
  copied-test flow:
  1. `probe-rpf`
  2. `backup-rpf`
  3. `codewalker-detect`
  4. `codewalker-readiness`
  5. `rpf-entry-manifest`
  6. `codewalker-resolve-targets`
  7. `codewalker-dry-replace-plan`
  8. `writer-permission`
  9. `codewalker-execution-gate`
  10. `codewalker-replace-apply` *(mutating — commented out)*
  11. `codewalker-post-write-verify`
  12. optional `codewalker-rollback-restore` *(mutating — commented out)*
- Optionally writes a **safe PowerShell checklist/script** under `.tmp` or the
  provided `--project-dir`. The script defaults to a plan/print mode; the two
  mutating commands stay commented out behind explicit placeholders the user must
  fill in and uncomment by hand.

## What it never does (plan / generate-script mode)

- **Does not call CodeWalker** and never sends an HTTP request (no GET, no POST).
- **Does not modify the target archive.** The target SHA-256 is captured before and
  after and must be unchanged (`archive_not_modified_in_plan_mode` gate).
- Does not execute any external tool.
- Does not parse RPF internals.
- Does not create backups (backup is only a generated **checklist step**).

## Execute mode

`--execute` requires the exact confirmation phrase:

```
I understand this will run the copied test archive harness
```

Even with a matching phrase, **this milestone performs no automatic full
execution**: `executionPerformed` stays `false` and the report status is
`execute_requested_not_performed`. The harness exists to guide a careful **manual**
real copied-test run, command by command, reviewing every gate report before any
mutating step.

## Safety gates

`target_rpf_present`, `target_rpf_extension_valid`, `target_marked_as_test_copy`,
`target_not_original_game_archive`, `target_path_allowed_for_test_execution`,
`base_url_valid`, `plan_generated`, `script_generated_only_if_requested`,
`execute_not_requested_or_confirmed`, `codewalker_not_called_in_plan_mode`,
`no_http_requests_in_plan_mode`, `no_external_tool_executed`,
`native_parser_not_used`, `archive_not_modified_in_plan_mode`.

## CLI

```
codewalker-manual-harness --target-rpf <path> --target-is-test-copy
    [--base-url <url>] [--project-dir <path>] [--bundle-dir <path>]
    [--generate-script] [--execute] [--confirm "<phrase>"] [--out <path>]
```

Example (plan + generate script):

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-manual-harness \
  --target-rpf "C:\Users\Marcel\Downloads\ReduxScannerTest\test-copy\update.rpf" \
  --target-is-test-copy --base-url http://localhost:5555 \
  --project-dir .tmp/real_codewalker_test --generate-script \
  --out .tmp/codewalker_manual_harness.json
```

Expected: exits 0; the target is accepted only if it exists and is not classified
as an original game path; a command checklist (and, with `--generate-script`, a
script) is produced; **no CodeWalker calls, no HTTP requests, no archive
modification**.

## Local-only test data

`C:\Users\Marcel\Downloads\ReduxScannerTest` is **optional local-only** data. It
must **never be committed**, never copied into `examples/`, and never used in
automated tests. Only copied test targets such as `test-copy\update.rpf` may be
used for optional manual testing — never original/clean files.

## Status after this milestone

- `codewalkerManualHarnessImplemented` → `true` (T0.6.8 marked implemented).
- `codewalkerReplaceApplyImplemented` and `codewalkerRollbackRestoreImplemented`
  remain `true`.
- `codewalkerWriteAllowedNow` and `writerAllowedNow` remain **`false`** globally.
- `NullRpfAdapter` stays active; **native RPF parsing is still not implemented**.
