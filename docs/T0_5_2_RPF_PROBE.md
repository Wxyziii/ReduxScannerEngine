# T0.5.2 — RPF Archive Probe + Tool Capability Detection

This milestone adds a **read-only probe** for target `.rpf` archives. It inspects
file-level metadata and detects available external tooling, without parsing or
modifying the archive.

> **Read-only.** `probe-rpf` never parses RPF internals and never modifies the
> target archive. Real RPF writing and native RPF parsing are still **not**
> implemented.

## What `probe-rpf` does

1. Verifies the target path exists, is a file, and ends with `.rpf`.
2. Reads file metadata (size) and computes a SHA-256 hash of the bytes.
3. Detects whether well-known external tools appear available on `PATH`
   (informational only — nothing is executed).
4. Reports capability flags, all conservative for this milestone:
   - `canReadMetadata`: `true`
   - `canParseRpf`: `false` (internals are not parsed)
   - `canWriteRpf`: `false`
   - `nativeWriterImplemented`: `false`
   - `modifiesTargetArchive`: `false`

The archive is only ever read. It is never parsed, modified, or written, and no
backup is created by this command (see `backup-rpf` for that).

## External tool detection

`detect_external_tools()` performs a best-effort PATH lookup for:
`OpenIV`, `CodeWalker`, `7z`, `powershell`, `cmd`.

This detection is **informational only**:

- A missing tool is reported as `found: false`, never as an error.
- No tool is ever executed (not even `--version`/`--help`).
- GUI tools (OpenIV, CodeWalker) are typically not on PATH and will usually report
  not found; that is expected and harmless.

## Usage

```
probe-rpf --target-rpf <path> [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- probe-rpf \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --out .tmp/rpf_probe_report.json
```

Expected:

- the command exits `0` for a valid fake `.rpf`,
- the report contains the file size and SHA-256 hash,
- `canParseRpf`, `canWriteRpf`, `nativeWriterImplemented`, and
  `modifiesTargetArchive` are all `false`,
- the original `fake_update.rpf` is unchanged.

It exits `1` when blocked (missing target, directory target, or non-`.rpf` target).

## Report shape (key fields)

```json
{
  "status": "probed",
  "targetArchivePath": "examples/rpf_fixtures/fake_update.rpf",
  "exists": true,
  "isFile": true,
  "extensionValid": true,
  "sizeBytes": 160,
  "hashAlgorithm": "SHA-256",
  "sha256": "...",
  "canReadMetadata": true,
  "canParseRpf": false,
  "canWriteRpf": false,
  "nativeWriterImplemented": false,
  "externalTools": [ { "tool": "7z", "found": false, "method": "path_lookup", "detail": "..." } ],
  "capabilities": [ { "name": "write_rpf", "available": false, "detail": "..." } ],
  "blocked": [],
  "modifiesTargetArchive": false
}
```

## Relationship to the write plan (T0.5.0 / T0.5.1)

`plan-rpf-write` safety-gate messages now point at both preflights:

- `backup_required` → `backup-rpf` (a hash-verified backup is required before any write).
- `hash_verification_required` → `probe-rpf` (capture target metadata/hash read-only).

These remain descriptive; `plan-rpf-write` is not yet wired to require probe or
backup output, and the native RPF writer is still not implemented.

## Test safety

All committed tests use the tiny fake fixture
(`examples/rpf_fixtures/fake_update.rpf`), which contains placeholder text only.
No real GTA V files or copyrighted archive content are used.

Real `update.rpf` files (if any) may be used only for optional, local, read-only
manual probing; they must never be committed, copied into `examples/`, or used in
automated tests.
