//! Opt-in native Reader progress over a user-configured WebDAV folder.
//! Conditional writes preserve remote changes until the reader chooses a side.
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::library::ReadingPosition;

const LIMIT: u64 = 32 * 1024;
const CONFIG_LIMIT: u64 = 2 * 1024 * 1024;
const JOB_LIMIT: Duration = Duration::from_secs(40);

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Config {
    pub enabled: bool,
    pub allow_local_http: bool,
    pub folder: String,
    pub username: String,
    pub password: String,
    pub device: String,
    pub seen: BTreeMap<String, Seen>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Seen {
    pub etag: String,
    pub position: Option<ReadingPosition>,
}

impl Config {
    pub fn validate(&self) -> Result<Url, String> {
        if !self.enabled {
            return Err("Turn on progress sync first".into());
        }
        if self.folder.len() > 2048
            || self.username.len() > 512
            || self.password.len() > 2048
            || self.device.chars().count() > 64
        {
            return Err("Sync settings exceed their text limits".into());
        }
        if self.username.contains(':') {
            return Err("A WebDAV username cannot contain a colon".into());
        }
        let mut url = Url::parse(self.folder.trim()).map_err(|_| "Enter the WebDAV folder URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Use an HTTP or HTTPS folder URL, with credentials in their separate fields".into(),
            );
        }
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        if url.scheme() == "http" {
            let host = url.host_str().unwrap_or("").trim_matches(['[', ']']);
            let ip = host.parse::<std::net::IpAddr>().ok();
            let loopback =
                ip.is_some_and(|ip| ip.is_loopback()) || host.eq_ignore_ascii_case("localhost");
            let local = match ip {
                Some(std::net::IpAddr::V4(ip)) => ip.is_private() || ip.is_link_local(),
                Some(std::net::IpAddr::V6(ip)) => {
                    ip.is_unique_local() || ip.is_unicast_link_local()
                }
                None => host.ends_with(".local"),
            };
            if !loopback && !(self.allow_local_http && local) {
                return Err("Use HTTPS, or enable HTTP for a local-network server".into());
            }
        }
        Ok(url)
    }

    pub fn load(path: &Path) -> Self {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return Self::default();
        };
        if !metadata.is_file() || metadata.len() > CONFIG_LIMIT {
            return Self::default();
        }
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        if bytes.len() as u64 > CONFIG_LIMIT {
            return Err(io::Error::other("Sync state exceeds 2 MiB"));
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let staged = path.with_extension(format!("sync-{}-{nonce}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staged)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&staged, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staged);
        }
        result
    }
}

/// Uses the existing format-specific library edition identity and original size.
/// This is a native protocol identity, independent of the KOReader sync namespace.
pub(crate) fn edition(book: u64, bytes: u64) -> String {
    let input = format!("primitive-reader-progress-v1:{book:016x}:{bytes}");
    ring::digest::digest(&ring::digest::SHA256, input.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub version: u8,
    pub edition: String,
    pub book: u64,
    pub position: Option<ReadingPosition>,
    pub device: String,
    pub updated_at: Option<u64>,
}

impl Record {
    pub fn validate(&self, expected: &str, book: u64) -> Result<(), String> {
        if self.version != 1 || self.edition != expected || self.book != book {
            return Err("The server position belongs to another edition or sync version".into());
        }
        if self.device.chars().count() > 64 {
            return Err("The server position exceeds the reading limits".into());
        }
        let Some(position) = &self.position else {
            return Ok(());
        };
        if position.chapter > 100_000 || position.page > 10_000_000 {
            return Err("The server position exceeds the reading limits".into());
        }
        if let Some(anchor) = &position.anchor
            && (anchor.chapter.len() > 4096
                || anchor.block > 16_000_000
                || anchor.offset > 16_000_000
                || anchor.illustration.is_some_and(|n| n > 16_000_000)
                || anchor.boundary.as_ref().is_some_and(|boundary| {
                    boundary.origin > 64 * 1024 * 1024
                        || boundary.relative > 10_000
                        || anchor.illustration.is_some()
                }))
        {
            return Err("The server returned an invalid source position".into());
        }
        // Anchor.book is the publication/text model revision. It is a different
        // domain from the library/archive identity above. The UI resolves an
        // incoming anchor against the current Probe before changing position.
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Remote {
    pub record: Record,
    pub etag: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum Operation {
    Fetch,
    Put {
        record: Record,
        expected: Option<String>,
    },
    Delete {
        expected: Option<String>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Request {
    pub config: Config,
    pub edition: String,
    pub book: u64,
    pub operation: Operation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum Reply {
    Found(Remote),
    Missing,
    Stored(Remote),
    Deleted(Remote),
    Changed,
}

fn strong_etag(value: &str) -> bool {
    value.len() >= 2
        && value.len() <= 256
        && value.starts_with('"')
        && value.ends_with('"')
        && !value[1..value.len() - 1]
            .bytes()
            .any(|b| b == b'"' || b < 0x21 || b == 0x7f)
}

impl Request {
    fn url(&self) -> Result<Url, String> {
        let base = self.config.validate()?;
        if self.edition.len() != 64 || !self.edition.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("The local edition identity is invalid".into());
        }
        if let Operation::Put { record, expected } = &self.operation {
            record.validate(&self.edition, self.book)?;
            if expected.as_deref().is_some_and(|tag| !strong_etag(tag)) {
                return Err("The server version is invalid".into());
            }
        }
        if let Operation::Delete { expected } = &self.operation
            && expected.as_deref().is_some_and(|tag| !strong_etag(tag))
        {
            return Err("The server version is invalid".into());
        }
        base.join(&format!("primitive-reader-v1-{}.json", self.edition))
            .map_err(|_| "The sync URL is invalid".into())
    }

    fn auth(&self) -> Option<String> {
        (!self.config.username.is_empty() || !self.config.password.is_empty()).then(|| {
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", self.config.username, self.config.password))
            )
        })
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!("PrimitiveOS-Reader/", env!("CARGO_PKG_VERSION")))
            .build(),
    )
}

fn connection_error(_: ureq::Error) -> String {
    "Could not reach the sync server. Check Wi-Fi, the server URL and the device clock.".into()
}

fn status_error(status: u16) -> String {
    match status {
        401 | 403 => "The sync server rejected these credentials".into(),
        300..=399 => "Use the sync folder's final URL; redirects are not followed".into(),
        409 => "The WebDAV folder is missing. Create it on the server first.".into(),
        _ => format!("The sync server returned HTTP {status}"),
    }
}

fn receive(
    mut response: ureq::http::Response<ureq::Body>,
    request: &Request,
) -> Result<Reply, String> {
    match response.status().as_u16() {
        404 => return Ok(Reply::Missing),
        200 => (),
        code => return Err(status_error(code)),
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|h| h.to_str().ok())
        .filter(|tag| strong_etag(tag))
        .ok_or("The server must provide strong ETags to preserve conflicting changes")?
        .to_owned();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|_| "The saved server position is too large or incomplete")?;
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|_| "The server position is not a valid Reader record")?;
    record.validate(&request.edition, request.book)?;
    Ok(Reply::Found(Remote { record, etag }))
}

pub(crate) fn execute(request: &Request) -> Result<Reply, String> {
    let url = request.url()?;
    let client = agent();
    let auth = request.auth();
    let fetch = || {
        let mut get = client
            .get(url.as_str())
            .header("Accept", "application/json");
        if let Some(auth) = &auth {
            get = get.header("Authorization", auth);
        }
        let response = get.call().map_err(connection_error)?;
        receive(response, request)
    };
    match &request.operation {
        Operation::Fetch => fetch(),
        Operation::Put { record, expected } => {
            let bytes = serde_json::to_vec(record)
                .map_err(|_| "The reading position could not be encoded")?;
            if bytes.len() as u64 > LIMIT {
                return Err("The reading position exceeds the sync limit".into());
            }
            let mut put = client
                .put(url.as_str())
                .header("Content-Type", "application/json");
            if let Some(auth) = &auth {
                put = put.header("Authorization", auth);
            }
            put = if let Some(tag) = expected {
                put.header("If-Match", tag)
            } else {
                put.header("If-None-Match", "*")
            };
            let response = put.send(bytes).map_err(connection_error)?;
            match response.status().as_u16() {
                412 => return Ok(Reply::Changed),
                200 | 201 | 204 => (),
                code => return Err(status_error(code)),
            }
            // A follow-up read also covers servers that omit ETag on PUT.
            match fetch()? {
                Reply::Found(remote) if remote.record == *record => Ok(Reply::Stored(remote)),
                _ => Ok(Reply::Changed),
            }
        }
        Operation::Delete { expected } => {
            // Keep a versioned tombstone so another offline client cannot
            // recreate a cleared position merely because GET returned 404.
            let record = Record {
                version: 1,
                edition: request.edition.clone(),
                book: request.book,
                position: None,
                device: String::new(),
                updated_at: None,
            };
            let mut clearing = request.clone();
            clearing.operation = Operation::Put {
                record,
                expected: expected.clone(),
            };
            match execute(&clearing)? {
                Reply::Stored(remote) => Ok(Reply::Deleted(remote)),
                other => Ok(other),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Equal,
    Send,
    Receive,
    Conflict,
    Cleared,
}

pub(crate) fn decide(
    local: &ReadingPosition,
    remote: Option<&Remote>,
    seen: Option<&Seen>,
) -> Decision {
    let Some(remote) = remote else {
        return if seen.is_some() {
            Decision::Cleared
        } else {
            Decision::Send
        };
    };
    let Some(remote_position) = &remote.record.position else {
        return Decision::Cleared;
    };
    if same_position(remote_position, local) {
        return Decision::Equal;
    }
    let Some(seen) = seen else {
        return Decision::Conflict;
    };
    if seen
        .position
        .as_ref()
        .is_some_and(|base| same_position(remote_position, base))
    {
        Decision::Send
    } else if seen
        .position
        .as_ref()
        .is_some_and(|base| same_position(local, base))
    {
        Decision::Receive
    } else {
        Decision::Conflict
    }
}

pub(crate) fn same_position(a: &ReadingPosition, b: &ReadingPosition) -> bool {
    match (&a.anchor, &b.anchor) {
        (Some(a), Some(b)) => a == b,
        (None, None) => a.chapter == b.chapter && a.page == b.page,
        _ => false,
    }
}

pub fn worker() -> crate::Result<()> {
    let request: Request = serde_json::from_reader(io::stdin().lock().take(CONFIG_LIMIT + LIMIT))?;
    serde_json::to_writer(io::stdout().lock(), &execute(&request))?;
    Ok(())
}

pub(crate) struct Job {
    child: Child,
    receiver: Receiver<Result<Reply, String>>,
    pub poll_at: Instant,
    end: Instant,
}

impl Job {
    pub fn start(request: &Request) -> crate::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--sync-worker");
        Self::spawn(command, request)
    }

    pub(crate) fn spawn(mut command: Command, request: &Request) -> crate::Result<Self> {
        request.url().map_err(io::Error::other)?;
        let mut transport = request.clone();
        transport.config.seen.clear();
        let payload = serde_json::to_vec(&transport)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut input = child.stdin.take().expect("piped input");
        let output = child.stdout.take().expect("piped output");
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("progress-sync".into())
            .spawn(move || {
                let reply = (|| {
                    input.write_all(&payload)?;
                    drop(input);
                    serde_json::from_reader(output.take(LIMIT + 4096)).map_err(io::Error::other)
                })()
                .unwrap_or_else(|_| Err("The sync worker ended before its result arrived".into()));
                let _ = sender.send(reply);
            });
        if let Err(error) = thread {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let now = Instant::now();
        Ok(Self {
            child,
            receiver,
            poll_at: now + Duration::from_millis(120),
            end: now + JOB_LIMIT,
        })
    }

    pub fn poll(&mut self, now: Instant) -> Option<Result<Reply, String>> {
        self.poll_at = now + Duration::from_millis(120);
        match self.receiver.try_recv() {
            Ok(reply) => Some(reply),
            Err(TryRecvError::Disconnected) => Some(Err("The sync worker stopped".into())),
            Err(TryRecvError::Empty) if now >= self.end => {
                Some(Err("Progress sync timed out".into()))
            }
            Err(TryRecvError::Empty) => None,
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
pub(crate) mod tests;
