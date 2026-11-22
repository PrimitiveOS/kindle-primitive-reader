//! Independent, opt-in KOReader sync-server protocol. Fixed PDF source pages only.
//! Credentials and request bodies travel over the worker's stdin, never argv.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
use url::Url;

const LIMIT: u64 = 32 * 1024;
const CONFIG_LIMIT: u64 = 16 * 1024;
pub(crate) const DEFAULT_SERVER: &str = "https://sync.koreader.rocks/";
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum Matching {
    #[default]
    Binary,
    Filename,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Config {
    pub enabled: bool,
    pub authorized: bool,
    pub allow_local_http: bool,
    pub server: String,
    pub username: String,
    pub key: String,
    pub device: String,
    pub device_id: String,
    pub matching: Matching,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            authorized: false,
            allow_local_http: false,
            server: DEFAULT_SERVER.into(),
            username: String::new(),
            key: String::new(),
            device: "Kindle".into(),
            device_id: String::new(),
            matching: Matching::Binary,
        }
    }
}
fn text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}
impl Config {
    pub(crate) fn origin(&self) -> Result<Url, String> {
        if self.server.len() > 2048 {
            return Err("The server URL is too long".into());
        }
        let mut url =
            Url::parse(self.server.trim()).map_err(|_| "Enter the KOReader server URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Use an HTTP or HTTPS server URL without credentials or query parameters".into(),
            );
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
                return Err("Use HTTPS, or allow HTTP for a local-network server".into());
            }
        }
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(url)
    }
    fn account(&self) -> Result<(), String> {
        self.origin()?;
        if !text(&self.username, 512)
            || self.key.len() != 32
            || !self
                .key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Enter a username and password, then sign in".into());
        }
        if !text(&self.device, 256)
            || self.device.chars().count() > 64
            || !text(&self.device_id, 128)
        {
            return Err("Enter a device name before syncing".into());
        }
        Ok(())
    }
    pub(crate) fn ready(&self) -> Result<(), String> {
        self.account()?;
        if !self.enabled || !self.authorized {
            return Err("Sign in and turn on KOReader sync first".into());
        }
        Ok(())
    }
    pub(crate) fn load(path: &Path) -> Self {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return Self::default();
        };
        if !meta.is_file() || meta.len() > CONFIG_LIMIT {
            return Self::default();
        }
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub(crate) fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        if bytes.len() as u64 > CONFIG_LIMIT {
            return Err(io::Error::other(
                "KOReader account exceeds its storage limit",
            ));
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_nanos());
        let staged = path.with_extension(format!("kosync-{}-{stamp}.tmp", std::process::id()));
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
            let _ = fs::remove_file(staged);
        }
        result
    }
}
pub(crate) fn password_key(password: &str) -> Result<String, String> {
    if password.trim().is_empty() || password.len() > 2048 {
        return Err("Enter a password of at most 2048 bytes".into());
    }
    Ok(format!("{:x}", md5::compute(password.as_bytes())))
}
pub(crate) fn device_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{:x}",
        md5::compute(format!("PrimitiveOS:{}:{now}", std::process::id()))
    )
}
pub(crate) fn document(path: &Path, matching: Matching) -> io::Result<String> {
    if matching == Matching::Filename {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| io::Error::other("The filename is not valid UTF-8"))?;
        return Ok(format!("{:x}", md5::compute(name.as_bytes())));
    }
    let mut file = fs::File::open(path)?;
    let mut hash = md5::Context::new();
    let mut bytes = [0; 1024];
    for offset in [
        0, 1024, 4096, 16384, 65536, 262144, 1048576, 4194304, 16777216, 67108864, 268435456,
        1073741824,
    ] {
        file.seek(SeekFrom::Start(offset))?;
        let mut count = 0;
        while count < bytes.len() {
            let n = file.read(&mut bytes[count..])?;
            if n == 0 {
                break;
            }
            count += n;
        }
        if count == 0 {
            break;
        }
        hash.consume(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn digest(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct Position {
    pub progress: String,
    pub percentage: f64,
    pub device: String,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,
}
impl Position {
    pub(crate) fn pdf(page: usize, count: usize, config: &Config) -> Result<Self, String> {
        if count == 0 || page >= count {
            return Err("The source page is outside this PDF".into());
        }
        Ok(Self {
            progress: (page + 1).to_string(),
            percentage: (((page + 1) as f64 / count as f64) * 10000.).floor() / 10000.,
            device: config.device.clone(),
            device_id: config.device_id.clone(),
            timestamp: None,
        })
    }
    pub(crate) fn page(&self, count: usize) -> Result<usize, String> {
        if !self.percentage.is_finite()
            || !(0.0..=1.0).contains(&self.percentage)
            || !text(&self.device, 256)
            || !text(&self.device_id, 128)
            || self
                .timestamp
                .is_some_and(|n| !n.is_finite() || n < 0. || n > 1e16)
        {
            return Err("The server returned invalid progress fields".into());
        }
        if self.progress.is_empty()
            || self.progress.len() > 10
            || !self.progress.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("This server position is not a PDF source page".into());
        }
        let page = self
            .progress
            .parse::<usize>()
            .ok()
            .and_then(|p| p.checked_sub(1))
            .filter(|p| *p < count)
            .ok_or("The server page is outside this PDF")?;
        Ok(page)
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) enum Operation {
    Register,
    Authorize,
    Fetch {
        document: String,
        count: usize,
    },
    Push {
        document: String,
        position: Position,
        count: usize,
    },
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Request {
    pub config: Config,
    pub operation: Operation,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum Reply {
    Authorized,
    Missing,
    Found(Position),
    Stored(Position),
    Changed(Option<Position>),
}
impl Request {
    fn url(&self) -> Result<Url, String> {
        self.config.account()?;
        let base = self.config.origin()?;
        let path = match &self.operation {
            Operation::Register => "users/create".to_owned(),
            Operation::Authorize => "users/auth".to_owned(),
            Operation::Fetch { document, count }
            | Operation::Push {
                document, count, ..
            } => {
                self.config.ready()?;
                if !digest(document) || *count == 0 || *count > 10_000_000 {
                    return Err("The local PDF identity or page count is invalid".into());
                }
                if let Operation::Push { position, .. } = &self.operation {
                    position.page(*count)?;
                }
                if matches!(self.operation, Operation::Fetch { .. }) {
                    format!("syncs/progress/{document}")
                } else {
                    "syncs/progress".into()
                }
            }
        };
        let url = base.join(&path).map_err(|_| "The server URL is invalid")?;
        if url.origin() != base.origin() {
            return Err("The request origin changed".into());
        }
        Ok(url)
    }
}
fn error_status(code: u16) -> String {
    match code {
        401 | 403 => "The KOReader server rejected these credentials".into(),
        300..=399 => "Use the server's final URL; redirects are not followed".into(),
        202 => "The server has not confirmed this update".into(),
        _ => format!("The KOReader server returned HTTP {code}"),
    }
}
fn body(mut response: ureq::http::Response<ureq::Body>, expected: u16) -> Result<Vec<u8>, String> {
    if response.status().as_u16() != expected {
        return Err(error_status(response.status().as_u16()));
    }
    response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|_| "The server reply is too large or incomplete".into())
}
fn position(bytes: &[u8], count: usize) -> Result<Option<Position>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "The server reply is not valid JSON")?;
    let object = value
        .as_object()
        .ok_or("The server position must be a JSON object")?;
    if !object.contains_key("percentage") {
        return Ok(None);
    }
    let p: Position =
        serde_json::from_value(value).map_err(|_| "The server returned malformed progress")?;
    p.page(count)?;
    Ok(Some(p))
}
pub(crate) fn execute(request: &Request) -> Result<Reply, String> {
    let url = request.url()?;
    let c = &request.config;
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(15)))
            .build(),
    );
    let network = |_: ureq::Error| {
        "Could not reach the KOReader server. Check Wi-Fi, its URL and the device clock."
            .to_string()
    };
    let fetch = |document: &str, count: usize| -> Result<Option<Position>, String> {
        let mut next = request.clone();
        next.operation = Operation::Fetch {
            document: document.into(),
            count,
        };
        let url = next.url()?;
        let reply = agent
            .get(url.as_str())
            .header("Accept", "application/vnd.koreader.v1+json")
            .header("x-auth-user", &c.username)
            .header("x-auth-key", &c.key)
            .call()
            .map_err(network)?;
        position(&body(reply, 200)?, count)
    };
    match &request.operation {
        Operation::Register => {
            let bytes =
                serde_json::to_vec(&serde_json::json!({"username":c.username,"password":c.key}))
                    .map_err(|_| "The account request could not be encoded")?;
            let reply = agent
                .post(url.as_str())
                .header("Accept", "application/vnd.koreader.v1+json")
                .header("Content-Type", "application/json")
                .send(bytes)
                .map_err(network)?;
            body(reply, 201)?;
            Ok(Reply::Authorized)
        }
        Operation::Authorize => {
            let reply = agent
                .get(url.as_str())
                .header("Accept", "application/vnd.koreader.v1+json")
                .header("x-auth-user", &c.username)
                .header("x-auth-key", &c.key)
                .call()
                .map_err(network)?;
            body(reply, 200)?;
            Ok(Reply::Authorized)
        }
        Operation::Fetch { document, count } => Ok(match fetch(document, *count)? {
            Some(p) => Reply::Found(p),
            None => Reply::Missing,
        }),
        Operation::Push {
            document,
            position,
            count,
        } => {
            let payload = serde_json::json!({"document":document,"progress":position.progress,"percentage":position.percentage,"device":position.device,"device_id":position.device_id});
            let reply = agent
                .put(url.as_str())
                .header("Accept", "application/vnd.koreader.v1+json")
                .header("Content-Type", "application/json")
                .header("x-auth-user", &c.username)
                .header("x-auth-key", &c.key)
                .send(
                    serde_json::to_vec(&payload)
                        .map_err(|_| "The position could not be encoded")?,
                )
                .map_err(network)?;
            body(reply, 200)?;
            let remote = fetch(document, *count)?;
            Ok(match remote {
                Some(ref p)
                    if p.progress == position.progress
                        && p.device_id == position.device_id
                        && p.device == position.device =>
                {
                    Reply::Stored(p.clone())
                }
                other => Reply::Changed(other),
            })
        }
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
    pub(crate) fn start(request: &Request) -> crate::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--kosync-worker");
        Self::spawn(command, request)
    }
    pub(crate) fn spawn(mut command: Command, request: &Request) -> crate::Result<Self> {
        request.url().map_err(io::Error::other)?;
        let payload = serde_json::to_vec(request)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut input = child.stdin.take().expect("piped input");
        let output = child.stdout.take().expect("piped output");
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("kosync".into())
            .spawn(move || {
                let result = (|| {
                    input.write_all(&payload)?;
                    drop(input);
                    serde_json::from_reader(output.take(LIMIT + 4096)).map_err(io::Error::other)
                })()
                .unwrap_or_else(|_| {
                    Err("The KOReader sync worker stopped before returning a result".into())
                });
                let _ = sender.send(result);
            });
        if let Err(e) = thread {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.into());
        }
        let now = Instant::now();
        Ok(Self {
            child,
            receiver,
            poll_at: now + Duration::from_millis(120),
            end: now + Duration::from_secs(40),
        })
    }
    pub(crate) fn poll(&mut self, now: Instant) -> Option<Result<Reply, String>> {
        self.poll_at = now + Duration::from_millis(120);
        match self.receiver.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Disconnected) => Some(Err("The KOReader sync worker stopped".into())),
            Err(TryRecvError::Empty) if now >= self.end => {
                Some(Err("KOReader sync timed out".into()))
            }
            _ => None,
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
