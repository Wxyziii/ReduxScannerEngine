# T0.5.0 — RPF Writer Planning + Safety Gate Design

This milestone designs the **future** RPF writer layer. It does **not** implement
real RPF archive modification.

> **No real RPF writing is implemented.** `plan-rpf-write` only reads an exported
> bundle and produces a structured plan with safety gates. It never opens, reads,
> or modifies the target archive. `safeToWrite` is always `false`, and the
> `real_rpf_writer_not_implemented` gate is terminal.

## Why direct RPF writes are dangerous

`update.rpf` is a large, encrypted, structured GTA V archive. A naive write can:

- corrupt the entire archive (one bad offset/size makes the whole file unreadable),
- break GTA V's NG encryption expectations (keys are derived from the archive name),
- silently desync the table of contents from entry data,
- destroy the user's only copy with no way back if no backup exists,
- produce a file that loads but crashes the game in non-obvious ways.

Because the target is the user's real game data, the writer must be treated as a
high-risk operation guarded by multiple independent safety layers.

## Required safety layers before any future write support

`plan-rpf-write` emits these gates. Each has `gate`, `passed`, `severity`
(`info` / `warning` / `blocking`), and `message`:

| Gate | Purpose |
|------|---------|
| `bundle_manifest_present` | A valid `bundle_manifest.json` must exist. |
| `bundle_safety_flags_valid` | Bundle must report `modifiesRpf=false`, `modifiesSourceWorkspace=false`, `exportedFromStageOnly=true`, format `redux_patch_bundle`. |
| `patch_plan_present` | `patch_plan.json` must be in the bundle. |
| `diff_report_present` | `diff_report.json` must be in the bundle. |
| `files_present` | `files/` must exist and contain patched files. |
| `target_archive_extension_is_rpf` | Target path must end with `.rpf`. |
| `backup_required` | A verified backup must exist before any write. |
| `restore_plan_required` | A verified rollback path must exist. |
| `hash_verification_required` | Written entries must be SHA-256 verified against the bundle. |
| `manual_confirmation_required` | Explicit human confirmation is required. |
| `real_rpf_writer_not_implemented` | Terminal blocking gate — no real writer exists. |

### Backup strategy

Before any write, copy the target archive to a timestamped backup
(`<archive>.bak`) and verify the copy by SHA-256. No write proceeds until the
backup is confirmed byte-for-byte.

### Restore strategy

On any write error or hash mismatch, restore the archive from the verified
backup and abort, leaving the original archive intact. The original is never
mutated in place without a confirmed restore path.

### Hash verification

Every entry a future writer commits must be SHA-256 verified against the
corresponding bundle file (the manifest records each file's hash). A mismatch
aborts the write and triggers restore.

### Manual confirmation gate

A real write must require explicit, separate human confirmation — it can never be
implied by simply running a command. This prevents accidental writes in scripts
or CI.

## API and CLI

```
plan-rpf-write --bundle-dir <path> --target-rpf <path> [--out <path>]
```

```
build_rpf_write_plan(bundle_dir: &Path, target_archive_path: &Path)
    -> Result<RpfWritePlan, String>
```

The command prints the plan JSON to stdout and optionally writes it with `--out`.
It is a planning command and exits successfully even though `safeToWrite` is
`false`.

## Future roadmap: from planning to a controlled write adapter

1. **T0.5.0 (this milestone)** — planning models + safety gates only. No writer.
2. **Read-only RPF inspection** — open archives read-only to map entries/offsets,
   still never writing.
3. **Sandbox write adapter** — write into a *copy* of an archive in a temp dir,
   never the real file, with full backup/restore/hash verification.
4. **Controlled real write** — gated behind explicit manual confirmation, a
   verified backup, and post-write hash verification, with automatic restore on
   any failure.

Every step keeps the source workspace, staged files, and the user's real game
archives safe by default.

## Confirmations for this milestone

- `safeToWrite` is always `false`.
- Real RPF writing is **not** implemented.
- The target archive is never opened, read, or modified.
- The bundle is never modified (except the optional `--out` plan file, which is
  written to the path the user specifies).
- No copyrighted GTA files are used; tests use fake, non-existent `.rpf` paths.
