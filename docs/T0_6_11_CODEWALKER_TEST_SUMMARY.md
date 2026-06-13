# T0.6.11 — CodeWalker Test Report Normalizer

## What it is

`codewalker-test-summary` reads the many reports produced by a copied-archive
CodeWalker test run and folds them into **one** normalized result. It answers, at a
glance:

- Was CodeWalker reachable?
- Was CodeWalker compatible enough for search/replace planning?
- Was target resolution successful?
- Was the dry replace plan valid?
- Was the execution gate eligible?
- Was replace apply attempted, and did it succeed or fail?
- Did the target hash change?
- Was post-write verification successful or suspicious?
- Is rollback available? Was it executed?
- What is the final state, and what is the next recommended action?

## What it is NOT

This command is strictly **read-only**. It:

- does **not** run the pipeline
- does **not** call CodeWalker
- does **not** send HTTP requests (of any method, including POST)
- does **not** execute external tools
- does **not** parse RPF internals
- does **not** modify any archive
- does **not** modify the input report files

It only reads existing report files and produces a normalized summary report. Global
`writerAllowed` stays `false` and the active adapter stays `NullRpfAdapter`.

## When to use it

After a real copied-archive test run (T0.6.10 and the underlying T0.6.x commands), point
this command at whatever reports were generated. You do not need every report — missing
reports produce warnings and an incomplete picture rather than an error. A provided file
that is unreadable or malformed is warned about and ignored; it never crashes the
command. Report shapes that are older or newer than expected are read tolerantly.

## Usage

```
codewalker-test-summary
  [--compatibility-probe-report <path>]
  [--readiness-report <path>]
  [--resolve-report <path>]
  [--dry-replace-plan <path>]
  [--execution-gate-report <path>]
  [--replace-apply-report <path>]
  [--post-write-verify-report <path>]
  [--rollback-restore-report <path>]
  [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-test-summary \
  --compatibility-probe-report .tmp/codewalker_compat_probe.json \
  --readiness-report .tmp/codewalker_readiness.json \
  --resolve-report .tmp/codewalker_resolve_targets.json \
  --dry-replace-plan .tmp/codewalker_dry_replace_plan.json \
  --execution-gate-report .tmp/codewalker_execution_gate.json \
  --replace-apply-report .tmp/codewalker_replace_apply.json \
  --post-write-verify-report .tmp/codewalker_post_write_verify.json \
  --out .tmp/codewalker_test_summary.json
```

The output (stdout, or the `--out` file when requested) is a
`CodeWalkerTestSummaryReport` with:

- `finalStatus` — one of `not_run`, `incomplete_reports`, `ready_for_execute`,
  `execution_failed_no_change`, `execution_succeeded_changed`, `execution_suspicious`,
  `rollback_available`, `rollback_restored`, `unknown`.
- tri-state pipeline facts (`true`/`false`/`null` where `null` means unknown), e.g.
  `codewalkerReachable`, `executionGateEligible`, `replaceSucceeded`, `targetHashChanged`,
  `postWriteSuspicious`, `rollbackExecuted`.
- `phases` — one entry per pipeline phase with its load state and `ok` verdict.
- `findings`, `warnings`, `blockedItems`.
- `recommendations` — the next safe action(s), e.g. `start CodeWalker.API`, `run
  readiness probe`, `fix ambiguous/unresolved targets`, `execute copied archive test`,
  `inspect failed replace response`, `run rollback restore if suspicious`, `proceed to
  next real test if succeeded`.
- standing guarantees, always: `noHttpRequestsSentBySummary: true`,
  `noArchiveModifiedBySummary: true`, `nativeParserUsed: false`,
  `externalToolExecuted: false`, `writerAllowedGlobal: false`.

## Final-status logic

- No replace-apply report and the execution gate is eligible → `ready_for_execute`.
- No replace-apply report and the gate is missing/not eligible → `not_run` /
  `incomplete_reports`.
- Replace apply failed and post-write says no change → `execution_failed_no_change`.
- Replace apply succeeded and post-write says the target changed →
  `execution_succeeded_changed`.
- Post-write result is suspicious → `execution_suspicious`.
- A rollback plan exists but rollback was not executed → `rollback_available`.
- Rollback restore says restored → `rollback_restored`.
- Otherwise → `unknown`.
