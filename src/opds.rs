//! Explicit catalog requests and isolated acquisition workers; no library data is uploaded.
mod model;
mod storage;
pub(crate) use model::{Entry, Feed, Link, Parsed, checked_url, parse, search_url};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
pub(crate) use storage::{Catalog, MAX_DOWNLOAD, Settings, discard, finalize, folder, temporary};
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Auth {
    pub origin: String,
    pub username: String,
    pub password: String,
}
impl Auth {
    pub fn catalog(catalog: &Catalog) -> Option<Self> {
        if catalog.username.is_empty() && catalog.password.is_empty() {
            return None;
        }
        Some(Self {
            origin: checked_url(&catalog.url)
                .ok()?
                .origin()
                .ascii_serialization(),
            username: catalog.username.clone(),
            password: catalog.password.clone(),
        })
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum Request {
    Feed {
        url: String,
        auth: Option<Auth>,
    },
    Download {
        link: Link,
        auth: Option<Auth>,
        root: PathBuf,
        temporary: PathBuf,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum Reply {
    Catalog(Parsed),
    Downloaded { bytes: u64 },
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(ABC[(a >> 2) as usize] as char);
        out.push(ABC[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            ABC[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ABC[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn redirect(current: &Url, location: &str) -> Result<Url, String> {
    let target = checked_url(
        current
            .join(location)
            .map_err(|_| "The server redirect address is invalid")?
            .as_str(),
    )?;
    if current.scheme() == "https" && target.scheme() != "https" {
        return Err("This catalog redirected from HTTPS to an insecure address".into());
    }
    Ok(target)
}
fn fetch(
    url: &str,
    auth: Option<&Auth>,
    seconds: u64,
) -> Result<(ureq::http::Response<ureq::Body>, Url), String> {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(seconds)))
            .user_agent("PrimitiveOS Reader OPDS/1")
            .build(),
    );
    let mut current = checked_url(url)?;
    for _ in 0..6 {
        let mut request = agent.get(current.as_str()).header("Accept-Encoding","identity")
            .header("Accept","application/opds+json, application/atom+xml, application/opensearchdescription+xml, application/epub+zip, application/pdf, */*;q=0.1");
        if let Some(auth) =
            auth.filter(|auth| auth.origin == current.origin().ascii_serialization())
        {
            if auth.username.len() > 1024
                || auth.password.len() > 4096
                || auth.username.contains(':')
            {
                return Err("The catalog credentials are invalid".into());
            }
            request = request.header(
                "Authorization",
                &format!(
                    "Basic {}",
                    base64(format!("{}:{}", auth.username, auth.password).as_bytes())
                ),
            );
        }
        let response = request.call().map_err(
            |_| "Could not reach this catalog; check Wi-Fi, its address and the device clock",
        )?;
        match response.status().as_u16() {
            200 => return Ok((response, current)),
            301 | 302 | 303 | 307 | 308 => {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or("The server redirect has no address")?;
                let target = redirect(&current, location)?;
                current = target;
            }
            401 | 403 => {
                return Err(
                    "This catalog needs valid credentials; edit its username and password".into(),
                );
            }
            404 => return Err("This catalog or publication was not found".into()),
            429 => return Err("This catalog is busy; try again later".into()),
            status => return Err(format!("The catalog returned HTTP {status}")),
        }
    }
    Err("The catalog redirected too many times".into())
}
fn run(request: &Request) -> Result<Reply, String> {
    match request {
        Request::Feed { url, auth } => {
            let (mut response, final_url) = fetch(url, auth.as_ref(), 20)?;
            if response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .is_some_and(|n| n > model::MAX_FEED as u64)
            {
                return Err("The catalog exceeds the 2 MiB response limit".into());
            }
            let bytes = response
                .body_mut()
                .with_config()
                .limit(model::MAX_FEED as u64 + 1)
                .read_to_vec()
                .map_err(|_| "The catalog response is incomplete or too large")?;
            Ok(Reply::Catalog(parse(&bytes, final_url.as_str())?))
        }
        Request::Download {
            link,
            auth,
            root,
            temporary,
        } => {
            let extension = link
                .extension()
                .ok_or("This acquisition needs another reader or service")?;
            let (mut response, _) = fetch(&link.url, auth.as_ref(), 150)?;
            if let Some(encoding) = response
                .headers()
                .get("content-encoding")
                .and_then(|v| v.to_str().ok())
                && !encoding.eq_ignore_ascii_case("identity")
            {
                return Err("The server ignored the uncompressed download request".into());
            }
            let actual = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(model::mime)
                .unwrap_or("");
            if matches!(actual, "text/html" | "application/xhtml+xml")
                && !matches!(extension, "html" | "xhtml")
            {
                return Err(
                    "The server returned a web page instead of the selected publication".into(),
                );
            }
            let length = response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.parse::<u64>())
                .transpose()
                .map_err(|_| "The download size is invalid")?;
            let mut created = false;
            let result = (|| {
                let mut file = storage::create_temp(root, temporary)
                    .map_err(|_| "Could not create a staging file in the selected folder")?;
                created = true;
                let bytes = storage::copy_response(
                    response
                        .body_mut()
                        .with_config()
                        .limit(MAX_DOWNLOAD + 1)
                        .reader(),
                    &mut file,
                    length,
                )?;
                file.sync_all()
                    .map_err(|_| "Could not finish saving this publication")?;
                drop(file);
                storage::validate_document(&root.join(temporary), extension)?;
                Ok(Reply::Downloaded { bytes })
            })();
            if result.is_err() && created {
                discard(root, temporary);
            }
            result
        }
    }
}
/// Private stdin/stdout protocol keeps passwords out of command lines and logs.
pub fn worker() -> crate::Result<()> {
    let request: Request = serde_json::from_reader(io::stdin().lock().take(64 * 1024))?;
    let reply = run(&request);
    serde_json::to_writer(io::stdout().lock(), &reply)?;
    Ok(())
}

pub(crate) struct Job {
    child: Child,
    receiver: Receiver<Result<Reply, String>>,
    pub poll_at: Instant,
    end: Instant,
    cleanup: Option<(PathBuf, PathBuf)>,
}
impl Job {
    pub fn start(request: &Request) -> crate::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--opds-worker");
        Self::spawn(command, request)
    }
    pub(crate) fn spawn(mut command: Command, request: &Request) -> crate::Result<Self> {
        match request {
            Request::Feed { url, .. } => {
                checked_url(url)?;
            }
            Request::Download {
                link,
                root,
                temporary,
                ..
            } => {
                checked_url(&link.url)?;
                link.extension().ok_or("This acquisition is unavailable")?;
                if !storage::owned_temp(temporary) {
                    return Err("The staging path is invalid".into());
                }
                folder(
                    root,
                    temporary
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("")),
                )?;
                if std::fs::symlink_metadata(root.join(temporary)).is_ok() {
                    return Err("The staging file already exists; start a new download".into());
                }
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let payload = serde_json::to_vec(request)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        if let Err(error) = std::thread::Builder::new()
            .name("opds-result".into())
            .spawn(move || {
                let reply = (|| {
                    input.write_all(&payload)?;
                    drop(input);
                    serde_json::from_reader(output.take(16 * 1024 * 1024)).map_err(io::Error::other)
                })()
                .unwrap_or_else(|_| {
                    Err("The catalog worker stopped before completing the request".into())
                });
                let _ = sender.send(reply);
            })
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let now = Instant::now();
        Ok(Self {
            child,
            receiver,
            poll_at: now + Duration::from_millis(120),
            end: now
                + Duration::from_secs(if matches!(request, Request::Download { .. }) {
                    180
                } else {
                    30
                }),
            cleanup: if let Request::Download {
                root, temporary, ..
            } = request
            {
                Some((root.clone(), temporary.clone()))
            } else {
                None
            },
        })
    }
    pub fn poll(&mut self, now: Instant) -> Option<Result<Reply, String>> {
        self.poll_at = now + Duration::from_millis(120);
        match self.receiver.try_recv() {
            Ok(result) => {
                if matches!(&result, Ok(Reply::Downloaded { .. })) {
                    self.cleanup = None;
                }
                Some(result)
            }
            Err(TryRecvError::Disconnected) => Some(Err("The catalog worker stopped".into())),
            Err(TryRecvError::Empty) if now >= self.end => {
                Some(Err("The catalog request timed out".into()))
            }
            Err(TryRecvError::Empty) => None,
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some((root, temporary)) = &self.cleanup {
            discard(root, temporary);
        }
    }
}
#[cfg(test)]
pub(crate) mod tests;
