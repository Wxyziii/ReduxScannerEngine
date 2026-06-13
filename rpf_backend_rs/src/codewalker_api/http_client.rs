//! Shared, minimal, safe HTTP/1.1 client for talking to a local CodeWalker.API.
//!
//! This is the single HTTP implementation used by every CodeWalker module
//! (detect, readiness, compat probe, search/resolve, replace apply). It uses only
//! the standard library (no TLS — `http://` only) and is deliberately small.
//!
//! It exists because the real CodeWalker.API (ASP.NET Core / Kestrel) returns
//! `Transfer-Encoding: chunked` JSON responses and binds IPv4 only, which the
//! earlier per-module hand-rolled clients could not handle. This client:
//!
//! * tries every resolved socket address (IPv4 first) until one connects, so
//!   `http://localhost:5555` works even when `::1` is dead and `127.0.0.1` is up;
//! * parses headers case-insensitively;
//! * decodes `Transfer-Encoding: chunked` bodies;
//! * honours `Content-Length`;
//! * falls back to read-until-close when neither is present;
//! * requests `Accept-Encoding: identity` (never gzip/deflate).
//!
//! It performs GET/OPTIONS for read-only probes and POST only for the already
//! gate-protected replace-apply path. It never mutates an archive and never parses
//! RPF internals.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Default CodeWalker.API base URL/port for local installs.
pub const DEFAULT_BASE_URL: &str = "http://localhost:5555";

/// How the response body was extracted from the raw stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyDecodeMode {
    /// Body length came from a `Content-Length` header.
    ContentLength,
    /// Body was `Transfer-Encoding: chunked` and was de-chunked.
    Chunked,
    /// No length/encoding hint; body is everything until the server closed.
    ConnectionClose,
    /// There was no body.
    Empty,
    /// A chunked body was advertised but framing was invalid.
    DecodeFailed,
}

impl BodyDecodeMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            BodyDecodeMode::ContentLength => "content_length",
            BodyDecodeMode::Chunked => "chunked",
            BodyDecodeMode::ConnectionClose => "connection_close",
            BodyDecodeMode::Empty => "empty",
            BodyDecodeMode::DecodeFailed => "decode_failed",
        }
    }
}

/// A fully read HTTP response with decode metadata for reports.
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    /// The socket address that actually connected (e.g. `127.0.0.1:5555`).
    pub connected_address: Option<String>,
    pub transfer_encoding: Option<String>,
    pub content_length: Option<usize>,
    pub body_decode_mode: BodyDecodeMode,
}

/// Per-request timeout. Short for probes; replace-apply passes a longer one.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1500);

// ── URL helpers ──────────────────────────────────────────────────────────────

/// Strip trailing slashes from a base URL (keep the scheme's `//`).
pub fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let no_trailing = trimmed.trim_end_matches('/');
    if no_trailing.is_empty() {
        trimmed.to_string()
    } else {
        no_trailing.to_string()
    }
}

/// A base URL is usable only if it is an `http://`/`https://` URL with a host.
pub fn base_url_valid(normalized: &str) -> bool {
    let rest = match normalized.strip_prefix("http://") {
        Some(r) => r,
        None => match normalized.strip_prefix("https://") {
            Some(r) => r,
            None => return false,
        },
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.split(':').next().unwrap_or("");
    !host.is_empty()
}

/// Percent-encode a query-parameter value (unreserved chars pass through).
pub fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Length-limit a response body for safe storage in reports. `max` is in chars.
pub fn sample_body(body: &str, max: usize) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    if body.chars().count() <= max {
        Some(body.to_string())
    } else {
        Some(body.chars().take(max).collect())
    }
}

/// Classify a (decoded) response body into a coarse shape string + JSON-parse
/// success. Call this AFTER decoding, never on raw chunk framing.
pub fn classify_shape(body: &str) -> (String, bool) {
    if body.trim().is_empty() {
        return ("empty".to_string(), false);
    }
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => {
            let shape = match v {
                serde_json::Value::Array(_) => "json_array",
                serde_json::Value::Object(_) => "json_object",
                serde_json::Value::String(_) => "json_string",
                serde_json::Value::Number(_) => "json_number",
                serde_json::Value::Bool(_) => "json_bool",
                serde_json::Value::Null => "json_null",
            };
            (shape.to_string(), true)
        }
        Err(_) => ("non_json".to_string(), false),
    }
}

// ── Public request entry points ──────────────────────────────────────────────

/// Read-only GET.
pub fn http_get(url: &str) -> Result<HttpResponse, String> {
    request("GET", url, None, DEFAULT_TIMEOUT)
}

/// Read-only OPTIONS (used by the compat probe; never POST).
pub fn http_options(url: &str) -> Result<HttpResponse, String> {
    request("OPTIONS", url, None, DEFAULT_TIMEOUT)
}

/// GET with a caller-chosen timeout.
pub fn http_get_with_timeout(url: &str, timeout: Duration) -> Result<HttpResponse, String> {
    request("GET", url, None, timeout)
}

/// POST a JSON body. Reached only by the gate-protected replace-apply path.
pub fn http_post_json(
    url: &str,
    json_body: &str,
    timeout: Duration,
) -> Result<HttpResponse, String> {
    request("POST", url, Some(json_body), timeout)
}

// ── Core implementation ──────────────────────────────────────────────────────

fn parse_status_line(bytes: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(bytes);
    let first = text.lines().next()?;
    let mut parts = first.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse::<u16>().ok()
}

/// Resolve all addresses for `host:port`, preferring IPv4 so that an IPv4-only
/// server (e.g. Kestrel on `0.0.0.0`) is reachable via `localhost` even when the
/// `::1` (IPv6) candidate is resolved first and dead.
fn resolve_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let mut addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("address resolution failed: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err("no socket address resolved".to_string());
    }
    addrs.sort_by_key(|a| if a.is_ipv4() { 0 } else { 1 });
    Ok(addrs)
}

/// Try each resolved address until a TCP connection succeeds.
fn connect_any(addrs: &[SocketAddr], timeout: Duration) -> Result<(TcpStream, SocketAddr), String> {
    let mut last_err = String::from("no addresses tried");
    for addr in addrs {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(s) => return Ok((s, *addr)),
            Err(e) => last_err = format!("connect failed for {addr}: {e}"),
        }
    }
    Err(last_err)
}

fn request(
    method: &str,
    url: &str,
    json_body: Option<&str>,
    timeout: Duration,
) -> Result<HttpResponse, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "only http:// is supported for the CodeWalker client".to_string())?;

    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let p = authority[i + 1..]
                .parse::<u16>()
                .map_err(|_| "invalid port in base URL".to_string())?;
            (&authority[..i], p)
        }
        None => (authority, 80u16),
    };

    let addrs = resolve_addrs(host, port)?;
    let (mut stream, connected) = connect_any(&addrs, timeout)?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();

    let mut req = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: rpf_scanner-codewalker-client\r\n\
         Accept: application/json\r\n\
         Accept-Encoding: identity\r\n\
         Connection: close\r\n"
    );
    if let Some(body) = json_body {
        req.push_str("Content-Type: application/json\r\n");
        req.push_str(&format!("Content-Length: {}\r\n", body.as_bytes().len()));
    }
    req.push_str("\r\n");
    if let Some(body) = json_body {
        req.push_str(body);
    }

    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("request write failed: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("response read failed: {e}"))?;

    let status = parse_status_line(&buf).ok_or_else(|| "no HTTP status line".to_string())?;

    // Split headers / body on the first CRLFCRLF (byte search).
    let split = find_subsequence(&buf, b"\r\n\r\n");
    let (header_bytes, body_bytes): (&[u8], &[u8]) = match split {
        Some(i) => (&buf[..i], &buf[i + 4..]),
        None => (&buf[..], &[]),
    };
    let headers = parse_headers(header_bytes);

    let transfer_encoding = header_value(&headers, "transfer-encoding");
    let content_length =
        header_value(&headers, "content-length").and_then(|v| v.trim().parse::<usize>().ok());

    let (body, mode) = decode_body(body_bytes, transfer_encoding.as_deref(), content_length);

    Ok(HttpResponse {
        status,
        body,
        connected_address: Some(connected.to_string()),
        transfer_encoding,
        content_length,
        body_decode_mode: mode,
    })
}

/// Extract the body using transfer-encoding / content-length / close semantics.
fn decode_body(
    body_bytes: &[u8],
    transfer_encoding: Option<&str>,
    content_length: Option<usize>,
) -> (String, BodyDecodeMode) {
    let chunked = transfer_encoding
        .map(|te| te.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);

    if chunked {
        match decode_chunked(body_bytes) {
            Some(decoded) => (
                String::from_utf8_lossy(&decoded).to_string(),
                BodyDecodeMode::Chunked,
            ),
            None => (String::new(), BodyDecodeMode::DecodeFailed),
        }
    } else if let Some(len) = content_length {
        let end = len.min(body_bytes.len());
        (
            String::from_utf8_lossy(&body_bytes[..end]).to_string(),
            BodyDecodeMode::ContentLength,
        )
    } else if body_bytes.is_empty() {
        (String::new(), BodyDecodeMode::Empty)
    } else {
        (
            String::from_utf8_lossy(body_bytes).to_string(),
            BodyDecodeMode::ConnectionClose,
        )
    }
}

/// Decode an HTTP/1.1 chunked body. Returns `None` on invalid framing.
fn decode_chunked(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        // Chunk-size line, terminated by CRLF.
        let line_end = find_subsequence(data, b"\r\n")?;
        let size_line = &data[..line_end];
        // Chunk extensions (";name=value") are ignored.
        let size_str = String::from_utf8_lossy(size_line);
        let size_hex = size_str.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        data = &data[line_end + 2..];

        if size == 0 {
            // Last chunk. Any trailing headers (until CRLFCRLF) are ignored.
            return Some(out);
        }

        if data.len() < size {
            return None; // truncated chunk
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size..];

        // Each chunk's data is followed by CRLF.
        if data.len() < 2 || &data[..2] != b"\r\n" {
            return None;
        }
        data = &data[2..];
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Parse header lines into (lowercased-name, value) pairs.
fn parse_headers(header_bytes: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(header_bytes);
    let mut out = Vec::new();
    // Skip the status line (first line).
    for line in text.split("\r\n").skip(1) {
        if line.is_empty() {
            continue;
        }
        if let Some(i) = line.find(':') {
            let name = line[..i].trim().to_ascii_lowercase();
            let value = line[i + 1..].trim().to_string();
            out.push((name, value));
        }
    }
    out
}

/// Case-insensitive header lookup (names already lowercased by `parse_headers`).
fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    let want = name.to_ascii_lowercase();
    headers
        .iter()
        .find(|(k, _)| *k == want)
        .map(|(_, v)| v.clone())
}
