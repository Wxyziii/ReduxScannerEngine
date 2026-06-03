# T0.5.7 — RPF Entry Manifest Schema

This milestone defines a structured **entry manifest**: the future writer's input
format. It maps the files in an exported patch bundle to the archive-relative
paths a future writer would replace, using only safe bundle metadata and the
exported patch files.

> **Read-only.** `rpf-entry-manifest` reads `bundle_manifest.json` and walks
> `bundle_dir/files/`. It never parses, opens, or modifies the target RPF, never
> modifies the bundle, and never executes external tools. `readyForWrite` is
> always `false`.

## What the manifest answers

- Which RPF archive-relative paths are targeted?
- Which exported bundle files provide the replacement content?
- What are the file sizes and SHA-256 hashes?
- Are paths normalized and safe?
- Are there duplicate targets?
- Is this still blocked from writing? **Yes.**

## What `rpf-entry-manifest` does

1. Reads and parses `bundle_manifest.json` (blocks if missing/invalid).
2. Verifies `files/` exists and is non-empty.
3. Walks `files/` and, for each exported file, produces an entry with:
   - `archiveRelativePath` (normalized, forward-slash, case-preserved),
   - `bundleFileRelativePath` (e.g. `files/common/data/visualsettings.dat`),
   - `bundleFileAbsolutePath`, `extension`, `sizeBytes`, `sha256`
     (`hashAlgorithm: "SHA-256"`),
   - `replacementSource: "bundle/files"`,
   - `wouldReplaceExistingEntry: true`,
   - `safePath`,
   - `operationKind: "replace_file_planned"`.
4. Validates path safety (see below) and detects duplicate declared targets.
5. Reports conservative safety flags: `modifiesRpf`, `nativeParserUsed`,
   `nativeWriterUsed`, `externalToolUsed`, and `readyForWrite` — all `false`.

## Path safety

Archive-relative paths are validated exactly (no suffix fallback):

- reject absolute paths (leading separator or drive prefix),
- reject any `..` (parent traversal) or `.` component,
- reject empty paths and empty components,
- normalize `\` to `/`, preserve case.

Unsafe declared paths and duplicate declared targets are reported as blocked
items (`unsafe_path`, `duplicate_target`) and set the report status to `blocked`.

## Usage

```
rpf-entry-manifest --bundle-dir <path> [--target-rpf <path>] [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- rpf-entry-manifest \
  --bundle-dir .tmp/redux_patch_bundle_entry_manifest_test \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --out .tmp/rpf_entry_manifest_report.json
```

Expected:

- exits `0` for a valid bundle (exits `1` if blocked),
- lists bundle files under `files/`, each with size and SHA-256,
- `readyForWrite`, `modifiesRpf`, `nativeParserUsed`, `nativeWriterUsed`, and
  `externalToolUsed` are all `false`,
- the target RPF and the bundle are unchanged.

## Future role

This schema will later be consumed by a controlled native or external-tool
writer adapter. The writer is intentionally **not** implemented yet: producing
this manifest never enables writing. A future writer would still have to pass the
existing safety gates (backup, restore, hash verification, manual confirmation,
adapter-supports-write) and the `write-readiness` decision before any real write.

## Test safety

All tests build tiny fake bundles in temp directories and use the fake `.rpf`
fixture as the (never-opened) target. No real GTA V files or copyrighted archive
content are used, no archive is opened or modified, and no external tool is
executed.
