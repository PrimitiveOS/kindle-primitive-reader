//! Local bookmarks, highlights and notes exchange with KOReader.

mod discovery;
mod discovery_job;
mod marks;
mod preview_worker;
#[cfg(test)]
mod tests;

use super::*;
use crate::koreader_sidecar::exchange;
use crate::library::BookState;
use primitive_ui::{Sheet, SheetAction};
use serde::{Deserialize, Serialize};

const SETTINGS_LIMIT: u64 = 16 * 1024;

#[derive(Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    export_on_close: bool,
    keep_all: bool,
    folder: Option<PathBuf>,
    device: Option<String>,
}

pub(super) struct State {
    path: PathBuf,
    settings: Preferences,
    preview: Option<Preview>,
    preview_job: Option<preview_worker::Job>,
    preview_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    preview_issue: Option<&'static str>,
    discovery: discovery_job::Manager,
    candidate: Option<Candidate>,
    announced: std::collections::VecDeque<(u64, String)>,
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    ToggleOnClose,
    ToggleKeepAll,
    DefaultFolder,
    Preview,
    PreviewBackground,
    CancelPreview,
    Apply,
    Export,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unavailable: usize,
}

struct Preview {
    id: u64,
    digest: String,
    summary: Summary,
    imported: bool,
    confirming: bool,
    background: Option<preview_worker::Reviewed>,
}

struct Target {
    path: PathBuf,
    source: PathBuf,
    adjacent: bool,
}

struct Candidate {
    book: u64,
    path: PathBuf,
    folder: Option<PathBuf>,
    digest: String,
    device: Option<String>,
}

struct Import {
    digest: String,
    summary: Summary,
    replacement: BookState,
}

impl State {
    pub(super) fn load(path: PathBuf) -> Self {
        let settings = read_import_file(&path, SETTINGS_LIMIT)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            settings,
            preview: None,
            preview_job: None,
            preview_busy: Default::default(),
            preview_issue: None,
            discovery: Default::default(),
            candidate: None,
            announced: Default::default(),
        }
    }

    fn save(&mut self, settings: Preferences) -> Result<()> {
        let bytes = serde_json::to_vec(&settings)?;
        if bytes.len() as u64 > SETTINGS_LIMIT {
            return Err("Choose an annotation folder with a shorter path".into());
        }
        primitive_app::save(&self.path, &bytes)?;
        self.settings = settings;
        Ok(())
    }

    fn device(&mut self) -> Result<String> {
        if let Some(device) = &self.settings.device {
            return Ok(device.clone());
        }
        let device = format!("PrimitiveOS-{}", random_token()?);
        let mut proposed = self.settings.clone();
        proposed.device = Some(device.clone());
        self.save(proposed)?;
        Ok(device)
    }

    pub(super) fn finish_preview(&mut self) {
        self.preview = None;
        self.preview_job = None;
        self.preview_issue = None;
    }

    pub(super) fn toggle(&self, action: Command) -> Option<bool> {
        match action {
            Command::ToggleOnClose => Some(self.settings.export_on_close),
            Command::ToggleKeepAll => Some(self.settings.keep_all),
            _ => None,
        }
    }
}

fn random_token() -> io::Result<String> {
    use ring::rand::SecureRandom;
    let mut id = [0; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut id)
        .map_err(|_| io::Error::other("Could not create an annotation exchange identity"))?;
    Ok(id.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn target_error(kind: io::ErrorKind, message: &'static str) -> io::Error {
    io::Error::new(kind, message)
}

fn transfer_digest(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl Reader {
    pub(super) fn annotation_exchange_settings(&self) -> Vec<Item> {
        let language = self.controls.interface_language;
        let mut rows = vec![
            Item::new(
                language.annotation_exchange_text("Export annotations on book closing"),
                Action::AnnotationExchange(Command::ToggleOnClose),
            )
            .detail(
                language.annotation_exchange_text(
                    "Save a local KOReader transfer when the book closes",
                ),
            ),
            Item::new(
                language.annotation_exchange_text("Keep all annotations on import"),
                Action::AnnotationExchange(Command::ToggleKeepAll),
            )
            .detail(
                language
                    .annotation_exchange_text("Keep older local marks missing from the transfer"),
            ),
        ];
        let folder = self.annotation_exchange.settings.folder.as_deref();
        rows.push(
            Item::new(
                language.annotation_exchange_text("Export / import folder"),
                Action::ExportSettings(
                    export_settings::Origin::Exchange,
                    export_settings::Command::Browse(
                        folder.unwrap_or_else(|| Path::new("")).to_owned(),
                    ),
                ),
            )
            .detail(folder.map_or_else(
                || {
                    language
                        .annotation_exchange_text("Book metadata folder")
                        .into()
                },
                export_settings::folder_label,
            )),
        );
        if folder.is_some() {
            rows.push(
                Item::new(
                    language.annotation_exchange_text("Use book metadata folder"),
                    Action::AnnotationExchange(Command::DefaultFolder),
                )
                .detail(language.annotation_exchange_text("The book’s adjacent .sdr folder")),
            );
        }
        if self.supports_annotation_exchange() {
            rows.extend([
                Item::new(
                    language.annotation_exchange_text("Import annotation transfer"),
                    Action::AnnotationExchange(Command::Preview),
                )
                .detail(
                    language.annotation_exchange_text(
                        "Review changes to bookmarks, highlights and notes",
                    ),
                ),
                Item::new(
                    language.annotation_exchange_text("Export annotation transfer"),
                    Action::AnnotationExchange(Command::Export),
                )
                .detail(language.annotation_exchange_text(
                    "Save a KOReader .annotations.lua file for this book",
                )),
            ]);
        }
        rows
    }

    pub(super) fn annotation_exchange_preview_items(&self) -> Vec<Item> {
        if self.annotation_exchange.preview_job.is_some() {
            return vec![
                Item::new("Checking annotation transfer…", Action::Heading),
                Item::new("Cancel", Action::AnnotationExchange(Command::CancelPreview)),
            ];
        }
        if let Some(issue) = self.annotation_exchange.preview_issue {
            return vec![
                Item::new(issue, Action::Heading),
                Item::new(
                    "Use standard preview",
                    Action::AnnotationExchange(Command::Preview),
                )
                .detail("This preview may take time"),
            ];
        }
        let Some(preview) = &self.annotation_exchange.preview else {
            return Vec::new();
        };
        let summary = preview.summary;
        if preview.imported {
            return vec![Item::new(
                "This transfer is already imported",
                Action::Heading,
            )];
        }
        let counts = format!(
            "{} added · {} updated · {} removed",
            summary.added, summary.updated, summary.removed
        );
        let mut rows = vec![Item::new("Transfer changes", Action::Heading).detail(counts)];
        if summary.unavailable != 0 {
            rows.push(
                Item::new(
                    format!("{} marks couldn’t be matched", summary.unavailable),
                    Action::Heading,
                )
                .detail("Older unmatched local marks will be kept"),
            );
        }
        if summary.removed != 0 {
            rows.push(
                Item::new(
                    "Import and remove older marks",
                    Action::AnnotationExchange(Command::Apply),
                )
                .detail("Removes the listed count of older local marks absent from the transfer"),
            );
        } else if summary.added > 0 || summary.updated > 0 {
            rows.push(Item::new(
                "Apply import",
                Action::AnnotationExchange(Command::Apply),
            ));
        } else if summary.unavailable == 0 {
            rows.push(
                Item::new(
                    "Use this transfer",
                    Action::AnnotationExchange(Command::Apply),
                )
                .detail("All marks are already up to date"),
            );
        }
        rows
    }

    pub(super) fn set_annotation_exchange_folder(&mut self, folder: Option<PathBuf>) -> Result<()> {
        let mut proposed = self.annotation_exchange.settings.clone();
        proposed.folder = folder;
        self.annotation_exchange.save(proposed)?;
        self.start_annotation_discovery();
        self.show(Screen::AnnotationExchange);
        Ok(())
    }

    pub(super) fn annotation_exchange_command(&mut self, command: Command) -> Result<()> {
        let mut proposed = self.annotation_exchange.settings.clone();
        match command {
            Command::ToggleOnClose => proposed.export_on_close = !proposed.export_on_close,
            Command::ToggleKeepAll => proposed.keep_all = !proposed.keep_all,
            Command::DefaultFolder => return self.set_annotation_exchange_folder(None),
            Command::Preview => return self.preview_annotation_exchange(),
            Command::PreviewBackground => return self.start_annotation_preview_background(),
            Command::CancelPreview => {
                self.cancel_annotation_preview();
                self.show(Screen::AnnotationExchange);
                return Ok(());
            }
            Command::Apply => {
                if let Some(preview) = &mut self.annotation_exchange.preview
                    && preview.summary.removed != 0
                {
                    preview.confirming = true;
                    return Ok(());
                }
                return self.apply_annotation_exchange();
            }
            Command::Export => {
                self.save_library()?;
                let summary = self.write_annotation_exchange()?;
                self.notice = Some(match summary {
                    (0, 0) => "No annotations to export".into(),
                    (0, unavailable) => format!("{unavailable} annotations could not be exported"),
                    (count, 0) => format!("{count} annotations exported"),
                    (count, unavailable) => {
                        format!("{count} annotations exported · {unavailable} unavailable")
                    }
                });
                return Ok(());
            }
        }
        self.annotation_exchange.save(proposed)
    }

    pub(super) fn supports_annotation_exchange(&self) -> bool {
        self.active.as_ref().is_some_and(|book| {
            book.entry.format == DocumentFormat::Epub
                && !crate::epub_members::is_folder_book(&book.entry.path)
        }) || self.fixed.as_ref().is_some_and(|book| {
            matches!(
                book.entry.format,
                DocumentFormat::Pdf | DocumentFormat::Djvu
            )
        })
    }

    /// Start an advisory scan of the configured transfer location. Actual
    /// import always goes through the existing independent preview and checks.
    pub(super) fn start_annotation_discovery(&mut self) {
        self.annotation_exchange.discovery.cancel();
        self.annotation_exchange.candidate = None;
        if !self.stats_awake || !self.supports_annotation_exchange() {
            return;
        }
        let Some((entry, book)) = self.current_document() else {
            return;
        };
        let request = discovery_job::Request {
            book,
            relative: entry.path.clone(),
            folder: self.annotation_exchange.settings.folder.clone(),
            root: self.library.verification_root().to_owned(),
            fixed: self.fixed.is_some(),
            device: self.annotation_exchange.settings.device.clone(),
            applied: self
                .library
                .state(book)
                .and_then(|state| state.annotation_exchange_digest.clone()),
        };
        self.annotation_exchange.discovery.start(request);
    }

    pub(super) fn cancel_annotation_discovery(&mut self) {
        self.annotation_exchange.discovery.cancel();
        self.annotation_exchange.candidate = None;
    }

    pub(super) fn pause_annotation_discovery(&mut self) {
        self.cancel_annotation_discovery();
    }

    pub(super) fn annotation_discovery_deadline(&self) -> Option<Instant> {
        self.annotation_exchange
            .discovery
            .deadline()
            .into_iter()
            .chain(
                self.annotation_discovery_announcement_ready()
                    .then(Instant::now),
            )
            .min()
    }

    pub(super) fn annotation_transfer_available(&self) -> bool {
        let Some(candidate) = &self.annotation_exchange.candidate else {
            return false;
        };
        self.annotation_exchange.settings.folder == candidate.folder
            && self.annotation_exchange.settings.device == candidate.device
            && self
                .current_document()
                .is_some_and(|(entry, id)| id == candidate.book && entry.path == candidate.path)
            && self.library.state(candidate.book).is_some_and(|state| {
                state.annotation_exchange_digest.as_deref() != Some(candidate.digest.as_str())
            })
    }

    fn annotation_discovery_announcement_ready(&self) -> bool {
        self.stats_awake
            && self.annotation_transfer_available()
            && self.notice.is_none()
            && self.notifications.deadline().is_none()
            && matches!(self.screen, Screen::Book | Screen::Fixed)
            && self
                .annotation_exchange
                .candidate
                .as_ref()
                .is_some_and(|candidate| {
                    !self
                        .annotation_exchange
                        .announced
                        .iter()
                        .any(|(book, digest)| {
                            *book == candidate.book && digest == &candidate.digest
                        })
                })
    }

    fn announce_discovered_annotation(&mut self) -> bool {
        if !self.annotation_discovery_announcement_ready() {
            return false;
        }
        let Some(candidate) = &self.annotation_exchange.candidate else {
            return false;
        };
        let key = (candidate.book, candidate.digest.clone());
        if self.annotation_exchange.announced.len() == 16 {
            self.annotation_exchange.announced.pop_front();
        }
        self.annotation_exchange.announced.push_back(key);
        let message = self
            .controls
            .interface_language
            .annotation_exchange_text("Annotation transfer found · open Navigation to review")
            .to_owned();
        self.announce_annotation_transfer(message);
        true
    }

    pub(super) fn poll_annotation_discovery(&mut self, now: Instant) -> Option<Change> {
        if self.announce_discovered_annotation() {
            return Some(Change::Partial);
        }
        let (request, digest) = self.annotation_exchange.discovery.poll(now)?;
        if self.annotation_exchange.settings.folder != request.folder
            || self.annotation_exchange.settings.device != request.device
            || !self.current_document().is_some_and(|(entry, id)| {
                id == request.book
                    && entry.path == request.relative
                    && self.fixed.is_some() == request.fixed
            })
            || self
                .library
                .state(request.book)
                .and_then(|state| state.annotation_exchange_digest.as_ref())
                != request.applied.as_ref()
        {
            return None;
        }
        let Some(digest) = digest else { return None };
        self.annotation_exchange.candidate = Some(Candidate {
            book: request.book,
            path: request.relative,
            folder: request.folder,
            digest,
            device: request.device,
        });
        if self.announce_discovered_annotation() {
            return Some(Change::Partial);
        }
        matches!(
            self.screen,
            Screen::BookTools | Screen::FixedTools | Screen::AnnotationExchange
        )
        .then_some(Change::Partial)
    }

    fn annotation_target(&self) -> io::Result<Target> {
        let (entry, id) = self.current_document().ok_or_else(|| {
            target_error(io::ErrorKind::InvalidInput, "Open a supported book first")
        })?;
        if !self.supports_annotation_exchange() {
            return Err(target_error(
                io::ErrorKind::Unsupported,
                "Transfer is unavailable for this edition",
            ));
        }
        if self.library.identify(entry)? != id {
            return Err(target_error(
                io::ErrorKind::InvalidData,
                "Reopen the changed book first",
            ));
        }
        let source = self.library.absolute_path(&entry.path)?;
        let adjacent = self.annotation_exchange.settings.folder.is_none();
        let folder = if let Some(folder) = &self.annotation_exchange.settings.folder {
            self.library.browse_folder(folder)?
        } else {
            let folder = source.with_extension("sdr");
            match fs::symlink_metadata(&folder) {
                Ok(metadata) if metadata.is_dir() => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(target_error(
                        io::ErrorKind::PermissionDenied,
                        "The book metadata folder is unavailable",
                    ));
                }
            }
            folder
        };
        let mut filename = source
            .file_name()
            .ok_or_else(|| {
                target_error(io::ErrorKind::InvalidInput, "This document has no filename")
            })?
            .to_owned();
        filename.push(".annotations.lua");
        Ok(Target {
            path: folder.join(filename),
            source,
            adjacent,
        })
    }

    fn read_annotation_exchange(&self) -> Result<(Import, bool)> {
        let (target, bytes, checksum) = with_storage(|| {
            let target = self.annotation_target()?;
            let bytes = read_import_file(&target.path, koreader_sidecar::MAX_SIDECAR_BYTES as u64)?;
            let checksum = koreader_sidecar::partial_md5(&mut fs::File::open(&target.source)?)?;
            Ok((target, bytes, format!("{:x}", md5::Digest(checksum))))
        })
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                target_error(
                    io::ErrorKind::NotFound,
                    "No annotation transfer was found for this book",
                )
            } else {
                error
            }
        })?;
        let snapshot = exchange::decode(&bytes)?;
        if snapshot.fixed != self.fixed.is_some()
            || snapshot
                .fingerprint
                .as_deref()
                .is_some_and(|hash| !hash.eq_ignore_ascii_case(&checksum))
        {
            return Err("The transfer belongs to a different document edition or format".into());
        }
        let state = self
            .library
            .state(self.open_book_id().ok_or("Open a book first")?)
            .ok_or("The open book’s reading data is unavailable")?;
        let digest = transfer_digest(&bytes);
        let imported = self.annotation_exchange.settings.device.as_ref() == Some(&snapshot.device)
            || state.annotation_exchange_digest.as_ref() == Some(&digest);
        let mut replacement = state.clone();
        let mut summary = Summary::default();
        if !imported {
            let trustworthy = snapshot.fingerprint.is_some() || target.adjacent;
            summary = self.apply_transfer(
                &target.source,
                snapshot,
                &mut replacement,
                self.annotation_exchange.settings.keep_all || !trustworthy,
            )?;
            // A transfer with unresolved positions has records that still
            // belong to the other reader. Keep its file eligible for retry.
            if summary.unavailable == 0 {
                replacement.annotation_exchange_digest = Some(digest.clone());
            }
        }
        Ok((
            Import {
                digest,
                summary,
                replacement,
            },
            imported,
        ))
    }

    fn preview_annotation_exchange(&mut self) -> Result<()> {
        let (import, imported) = self.read_annotation_exchange()?;
        self.show(Screen::AnnotationExchangePreview);
        self.annotation_exchange.preview_issue = None;
        self.annotation_exchange.preview_job = None;
        self.annotation_exchange.preview = Some(Preview {
            id: import.replacement.id,
            digest: import.digest,
            summary: import.summary,
            imported,
            confirming: false,
            background: None,
        });
        Ok(())
    }

    fn annotation_exchange_sheet(&self) -> Option<Sheet<'_>> {
        self.annotation_exchange
            .preview
            .as_ref()
            .filter(|preview| preview.confirming)?;
        let (width, height) = self.reading_viewport().dimensions();
        Some(Sheet::centered(
            Rect::new(0, 0, width as i32, height as i32),
            "Remove older marks?",
            "This import removes older local marks absent from the transfer.",
            "Import",
        ))
    }

    pub(super) fn draw_annotation_exchange_confirmation(&self, canvas: &mut Canvas) {
        if let Some(sheet) = self.annotation_exchange_sheet() {
            sheet.draw(canvas, None);
        }
    }

    pub(super) fn annotation_exchange_press(&self, input: Input) -> Option<Option<(f32, f32)>> {
        let sheet = self.annotation_exchange_sheet()?;
        Some(match input {
            Input::Tap(x, y)
                if [SheetAction::Cancel, SheetAction::Confirm]
                    .into_iter()
                    .any(|action| sheet.button(action).contains(x, y)) =>
            {
                Some((x, y))
            }
            _ => None,
        })
    }

    pub(super) fn annotation_exchange_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let sheet = self.annotation_exchange_sheet()?;
        let action = match input {
            Input::Tap(x, y) => sheet.hit(x, y),
            Input::PageBack => Some(SheetAction::Cancel),
            _ => None,
        };
        if let Some(action) = action {
            self.annotation_exchange.preview.as_mut()?.confirming = false;
            return Some(if action == SheetAction::Confirm {
                match self.apply_annotation_exchange() {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                }
            } else {
                Change::Partial
            });
        }
        Some(Change::None)
    }

    fn apply_annotation_exchange(&mut self) -> Result<()> {
        let preview = self
            .annotation_exchange
            .preview
            .as_ref()
            .ok_or("Preview the transfer first")?;
        if preview
            .background
            .as_ref()
            .is_some_and(|reviewed| !reviewed.matches(self, preview.id))
        {
            return Err("The book or reading data changed. Preview it again.".into());
        }
        let (import, imported) = self.read_annotation_exchange()?;
        if preview
            .background
            .as_ref()
            .is_some_and(|reviewed| !reviewed.matches_replacement(&import.replacement))
        {
            return Err("The verified transfer result changed. Preview it again.".into());
        }
        if preview.id != import.replacement.id
            || preview.digest != import.digest
            || preview.imported != imported
            || preview.summary != import.summary
        {
            return Err("The annotation transfer has changed. Preview it again.".into());
        }
        self.library
            .commit_annotation_exchange(import.replacement)?;
        self.show(Screen::AnnotationExchange);
        self.notice = Some("Annotation transfer imported".into());
        Ok(())
    }

    pub(super) fn annotation_close_warning(&mut self) -> Option<String> {
        if !self.annotation_exchange.settings.export_on_close
            || !self.supports_annotation_exchange()
        {
            return None;
        }
        match self.write_annotation_exchange() {
            Ok((_, 0)) => None,
            Ok((_, unavailable)) => {
                Some(format!("{unavailable} annotations could not be exported"))
            }
            Err(error) => Some(format!("Annotations could not be exported: {error}")),
        }
    }

    fn write_annotation_exchange(&mut self) -> Result<(usize, usize)> {
        let id = self.open_book_id().ok_or("Open a book first")?;
        let state = self
            .library
            .state(id)
            .ok_or("The open book’s reading data is unavailable")?;
        if state.bookmarks.is_empty()
            && state.highlights.is_empty()
            && state.pdf_highlights.is_empty()
        {
            return Ok((0, 0));
        }
        let target = with_storage(|| self.annotation_target())?;
        let (marks, unavailable) = self.export_transfer_marks(&target.source, state)?;
        // KOReader can remove unmatched marks when importing a newer file.
        // Do not hand it an incomplete snapshot of our own saved marks.
        if unavailable != 0 || marks.is_empty() {
            return Ok((0, unavailable));
        }
        let applied = state.annotation_exchange_digest.clone();
        let device = self.annotation_exchange.device()?;
        let datetime = crate::mark_time::now().ok_or("The device date is unavailable")?;
        let written = marks.len();
        with_storage(|| {
            let confirmed_target = self.annotation_target()?;
            if confirmed_target.path != target.path || confirmed_target.source != target.source {
                return Err(target_error(
                    io::ErrorKind::InvalidData,
                    "The book or transfer folder changed",
                ));
            }
            let checksum = koreader_sidecar::partial_md5(&mut fs::File::open(&target.source)?)?;
            let checksum = format!("{:x}", md5::Digest(checksum));
            let result = exchange::encode(&exchange::Snapshot {
                device: device.clone(),
                fixed: self.fixed.is_some(),
                datetime,
                fingerprint: Some(checksum.clone()),
                incomplete: false,
                unavailable,
                marks,
            })
            .map_err(io::Error::other)?;
            match read_import_file(&target.path, koreader_sidecar::MAX_SIDECAR_BYTES as u64) {
                Ok(bytes) => {
                    let existing = exchange::decode(&bytes).map_err(io::Error::other)?;
                    let same = existing.device == device
                        && existing.fingerprint.as_ref() == Some(&checksum);
                    let imported = applied
                        .as_ref()
                        .is_some_and(|digest| *digest == transfer_digest(&bytes))
                        && existing.unavailable == 0
                        && !existing.incomplete;
                    if !same && !imported {
                        return Err(target_error(
                            io::ErrorKind::AlreadyExists,
                            "The folder has a transfer from another reader or edition. Import it first.",
                        ));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            if target.adjacent {
                let folder = target.path.parent().ok_or_else(|| {
                    target_error(
                        io::ErrorKind::InvalidInput,
                        "The book has no metadata folder",
                    )
                })?;
                match fs::create_dir(folder) {
                    Ok(()) => {}
                    Err(error)
                        if error.kind() == io::ErrorKind::AlreadyExists
                            && fs::symlink_metadata(folder)?.is_dir() => {}
                    Err(error) => return Err(error),
                }
            }
            let temporary = target
                .path
                .with_file_name(format!(".reader-{}.tmp", random_token()?));
            write_transfer(&target.path, &temporary, result.as_bytes())
        })?;
        Ok((written, unavailable))
    }
}

fn write_transfer(path: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    let saved = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let saved = saved.and_then(|()| fs::rename(temporary, path));
    if saved.is_err() {
        let _ = fs::remove_file(temporary);
    }
    saved
}
