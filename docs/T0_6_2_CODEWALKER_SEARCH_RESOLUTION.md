# T0.6.2 — CodeWalker Search + Target Resolution Plan

This milestone adds a **read-only** CodeWalker search and target-resolution
planner. It introduces `codewalker-resolve-targets`: a reporting command that
reads the RPF entry manifest (T0.5.7) and maps each entry to CodeWalker search
results — without calling any write/mutation endpoint.

> **No writing. No mutation. No execution.** This milestone uses only HTTP `GET`.
> It never calls replace/import/reload-services/set-config or any mutation/POST
> endpoint, never executes CodeWalker as a process, does not implement RPF writing
> or native RPF parsing, and never opens or modifies an RPF archive. The active
> adapter stays `NullRpfAdapter`. `writerAllowed` stays `false`.

## What it does

1. Reads the entry manifest report and extracts each `archiveRelativePath`.
2. Probes CodeWalker.API readiness (read-only, builds on `codewalker-readiness`).
3. For each entry, derives the filename (basename) and issues a safe
   `GET /api/search-file?filename=<url-encoded-basename>`.
4. Parses results tolerantly (JSON array of strings, array of path-like objects,
   or an object wrapping `results`/`matches`/`files`/`paths`/`data`).
5. Normalizes result paths (backslash → forward slash, case preserved) and
   classifies each candidate against the archive-relative path.

## Matching & resolution rules

Candidate confidence:

- **exact** — normalized result equals the archive-relative path,
- **suffix** — normalized result ends with the archive-relative path,
- **filename_only** — only the basename matches (not enough to resolve),
- **none** — no match.

Resolution:

- exactly one **exact** match → resolved (`matchType: exact`),
- else exactly one **suffix** match → resolved (`matchType: suffix`),
- else more than one matching candidate → **ambiguous**, unresolved,
- else only **filename-only** candidates → unresolved (filename-only),
- else → unresolved (no match).

A filename-only match is **never** enough to resolve. One unresolved target does
not fail the whole report.

## What it never does

- Never calls `/api/replace-file`, `/api/import`, `/api/reload-services`,
  `/api/set-config`, or any other mutation endpoint.
- Never issues a POST (or any non-GET) request.
- Never executes CodeWalker as a process.
- Never opens, reads, or writes an RPF archive. Never creates backups.

If CodeWalker.API is **offline**, the command returns a valid report with all
targets unresolved and a `codewalker_api_offline` blocked item, exiting `0`.

## Always-false fields

`getRequestsOnly` is `true`; the following stay `false`: `postRequestsUsed`,
`replaceEndpointCalled`, `importEndpointCalled`, `reloadServicesCalled`,
`setConfigCalled`, `mutationEndpointsCalled`, `externalToolExecuted`,
`modifiesArchive`, `writerAllowed`, `canWriteArchive`.

## Usage

```
codewalker-resolve-targets --entry-manifest-report <path> [--base-url <url>] [--readiness-report <path>] [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-resolve-targets --entry-manifest-report .tmp/rpf_entry_manifest_report.json \
  --base-url http://localhost:5555 --out .tmp/codewalker_resolve_targets.json
```

Expected if CodeWalker.API is **not** running:

- exits `0`, `codewalkerApiReachable` false,
- `resolvedTargets` empty, `unresolvedTargets` includes every manifest entry,
- `canWriteArchive`, `writerAllowed`, and every mutation/write flag false.

Expected with a reachable server returning matching results:

- exits `0`, `GET /api/search-file?filename=<file>` called per entry,
- unique exact/suffix matches resolved (`selectedCandidate` recorded),
- ambiguous / filename-only candidates remain unresolved,
- `writerAllowed`, `canWriteArchive` false; no mutation endpoint called.

## Safety gates

`entry_manifest_report_present`, `entry_manifest_loaded`,
`codewalker_readiness_context_loaded_or_not_required`, `codewalker_api_reachable`,
`codewalker_ready_for_search`, `search_endpoint_called_get_only`,
`no_post_requests_used`, `write_endpoints_not_called`,
`replace_endpoint_not_called`, `import_endpoint_not_called`,
`reload_services_not_called`, `set_config_not_called`,
`mutation_endpoints_not_called`, `null_adapter_still_active`,
`writer_allowed_false`, `archive_not_modified`.

## Next

**T0.6.3** will build a CodeWalker **dry replace plan** from the resolved targets
produced here. Until the full T0.6.x safety gates pass, CodeWalker writing remains
disabled and no archive mutation occurs.
