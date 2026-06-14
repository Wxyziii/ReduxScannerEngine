# T0.6.13 — Archive-Prefix-Aware CodeWalker Target Resolution

This milestone enhances `codewalker-resolve-targets` so that suffix matches which
would otherwise be **ambiguous** can be resolved safely when the caller provides an
intended **archive prefix / archive context**. It is purely a resolution-quality
improvement.

> **No writing. No mutation. No execution.** Still uses only HTTP `GET`. It never
> issues a POST, never calls replace/import/reload-services/set-config or any
> mutation endpoint, never executes CodeWalker, does not implement RPF writing or
> native RPF parsing, and never opens or modifies an RPF archive. The active adapter
> stays `NullRpfAdapter`. `writerAllowed` stays `false`.

## Why

CodeWalker search for `visualsettings.dat` (entry `common/data/visualsettings.dat`)
returns archive-prefixed candidates such as:

- `common.rpf\data\visualsettings.dat`
- `update.rpf\common\data\visualsettings.dat`
- `update\update.rpf\common\data\visualsettings.dat`
- `update\x64\update.rpf\common\data\visualsettings.dat`

Three of these are valid suffix matches, so the old conservative resolver marked the
entry **ambiguous** and blocked planning. That is correct when intent is unknown — but
when the caller knows they target `update/update.rpf`, the resolver should be able to
pick that candidate deterministically.

## New CLI inputs

```
codewalker-resolve-targets --entry-manifest-report <path> [--base-url <url>]
    [--readiness-report <path>]
    [--preferred-archive <prefix>]
    [--preferred-archive-path <path>]
    [--allow-archive-prefix-resolution]
    [--out <out.json>]
```

- `--preferred-archive` — the intended archive prefix, e.g. `update/update.rpf`,
  `update.rpf`, or `update/x64/update.rpf`.
- `--preferred-archive-path` — alternative archive path; used when
  `--preferred-archive` is absent.
- `--allow-archive-prefix-resolution` — explicit opt-in. Prefix resolution is only
  active when this flag is set **and** a preferred archive is provided.

With no preferred archive (or without the opt-in flag), the conservative ambiguity
behavior is **unchanged**.

## Path normalization

For comparison, candidate and entry paths are normalized:

- backslashes → forward slashes,
- repeated slashes collapsed,
- leading slash trimmed,
- preferred-archive matching is **case-insensitive**.

Reports keep both the original candidate string (`candidateOriginalPath`) and the
normalized form (`candidateNormalizedPath`).

Example: `update\update.rpf\common\data\visualsettings.dat` →
`update/update.rpf/common/data/visualsettings.dat`, which matches preferred archive
`update/update.rpf` for entry `common/data/visualsettings.dat`.

## Matching priority (deterministic)

A. **Exact** normalized full-path match (`candidate == entry`) — always wins.
B. **Preferred archive + entry suffix** — `candidate == preferred + "/" + entry`.
C. **Preferred prefix + entry suffix** — candidate starts with `preferred + "/"` and
   ends with `/entry` (or equals entry).
D. **Filename-only** stays weak and never resolves on its own.
E. Multiple candidates tied on the strongest applicable rule → **ambiguous**.
F. No candidate matches the preferred archive → **unresolved**, with a clear blocker
   naming the preferred archive.

When prefix resolution is enabled, rules B/C are evaluated (after exact) instead of
the generic suffix rule.

## Report additions

Top level:

- `preferredArchive`, `preferredArchivePath` (optional),
- `archivePrefixResolutionEnabled` (true/false).

Per target:

- `resolutionStrategy` — one of `exact`, `preferred_archive_suffix`, `suffix`,
  `filename_only`, `ambiguous`, `unresolved`,
- `ambiguityReason` (optional),
- `selectedCandidate` (optional, existing).

Per candidate:

- `candidateOriginalPath`, `candidateNormalizedPath`,
- `matchedPreferredArchive` (true/false),
- `matchedArchivePrefix` (optional).

Existing fields are preserved for backwards compatibility.

## Downstream

`codewalker-dry-replace-plan` consumes the resolved target unchanged: a target
resolved via the preferred archive appears in `resolvedTargets` with its
`selectedCandidate`, so the dry plan produces a planned request as usual. No
execution behavior changes; `writerAllowed` stays false.

## Real copied-archive test usage

When resolving `update.rpf` targets against a live CodeWalker.API that returns
archive-prefixed candidates, pass the preferred archive:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-resolve-targets \
    --entry-manifest-report .tmp/live_plan/rpf_entry_manifest_report.json \
    --base-url http://localhost:5555 \
    --preferred-archive "update/update.rpf" \
    --allow-archive-prefix-resolution \
    --out .tmp/live_plan/codewalker_resolve_targets.json
```

This is an explicit per-run preference, **not** a global default for all files.

## Tests

Mock-HTTP only — no real CodeWalker, no real GTA files, no archive mutation, no
`POST /api/replace-file`. Coverage includes: conservative ambiguity preserved without
a preference; resolution for `update/update.rpf`, `update.rpf`, and
`update/x64/update.rpf`; backslash normalization; case-insensitive preferred archive;
blocking on no match and on multiple equal matches; exact match still wins;
filename-only still weak; selected-candidate and resolution-strategy reporting; the
`fileName` query parameter; dry-replace acceptance of a preferred-archive-resolved
target; and a plan-ready test-run after preferred-archive resolution.
