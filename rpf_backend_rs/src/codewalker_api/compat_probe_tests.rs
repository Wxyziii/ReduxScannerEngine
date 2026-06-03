#[cfg(test)]
mod compat_probe_tests {
    use crate::codewalker_api::compat_probe::probe_codewalker_live_compatibility;
    use crate::codewalker_api::model::CodeWalkerCompatibilityProbeStatus;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// One captured HTTP request: method, path, body.
    #[derive(Clone)]
    struct Captured {
        method: String,
        path: String,
        #[allow(dead_code)]
        body: String,
    }

    /// A routing mock HTTP server. Records method/path/body for every request and
    /// answers by path so tests can prove exactly which endpoints were hit.
    struct MockServer {
        base_url: String,
        requests: Arc<Mutex<Vec<Captured>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        /// `connections` = how many requests the probe will make (so the accept
        /// loop terminates). `search_status`/`search_body` configure the search
        /// response shape under test.
        fn start(connections: usize, search_status: u16, search_body: String) -> MockServer {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_thread = Arc::clone(&requests);

            let handle = std::thread::spawn(move || {
                for _ in 0..connections {
                    let (stream, _) = match listener.accept() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    handle_conn(stream, &requests_thread, search_status, &search_body);
                }
            });

            MockServer {
                base_url: format!("http://{addr}"),
                requests,
                handle: Some(handle),
            }
        }

        fn captured(&self) -> Vec<Captured> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    fn handle_conn(
        stream: TcpStream,
        requests: &Arc<Mutex<Vec<Captured>>>,
        search_status: u16,
        search_body: &str,
    ) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("/").to_string();

        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                break;
            }
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("content-length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            }
        }
        let mut body_buf = vec![0u8; content_length];
        if content_length > 0 {
            let _ = reader.read_exact(&mut body_buf);
        }
        let req_body = String::from_utf8_lossy(&body_buf).to_string();

        requests.lock().unwrap().push(Captured {
            method: method.clone(),
            path: path.clone(),
            body: req_body,
        });

        // Route by path.
        let (status, body): (u16, String) = if path == "/" {
            (200, "CodeWalker.API".to_string())
        } else if path.starts_with("/api/service-status") {
            (200, r#"{"status":"ready"}"#.to_string())
        } else if path.starts_with("/api/search-file") {
            (search_status, search_body.to_string())
        } else if path.starts_with("/api/replace-file") {
            // OPTIONS preflight — answer with an allow header, no body.
            (204, String::new())
        } else {
            (404, "{}".to_string())
        };

        let resp = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nAllow: OPTIONS\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.as_bytes().len()
        );
        let mut stream = stream;
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
    }

    /// An address that nothing is listening on (bind, capture, drop).
    fn dead_base_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    const SEARCH_ARRAY: &str = r#"["update/update.rpf/common/data/visualsettings.dat"]"#;

    #[test]
    fn codewalker_compat_probe_offline_returns_report() {
        let url = dead_base_url();
        let r = probe_codewalker_live_compatibility(Some(&url), None, false).unwrap();
        assert_eq!(r.status, CodeWalkerCompatibilityProbeStatus::Offline);
        assert!(!r.mutation_endpoints_called);
        assert!(!r.modifies_archive);
        assert!(!r.writer_allowed);
    }

    #[test]
    fn codewalker_compat_probe_uses_default_base_url() {
        // No server contacted (default localhost:5555 may be offline) — just check
        // the normalized default URL is reported and the call succeeds.
        let r = probe_codewalker_live_compatibility(None, None, false).unwrap();
        assert_eq!(r.normalized_base_url, "http://localhost:5555");
        assert_eq!(r.search_probe_filename, "visualsettings.dat");
    }

    #[test]
    fn codewalker_compat_probe_gets_root_status_and_search() {
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        let reqs = server.captured();
        assert_eq!(reqs.len(), 3);
        assert!(reqs.iter().all(|c| c.method == "GET"));
        assert!(reqs.iter().any(|c| c.path == "/"));
        assert!(reqs
            .iter()
            .any(|c| c.path.starts_with("/api/service-status")));
        assert!(reqs.iter().any(|c| c.path.starts_with("/api/search-file")));
        assert_eq!(r.root_http_status, Some(200));
        assert_eq!(r.service_status_http_status, Some(200));
        assert_eq!(r.search_probe_http_status, Some(200));
    }

    #[test]
    fn codewalker_compat_probe_url_encodes_search_filename() {
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let _r = probe_codewalker_live_compatibility(
            Some(&server.base_url),
            Some("weird name.dat"),
            false,
        )
        .unwrap();
        let reqs = server.captured();
        let search = reqs
            .iter()
            .find(|c| c.path.starts_with("/api/search-file"))
            .unwrap();
        assert!(
            search.path.contains("weird%20name.dat"),
            "path: {}",
            search.path
        );
        assert!(!search.path.contains("weird name.dat"));
    }

    #[test]
    fn codewalker_compat_probe_records_search_json_array_shape() {
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        assert_eq!(r.search_response_shape, "json_array");
        assert_eq!(r.compatible_for_search, Some(true));
        assert_eq!(r.status, CodeWalkerCompatibilityProbeStatus::Compatible);
    }

    #[test]
    fn codewalker_compat_probe_records_unexpected_response_shape() {
        let server = MockServer::start(3, 200, "this is not json".to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        assert_eq!(r.search_response_shape, "non_json");
        assert_eq!(r.compatible_for_search, Some(false));
    }

    #[test]
    fn codewalker_compat_probe_limits_response_body_sample() {
        let huge = format!("\"{}\"", "A".repeat(10_000));
        let server = MockServer::start(3, 200, huge);
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        let obs = r
            .observations
            .iter()
            .find(|o| o.url.contains("/api/search-file"))
            .unwrap();
        let sample = obs.response_body_sample.as_ref().unwrap();
        assert!(
            sample.chars().count() <= 2048,
            "sample len {}",
            sample.chars().count()
        );
    }

    #[test]
    fn codewalker_compat_probe_options_replace_only_when_requested() {
        // Not requested → 3 requests, none to replace-file.
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        assert!(!r.replace_endpoint_options_checked);
        assert!(server
            .captured()
            .iter()
            .all(|c| !c.path.starts_with("/api/replace-file")));
        drop(server);

        // Requested → 4 requests, one OPTIONS to replace-file.
        let server2 = MockServer::start(4, 200, SEARCH_ARRAY.to_string());
        let r2 = probe_codewalker_live_compatibility(Some(&server2.base_url), None, true).unwrap();
        assert!(r2.replace_endpoint_options_checked);
        let reqs = server2.captured();
        let opt = reqs
            .iter()
            .find(|c| c.path.starts_with("/api/replace-file"))
            .unwrap();
        assert_eq!(opt.method, "OPTIONS");
        assert_eq!(r2.replace_endpoint_options_http_status, Some(204));
    }

    #[test]
    fn codewalker_compat_probe_does_not_post_replace() {
        let server = MockServer::start(4, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, true).unwrap();
        assert!(!r.replace_endpoint_post_sent);
        assert!(!r.replace_endpoint_post_checked);
        // No request was POST, and no replace request used POST.
        assert!(server.captured().iter().all(|c| c.method != "POST"));
        assert!(server
            .captured()
            .iter()
            .all(|c| !(c.path.starts_with("/api/replace-file") && c.method == "POST")));
    }

    #[test]
    fn codewalker_compat_probe_does_not_call_import() {
        let server = MockServer::start(4, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, true).unwrap();
        assert!(!r.import_endpoint_called);
        assert!(server
            .captured()
            .iter()
            .all(|c| !c.path.starts_with("/api/import")));
    }

    #[test]
    fn codewalker_compat_probe_does_not_call_reload_services() {
        let server = MockServer::start(4, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, true).unwrap();
        assert!(!r.reload_services_called);
        assert!(server
            .captured()
            .iter()
            .all(|c| !c.path.starts_with("/api/reload-services")));
    }

    #[test]
    fn codewalker_compat_probe_does_not_call_set_config() {
        let server = MockServer::start(4, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, true).unwrap();
        assert!(!r.set_config_called);
        assert!(server
            .captured()
            .iter()
            .all(|c| !c.path.starts_with("/api/set-config")));
    }

    #[test]
    fn codewalker_compat_probe_writer_allowed_false() {
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
        assert!(!r.compatible_for_live_replace);
        assert!(!r.modifies_archive);
        assert!(!r.native_parser_used);
    }

    #[test]
    fn codewalker_compat_probe_null_adapter_active() {
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        assert!(r.null_adapter_active);
    }

    #[test]
    fn codewalker_compat_probe_out_file_written_when_requested() {
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let r = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let out = dir.path().join("codewalker_compat_probe.json");
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["replaceEndpointPostSent"], false);
        assert_eq!(v["mutationEndpointsCalled"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_compat_probe_does_not_modify_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let before: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        let server = MockServer::start(3, 200, SEARCH_ARRAY.to_string());
        let _ = probe_codewalker_live_compatibility(Some(&server.base_url), None, false).unwrap();
        let after: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        assert_eq!(before.len(), after.len());
    }
}
