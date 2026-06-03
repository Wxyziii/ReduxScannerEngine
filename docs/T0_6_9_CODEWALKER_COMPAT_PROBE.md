# T0.6.9 — CodeWalker Live Compatibility Probe

A **live compatibility probe** for CodeWalker.API. Before running a real copied
`update.rpf` replace, we need to know whether the actual CodeWalker.API instance
supports the endpoint paths and request/response shapes our dry replace plan
expects. This probe answers that question using **only safe, non-mutating
requests**.

## What it does

- Normalizes the base URL (default `http://localhost:5555`).
- `GET /` — root reachability.
- `GET /api/service-status` — readiness/health shape.
- `GET /api/search-file?filename=<encoded name>` — search availability and response
  shape (default filename `visualsettings.dat`).
- **Only** with `--check-replace-options`: a single **HTTP `OPTIONS`** on
  `/api/replace-file`. If OPTIONS is unsupported, that is recorded safely.
- Records HTTP status codes and a coarse **response shape** per endpoint
  (`json_array`, `json_object`, `non_json`, `empty`, `unreachable`, …).
- Stores a **length-limited** response body sample (capped at 2048 chars).
- Produces a compatibility report with `compatible_for_search`,
  `compatible_for_dry_replace_planning`, and `compatible_for_live_replace` verdicts.

If CodeWalker is offline, the probe returns a valid `offline` report — **not** an
error (exit 0).

## What it never does

- **No `POST /api/replace-file`.** The replace endpoint is only ever touched with
  `OPTIONS`, and only when explicitly requested.
- Does **not** call `/api/import`, `/api/reload-services`, or `/api/set-config`.
- Does **not** execute CodeWalker as a process or any external tool.
- Does **not** parse RPF internals.
- Does **not** modify any archive or file.
- `writerAllowed` stays `false`; the active adapter stays `NullRpfAdapter`.

## Safety gates

`base_url_valid`, `safe_default_probe_mode`, `root_checked_get_only`,
`service_status_checked_get_only`, `search_checked_get_only`,
`replace_options_only_if_requested`, `replace_post_not_sent`,
`import_endpoint_not_called`, `reload_services_not_called`, `set_config_not_called`,
`mutation_endpoints_not_called`, `external_tool_not_executed`,
`native_parser_not_used`, `archive_not_modified`, `null_adapter_still_active`,
`writer_allowed_false`.

## CLI

```
codewalker-compat-probe [--base-url <url>] [--search-filename <name>]
    [--check-replace-options] [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-compat-probe --base-url http://localhost:5555 \
  --search-filename visualsettings.dat --check-replace-options \
  --out .tmp/codewalker_compat_probe.json
```

Expected:

- Exits 0.
- **Offline:** `status` is `offline`; no mutation endpoints called;
  `writerAllowed` false; `modifiesArchive` false.
- **Online:** records root/status/search HTTP statuses and the search response
  shape; with `--check-replace-options` records the `OPTIONS /api/replace-file`
  status. Never POSTs replace; never calls import/reload/set-config; no archive
  mutation.

## Testing

Automated tests use a **local mock HTTP server only** that records method/path/body
for every request. They prove no `POST /api/replace-file` is sent and that
import/reload-services/set-config are never called. No real CodeWalker.API, no real
GTA files, no RPF parsing, no archive modification.

## Purpose

This probe helps discover the **real CodeWalker.API response shapes** before any
live copied-archive write, so a future live-replace milestone can rely on verified
endpoint behavior rather than assumptions.
