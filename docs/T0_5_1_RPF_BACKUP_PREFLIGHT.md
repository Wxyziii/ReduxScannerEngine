# T0.5.1 — RPF Backup + Hash Verification Preflight

This milestone adds a **backup preflight** for target `.rpf` archives. It copies a
target archive into a backup directory and verifies the copy by SHA-256 before any
future writer could ever be allowed to run.

> **Read/copy only.** `backup-rpf` never modifies or writes the original target
> archive. Real RPF writing is still **not** implemented.

## What `backup-rpf` does

1. Verifies the target path exists, is a file, and ends with `.rpf`.
2. Reads the original archive bytes (read-only) and computes a SHA-256 hash.
3. Creates the backup directory if it does not exist.
4. Copies the archive into the backup directory using a deterministic name:
   `<original-name>.<hash-prefix>.backup` (e.g. `fake_update.rpf.7dc2023c9dee.backup`).
5. Reads the backup back and computes its SHA-256 hash.
6. Sets `hashVerified` and `safeForFutureWrite` to `true` only when the original
   and backup hashes (and sizes) match.

The original target archive is only ever read. It is never modified or written.

## Why hash verification matters

A backup is only useful if it is provably identical to the original. Comparing the
SHA-256 of the original against the SHA-256 of the written copy proves the backup
is byte-for-byte intact. A future writer must refuse to proceed unless a
hash-verified backup exists, so the user always has a guaranteed clean restore
point.

## Usage

```
backup-rpf --target-rpf <path> --backup-dir <path> [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- backup-rpf \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --backup-dir .tmp/rpf_backups \
  --out .tmp/rpf_backup_report.json
```

Expected:

- the backup directory is created,
- the backup file is created,
- `originalHash` equals `backupHash`,
- `hashVerified` is `true`,
- `safeForFutureWrite` is `true`,
- the original `fake_update.rpf` is unchanged.

The command exits `0` on a verified backup and `1` when blocked
(missing target, non-`.rpf` target, directory target, or hash mismatch).

## Report shape (key fields)

```json
{
  "status": "backed_up",
  "targetArchivePath": "examples/rpf_fixtures/fake_update.rpf",
  "backupDir": ".tmp/rpf_backups",
  "backupFilePath": ".tmp/rpf_backups/fake_update.rpf.<prefix>.backup",
  "originalSizeBytes": 0,
  "backupSizeBytes": 0,
  "hashAlgorithm": "SHA-256",
  "originalHash": "...",
  "backupHash": "...",
  "hashVerified": true,
  "safeForFutureWrite": true,
  "blocked": [],
  "modifiesTargetArchive": false,
  "realWriterImplemented": false
}
```

## Relationship to the write plan (T0.5.0)

`plan-rpf-write` already emits a `backup_required` safety gate. Its message now
points at `backup-rpf` and notes that future writing will require a successful,
hash-verified `RpfBackupReport`. This milestone deliberately focuses on backup
creation and verification only; `plan-rpf-write` is not yet wired to require a
backup report.

## Future write support requirement

Before any controlled RPF write is ever implemented, it must:

1. require a successful, hash-verified `RpfBackupReport`,
2. keep the verified backup available for restore,
3. SHA-256 verify every written entry against the bundle,
4. require explicit manual confirmation,
5. restore from backup on any failure.

## Test safety

All tests use a tiny fake fixture (`examples/rpf_fixtures/fake_update.rpf`) that
contains placeholder text only. No real GTA V files or copyrighted archive content
are used anywhere.
