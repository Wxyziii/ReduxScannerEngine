# T0.6.10 — CodeWalker Real Copied Archive Test Run Coordinator

## What this is

T0.6.10 adds a single **coordinator** for a full CodeWalker copied-test replace
cycle. It validates every required input and produces one run report that
describes — and, in execute mode, drives — the existing pipeline:

```
validate-inputs → codewalker-replace-apply → codewalker-post-write-verify
```

It is the first phase intended to support a real local copied `update.rpf`
test, but it is **safe by default**.

CLI command: `codewalker-test-run`
Module: `rpf_backend_rs/src/codewalker_api/test_run.rs`
Entry point: `build_or_run_codewalker_copied_archive_test(...)`

## Plan mode is the default (and safe)

When `--execute` is **not** given, the coordinator runs in `plan_only` mode:

- It **calls nothing** — no CodeWalker, no process execution.
- It sends **no HTTP request** of any kind.
- It **does not modify** the target archive (the target SHA-256 is recorded
  before and after and must be identical).
- It validates all required inputs and produces a planned step sequence.
- It reports whether the run is **ready for execute mode** (`readyForExecute`).

## Execute mode requires explicit confirmation

When `--execute` is given, the coordinator additionally requires the **exact**
confirmation phrase:

```
I understand this will run the real copied archive CodeWalker test
```

Execute mode only proceeds when **every eligibility gate passes**:

- target exists, has a `.rpf` extension, and is **not** an original game path,
- the execution gate report classifies the target as a `copied_test_archive`,
- the execution gate reported `codewalkerExecutionEligible: true`,
- the dry replace plan has at least one planned request,
- the optional compatibility probe (if provided) is not blocking,
- every required input report loaded.

Only then does it:

1. Invoke the existing **`apply_codewalker_replace_on_test_archive`** (T0.6.5)
   with that function's own confirmation phrase, writing
   `replace_apply_report.json` under `--project-dir`.
2. Invoke the existing **post-write verification** (T0.6.6), writing
   `post_write_verify_report.json` under `--project-dir`.

If execute mode is too risky or any required report is missing, the coordinator
**blocks** and does not call replace apply.

## Only copied test archives are allowed

The coordinator classifies the target conservatively and **blocks original game
install paths** (e.g. anything resembling `Grand Theft Auto V`,
`steamapps/common`, `.../update/update.rpf` under a games folder). It only ever
modifies the **copied test target**, and only through the existing T0.6.5
replace apply behavior.

## It never rolls back automatically

`rollbackRestoreInvoked` is always `false`. Rollback remains a separate,
explicitly-confirmed command (`codewalker-rollback-restore`).

## What it never does

- Never targets an original game archive.
- Never uses real GTA files in automated tests.
- Never parses RPF internals.
- Never executes CodeWalker as a process or any external tool.
- Never performs automatic full pipeline mutation unless `--execute` and the
  exact confirmation phrase are provided and all gates pass.
- Global `writerAllowed` stays `false`; `NullRpfAdapter` stays active.

## Tests use fake fixtures and mock servers only

The automated tests (`test_run_tests.rs`) use tiny fake `.rpf` fixtures and a
local mock HTTP server. They never require a real CodeWalker.API, never use real
GTA files, and never parse RPF internals. Plan-mode tests assert the target
SHA-256 is unchanged and that original game paths are blocked.

## The optional local test folder

A local folder such as `C:\Users\Marcel\Downloads\ReduxScannerTest` may be used
for **optional manual testing only**. Nothing from it is committed, copied into
`examples/`, or used in automated tests. Only a copied test target such as
`...\ReduxScannerTest\test-copy\update.rpf` may be used.

## CLI

```
codewalker-test-run --target-rpf <path> --project-dir <path>
                    --backup-report <path> --readiness-report <path>
                    --entry-manifest-report <path> --resolve-report <path>
                    --dry-replace-plan <path> --execution-gate-report <path>
                    [--compatibility-probe-report <path>] [--base-url <url>]
                    [--execute] [--confirm "<phrase>"] [--out <path>]
```

### Plan-only example

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-test-run \
  --target-rpf "C:\Users\Marcel\Downloads\ReduxScannerTest\test-copy\update.rpf" \
  --project-dir .tmp/real_codewalker_test \
  --backup-report .tmp/rpf_backup_report.json \
  --readiness-report .tmp/codewalker_readiness.json \
  --entry-manifest-report .tmp/rpf_entry_manifest_report.json \
  --resolve-report .tmp/codewalker_resolve_targets.json \
  --dry-replace-plan .tmp/codewalker_dry_replace_plan.json \
  --execution-gate-report .tmp/codewalker_execution_gate.json \
  --compatibility-probe-report .tmp/codewalker_compat_probe.json \
  --base-url http://localhost:5555 \
  --out .tmp/codewalker_test_run.json
```

Expected: exits 0, no CodeWalker call, no HTTP requests, no target
modification, validates inputs, reports `readyForExecute`.

### Execute example (only when ready and CodeWalker.API is running)

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-test-run \
  ... (same inputs) ... \
  --execute --confirm "I understand this will run the real copied archive CodeWalker test" \
  --out .tmp/codewalker_test_run_execute.json
```

Expected: only runs when all gates pass, invokes replace apply + post-write
verification, does not rollback automatically, writes the report, never targets
an original game archive.
