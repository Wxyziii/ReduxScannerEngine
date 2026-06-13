# T0.6.12 — CodeWalker HTTP Client Compatibility Fix

## The real blocker

The first live copied-archive test against a running CodeWalker.API (ASP.NET Core /
Kestrel) found the server **online and healthy**, but the scanner's per-module
hand-rolled HTTP clients could not talk to it. Two distinct bugs:

### Bug 1 — `localhost` resolves to IPv6, server is IPv4-only
- `http://localhost:5555` resolved `::1` first; CodeWalker.API listens on IPv4
  `0.0.0.0:5555`.
- The old client used only the first `to_socket_addrs()` result, so it connected to
  the dead `::1` and reported `offline`. `http://127.0.0.1:5555` worked.

### Bug 2 — chunked transfer encoding not decoded
- CodeWalker.API returns `Transfer-Encoding: chunked` with no `Content-Length`.
- Raw body framing looks like `ca\r\n{json...}\r\n0\r\n\r\n`.
- The old client treated the chunk framing as the body, so `serde_json` failed and
  `/api/service-status` and `/api/search-file` were misclassified as `non_json` even
  though the API returns valid JSON.

Observed live evidence:
- `GET /api/service-status` → `{ "gtaPath": "...", "servicesReady": true, "statusMessage": "Services are ready", "reloadVersion": 0, "timestamp": ... }`
- `GET /api/search-file?fileName=visualsettings.dat` → JSON array of matching archive paths.

## The fix

A single shared, safe HTTP client now backs every CodeWalker module:
`rpf_backend_rs/src/codewalker_api/http_client.rs`. Used by `detect`, `readiness`,
`compat_probe`, `search`/resolve, and `replace_apply` — the duplicated hand-rolled
clients are gone.

Behaviour:
- **Address fallback**: resolves all addresses, tries each until one connects,
  preferring IPv4. `http://localhost:5555` works against an IPv4-only listener even
  when `::1` is dead; `http://127.0.0.1:5555` still works.
- **Chunked decoding**: `Transfer-Encoding: chunked` bodies are de-chunked (hex chunk
  size line, chunk data, CRLF, terminating `0` chunk; trailing headers ignored).
  Invalid framing yields `bodyDecodeMode = decode_failed` rather than raw framing.
- **Content-Length** bodies honoured; **connection-close** fallback when neither
  Content-Length nor chunked is present.
- **Case-insensitive** header parsing; CRLF handling.
- Requests `Accept: application/json` and `Accept-Encoding: identity` — never
  gzip/deflate.
- GET/OPTIONS for read-only probes; POST only on the already gate-protected
  replace-apply path.
- Response bodies are length-limited for report samples **after** decoding.

## Search query parameter casing

The real API uses `fileName` (camelCase): `GET /api/search-file?fileName=...`.
Search/resolve and the compatibility probe now use `fileName`. A
`searchQueryParameterUsed` field records this in the compat report.

## Report additions

- Compat probe observations: `connectedAddress`, `transferEncoding`, `contentLength`,
  `bodyDecodeMode`.
- Compat probe report: `searchQueryParameterUsed`.
- `responseShape` / `jsonParseSuccess` are computed **after** body decoding.

## Live verification (this phase)

With CodeWalker.API running locally as Administrator:

| Command | Result |
|---|---|
| `codewalker-compat-probe --base-url http://localhost:5555` | `status: compatible`, `serviceStatusShape: json_object`, `searchResponseShape: json_array`, `compatibleForSearch: true` |
| `codewalker-compat-probe --base-url http://127.0.0.1:5555` | identical (`compatible`) |
| `codewalker-readiness --base-url http://localhost:5555` | `status: ready`, `serviceStatusJsonParseSuccess: true`, `servicesReady: true`, `codewalkerApiReadyForSearch: true` |

Safety, this phase:
- No execute mode was run.
- No `POST /api/replace-file` was sent in live verification (only GET + one OPTIONS to
  the replace endpoint, which returned 405).
- `/api/import`, `/api/reload-services`, `/api/set-config` were not called.
- No archive was modified.
- No CodeWalker.API files were modified.
- No RPF internals were parsed.
- Global `writerAllowed` remains `false`; `NullRpfAdapter` remains active.

Automated tests use mock HTTP servers only (several reply chunked); no real CodeWalker,
no real GTA files, no archive mutation.
