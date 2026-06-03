# T0.6.6 — CodeWalker Post-Write Verification + Rollback Plan

## Summary

`codewalker-post-write-verify` answers, after a CodeWalker replace attempt:

> *"What changed, was the result expected, and what backup would be used for
> rollback?"*

It is **local and read-only**. It reads the target copied-test `.rpf` file and
four reports, compares hashes, classifies the outcome, and builds a **rollback
plan** pointing at the verified backup. It never restores the backup and never
executes rollback.

## Inputs

- the target copied-test `.rpf` file (read only, to compute its current SHA-256)
- the **replace apply report** (T0.6.5) — status, `replaceRequestsSent`,
  `successfulReplaceCount`/`failedReplaceCount`, `originalTargetSha256`,
  `postExecutionTargetSha256`
- the **backup report** (T0.5.1) — `hashVerified`, `safeForFutureWrite`,
  `backupFilePath`, `originalHash`, `backupHash`, `targetArchivePath`
- the **execution gate report** (T0.6.4) — `codewalkerExecutionEligible`,
  `targetArchiveClassification`
- the **dry replace plan** (T0.6.3) — planned request count

## What it does

- computes the **current target SHA-256** and size
- compares the current hash against the apply report's pre/post hashes and the
  backup's original hash (`true` / `false` / `unknown`)
- classifies a `verificationResult`:
  - `no_execution_no_change` — no request sent, target unchanged
  - `execution_failed_no_change` — request failed, target unchanged
  - `execution_failed_but_target_changed_suspicious` — request failed yet target
    changed
  - `execution_succeeded_target_changed` — request succeeded and target changed
  - `execution_succeeded_but_target_unchanged_suspicious` — request succeeded yet
    target unchanged
  - `unknown` — could not classify
- builds a **rollback plan** when the backup is valid (loaded, hash-verified,
  safe for future write, the backup file exists, and the backup target matches);
  `rollbackRecommended` is set for suspicious or succeeded-changed states

## What it does NOT do

- **does not copy the backup over the target** / does not restore
- **does not modify** the target archive
- **does not execute** rollback — `rollbackExecuted` and
  `rollbackExecutionAllowed` are always `false`; the rollback plan's
  `rollbackExecutionSupported` and `safeToExecuteNow` are always `false`
- **does not call** CodeWalker (`/api/replace-file`, `/api/import`,
  `/api/reload-services`, `/api/set-config`, search) — none are called
- **does not send** any HTTP request, and never uses POST
- **does not execute** CodeWalker as a process or any external tool
- **does not parse** RPF internals (no native parser/writer)
- **does not create** backups
- **does not modify** source/staged/bundle/report files (only the optional
  `--out` JSON is written)

The global `writerAllowed` stays `false` and the active adapter stays
`NullRpfAdapter`.

## Usage

```
codewalker-post-write-verify --target-rpf <path> --replace-apply-report <path>
        --backup-report <path> --execution-gate-report <path>
        --dry-replace-plan <path> [--out <out.json>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-post-write-verify \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --replace-apply-report .tmp/codewalker_replace_apply.json \
  --backup-report .tmp/rpf_backup_report.json \
  --execution-gate-report .tmp/codewalker_execution_gate.json \
  --dry-replace-plan .tmp/codewalker_dry_replace_plan.json \
  --out .tmp/codewalker_post_write_verify.json
```

The command always exits `0` as a reporting command.

## Output

The report (`CodeWalkerPostWriteVerifyReport`) includes:

- `status`: `verified` or `invalid_input`
- target facts: `targetCurrentSha256`, `targetCurrentSizeBytes`, existence,
  extension validity
- hash comparisons: `targetHashMatchesApplyReportPostHash`,
  `targetHashChangedFromPreApply`, `targetHashMatchesBackupOriginalHash`
- `verificationResult`, `rollbackPlan`, `rollbackAvailable`,
  `rollbackRecommended`, `rollbackExecuted: false`,
  `rollbackExecutionAllowed: false`
- `gates`, `warnings`, `blockedItems`, `summary`
- safety mirror (all `false`): `httpRequestsSent`, `postRequestsSent`,
  `replaceEndpointCalled`, `importEndpointCalled`, `reloadServicesCalled`,
  `setConfigCalled`, `externalToolExecuted`, `nativeParserUsed`,
  `nativeWriterUsed`, `modifiesArchive`, `writerAllowed`

The rollback plan records `restoreMethodPlanned: "copy_backup_over_target"` and
`rollbackRequiresExplicitFutureConfirm: true`.

## Next milestone

**T0.6.7** will implement **controlled rollback execution from backup** —
restoring a copied test archive from its verified backup, heavily gated and behind
explicit confirmation. Native RPF parsing and global RPF writing remain
unimplemented.
