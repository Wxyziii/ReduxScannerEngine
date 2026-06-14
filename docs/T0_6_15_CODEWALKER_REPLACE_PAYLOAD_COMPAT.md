# T0.6.15 — CodeWalker Replace Payload Compatibility Fix

This milestone corrects the `POST /api/replace-file` request body so it matches the
real CodeWalker.API contract. It is a payload-shape fix only — no gate, validator,
or safety behavior changed.

> **No gate weakening. No native parsing. No original archive touched. No
> CodeWalker.API file modified.** The active adapter stays `NullRpfAdapter` and the
> global `writerAllowed` stays `false`.

## T0.6.14 live failure

The first real copied-archive execute sent (through the gated coordinator):

```json
{
  "archivePath": "update/update.rpf/common/data/visualsettings.dat",
  "archiveRelativePath": "common/data/visualsettings.dat",
  "dryRunOnly": false,
  "execute": true,
  "rpfPath": "update/update.rpf/common/data/visualsettings.dat",
  "sourceFilePath": ".tmp\\live_plan\\bundle\\files\\common\\data\\visualsettings.dat"
}
```

CodeWalker.API responded:

```
HTTP 400
"Invalid or missing localFilePath."
```

The copied archive was byte-identical before/after (clean failure, not suspicious).

## CodeWalker.API contract discovered

The install at `C:\Users\Marcel\Downloads\CodeWalker.API` is a published single-file
binary (`CodeWalker.API.exe`), not source. The contract was recovered **read-only**
from the binary's metadata/string tables (no files modified):

- `ReplaceController` exposes `POST /api/replace-file`, binding a `ReplaceFileForm`
  DTO with `[FromBody]` (JSON; `application/json`).
- Two required-field validation messages exist verbatim:
  - `"Invalid or missing localFilePath."`
  - `"Invalid or missing rpfFilePath."`
- `ReplaceFileForm` properties: `LocalFilePath`, `RpfFilePath`, `RpfArchivePath`
  (only the first two are required).
- The owning RPF is resolved server-side from the entry path via `RpfMan` /
  `TryResolveEntryPath` — the client does not pass a separate archive handle.

### Required request payload

```json
{
  "localFilePath": "<ABSOLUTE local path to the replacement file>",
  "rpfFilePath": "update/update.rpf/common/data/visualsettings.dat"
}
```

- `localFilePath` — absolute path on the machine running CodeWalker.API to the
  replacement (bundle) file. Must be absolute and exist.
- `rpfFilePath` — full in-archive entry path (the resolved CodeWalker target). The
  server derives the owning RPF from it.
- `rpfArchivePath` is optional and is **not** sent.
- No `sourceFilePath`, `rpfPath`, `archivePath`, `dryRunOnly`, or `execute` keys.

## Scanner payload: before vs after

| Aspect | Before (T0.6.14) | After (T0.6.15) |
|---|---|---|
| Local file key | `sourceFilePath` (relative) | `localFilePath` (absolute) |
| Target key | `rpfPath` + `archivePath` | `rpfFilePath` |
| Extra keys | `archiveRelativePath`, `dryRunOnly`, `execute` | none |
| Body shape | 6 keys | exactly 2 keys |

## Changes

- `model.rs` — new `CodeWalkerReplaceActualPayload { localFilePath, rpfFilePath }`;
  `CodeWalkerDryReplacePayload` now distinguishes `actualRequestPayload` (the exact
  wire body) from scanner-side metadata, and records `apiContractName`,
  `localFilePath`, `localFilePathIsAbsolute`, `localFilePathExists`,
  `codewalkerTargetPath`, `requestSchemaValidated`.
- `dry_replace.rs` — computes the absolute `localFilePath` (current-dir join, no
  Windows `\\?\` verbatim prefix), sets `rpfFilePath` to the resolved target, and
  emits the corrected `actualRequestPayload`.
- `replace_apply.rs` — sends exactly `{ localFilePath, rpfFilePath }`; a new blocking
  gate `local_file_paths_absolute_and_exist` refuses to POST unless every planned
  request has an absolute, existing `localFilePath` and a non-empty `rpfFilePath`.
  Import/reload-services/set-config/search remain uncalled; the shared HTTP client is
  still used; all prior execution gates are preserved.

`localFilePath` must be absolute: if a planned request carries a relative or missing
local path, the request is **blocked before any POST** (`replace_payload_contract_invalid`).

## Tests

Mock-HTTP only; no real CodeWalker, no real GTA files. Coverage: dry plan emits an
absolute `localFilePath` and an `actualRequestPayload` with exactly the contract
keys; replace-apply sends `localFilePath`/`rpfFilePath` and never `sourceFilePath`;
relative or non-existent local paths block before POST; execution gates still
required; plan-only sends no POST; an HTTP 400 with the target unchanged still
classifies as `execution_failed_no_change`; the coordinator execute path sends the
corrected body.

## Safety

No gate weakened, no original game archive touched, no CodeWalker.API file modified,
no native RPF parsing, `writerAllowed` stays `false`, `NullRpfAdapter` stays active.
