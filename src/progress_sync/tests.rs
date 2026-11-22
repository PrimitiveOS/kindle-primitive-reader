use super::*;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct Stored {
    record: Option<Vec<u8>>,
    version: usize,
    calls: usize,
    next_get: Option<&'static str>,
    drop_next_put_reply: bool,
}

pub(crate) struct Server {
    url: String,
    state: Arc<Mutex<Stored>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("reader-sync-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Server {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/reading/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(Stored::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = state.clone();
        let finished = stop.clone();
        let thread = std::thread::spawn(move || {
            while !finished.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut head = Vec::new();
                let mut byte = [0];
                while !head.ends_with(b"\r\n\r\n") {
                    if stream.read_exact(&mut byte).is_err() || head.len() >= 16_384 {
                        break;
                    }
                    head.push(byte[0]);
                }
                let text = String::from_utf8(head).unwrap();
                let mut lines = text.lines();
                let request = lines.next().unwrap_or("");
                let method = request.split_whitespace().next().unwrap_or("");
                let headers: BTreeMap<_, _> = lines
                    .filter_map(|line| line.split_once(':'))
                    .map(|(a, b)| (a.to_ascii_lowercase(), b.trim().to_owned()))
                    .collect();
                let length: usize = headers
                    .get("content-length")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                assert!(length <= 65536);
                let mut body = vec![0; length];
                if stream.read_exact(&mut body).is_err() {
                    continue;
                }
                let mut state = shared.lock().unwrap();
                state.calls += 1;
                let tag = format!("\"v{}\"", state.version);
                let mut code = 200;
                let mut etag = Some(tag.clone());
                let mut reply = Vec::new();
                let mut declared = None;
                let mut drop_reply = false;
                if headers.get("authorization").map(String::as_str) != Some("Basic c2ltOnB3") {
                    code = 401;
                } else {
                    match method {
                        "GET" => {
                            if let Some(mode) = state.next_get.take() {
                                match mode {
                                    "redirect" => code = 302,
                                    "missing-tag" => {
                                        etag = None;
                                        reply = state.record.clone().unwrap();
                                    }
                                    "weak-tag" => {
                                        etag = Some(format!("W/{tag}"));
                                        reply = state.record.clone().unwrap();
                                    }
                                    "truncated" => {
                                        reply = b"{}".to_vec();
                                        declared = Some(1000);
                                    }
                                    "large" => reply = vec![b'x'; LIMIT as usize + 1],
                                    "bad-json" => reply = b"not-json".to_vec(),
                                    _ => panic!("unknown response"),
                                }
                            } else if let Some(bytes) = &state.record {
                                reply = bytes.clone();
                            } else {
                                code = 404;
                                etag = None;
                            }
                        }
                        "PUT" => {
                            let create =
                                headers.get("if-none-match").map(String::as_str) == Some("*");
                            let update = headers.get("if-match") == Some(&tag);
                            if (create && state.record.is_some())
                                || (!create && (!update || state.record.is_none()))
                            {
                                code = 412;
                            } else {
                                state.record = Some(body);
                                state.version += 1;
                                code = 201;
                                etag = Some(format!("\"v{}\"", state.version));
                                drop_reply = std::mem::take(&mut state.drop_next_put_reply);
                            }
                        }
                        "DELETE" => {
                            if state.record.is_some() && headers.get("if-match") != Some(&tag) {
                                code = 412;
                            } else {
                                state.record = None;
                                code = 204;
                                etag = None;
                            }
                        }
                        _ => code = 405,
                    }
                }
                drop(state);
                // Commit the conditional PUT, then close the actual TCP connection
                // before any response bytes: the caller cannot know it succeeded.
                if drop_reply {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let mut head = format!(
                    "HTTP/1.1 {code} Result\r\nConnection: close\r\nContent-Length: {}\r\n",
                    declared.unwrap_or(reply.len())
                );
                if let Some(tag) = etag {
                    head.push_str(&format!("ETag: {tag}\r\n"));
                }
                if code == 302 {
                    head.push_str("Location: http://127.0.0.1:9/other\r\n");
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&reply);
            }
        });
        Self {
            url,
            state,
            stop,
            thread: Some(thread),
        }
    }
    pub(crate) fn config(&self) -> Config {
        Config {
            enabled: true,
            folder: self.url.clone(),
            username: "sim".into(),
            password: "pw".into(),
            device: "Oasis".into(),
            ..Default::default()
        }
    }
    pub(crate) fn calls(&self) -> usize {
        self.state.lock().unwrap().calls
    }
    pub(crate) fn version(&self) -> usize {
        self.state.lock().unwrap().version
    }
    pub(crate) fn record(&self) -> Option<Record> {
        self.state
            .lock()
            .unwrap()
            .record
            .as_ref()
            .map(|bytes| serde_json::from_slice(bytes).unwrap())
    }
    pub(crate) fn interrupt_next_put_reply(&self) {
        self.state.lock().unwrap().drop_next_put_reply = true;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn position(page: usize) -> ReadingPosition {
    ReadingPosition {
        chapter: 0,
        page,
        anchor: None,
    }
}
fn record(page: usize) -> Record {
    Record {
        version: 1,
        edition: edition(42, 1000),
        book: 42,
        position: Some(position(page)),
        device: "Oasis".into(),
        updated_at: Some(1_740_777_900),
    }
}
fn request(server: &Server, operation: Operation) -> Request {
    Request {
        config: server.config(),
        edition: edition(42, 1000),
        book: 42,
        operation,
    }
}
fn found(reply: Reply) -> Remote {
    match reply {
        Reply::Found(r) | Reply::Stored(r) => r,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn two_clients_preserve_concurrent_offline_progress_with_conditional_writes() {
    let server = Server::start();
    assert!(matches!(
        execute(&request(&server, Operation::Fetch)).unwrap(),
        Reply::Missing
    ));
    let first = found(
        execute(&request(
            &server,
            Operation::Put {
                record: record(1),
                expected: None,
            },
        ))
        .unwrap(),
    );
    let base = Seen {
        etag: first.etag.clone(),
        position: first.record.position.clone(),
    };
    let a = found(
        execute(&request(
            &server,
            Operation::Put {
                record: record(2),
                expected: Some(base.etag.clone()),
            },
        ))
        .unwrap(),
    );
    assert!(matches!(
        execute(&request(
            &server,
            Operation::Put {
                record: record(3),
                expected: Some(base.etag.clone())
            }
        ))
        .unwrap(),
        Reply::Changed
    ));
    assert_eq!(
        decide(&position(3), Some(&a), Some(&base)),
        Decision::Conflict
    );
    let b = found(
        execute(&request(
            &server,
            Operation::Put {
                record: record(3),
                expected: Some(a.etag),
            },
        ))
        .unwrap(),
    );
    assert_eq!(b.record.position, Some(position(3)));
    let b_base = Seen {
        etag: b.etag.clone(),
        position: Some(position(3)),
    };
    assert_eq!(
        decide(&position(3), Some(&b), Some(&b_base)),
        Decision::Equal
    );
    assert_eq!(
        decide(&position(4), Some(&b), Some(&b_base)),
        Decision::Send
    );
    assert_eq!(
        decide(&position(1), Some(&b), Some(&base)),
        Decision::Receive
    );
    assert_eq!(decide(&position(3), Some(&b), None), Decision::Equal);
    assert_eq!(decide(&position(4), Some(&b), None), Decision::Conflict);
    assert_eq!(decide(&position(1), None, None), Decision::Send);
}

#[test]
fn deletion_and_an_interrupted_write_recover_without_overwriting_another_client() {
    let server = Server::start();
    let first = found(
        execute(&request(
            &server,
            Operation::Put {
                record: record(4),
                expected: None,
            },
        ))
        .unwrap(),
    );
    let seen = Seen {
        etag: first.etag.clone(),
        position: Some(position(4)),
    };
    // A client that lost PUT's reply can recover by fetching the identical record.
    let fetched = found(execute(&request(&server, Operation::Fetch)).unwrap());
    assert_eq!(decide(&position(4), Some(&fetched), None), Decision::Equal);
    assert!(matches!(
        execute(&request(
            &server,
            Operation::Delete {
                expected: Some("\"old\"".into())
            }
        ))
        .unwrap(),
        Reply::Changed
    ));
    let cleared = match execute(&request(
        &server,
        Operation::Delete {
            expected: Some(first.etag),
        },
    ))
    .unwrap()
    {
        Reply::Deleted(remote) => remote,
        other => panic!("unexpected {other:?}"),
    };
    assert!(cleared.record.position.is_none());
    let fetched = found(execute(&request(&server, Operation::Fetch)).unwrap());
    assert_eq!(
        decide(&position(4), Some(&fetched), None),
        Decision::Cleared
    );
    assert_eq!(decide(&position(4), None, Some(&seen)), Decision::Cleared);
    assert!(matches!(
        execute(&request(
            &server,
            Operation::Delete {
                expected: Some(cleared.etag)
            }
        ))
        .unwrap(),
        Reply::Deleted(_)
    ));
    assert_eq!(position(4).page, 4); // Remote deletion never resets local reading.
}

#[test]
fn disabled_auth_redirect_and_bad_record_cases_leave_the_server_unchanged() {
    let server = Server::start();
    let mut get = request(&server, Operation::Fetch);
    get.config.enabled = false;
    assert!(execute(&get).is_err());
    assert_eq!(server.state.lock().unwrap().calls, 0);
    get.config.enabled = true;
    get.config.password = "incorrect".into();
    assert!(execute(&get).unwrap_err().contains("credentials"));
    found(
        execute(&request(
            &server,
            Operation::Put {
                record: record(2),
                expected: None,
            },
        ))
        .unwrap(),
    );
    let before = server.state.lock().unwrap().record.clone();
    for mode in [
        "redirect",
        "missing-tag",
        "weak-tag",
        "truncated",
        "large",
        "bad-json",
    ] {
        server.state.lock().unwrap().next_get = Some(mode);
        assert!(
            execute(&request(&server, Operation::Fetch)).is_err(),
            "{mode}"
        );
        assert_eq!(server.state.lock().unwrap().record, before);
    }
    let mut wrong = record(2);
    wrong.book = 43;
    server.state.lock().unwrap().record = Some(serde_json::to_vec(&wrong).unwrap());
    assert!(
        execute(&request(&server, Operation::Fetch))
            .unwrap_err()
            .contains("another edition")
    );
}

#[test]
fn invalid_urls_tags_and_saved_credentials_are_bounded_and_atomic() {
    let server = Server::start();
    let mut get = request(&server, Operation::Fetch);
    for url in [
        "file:///tmp/test",
        "https://user:pw@example.test/path",
        "https://example.test/path?q=secret",
        "https://example.test/#fragment",
    ] {
        get.config.folder = url.into();
        assert!(get.url().is_err());
    }
    for tag in [
        "W/\"v1\"",
        "\"v1\"\r\nSecret: x",
        "unquoted",
        "\"nested\"quote\"",
    ] {
        assert!(!strong_etag(tag));
    }
    let fixture = Scratch::new();
    let path = fixture.0.join("sync.json");
    let cfg = server.config();
    cfg.save(&path).unwrap();
    assert_eq!(Config::load(&path).password, "pw");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let bytes = fs::read(&path).unwrap();
    let mut huge = cfg;
    huge.password = "x".repeat(CONFIG_LIMIT as usize + 1);
    assert!(huge.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(server.state.lock().unwrap().calls, 0);
}

#[test]
fn sync_cancellation_reaps_the_process_and_clock_changes_do_not_resolve_conflicts() {
    let server = Server::start();
    let request = request(&server, Operation::Fetch);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exec sleep 30"]);
    let mut job = Job::spawn(command, &request).unwrap();
    let now = Instant::now();
    assert!(job.poll(now).is_none());
    assert!(
        job.poll(now + JOB_LIMIT + Duration::from_secs(1))
            .unwrap()
            .is_err()
    );
    drop(job);
    assert!(now.elapsed() < Duration::from_secs(2));
    let mut remote = Remote {
        record: record(4),
        etag: "\"x\"".into(),
    };
    let seen = Seen {
        etag: "\"old\"".into(),
        position: Some(position(1)),
    };
    for time in [None, Some(0), Some(u64::MAX)] {
        remote.record.updated_at = time;
        assert_eq!(
            decide(&position(3), Some(&remote), Some(&seen)),
            Decision::Conflict
        );
    }
}

#[test]
fn an_actual_lost_put_acknowledgement_is_recovered_by_fetch_without_a_second_write() {
    let server = Server::start();
    let sent = record(7);
    server.interrupt_next_put_reply();
    let result = execute(&request(
        &server,
        Operation::Put {
            record: sent.clone(),
            expected: None,
        },
    ));
    // A transport may retry an idempotent PUT after a lost connection; its
    // conditional retry must then report Changed rather than replace the record.
    assert!(matches!(result, Err(_) | Ok(Reply::Changed)));
    assert_eq!(server.record(), Some(sent.clone()));
    assert_eq!(server.version(), 1);
    let fetched = found(execute(&request(&server, Operation::Fetch)).unwrap());
    assert_eq!(fetched.record, sent);
    assert_eq!(decide(&position(7), Some(&fetched), None), Decision::Equal);
    assert_eq!(
        server.version(),
        1,
        "recovering a lost acknowledgement is read-only"
    );
}

#[test]
fn version1_never_transfers_or_downgrades_dom_boundary_positions() {
    let server = Server::start();
    let mut boundary = record(0);
    boundary.position.as_mut().unwrap().anchor = Some(crate::Anchor {
        book: 7,
        chapter: "chapter.xhtml".into(),
        revision: 8,
        block: 9,
        offset: 0,
        illustration: None,
        boundary: Some(crate::SourceBoundary {
            origin: 25,
            relative: 0,
        }),
    });
    assert!(boundary.validate(&boundary.edition, boundary.book).is_err());
    assert!(
        execute(&request(
            &server,
            Operation::Put {
                record: boundary.clone(),
                expected: None
            }
        ))
        .is_err()
    );
    assert_eq!(
        server.calls(),
        0,
        "outbound rejection happens before any HTTP request"
    );
    server.state.lock().unwrap().record = Some(serde_json::to_vec(&boundary).unwrap());
    assert!(execute(&request(&server, Operation::Fetch)).is_err());
    assert_eq!(
        server.calls(),
        1,
        "incoming v1 boundary is explicitly rejected after fetch"
    );
    let scratch = Scratch::new();
    let path = scratch.0.join("sync.json");
    let mut config = server.config();
    config.seen.insert(
        boundary.edition.clone(),
        Seen {
            etag: "\"v1\"".into(),
            position: boundary.position.clone(),
        },
    );
    assert!(config.save(&path).is_err());
    assert!(!path.exists());
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let loaded = Config::load(&path);
    assert!(loaded.seen.is_empty());
    assert_eq!(loaded.folder, config.folder);
}
