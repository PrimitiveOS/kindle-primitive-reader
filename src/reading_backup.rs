//! Bounded local backup envelope and recoverable multi-file publication.
//! Targets are enum values, never paths supplied by an imported document.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(crate) const LIMIT: u64 = 32 * 1024 * 1024;
pub(crate) const MAX_COMPONENTS: usize = 2048;
const DIRECTORY: &str = ".reading-backup-transaction";
const JOURNAL: &str = "journal.json";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "book")]
pub(crate) enum Key {
    Library,
    Controls,
    FooterCustomText,
    Profiles,
    Gestures,
    Dictionaries,
    Lookup,
    Statistics,
    History,
    Hyphenation,
    Browser,
    BookMetadata,
    Timer,
    DocumentDefaults,
    StyleTweaks,
    Pace(u64),
}
impl Key {
    pub(crate) const SETTINGS: [Self; 15] = [
        Self::Controls,
        Self::FooterCustomText,
        Self::Profiles,
        Self::Gestures,
        Self::Dictionaries,
        Self::Lookup,
        Self::Statistics,
        Self::History,
        Self::Hyphenation,
        Self::Browser,
        Self::BookMetadata,
        Self::Timer,
        Self::DocumentDefaults,
        Self::StyleTweaks,
        Self::Library,
    ];
    pub(crate) fn relative(self, state: &Path) -> PathBuf {
        match self {
            Self::Library => state.file_name().unwrap().into(),
            Self::Controls => "reading-controls.json".into(),
            Self::FooterCustomText => "status-bar-custom-text.json".into(),
            Self::Profiles => "reading-profiles.json".into(),
            Self::Gestures => "reading-gestures.json".into(),
            Self::Dictionaries => "dictionary-preference.json".into(),
            Self::Lookup => "lookup-history.json".into(),
            Self::Statistics => "reading-stats.json".into(),
            Self::History => "reading-history.json".into(),
            Self::Hyphenation => "user-hyphenation.json".into(),
            Self::Browser => "bookmark-browser.json".into(),
            Self::BookMetadata => "bookmark-metadata.json".into(),
            Self::Timer => "read-timer.json".into(),
            Self::DocumentDefaults => crate::document_defaults::FILE.into(),
            Self::StyleTweaks => crate::user_styles::FILE.into(),
            Self::Pace(book) => PathBuf::from("reading-pace").join(format!("{book:016x}.json")),
        }
    }
    pub(crate) fn label(self) -> String {
        match self {
            Self::Library => "Book progress, marks and collections".into(),
            Self::Controls => "Reading controls".into(),
            Self::FooterCustomText => "Custom status bar text".into(),
            Self::Profiles => "Reading profiles".into(),
            Self::Gestures => "Reading gestures".into(),
            Self::Dictionaries => "Dictionary preferences".into(),
            Self::Lookup => "Vocabulary and local lookup history".into(),
            Self::Statistics => "Undated reading totals".into(),
            Self::History => "Daily reading history and calendar".into(),
            Self::Hyphenation => "Personal hyphenation rules".into(),
            Self::Browser => "Bookmark browser preferences".into(),
            Self::BookMetadata => "Retained book metadata".into(),
            Self::Timer => "Read timer preferences".into(),
            Self::DocumentDefaults => "New book defaults".into(),
            Self::StyleTweaks => "User style tweaks and book CSS".into(),
            Self::Pace(_) => "Reading time estimates".into(),
        }
    }
    pub(crate) fn limit(self) -> u64 {
        match self {
            Self::Library => crate::library::MAX_BACKUP_BYTES,
            Self::Controls => 4096,
            Self::FooterCustomText => 1024,
            Self::Profiles => 32 * 1024,
            Self::Gestures | Self::Timer => 8192,
            Self::Dictionaries | Self::DocumentDefaults => 64 * 1024,
            Self::Lookup | Self::BookMetadata => 8 * 1024 * 1024,
            Self::Statistics | Self::Pace(_) => 512 * 1024,
            Self::History => 16 * 1024 * 1024,
            Self::Hyphenation | Self::Browser => 1024 * 1024,
            Self::StyleTweaks => crate::user_styles::MAX_FILE_BYTES,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Component {
    pub key: Key,
    pub data: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Envelope {
    format: String,
    version: u8,
    pub library: Value,
    pub components: Vec<Component>,
}
impl Envelope {
    pub(crate) fn new(library: Value, components: Vec<Component>) -> Self {
        Self {
            format: "primitiveos-reader-backup".into(),
            version: 2,
            library,
            components,
        }
    }
    pub(crate) fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > LIMIT {
            return Err(invalid("The backup exceeds 32 MiB"));
        }
        let value: Value = serde_json::from_slice(bytes)?;
        // The existing v1 book-library backup remains an accepted input.
        if value.get("format").is_none() && value.get("books").is_some() {
            if bytes.len() as u64 > crate::library::MAX_BACKUP_BYTES {
                return Err(invalid("The legacy backup is too large"));
            }
            return Ok(Self::new(value, Vec::new()));
        }
        let parsed: Self = serde_json::from_value(value)?;
        if parsed.format != "primitiveos-reader-backup"
            || parsed.version != 2
            || parsed.components.len() > MAX_COMPONENTS
        {
            return Err(invalid(
                "The backup format, version or component count is unsupported",
            ));
        }
        let mut keys = BTreeSet::new();
        for component in &parsed.components {
            if component.key == Key::Library || !keys.insert(component.key) {
                return Err(invalid(
                    "The backup contains duplicate or invalid components",
                ));
            }
            if serde_json::to_vec(&component.data)?.len() as u64 > component.key.limit() {
                return Err(invalid("A backup component exceeds its size limit"));
            }
        }
        Ok(parsed)
    }
    pub(crate) fn bytes(&self) -> io::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() as u64 > LIMIT {
            return Err(invalid("The complete backup exceeds 32 MiB"));
        }
        Ok(bytes)
    }
}
pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn root(state: &Path) -> io::Result<PathBuf> {
    if state.file_name().is_none() {
        return Err(invalid("The library state filename is invalid"));
    }
    let filename = state
        .file_name()
        .unwrap()
        .to_str()
        .ok_or_else(|| invalid("The library state filename is not UTF-8"))?;
    if filename.eq_ignore_ascii_case(DIRECTORY)
        || filename.eq_ignore_ascii_case("reading-pace")
        || Key::SETTINGS
            .into_iter()
            .filter(|key| *key != Key::Library)
            .any(|key| {
                key.relative(state)
                    .to_str()
                    .is_some_and(|reserved| filename.eq_ignore_ascii_case(reserved))
            })
    {
        return Err(invalid(
            "The library filename collides with a reserved reading-data target",
        ));
    }
    fs::canonicalize(
        state
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )
}
fn regular(path: &Path, limit: u64) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return Err(invalid(
            "A reading-data file is not regular or exceeds its size limit",
        ));
    }
    Ok(())
}
pub(crate) fn read(state: &Path, key: Key) -> io::Result<Vec<u8>> {
    let root = root(state)?;
    let path = target(&root, state, key, false)?;
    regular(&path, key.limit())?;
    let bytes = fs::read(path)?;
    if bytes.len() as u64 > key.limit() {
        return Err(invalid("A reading-data file grew beyond its limit"));
    }
    Ok(bytes)
}
fn target(root: &Path, state: &Path, key: Key, create: bool) -> io::Result<PathBuf> {
    if matches!(key, Key::Pace(_)) {
        let directory = root.join("reading-pace");
        if create && !directory.exists() {
            fs::create_dir(&directory)?;
        }
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid(
                "The reading-pace folder is not a regular local directory",
            ));
        }
    }
    Ok(root.join(key.relative(state)))
}
fn private_file(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}
fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn copy(source: &Path, destination: &Path) -> io::Result<()> {
    regular(source, LIMIT)?;
    let mut output = private_file(destination)?;
    let count = io::copy(&mut fs::File::open(source)?.take(LIMIT + 1), &mut output)?;
    if count > LIMIT {
        return Err(invalid(
            "A previous reading-data file is too large to preserve",
        ));
    }
    output.sync_all()
}
fn digest(path: &Path) -> io::Result<String> {
    regular(path, LIMIT)?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut file = fs::File::open(path)?;
    let mut block = [0u8; 32 * 1024];
    loop {
        let count = file.read(&mut block)?;
        if count == 0 {
            break;
        }
        context.update(&block[..count]);
    }
    Ok(context
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn sync_directory(path: &Path) -> io::Result<()> {
    match fs::File::open(path)?.sync_all() {
        // Some removable filesystems cannot fsync directories; the file journal
        // still protects process interruption. Physical power-loss is a separate gate.
        Err(e) if matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP)) => Ok(()),
        result => result,
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    key: Key,
    old_sha256: Option<String>,
    old_bytes: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    committed: bool,
    entries: Vec<Entry>,
}
fn save_journal(directory: &Path, journal: &Journal) -> io::Result<()> {
    primitive_app::save(&directory.join(JOURNAL), &serde_json::to_vec(journal)?)?;
    fs::File::open(directory.join(JOURNAL))?.sync_all()?;
    sync_directory(directory)
}
fn rollback(root: &Path, state: &Path, directory: &Path, journal: &Journal) -> io::Result<()> {
    // Validate every immutable old slot before restoring any target.
    let mut keys = BTreeSet::new();
    let mut total = 0u64;
    for (index, entry) in journal.entries.iter().enumerate() {
        if !keys.insert(entry.key) {
            return Err(invalid("The recovery journal repeats a target"));
        }
        total = total.saturating_add(entry.old_bytes);
        if total > LIMIT {
            return Err(invalid("The recovery journal exceeds its size limit"));
        }
        if let Some(expected) = &entry.old_sha256 {
            let path = directory.join(format!("old-{index}"));
            if digest(&path)? != *expected || fs::metadata(path)?.len() != entry.old_bytes {
                return Err(invalid("A backup recovery slot is incomplete or changed"));
            }
        } else if entry.old_bytes != 0 {
            return Err(invalid("The recovery journal is inconsistent"));
        }
        let destination = target(root, state, entry.key, true)?;
        if let Ok(meta) = fs::symlink_metadata(&destination)
            && (!meta.is_file() || meta.file_type().is_symlink())
        {
            return Err(invalid("A recovery destination is not a regular file"));
        }
    }
    for (index, entry) in journal.entries.iter().enumerate() {
        let destination = target(root, state, entry.key, true)?;
        if entry.old_sha256.is_some() {
            let temporary = directory.join(format!("restore-{index}"));
            match fs::symlink_metadata(&temporary) {
                Ok(_) => fs::remove_file(&temporary)?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
            copy(&directory.join(format!("old-{index}")), &temporary)?;
            fs::rename(temporary, &destination)?;
        } else {
            match fs::remove_file(&destination) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                result => result?,
            }
        }
        sync_directory(destination.parent().unwrap())?;
    }
    Ok(())
}
/// Run before Library or any preference is loaded.
pub(crate) fn recover(state: &Path) -> io::Result<()> {
    let root = match root(state) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    };
    let directory = root.join(DIRECTORY);
    let metadata = match fs::symlink_metadata(&directory) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("Backup recovery directory is invalid"));
    }
    let journal_path = directory.join(JOURNAL);
    if fs::symlink_metadata(&journal_path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
        // Preparation never changes targets before the durable journal is ready.
        fs::remove_dir_all(directory)?;
        return Ok(());
    }
    regular(&journal_path, 1024 * 1024)?;
    let journal: Journal = serde_json::from_slice(&fs::read(journal_path)?)?;
    if journal.version != 1 || journal.entries.len() > MAX_COMPONENTS + 1 {
        return Err(invalid("The backup recovery journal is unsupported"));
    }
    if !journal.committed {
        rollback(&root, state, &directory, &journal)?;
    }
    fs::remove_dir_all(directory)?;
    sync_directory(&root)
}
pub(crate) fn transaction<T>(
    state: &Path,
    updates: &[(Key, Vec<u8>)],
    finish: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    transaction_observed(state, updates, finish, |_| {})
}

/// Stage a future raw-anchor schema upgrade only after retaining the exact v1
/// state independently of the restore journal. This is not called by the
/// current Reader: its v2 schema and consumers are not enabled yet.
///
/// Caller must hold the storage guard and supply the actual v1/v2 semantic
/// validators. The journal's old slot is checked *after* it is captured and
/// before commit, so an intervening state change causes rollback rather than
/// publishing against the wrong backup.
#[allow(dead_code)]
pub(crate) fn publish_raw_anchor_v2(
    state: &Path,
    expected_v1: &[u8],
    replacement_v2: &[u8],
    validate_v1: impl FnOnce(&[u8]) -> io::Result<()>,
    validate_v2: impl Fn(&[u8]) -> io::Result<()>,
) -> io::Result<PathBuf> {
    if expected_v1.len() as u64 > crate::library::MAX_STATE_BYTES
        || replacement_v2.len() as u64 > crate::library::MAX_STATE_BYTES
    {
        return Err(invalid("The reader state exceeds its size limit"));
    }
    let old: Value = serde_json::from_slice(expected_v1)?;
    if !(old.is_array() || old.get("version").and_then(Value::as_u64) == Some(1)) {
        return Err(invalid("The prior reader state is not version 1"));
    }
    let new: Value = serde_json::from_slice(replacement_v2)?;
    if new.get("version").and_then(Value::as_u64) != Some(2) {
        return Err(invalid("The replacement reader state is not version 2"));
    }
    validate_v1(expected_v1)?;
    validate_v2(replacement_v2)?;
    recover(state)?;
    let root = root(state)?;
    let destination = target(&root, state, Key::Library, false)?;
    if read_nofollow(&destination, crate::library::MAX_STATE_BYTES)? != expected_v1 {
        return Err(invalid("Reader state changed before its v1 backup"));
    }
    let hash = ring::digest::digest(&ring::digest::SHA256, expected_v1);
    let name = format!(
        ".reader-v1-{}.bak",
        hash.as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let backup = root.join(name);
    if backup == destination {
        return Err(invalid("The v1 backup collides with the reader state"));
    }
    match fs::symlink_metadata(&backup) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A partial file left by interruption is retained and will fail
            // the exact-content verification on the next attempt.
            write(&backup, expected_v1)?;
            sync_directory(&root)?;
        }
        Err(error) => return Err(error),
    }
    if read_nofollow(&backup, crate::library::MAX_STATE_BYTES)? != expected_v1 {
        return Err(invalid("The retained v1 backup is incomplete or changed"));
    }
    let update = [(Key::Library, replacement_v2.to_vec())];
    transaction(state, &update, || {
        let old_slot = root.join(DIRECTORY).join("old-0");
        if read_nofollow(&old_slot, crate::library::MAX_STATE_BYTES)? != expected_v1
            || read_nofollow(&backup, crate::library::MAX_STATE_BYTES)? != expected_v1
        {
            return Err(invalid(
                "Reader state or its v1 backup changed during upgrade",
            ));
        }
        let current = read_nofollow(&destination, crate::library::MAX_STATE_BYTES)?;
        if current != replacement_v2 {
            return Err(invalid(
                "The upgraded reader state changed during publication",
            ));
        }
        validate_v2(&current)
    })?;
    Ok(backup)
}

fn read_nofollow(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid("A reader backup is unavailable or too large"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("A reader backup grew beyond its size limit"));
    }
    Ok(bytes)
}
fn transaction_observed<T>(
    state: &Path,
    updates: &[(Key, Vec<u8>)],
    finish: impl FnOnce() -> io::Result<T>,
    mut step: impl FnMut(usize),
) -> io::Result<T> {
    recover(state)?;
    let root = root(state)?;
    let directory = root.join(DIRECTORY);
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let mut journal = Journal {
        version: 1,
        committed: false,
        entries: Vec::new(),
    };
    let prepare = (|| {
        if updates.len() > MAX_COMPONENTS + 1 {
            return Err(invalid("Too many restore components"));
        }
        let mut keys = BTreeSet::new();
        let (mut old_total, mut new_total) = (0u64, 0u64);
        for (index, (key, bytes)) in updates.iter().enumerate() {
            new_total = new_total.saturating_add(bytes.len() as u64);
            if bytes.len() as u64 > key.limit() || new_total > LIMIT || !keys.insert(*key) {
                return Err(invalid("The restore target list is duplicate or too large"));
            }
            let destination = target(&root, state, *key, true)?;
            let (old_sha256, old_bytes) = match fs::symlink_metadata(&destination) {
                Ok(_) => {
                    regular(&destination, LIMIT)?;
                    let count = fs::metadata(&destination)?.len();
                    old_total = old_total.saturating_add(count);
                    if old_total > LIMIT {
                        return Err(invalid("Existing data exceeds the rollback limit"));
                    }
                    let slot = directory.join(format!("old-{index}"));
                    copy(&destination, &slot)?;
                    (Some(digest(&slot)?), count)
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => (None, 0),
                Err(e) => return Err(e),
            };
            write(&directory.join(format!("new-{index}")), bytes)?;
            journal.entries.push(Entry {
                key: *key,
                old_sha256,
                old_bytes,
            });
        }
        save_journal(&directory, &journal)?;
        sync_directory(&root)
    })();
    if let Err(error) = prepare {
        let _ = fs::remove_dir_all(&directory);
        return Err(error);
    }
    step(0);
    let apply = (|| {
        for (index, entry) in journal.entries.iter().enumerate() {
            let destination = target(&root, state, entry.key, true)?;
            if let Ok(metadata) = fs::symlink_metadata(&destination)
                && (!metadata.is_file() || metadata.file_type().is_symlink())
            {
                return Err(invalid("A restore destination changed"));
            }
            fs::rename(directory.join(format!("new-{index}")), &destination)?;
            sync_directory(destination.parent().unwrap())?;
            step(index + 1);
        }
        let value = finish()?;
        journal.committed = true;
        save_journal(&directory, &journal)?;
        step(updates.len() + 1);
        Ok(value)
    })();
    match apply {
        Ok(value) => {
            // A committed journal can be removed on next startup if cleanup fails.
            let _ = fs::remove_dir_all(&directory);
            Ok(value)
        }
        Err(error) => {
            // If publishing the commit marker failed after its atomic rename,
            // establish a durable rollback state before changing any target back.
            if journal.committed {
                journal.committed = false;
                save_journal(&directory, &journal)?;
            }
            rollback(&root, state, &directory, &journal).map_err(|recovery| {
                io::Error::other(format!(
                    "Restore failed ({error}); recovery must finish before startup ({recovery})"
                ))
            })?;
            fs::remove_dir_all(&directory)?;
            Err(error)
        }
    }
}

#[cfg(test)]
pub(crate) fn interrupt_for_test(state: &Path, updates: &[(Key, Vec<u8>)], at: usize) {
    let _ = transaction_observed(
        state,
        updates,
        || Ok(()),
        |step| {
            if step == at {
                panic!("Simulated interrupted restore");
            }
        },
    );
}

#[cfg(test)]
mod tests;
