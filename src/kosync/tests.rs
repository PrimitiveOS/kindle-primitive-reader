use super::*;
use std::{
    collections::BTreeMap,
    net::TcpListener,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Default)]
pub(crate) struct State {
    pub positions: BTreeMap<String, Position>,
    pub calls: Vec<(String, String, serde_json::Value)>,
    pub next: Option<(u16, String)>,
}
pub(crate) struct Server {
    pub url: String,
    pub state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut head = Vec::new();
                let mut one = [0];
                while !head.ends_with(b"\r\n\r\n") && head.len() < 16_384 {
                    if stream.read_exact(&mut one).is_err() {
                        break;
                    }
                    head.push(one[0]);
                }
                let text = String::from_utf8_lossy(&head);
                let mut lines = text.lines();
                let first = lines.next().unwrap_or("");
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("");
                let headers: BTreeMap<_, _> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let length = headers
                    .get("content-length")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                if length > 65536 {
                    continue;
                }
                let mut bytes = vec![0; length];
                if stream.read_exact(&mut bytes).is_err() {
                    continue;
                }
                let payload = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .unwrap_or(serde_json::Value::Null);
                let mut state = shared.lock().unwrap();
                state
                    .calls
                    .push((method.into(), path.into(), payload.clone()));
                let mut code = 200;
                let mut reply = serde_json::json!({});
                let mut raw = None;
                if let Some((status, body)) = state.next.take() {
                    code = status;
                    raw = Some(body)
                } else if method == "POST" && path == "/users/create" {
                    if payload["username"] == "qa"
                        && payload["password"] == password_key("qa-password").unwrap()
                    {
                        code = 201
                    } else {
                        code = 402
                    }
                } else if headers.get("x-auth-user").map(String::as_str) != Some("qa")
                    || headers.get("x-auth-key") != Some(&password_key("qa-password").unwrap())
                {
                    code = 401
                } else if headers.get("accept").map(String::as_str)
                    != Some("application/vnd.koreader.v1+json")
                {
                    code = 406
                } else if method == "GET" && path == "/users/auth" {
                } else if method == "PUT" && path == "/syncs/progress" {
                    let document = payload["document"].as_str().unwrap_or("");
                    if let Ok(p) = serde_json::from_value::<Position>(payload.clone()) {
                        state.positions.insert(document.into(), p);
                        reply = serde_json::json!({"document":document,"timestamp":1234});
                    } else {
                        code = 400
                    }
                } else if method == "GET" && path.starts_with("/syncs/progress/") {
                    if let Some(p) = state.positions.get(&path[16..]) {
                        reply = serde_json::to_value(p).unwrap()
                    }
                } else {
                    code = 404
                }
                let bytes = raw.unwrap_or_else(|| reply.to_string());
                let extra = if (300..400).contains(&code) {
                    "Location: http://127.0.0.1:1/must-not-follow\r\n"
                } else {
                    ""
                };
                let response = format!(
                    "HTTP/1.1 {code} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{bytes}",
                    bytes.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            url,
            state,
            stop,
            thread: Some(thread),
        }
    }
    pub fn config(&self) -> Config {
        Config {
            server: self.url.clone(),
            username: "qa".into(),
            key: password_key("qa-password").unwrap(),
            authorized: true,
            enabled: true,
            device: "Rust QA".into(),
            device_id: "rust-qa".into(),
            ..Config::default()
        }
    }
    pub fn calls(&self) -> usize {
        self.state.lock().unwrap().calls.len()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kosync-{}-{}", std::process::id(), device_id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn request(server: &Server, operation: Operation) -> Request {
    Request {
        config: server.config(),
        operation,
    }
}
#[test]
fn authored_two_clients_interchange_exact_pdf_pages_without_metadata_or_native_records() {
    let server = Server::start();
    let document = "0123456789abcdef0123456789abcdef".to_string();
    let first = server.config();
    let mut second = first.clone();
    second.device = "Second reader".into();
    second.device_id = "second-id".into();
    let position = Position::pdf(3, 10, &first).unwrap();
    let push = request(
        &server,
        Operation::Push {
            document: document.clone(),
            position: position.clone(),
            count: 10,
        },
    );
    assert!(matches!(execute(&push).unwrap(),Reply::Stored(p) if p==position));
    let pull = Request {
        config: second.clone(),
        operation: Operation::Fetch {
            document: document.clone(),
            count: 10,
        },
    };
    assert!(matches!(execute(&pull).unwrap(),Reply::Found(p) if p.page(10)==Ok(3)));
    let mut next = pull;
    next.operation = Operation::Push {
        document: document.clone(),
        position: Position::pdf(8, 10, &second).unwrap(),
        count: 10,
    };
    assert!(matches!(execute(&next).unwrap(),Reply::Stored(p) if p.progress=="9"));
    for (method, _, value) in &server.state.lock().unwrap().calls {
        if method == "PUT" {
            assert!(value.get("metadata").is_none());
            assert!(value.get("anchor").is_none());
            assert!(value["progress"].is_string());
        }
    }
}
#[test]
fn exact_authentication_and_status_contract_rejects_unconfirmed_or_redirected_work() {
    let server = Server::start();
    for op in [Operation::Register, Operation::Authorize] {
        assert!(matches!(
            execute(&request(&server, op)).unwrap(),
            Reply::Authorized
        ));
    }
    let mut bad = request(&server, Operation::Authorize);
    bad.config.key = password_key("wrong").unwrap();
    assert!(execute(&bad).unwrap_err().contains("credentials"));
    for code in [202, 301, 401, 402, 404, 429, 500] {
        server.state.lock().unwrap().next = Some((code, "{}".into()));
        let calls = server.calls();
        assert!(execute(&request(&server, Operation::Authorize)).is_err());
        assert_eq!(server.calls(), calls + 1);
    }
}
#[test]
fn untrusted_positions_and_bounds_fail_before_navigation_or_transmission() {
    let server = Server::start();
    let config = server.config();
    let valid = Position::pdf(0, 2, &config).unwrap();
    assert_eq!(valid.percentage, 0.5);
    for progress in [
        "0",
        "3",
        "-1",
        "1.0",
        "/body/p/text().0",
        "99999999999999999",
    ] {
        let mut p = valid.clone();
        p.progress = progress.into();
        assert!(p.page(2).is_err());
    }
    for percentage in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let mut p = valid.clone();
        p.percentage = percentage;
        assert!(p.page(2).is_err());
    }
    for input in [
        "[]",
        "{",
        r#"{"percentage":"0.5"}"#,
        r#"{"progress":"1","percentage":0.5,"device":"x","device_id":"x","timestamp":-1}"#,
    ] {
        assert!(position(input.as_bytes(), 2).is_err());
    }
    assert!(position(b"{}", 2).unwrap().is_none());
    let mut p = valid;
    p.progress = "0".into();
    let calls = server.calls();
    assert!(
        execute(&request(
            &server,
            Operation::Push {
                document: "a".repeat(32),
                position: p,
                count: 2
            }
        ))
        .is_err()
    );
    assert_eq!(server.calls(), calls);
}
#[test]
fn account_storage_is_private_atomic_and_does_not_store_plaintext_password() {
    let scratch = Scratch::new();
    let path = scratch.0.join("kosync.json");
    let server = Server::start();
    let config = server.config();
    config.save(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("qa-password"));
    assert_eq!(Config::load(&path).key, config.key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let mut huge = config;
    huge.username = "a".repeat(20_000);
    assert!(huge.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
}
#[test]
fn off_invalid_origin_accounts_and_foreign_document_keys_never_contact_a_server() {
    let server = Server::start();
    for url in [
        "http://example.test/",
        "https://u:p@example.test/",
        "https://example.test/?token=secret",
        "file:///tmp/x",
    ] {
        let mut r = request(&server, Operation::Authorize);
        r.config.server = url.into();
        assert!(execute(&r).is_err());
    }
    for name in ["bad\r\nHeader:x", ""] {
        let mut r = request(&server, Operation::Authorize);
        r.config.username = name.into();
        assert!(execute(&r).is_err());
    }
    let mut r = request(
        &server,
        Operation::Fetch {
            document: "a".repeat(32),
            count: 2,
        },
    );
    r.config.enabled = false;
    assert!(execute(&r).is_err());
    r.config.enabled = true;
    r.config.authorized = false;
    assert!(execute(&r).is_err());
    assert_eq!(server.calls(), 0);
}
#[test]
fn partial_digest_matches_explicit_sampling_and_filename_retains_case_and_extension() {
    let scratch = Scratch::new();
    let path = scratch.0.join("Authored.PDF");
    let bytes: Vec<u8> = (0..20000).map(|i| (i % 251) as u8).collect();
    fs::write(&path, &bytes).unwrap();
    let mut expected = md5::Context::new();
    for offset in [0, 1024, 4096, 16384] {
        expected.consume(&bytes[offset..(offset + 1024).min(bytes.len())]);
    }
    assert_eq!(
        document(&path, Matching::Binary).unwrap(),
        format!("{:x}", expected.finalize())
    );
    assert_eq!(
        document(&path, Matching::Filename).unwrap(),
        format!("{:x}", md5::compute("Authored.PDF"))
    );
    let before = document(&path, Matching::Binary).unwrap();
    let mut changed = bytes.clone();
    changed[3000] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert_eq!(document(&path, Matching::Binary).unwrap(), before);
    changed[4096] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert_ne!(document(&path, Matching::Binary).unwrap(), before);
    for length in [0, 1, 1023, 1024, 1025, 2048, 4096] {
        fs::write(&path, &bytes[..length]).unwrap();
        assert!(document(&path, Matching::Binary).is_ok());
    }
}
#[test]
fn malformed_truncated_and_oversized_server_data_never_become_empty_progress() {
    let server = Server::start();
    let r = request(
        &server,
        Operation::Fetch {
            document: "a".repeat(32),
            count: 2,
        },
    );
    for data in ["[]".to_string(), "{".to_string(), "x".repeat(40_000)] {
        server.state.lock().unwrap().next = Some((200, data));
        assert!(execute(&r).is_err());
    }
}
