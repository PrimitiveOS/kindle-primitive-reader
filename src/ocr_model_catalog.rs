//! Explicit, license-attributed downloads for offline OCR language packs.
//!
//! The catalog is generated from the top-level model files in the pinned
//! Tesseract `tessdata_fast` source tree. Models are downloaded only after a
//! user's selection and remain separate from the Reader source package.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub(crate) const SOURCE_REVISION: &str = "65727574dfcd264acbb0c3e07860e4e9e9b22185";
const SOURCE_BASE: &str = "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast";
const MAX_MODEL_BYTES: u64 = 16 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const JOB_TIMEOUT: Duration = Duration::from_secs(180);
const POLL_INTERVAL: Duration = Duration::from_millis(150);
const WORKER_REPLY_LIMIT: u64 = 8 * 1024;
const DOWNLOAD_PROGRESS_BYTES: u64 = 512 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(750);
const APACHE_LICENSE: &str = include_str!("../docs/licenses/ocr/tessdata_fast-LICENSE");

#[derive(Clone, Copy)]
pub(crate) struct Model {
    pub(crate) code: &'static str,
    pub(crate) bytes: u64,
}

// Sizes are Git tree object sizes at SOURCE_REVISION. `equ` is a mathematical
// model and `osd` is the orientation/script detector, not a reading language.
const MODEL_SIZES: &str = "afr 2652786\namh 5470094\nara 1432056\nasm 2045427\naze 3524799\naze_cyrl 1943672\nbel 3692098\nben 855841\nbod 1966440\nbos 2498242\nbre 6335007\nbul 1675212\ncat 1146012\nceb 716214\nces 3795684\nchi_sim 2469156\nchi_sim_vert 1927902\nchi_tra 2366642\nchi_tra_vert 1824756\nchr 374944\ncos 2299099\ncym 2208919\ndan 2580059\ndeu 1525436\ndiv 1774535\ndzo 449596\nell 1419514\neng 4113088\nenm 3101551\nepo 4728393\nest 4458101\neus 5175921\nfao 3439772\nfas 431500\nfil 1841715\nfin 7865732\nfra 1130365\nfrk 6423052\nfrm 2024948\nfry 1906018\ngla 3068307\ngle 1181824\nglg 2554555\ngrc 2246328\nguj 1418394\nhat 1976902\nheb 961404\nhin 1122751\nhrv 4103348\nhun 5296273\nhye 3463717\niku 2802266\nind 1122661\nisl 2278973\nita 2701314\nita_old 3286123\njav 2982741\njpn 2471260\njpn_vert 3037480\nkan 3608331\nkat 2524769\nkat_old 423167\nkaz 4734644\nkhm 1446926\nkir 9928497\nkmr 3568615\nkor 1677415\nkor_vert 1113715\nlao 6386744\nlat 3187463\nlav 2717247\nlit 3154896\nltz 2606426\nmal 5275996\nmar 2118233\nmkd 1600188\nmlt 2308796\nmon 2137042\nmri 862973\nmsa 1747801\nmya 4640561\nnep 1002911\nnld 6050296\nnor 3610079\noci 6322087\nori 1480066\npan 497721\npol 4765518\npor 1982756\npus 1772087\nque 5026336\nron 2376323\nrus 3861738\nsan 12404677\nsin 1727413\nslk 4427661\nslv 3003829\nsnd 1694035\nspa 2294433\nspa_old 2894755\nsqi 1874705\nsrp 2149931\nsrp_latn 3281787\nsun 1369500\nswa 2167651\nswe 4167034\nsyr 2207208\ntam 3237963\ntat 1072896\ntel 2769654\ntgk 2602685\ntha 1072600\ntir 378822\nton 947249\ntur 4550554\nuig 2794272\nukr 3825102\nurd 1398718\nuzb 6470703\nuzb_cyrl 1563501\nvie 531275\nyid 545606\nyor 963400";

pub(crate) fn models() -> impl Iterator<Item = Model> {
    MODEL_SIZES.lines().filter_map(|line| {
        let (code, bytes) = line.trim().split_once(' ')?;
        let bytes = bytes.parse().ok()?;
        Some(Model { code, bytes })
    })
}

pub(crate) fn model(index: usize) -> Option<Model> {
    models().nth(index)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstalledState {
    Missing,
    Installed,
    Invalid,
}

pub(crate) fn installed_state(root: &Path, model: Model) -> InstalledState {
    match fs::symlink_metadata(model_path(root, model)) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == model.bytes =>
        {
            InstalledState::Installed
        }
        Ok(_) => InstalledState::Invalid,
        Err(error) if error.kind() == io::ErrorKind::NotFound => InstalledState::Missing,
        Err(_) => InstalledState::Invalid,
    }
}

#[derive(Serialize, Deserialize)]
struct WorkerRequest {
    index: usize,
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct WorkerReply {
    result: Result<(), String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DownloadProgress {
    received: u64,
    total: u64,
    installing: bool,
}

impl DownloadProgress {
    fn label(self) -> String {
        if self.installing {
            "Verifying and installing model…".into()
        } else {
            format!(
                "Downloaded {} of {}",
                format_download_size(self.received),
                format_download_size(self.total)
            )
        }
    }
}

enum WorkerEvent {
    Progress(DownloadProgress),
    Finished(Result<(), String>),
}

pub(crate) enum Poll {
    Pending { progress_changed: bool },
    Finished(Result<(), String>),
}

#[derive(Deserialize)]
struct ErrorResponse {
    message: String,
}

/// Child-process entry point. Network reads and storage writes stay out of the
/// Reader event loop, and a parent exit is observed as cancellation via EOF.
pub fn worker() -> crate::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > 4 * 1024 {
        return Err("OCR language request is invalid".into());
    }
    let mut payload = vec![0; length];
    input.read_exact(&mut payload)?;
    drop(input);
    let request: WorkerRequest = serde_json::from_slice(&payload)?;
    let canceled = Arc::new(AtomicBool::new(false));
    let monitor_canceled = Arc::clone(&canceled);
    let _monitor = thread::Builder::new()
        .name("ocr-model-cancel".into())
        .spawn(move || {
            let mut byte = [0; 1];
            let received = io::stdin().lock().read_exact(&mut byte).is_ok();
            if !received || byte[0] != 0 {
                monitor_canceled.store(true, Ordering::Release);
            }
        })?;
    let result = model(request.index)
        .ok_or_else(|| "That OCR language is not in the catalog".to_owned())
        .and_then(|model| download_and_install(model, &request.root, &canceled));
    serde_json::to_writer(io::stdout().lock(), &WorkerReply { result })?;
    Ok(())
}

fn model_path(root: &Path, model: Model) -> PathBuf {
    root.join(format!("{}.traineddata", model.code))
}

fn notice_path(root: &Path, model: Model) -> PathBuf {
    root.join(format!("{}.ATTRIBUTION.txt", model.code))
}

pub(crate) struct Job {
    index: usize,
    child: Option<Child>,
    input: Option<ChildStdin>,
    result: Receiver<WorkerEvent>,
    progress: Option<DownloadProgress>,
    poll_at: Instant,
    deadline: Instant,
    cancel_requested: bool,
}

impl Job {
    pub(crate) fn start(index: usize, root: PathBuf) -> crate::Result<Self> {
        model(index).ok_or("That OCR language is not in the catalog")?;
        let payload = serde_json::to_vec(&WorkerRequest { index, root })?;
        let length = u32::try_from(payload.len())
            .map_err(|_| "OCR language request is too large")?
            .to_be_bytes();
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--ocr-model-download-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut input = child.stdin.take().expect("piped OCR model request");
        let output = child.stdout.take().expect("piped OCR model response");
        let progress_output = child.stderr.take().expect("piped OCR model progress");
        if let Err(error) = input
            .write_all(&length)
            .and_then(|()| input.write_all(&payload))
            .and_then(|()| input.flush())
        {
            drop(input);
            reap_child(child);
            return Err(error.into());
        }
        let (sender, result) = mpsc::channel();
        let result_sender = sender.clone();
        if let Err(error) = thread::Builder::new()
            .name("ocr-model-download-result".into())
            .spawn(move || {
                let response = (|| -> std::io::Result<_> {
                    let reply: WorkerReply =
                        serde_json::from_reader(output.take(WORKER_REPLY_LIMIT))
                            .map_err(io::Error::other)?;
                    Ok(reply.result)
                })()
                .unwrap_or_else(|_| {
                    Err("OCR language download ended before installation completed".into())
                });
                let _ = result_sender.send(WorkerEvent::Finished(response));
            })
        {
            let _ = input.write_all(&[1]);
            drop(input);
            reap_child(child);
            return Err(error.into());
        }
        let progress_sender = sender.clone();
        if let Err(error) = thread::Builder::new()
            .name("ocr-model-download-progress".into())
            .spawn(move || {
                for line in BufReader::new(progress_output)
                    .lines()
                    .map_while(Result::ok)
                {
                    if let Some(progress) = parse_download_progress(&line) {
                        if progress_sender
                            .send(WorkerEvent::Progress(progress))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            })
        {
            let _ = input.write_all(&[1]);
            drop(input);
            reap_child(child);
            return Err(error.into());
        }
        drop(sender);
        let now = Instant::now();
        Ok(Self {
            index,
            child: Some(child),
            input: Some(input),
            result,
            progress: None,
            poll_at: now + POLL_INTERVAL,
            deadline: now + JOB_TIMEOUT,
            cancel_requested: false,
        })
    }

    pub(crate) fn model_index(&self) -> usize {
        self.index
    }

    pub(crate) fn progress_label(&self) -> Option<String> {
        self.progress.map(DownloadProgress::label)
    }

    pub(crate) fn cancel(&mut self) {
        if !self.cancel_requested {
            self.cancel_requested = true;
            if let Some(mut input) = self.input.take() {
                let _ = input.write_all(&[1]);
                let _ = input.flush();
            }
        }
    }

    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel_requested
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.poll_at
    }

    pub(crate) fn poll(&mut self, now: Instant) -> Poll {
        self.poll_at = now + POLL_INTERVAL;
        let mut progress_changed = false;
        loop {
            match self.result.try_recv() {
                Ok(WorkerEvent::Progress(progress)) => {
                    if self.progress != Some(progress) {
                        self.progress = Some(progress);
                        progress_changed = true;
                    }
                }
                Ok(WorkerEvent::Finished(result)) => {
                    self.reap_if_finished();
                    return Poll::Finished(result);
                }
                Err(TryRecvError::Disconnected) => {
                    self.reap_if_finished();
                    return Poll::Finished(Err(
                        "OCR language installer stopped unexpectedly".into(),
                    ));
                }
                Err(TryRecvError::Empty) if now >= self.deadline => {
                    self.cancel();
                    return Poll::Finished(Err("OCR language download timed out".into()));
                }
                Err(TryRecvError::Empty) => {
                    return Poll::Pending { progress_changed };
                }
            }
        }
    }

    fn reap_if_finished(&mut self) {
        if let Some(mut child) = self.child.take() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                _ => reap_child(child),
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        self.reap_if_finished();
    }
}

fn reap_child(mut child: Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = thread::Builder::new()
        .name("ocr-model-download-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

fn download_and_install(model: Model, root: &Path, canceled: &AtomicBool) -> Result<(), String> {
    if canceled.load(Ordering::Acquire) {
        return Err("OCR language download canceled".into());
    }
    let url = format!("{SOURCE_BASE}/{SOURCE_REVISION}/{}.traineddata", model.code);
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(3)
            .http_status_as_error(false)
            .user_agent("PrimitiveOS Reader OCR language installer")
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build(),
    );
    let mut response = agent
        .get(&url)
        .call()
        .map_err(|_| "Could not download the OCR language over HTTPS".to_owned())?;
    if response.status().as_u16() != 200 {
        let status = response.status().as_u16();
        let detail = response
            .body_mut()
            .with_config()
            .limit(4096)
            .read_to_string()
            .ok()
            .and_then(|text| serde_json::from_str::<ErrorResponse>(&text).ok())
            .map(|error| error.message);
        return Err(detail.unwrap_or_else(|| format!("OCR model source returned HTTP {status}")));
    }
    if response
        .body()
        .content_length()
        .is_some_and(|length| length != model.bytes)
    {
        return Err("The OCR model source returned an unexpected file size".into());
    }
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_MODEL_BYTES)
        .reader();
    primitive_app::with_storage(|| install_with_storage(root, model, &mut reader, canceled))
        .map_err(|error| error.to_string())
}

fn install_with_storage(
    root: &Path,
    model: Model,
    reader: &mut impl Read,
    canceled: &AtomicBool,
) -> io::Result<()> {
    check_cancelled(canceled)?;
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(io::Error::other("OCR model storage is not a directory")),
        Err(error) => return Err(error),
    }
    if installed_state(root, model) != InstalledState::Missing
        || fs::symlink_metadata(notice_path(root, model)).is_ok()
    {
        return Err(io::Error::other("This OCR language model already exists"));
    }
    if model.bytes == 0 || model.bytes > MAX_MODEL_BYTES {
        return Err(io::Error::other("The OCR model size is invalid"));
    }
    let stage = root.join(format!(
        ".install-{}-{}-{}",
        model.code,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&stage)?;
    let destination = model_path(root, model);
    let attribution = notice_path(root, model);
    let staged_model = stage.join(format!("{}.traineddata", model.code));
    let staged_attribution = stage.join(format!("{}.ATTRIBUTION.txt", model.code));
    let mut published_model = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_model)?;
        let mut total = 0u64;
        let mut chunk = [0; 32 * 1024];
        let mut last_reported = 0u64;
        let mut last_reported_at = Instant::now();
        loop {
            check_cancelled(canceled)?;
            let count = reader.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            total = total.saturating_add(count as u64);
            if total > model.bytes || total > MAX_MODEL_BYTES {
                return Err(io::Error::other(
                    "The OCR model exceeds its pinned source size",
                ));
            }
            file.write_all(&chunk[..count])?;
            if total.saturating_sub(last_reported) >= DOWNLOAD_PROGRESS_BYTES
                || last_reported_at.elapsed() >= DOWNLOAD_PROGRESS_INTERVAL
            {
                report_download_progress(total, model.bytes, false);
                last_reported = total;
                last_reported_at = Instant::now();
            }
        }
        if total != model.bytes {
            return Err(io::Error::other(
                "The OCR model does not match its pinned source size",
            ));
        }
        report_download_progress(total, model.bytes, false);
        file.flush()?;
        file.sync_all()?;
        report_download_progress(total, model.bytes, true);
        let attribution_text = format!(
            "Tesseract OCR language model: {}\nSource: {SOURCE_BASE}/{SOURCE_REVISION}/{}.traineddata\nSource revision: {SOURCE_REVISION}\nModel size: {} bytes\nLicense: Apache License 2.0\n\nThis model was installed by explicit user action. It is third-party model data and is not part of the PrimitiveOS Reader source license.\n\n{APACHE_LICENSE}",
            model.code, model.code, model.bytes,
        );
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_attribution)?
            .write_all(attribution_text.as_bytes())?;
        check_cancelled(canceled)?;
        fs::rename(&staged_model, &destination)?;
        published_model = true;
        check_cancelled(canceled)?;
        if let Err(error) = fs::rename(&staged_attribution, &attribution) {
            let _ = fs::remove_file(&destination);
            published_model = false;
            return Err(error);
        }
        Ok(())
    })();
    if let Err(error) = result {
        if published_model {
            let _ = fs::remove_file(&destination);
        }
        let _ = fs::remove_file(&attribution);
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    if let Err(error) = fs::remove_dir(&stage) {
        let _ = fs::remove_file(&destination);
        let _ = fs::remove_file(&attribution);
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    Ok(())
}

fn check_cancelled(canceled: &AtomicBool) -> io::Result<()> {
    if canceled.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "OCR language download canceled",
        ))
    } else {
        Ok(())
    }
}

fn report_download_progress(received: u64, total: u64, installing: bool) {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "OCR_MODEL_PROGRESS {received} {total} {installing}");
    let _ = stderr.flush();
}

fn parse_download_progress(line: &str) -> Option<DownloadProgress> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "OCR_MODEL_PROGRESS" {
        return None;
    }
    Some(DownloadProgress {
        received: fields.next()?.parse().ok()?,
        total: fields.next()?.parse().ok()?,
        installing: fields.next()?.parse().ok()?,
    })
}

fn format_download_size(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KiB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}
