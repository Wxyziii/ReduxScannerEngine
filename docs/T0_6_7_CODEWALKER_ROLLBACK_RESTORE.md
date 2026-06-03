# T0.6.7 — Controlled Rollback Restore From Backup

## Summary

`codewalker-rollback-restore` copies a **verified backup file** back over a
**copied test** target archive. It is the first command that may modify a target
archive on disk — and it is heavily gated. The only mutation it can perform is the
gated `copy_backup_over_target`.

It runs only when **all** of these hold:

- the target `.rpf` file exists and has a `.rpf` extension
- the target path is **not** an original game install path
- the T0.6.6 **post-write verification report** has a ready rollback plan
  (`rollbackPlan.rollbackPlanStatus == "ready"`, `rollbackAvailable == true`) and
  confirms the target is a copied test archive
- the T0.5.1 **backup report** is `hashVerified` and `safeForFutureWrite`, names
  an existing backup file, and (if present) its `targetArchivePath` matches the
  restore target
- the **recomputed** backup-file SHA-256 matches the backup report's `backupHash`
- the explicit `--execute-rollback` flag is present
- the exact confirmation phrase matches:
  `I understand this will restore the copied test archive from backup`

If any blocking gate fails, the target is **NOT modified**.

## Scope and hard limits

`codewalker-rollback-restore`:

- restores **only a copied test archive** — never an original game archive
- **blocks original game install paths** (conservative path detection: `Grand
  Theft Auto V`, `GTA V`, `steamapps/common`, `Epic Games/GTAV`, `Rockstar
  Games/Grand Theft Auto V`, or `/update/update.rpf` under a game-like path)
- **verifies the backup hash before restoring** (recompute + compare to report)
- **does not call** CodeWalker (no replace/import/reload-services/set-config/search)
- **does not send** any HTTP request, and never uses POST
- **does not execute** CodeWalker as a process or any external tool
- **does not parse** RPF internals (no native parser/writer)
- **does not create** a backup
- **does not modify** source/staged/bundle/report files (only the optional `--out`
  JSON is written)

Global `writerAllowed` stays `false` and the active adapter stays
`NullRpfAdapter`. The restore is a scoped file copy, not a general writer.

## What happens on a successful restore

1. compute the target SHA-256 **before**
2. copy the verified backup file over the target (`copy_backup_over_target`)
3. compute the target SHA-256 **after**
4. set `restoredTargetMatchesBackup` = (target-after == backup hash)
5. `rollbackExecuted: true`, `modifiesArchive: true`,
   `rollbackExecutionAllowed: true` (scoped to this operation only)

## Usage

```
codewalker-rollback-restore --target-rpf <path> --post-write-verify-report <path>
        --backup-report <path> --execute-rollback --confirm "<phrase>" [--out <out.json>]
```

Example (use a copied temp target, never the committed fixture directly):

```
cp examples/rpf_fixtures/fake_update.rpf .tmp/fake_update_rollback_target.rpf

cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-rollback-restore \
  --target-rpf .tmp/fake_update_rollback_target.rpf \
  --post-write-verify-report .tmp/codewalker_post_write_verify.json \
  --backup-report .tmp/rpf_backup_report.json \
  --execute-rollback \
  --confirm "I understand this will restore the copied test archive from backup" \
  --out .tmp/codewalker_rollback_restore.json
```

The command always exits `0` as a reporting command.

- Without `--execute-rollback`, a wrong/missing `--confirm`, an unverified/
  mismatched backup, a non-`.rpf` or original-path target, or a non-ready rollback
  plan: the target is **not modified** (`status: blocked` / `invalid_input`).
- If the backup report's target path does not match `--target-rpf`: blocked on
  `backup_target_matches_target`.
- With all gates passing: `status: restored`, `rollbackExecuted: true`,
  `restoredTargetMatchesBackup: true`, `modifiesArchive: true`.

## Output

The report (`CodeWalkerRollbackRestoreReport`) includes:

- `status`: `restored` / `blocked` / `invalid_input` / `restore_failed`
- authorization: `executeRollbackRequested`, `confirmationPhraseMatched`
- target facts: existence, extension validity, `targetClassification`,
  `copiedTestArchiveConfirmed`, `targetNotOriginalGameArchive`
- backup facts: `backupFileExists`, `backupHashVerified`,
  `backupHashMatchesReport`, `backupSafeForFutureWrite`,
  `backupTargetMatchesTarget`, `backupSha256`
- execution: `rollbackExecuted`, `rollbackExecutionAllowed`, `targetSha256Before`,
  `targetSha256After`, `restoredTargetMatchesBackup`,
  `restoreMethod: "copy_backup_over_target"`
- `gates`, `warnings`, `blockedItems`, `summary`
- safety mirror (all `false`): `httpRequestsSent`, `postRequestsSent`,
  `replaceEndpointCalled`, `importEndpointCalled`, `reloadServicesCalled`,
  `setConfigCalled`, `externalToolExecuted`, `nativeParserUsed`,
  `nativeWriterUsed`, `writerAllowed`; `modifiesArchive` is `true` only when a
  restore copy actually happened

## Status of the CodeWalker route

With T0.6.7, the copied-test-archive write/verify/rollback loop is complete:
detect → readiness → search/resolve → dry plan → execution gate → replace apply →
post-write verify → rollback restore. Native RPF parsing and global RPF writing
remain unimplemented; global `writerAllowed` remains `false`.
