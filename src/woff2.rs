//! Run WOFF 2 reconstruction in the separately resource-limited helper.
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock, mpsc},
    time::{Duration, Instant},
};

const MAX_FONT: usize = 2 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 32;
const FAILED_TTL: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

struct Entry {
    digest: [u8; 32],
    limit: usize,
    value: Option<Arc<Vec<u8>>>,
    retry_after: Instant,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn helper() -> std::io::Result<PathBuf> {
    // This override is available only to debug builds on a non-Linux host.
    // Its helper accepts only the pinned harmless simulator fixture.
    #[cfg(all(debug_assertions, not(target_os = "linux")))]
    if let Some(path) = std::env::var_os("READER_WOFF2_TEST_HELPER") {
        return Ok(path.into());
    }
    let exe = std::env::current_exe()?;
    Ok(exe
        .parent()
        .ok_or_else(|| std::io::Error::other("Reader executable has no directory"))?
        .join("reader-woff2-worker"))
}

/// A deadline is shared by all WOFF 2 faces encountered in one chapter.
/// If a helper is busy, unavailable, times out, or returns invalid output,
/// the caller falls through to the next publisher-specified font source.
pub(crate) fn decode(input: &[u8], limit: usize, deadline: Instant) -> Option<Vec<u8>> {
    if input.len() > MAX_FONT || !input.starts_with(b"wOF2") || limit == 0 || limit > MAX_FONT {
        return None;
    }
    let digest: [u8; 32] = ring::digest::digest(&ring::digest::SHA256, input)
        .as_ref()
        .try_into()
        .ok()?;
    // Do not block the UI behind a decoder already serving another layout.
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(Cache::default()))
        .try_lock()
        .ok()?;
    if let Some(index) = cache.entries.iter().position(|entry| {
        entry.digest == digest
            && entry.limit == limit
            && (entry.value.is_some() || Instant::now() < entry.retry_after)
    }) {
        let entry = cache.entries.remove(index)?;
        let value = entry.value.as_ref().map(|bytes| (**bytes).clone());
        cache.entries.push_back(entry);
        return value;
    }
    if Instant::now() >= deadline {
        return None;
    }
    let mut command = Command::new(helper().ok()?);
    command.env_clear();
    #[cfg(all(debug_assertions, not(target_os = "linux")))]
    command.arg("--host-fixture-test");
    let result = run(command, input, limit, deadline)
        .ok()
        .and_then(|output| {
            if skrifa::FontRef::new(&output).is_err() {
                None
            } else {
                Some(Arc::new(output))
            }
        });
    let bytes = result.as_ref().map_or(0, |data| data.len());
    while cache.entries.len() >= MAX_CACHE_ENTRIES
        || cache.bytes.saturating_add(bytes) > MAX_CACHE_BYTES
    {
        let Some(old) = cache.entries.pop_front() else {
            break;
        };
        cache.bytes = cache
            .bytes
            .saturating_sub(old.value.as_ref().map_or(0, |data| data.len()));
    }
    cache.bytes += bytes;
    cache.entries.push_back(Entry {
        digest,
        limit,
        value: result.clone(),
        retry_after: Instant::now() + FAILED_TTL,
    });
    result.map(|data| (*data).clone())
}

struct Guard(Child, bool);
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.1 {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

enum Message {
    Input(std::io::Result<()>),
    Output(std::io::Result<Vec<u8>>),
}

fn run(
    mut command: Command,
    input: &[u8],
    limit: usize,
    deadline: Instant,
) -> std::io::Result<Vec<u8>> {
    if Instant::now() >= deadline || input.len() > MAX_FONT || limit == 0 || limit > MAX_FONT {
        return Err(std::io::Error::other("WOFF 2 budget exceeded"));
    }
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut child = Guard(child, false);
    let mut stdin = child
        .0
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("worker input unavailable"))?;
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("worker output unavailable"))?;
    let mut payload = Vec::with_capacity(12 + input.len());
    payload.extend_from_slice(b"W2H1");
    payload.extend_from_slice(&(limit as u32).to_be_bytes());
    payload.extend_from_slice(&(input.len() as u32).to_be_bytes());
    payload.extend_from_slice(input);
    let (sender, receiver) = mpsc::channel();
    let input_sender = sender.clone();
    std::thread::Builder::new()
        .name("woff2-input".into())
        .spawn(move || {
            let result = stdin.write_all(&payload);
            drop(stdin);
            let _ = input_sender.send(Message::Input(result));
        })?;
    std::thread::Builder::new()
        .name("woff2-output".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(Message::Output(result));
        })?;
    let mut input_done = false;
    let mut output = None;
    let mut status = None;
    loop {
        while let Ok(message) = receiver.try_recv() {
            match message {
                Message::Input(result) => {
                    result?;
                    input_done = true;
                }
                Message::Output(result) => {
                    let bytes = result?;
                    if bytes.len() > limit {
                        return Err(std::io::Error::other("worker output exceeds limit"));
                    }
                    output = Some(bytes);
                }
            }
        }
        if status.is_none() {
            status = child.0.try_wait()?;
            child.1 = status.is_some();
        }
        if let (true, Some(status), Some(_)) = (input_done, status.as_ref(), output.as_ref()) {
            if !status.success() {
                return Err(std::io::Error::other("WOFF 2 worker failed"));
            }
            return Ok(output.unwrap());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::other("WOFF 2 worker timed out"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests;
