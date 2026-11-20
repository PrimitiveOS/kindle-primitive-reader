//! Run the installed OCR engine from a private temporary directory.

use std::env;
use std::fs::{self, DirBuilder, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use primitive_app::with_storage;

use crate::Result;
use crate::fixed_text::PdfPageText;
use crate::ocr::{MAX_OCR_PIXELS, MAX_OCR_SIDE, MAX_TSV_BYTES, OcrError, parse_tsv};

const ENGINE: &str = "tesseract";
const MODEL: &str = "eng.traineddata";
const MAX_ENGINE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MODEL_BYTES: u64 = 16 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
static STAGING_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug)]
pub(crate) struct OcrAssets {
    directory: PathBuf,
}

impl OcrAssets {
    pub(crate) fn discover(home: &Path) -> Option<Self> {
        with_storage(|| {
            let assets = Self {
                directory: fs::canonicalize(home)?.join("ocr"),
            };
            assets.validate()?;
            Ok(assets)
        })
        .ok()
    }

    fn validate(&self) -> io::Result<()> {
        regular_directory(&self.directory)?;
        regular_directory(&self.directory.join("tessdata"))?;
        let engine = asset_metadata(&self.directory.join(ENGINE), MAX_ENGINE_BYTES)?;
        if engine.permissions().mode() & 0o111 == 0 {
            return Err(io::Error::other(
                "The installed OCR engine is not executable.",
            ));
        }
        asset_metadata(
            &self.directory.join("tessdata").join(MODEL),
            MAX_MODEL_BYTES,
        )?;
        Ok(())
    }

    fn stage(&self, runtime: &Path) -> io::Result<WorkDirectory> {
        let work = WorkDirectory::create(runtime)?;
        with_storage(|| {
            self.validate()?;
            let tessdata = work.0.join("tessdata");
            DirBuilder::new().mode(0o700).create(&tessdata)?;
            copy_asset(
                &self.directory.join(ENGINE),
                &work.0.join(ENGINE),
                MAX_ENGINE_BYTES,
                0o700,
            )?;
            copy_asset(
                &self.directory.join("tessdata").join(MODEL),
                &tessdata.join(MODEL),
                MAX_MODEL_BYTES,
                0o600,
            )
        })?;
        Ok(work)
    }
}

fn regular_directory(path: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(io::Error::other("The OCR directory is unavailable."));
    }
    Ok(())
}

fn asset_metadata(path: &Path, limit: u64) -> io::Result<Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
        return Err(io::Error::other(
            "An installed OCR file is invalid or too large.",
        ));
    }
    Ok(metadata)
}

fn copy_asset(source: &Path, destination: &Path, limit: u64, mode: u32) -> io::Result<()> {
    let expected = asset_metadata(source, limit)?;
    let input = File::open(source)?;
    let opened = input.metadata()?;
    if !opened.is_file()
        || opened.dev() != expected.dev()
        || opened.ino() != expected.ino()
        || opened.len() != expected.len()
    {
        return Err(io::Error::other(
            "An installed OCR file changed while staging.",
        ));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(destination)?;
    let size = io::copy(&mut input.take(limit + 1), &mut output)?;
    if size != expected.len() {
        return Err(io::Error::other(
            "An installed OCR file changed while staging.",
        ));
    }
    Ok(())
}

struct WorkDirectory(PathBuf);

impl WorkDirectory {
    fn create(base: &Path) -> io::Result<Self> {
        if !base.is_absolute() {
            return Err(io::Error::other(
                "The OCR runtime directory must be absolute.",
            ));
        }
        regular_directory(base)?;
        for _ in 0..16 {
            let counter = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = base.join(format!("reader-ocr-{}-{counter}", std::process::id()));
            match DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => return Ok(Self(directory)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::other(
            "Could not reserve an OCR runtime directory.",
        ))
    }
}

impl Drop for WorkDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) struct OcrJob {
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    result: Receiver<Result<PdfPageText>>,
}

impl OcrJob {
    pub(crate) fn start(
        assets: &OcrAssets,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let count = u64::from(width) * u64::from(height);
        if count == 0
            || count > u64::from(MAX_OCR_PIXELS)
            || width.max(height) > MAX_OCR_SIDE
            || count as usize != pixels.len()
        {
            return Err(OcrError::InvalidRaster.into());
        }
        let runtime = env::var_os("KL_NATIVE_APP_RUN")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        let staged = assets.stage(&runtime)?;
        Self::launch(staged, pixels.to_vec(), width, height, TIMEOUT)
    }

    fn launch(
        staged: WorkDirectory,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        timeout: Duration,
    ) -> Result<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancelled);
        let (send, result) = sync_channel(1);
        let worker = thread::Builder::new()
            .name("reader-ocr".to_owned())
            .spawn(move || {
                let outcome = recognize(&staged.0, pixels, width, height, &signal, timeout);
                drop(staged);
                let _ = send.send(outcome);
            })?;
        Ok(Self {
            cancelled,
            worker: Some(worker),
            result,
        })
    }

    pub(crate) fn poll(&mut self) -> Result<Option<PdfPageText>> {
        if self.worker.is_none() {
            return Ok(None);
        }
        match self.result.try_recv() {
            Ok(outcome) => {
                self.join();
                outcome.map(Some)
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.join();
                Err("The OCR worker stopped unexpectedly.".into())
            }
        }
    }

    fn join(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for OcrJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.join();
    }
}

struct RunningChild {
    process: Child,
    exited: bool,
}

impl RunningChild {
    fn check_exit(&mut self) -> Result<()> {
        if !self.exited
            && let Some(status) = self.process.try_wait()?
        {
            self.exited = true;
            if !status.success() {
                return Err("The OCR engine could not recognize this page.".into());
            }
        }
        Ok(())
    }
}

impl Drop for RunningChild {
    fn drop(&mut self) {
        if !self.exited {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn recognize(
    directory: &Path,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    cancelled: &AtomicBool,
    timeout: Duration,
) -> Result<PdfPageText> {
    if cancelled.load(Ordering::Acquire) {
        return Err("OCR was cancelled.".into());
    }
    let deadline = Instant::now() + timeout;
    let child = Command::new(directory.join(ENGINE))
        .args(["stdin", "stdout", "--tessdata-dir"])
        .arg(directory.join("tessdata"))
        .args([
            "-l",
            "eng",
            "--oem",
            "1",
            "--psm",
            "3",
            "-c",
            "tessedit_create_tsv=1",
        ])
        .current_dir(directory)
        .env_clear()
        .env("LC_ALL", "C")
        .env("OMP_THREAD_LIMIT", "1")
        .env("TMPDIR", directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut child = RunningChild {
        process: child,
        exited: false,
    };
    let mut stdin = child
        .process
        .stdin
        .take()
        .ok_or("OCR input pipe is unavailable.")?;
    let mut stdout = child
        .process
        .stdout
        .take()
        .ok_or("OCR output pipe is unavailable.")?;
    let mut writer = Some(
        thread::Builder::new()
            .name("reader-ocr-input".to_owned())
            .spawn(move || -> io::Result<()> {
                write!(stdin, "P5\n{width} {height}\n255\n")?;
                stdin.write_all(&pixels)
            })?,
    );
    let mut reader = Some(
        thread::Builder::new()
            .name("reader-ocr-output".to_owned())
            .spawn(move || read_output(&mut stdout))?,
    );
    let mut bytes = None;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err("OCR was cancelled.".into());
        }
        let now = Instant::now();
        if now >= deadline {
            return Err("OCR timed out on this page.".into());
        }
        if reader.as_ref().is_some_and(JoinHandle::is_finished) {
            bytes = Some(
                reader
                    .take()
                    .unwrap()
                    .join()
                    .map_err(|_| "OCR output transfer failed.")??,
            );
        }
        child.check_exit()?;
        if writer.as_ref().is_some_and(JoinHandle::is_finished) {
            writer
                .take()
                .unwrap()
                .join()
                .map_err(|_| "OCR input transfer failed.")??;
        }
        if child.exited
            && writer.is_none()
            && let Some(bytes) = bytes.take()
        {
            return Ok(parse_tsv(&bytes, width, height)?.text);
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
    }
}

fn read_output(input: &mut impl Read) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut block = [0; 8192];
    loop {
        let count = match input.read(&mut block) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            outcome => outcome?,
        };
        if count == 0 {
            break;
        }
        if output.len() + count > MAX_TSV_BYTES {
            return Err(OcrError::OutputTooLarge.into());
        }
        output.extend_from_slice(&block[..count]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const SUCCESS_SCRIPT: &str = r#"#!/bin/sh
[ "$#" -eq 12 ] && [ "$1" = stdin ] && [ "$2" = stdout ] || exit 1
[ "$3" = --tessdata-dir ] && [ -s "$4/eng.traineddata" ] || exit 1
[ "$5" = -l ] && [ "$6" = eng ] && [ "${11}" = -c ] && [ "${12}" = tessedit_create_tsv=1 ] || exit 1
[ "$OMP_THREAD_LIMIT" = 1 ] || exit 1
IFS= read -r magic && IFS= read -r size && IFS= read -r max || exit 1
[ "$magic" = P5 ] && [ "$size" = '2 2' ] && [ "$max" = 255 ] || exit 1
IFS= read -r pixels || [ "$pixels" = abcd ] || exit 1
[ "$pixels" = abcd ] || exit 1
printf '%b\n' 'level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext' '1\t1\t0\t0\t0\t0\t0\t0\t2\t2\t-1\t' '5\t1\t1\t1\t1\t1\t0\t0\t2\t2\t98\tTide'
"#;

    const BUSY_SCRIPT: &str = r#"#!/bin/sh
printf '%s\n' started > began
while :; do :; done
"#;

    fn install(root: &Path, engine: &str) -> OcrAssets {
        let home = root.join("installed");
        fs::create_dir_all(home.join("ocr/tessdata")).unwrap();
        fs::write(home.join("ocr/tessdata").join(MODEL), b"model").unwrap();
        let path = home.join("ocr").join(ENGINE);
        fs::write(&path, engine).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        OcrAssets::discover(&home).unwrap()
    }

    fn wait_for(job: &mut OcrJob) -> Result<PdfPageText> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(output) = job.poll()? {
                return Ok(output);
            }
            assert!(Instant::now() < deadline, "OCR worker test did not finish");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn stages_assets_and_streams_the_raster_to_a_fixed_cli() {
        let root = WorkDirectory::create(&env::temp_dir()).unwrap();
        let assets = install(&root.0, SUCCESS_SCRIPT);
        assert!(OcrJob::start(&assets, b"abc", 2, 2).is_err());
        let staged = assets.stage(&root.0).unwrap();
        let path = staged.0.clone();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let mut job =
            OcrJob::launch(staged, b"abcd".to_vec(), 2, 2, Duration::from_secs(2)).unwrap();
        let output = wait_for(&mut job).unwrap();
        assert_eq!(output.text, "Tide");
        assert_eq!(output.words[0].bytes, 0..4);
        assert!(!path.exists());
    }

    #[test]
    fn cancels_a_live_ocr_process_when_the_job_is_dropped() {
        let root = WorkDirectory::create(&env::temp_dir()).unwrap();
        let assets = install(&root.0, BUSY_SCRIPT);
        let staged = assets.stage(&root.0).unwrap();
        let path = staged.0.clone();
        let job = OcrJob::launch(staged, vec![0; 4], 2, 2, Duration::from_secs(3)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !path.join("began").is_file() {
            assert!(Instant::now() < deadline, "mock OCR engine did not start");
            thread::sleep(Duration::from_millis(5));
        }
        let began = Instant::now();
        drop(job);
        assert!(began.elapsed() < Duration::from_secs(1));
        assert!(!path.exists());
    }

    #[test]
    fn watchdog_times_out_independently_of_ui_polling() {
        let root = WorkDirectory::create(&env::temp_dir()).unwrap();
        let assets = install(&root.0, BUSY_SCRIPT);
        let staged = assets.stage(&root.0).unwrap();
        let path = staged.0.clone();
        let mut job = OcrJob::launch(staged, vec![0; 4], 2, 2, Duration::from_millis(80)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while path.exists() {
            assert!(
                Instant::now() < deadline,
                "OCR watchdog did not remove its working directory"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            wait_for(&mut job).unwrap_err().to_string(),
            "OCR timed out on this page."
        );
    }

    #[test]
    fn rejects_linked_assets_and_limits_engine_output() {
        let root = WorkDirectory::create(&env::temp_dir()).unwrap();
        let assets = install(&root.0, SUCCESS_SCRIPT);
        let model = assets.directory.join("tessdata").join(MODEL);
        fs::remove_file(&model).unwrap();
        symlink(assets.directory.join(ENGINE), &model).unwrap();
        assert!(OcrAssets::discover(assets.directory.parent().unwrap()).is_none());
        assert!(assets.stage(&root.0).is_err());
        let err = read_output(&mut io::repeat(b'x').take(MAX_TSV_BYTES as u64 + 1)).unwrap_err();
        assert_eq!(
            err.downcast_ref::<OcrError>(),
            Some(&OcrError::OutputTooLarge)
        );
    }
}
