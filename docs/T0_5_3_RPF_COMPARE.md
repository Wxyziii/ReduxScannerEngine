# T0.5.3 — Clean vs Modded RPF Hash/Metadata Compare

This milestone adds a **read-only comparison** layer for two `.rpf` archives. It
compares a clean archive against a modded archive at the external file
metadata / hash level only — it does not parse archive internals and does not
modify either file.

> **Read-only.** `compare-rpf` never parses RPF internals and never modifies
> either archive. Real RPF writing and native RPF parsing are still **not**
> implemented.

## What `compare-rpf` does

1. Verifies both paths exist, are files, and end with `.rpf`.
2. Reads file metadata (size) and computes a SHA-256 hash for each archive.
3. Compares the two sizes and the two hashes.
4. Sets `archivesDiffer` to `true` when the size or hash differs.
5. Reports conservative capability flags for this milestone:
   - `canCompareInternals`: `false` (internals are not parsed)
   - `nativeParserImplemented`: `false`
   - `modifiesCleanArchive`: `false`
   - `modifiesModdedArchive`: `false`

Both archives are only ever read. They are never parsed, modified, or written.

This is useful for **proving whether two archives differ** before any future,
deeper inspection — for example, confirming that a modded `update.rpf` is in
fact different from a clean one without yet knowing *what* changed inside.

## Usage

```
compare-rpf --clean-rpf <path> --modded-rpf <path> [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- compare-rpf \
  --clean-rpf examples/rpf_fixtures/fake_update.rpf \
  --modded-rpf examples/rpf_fixtures/fake_modded_update.rpf \
  --out .tmp/rpf_compare_report.json
```

Expected:

- the command exits `0` when both fake `.rpf` files are valid,
- the report contains both file sizes and both SHA-256 hashes,
- `hashDiffers` and `archivesDiffer` are `true` (the two fixtures differ),
- `canCompareInternals`, `nativeParserImplemented`, `modifiesCleanArchive`, and
  `modifiesModdedArchive` are all `false`,
- both fixtures are unchanged.

It exits `1` when blocked (missing target, directory target, or non-`.rpf`
target for either side).

## Report shape (key fields)

```json
{
  "status": "compared",
  "cleanArchivePath": "examples/rpf_fixtures/fake_update.rpf",
  "moddedArchivePath": "examples/rpf_fixtures/fake_modded_update.rpf",
  "cleanSizeBytes": 160,
  "moddedSizeBytes": 230,
  "hashAlgorithm": "SHA-256",
  "cleanSha256": "...",
  "moddedSha256": "...",
  "sizeDiffers": true,
  "hashDiffers": true,
  "archivesDiffer": true,
  "differences": [ { "kind": "hash", "cleanValue": "...", "moddedValue": "..." } ],
  "canCompareInternals": false,
  "nativeParserImplemented": false,
  "blocked": [],
  "modifiesCleanArchive": false,
  "modifiesModdedArchive": false
}
```

## Test safety

All committed tests use the tiny fake fixtures
(`examples/rpf_fixtures/fake_update.rpf` and
`examples/rpf_fixtures/fake_modded_update.rpf`), which contain placeholder text
only. No real GTA V files or copyrighted archive content are used.

Real `update.rpf` files (if any) may be used only for optional, local,
read-only manual comparison; they must never be committed, copied into
`examples/`, or used in automated tests.
