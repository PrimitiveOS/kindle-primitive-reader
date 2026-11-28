//! Download and install StarDict packages from KOReader's pinned catalogue.
//!
//! The catalog source is fetched on demand and cached only in the user's Reader
//! data directory. Each dictionary is downloaded by explicit user action, and
//! the source's license text and attribution are kept beside the installed data.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use bzip2::read::BzDecoder;
use flate2::read::MultiGzDecoder;
use serde::{Deserialize, Serialize};

const KO_READER_REVISION: &str = "9192014d8bd82a91dc1012473be0f238dedfdb54";
const EXPECTED_CATALOG_PACKAGES: usize = 803;
const DICTIONARY_SOURCE_SHA256: &str =
    "a3d4fac58f8afa76f27b424b1c66bf1748d86ca9e97dff8a9fe955eedfbb8ea6";
const LANGUAGE_SOURCE_SHA256: &str =
    "0c34fccc140f17ba1454f347fea0bee1615e8c8f0af132de431308346c5e6790";
const CATALOG_SOURCE: &str = "https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/dictionaries.lua";
const CATALOG_RAW_URL: &str = "https://raw.githubusercontent.com/koreader/koreader/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/dictionaries.lua";
const ISO_LANGUAGE_RAW_URL: &str = "https://raw.githubusercontent.com/koreader/koreader/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/isolanguage.lua";
const CATALOG_CACHE_FILE: &str = ".dictionary-catalog-cache.json";
const MAX_CATALOG_SOURCE_BYTES: usize = 512 * 1024;
const MAX_CATALOG_CACHE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_CATALOG_PACKAGES: usize = 2048;
const MAX_WORKER_REQUEST_BYTES: usize = 16 * 1024;
pub(crate) const MAX_COMPRESSED_BYTES: usize = 32 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 512;
const TAR_BLOCK: usize = 512;
const WORKER_TIMEOUT: Duration = Duration::from_secs(180);
const WORKER_POLL: Duration = Duration::from_millis(150);
const WORKER_RESPONSE_LIMIT: u64 = 8 * 1024;
const DOWNLOAD_PROGRESS_BYTES: u64 = 512 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(750);
const DOWNLOAD_CHUNK_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Package {
    pub(crate) slug: String,
    pub(crate) title: String,
    pub(crate) source_code: String,
    pub(crate) target_code: String,
    pub(crate) source_language: String,
    pub(crate) target_language: String,
    pub(crate) ifo_language: String,
    pub(crate) entries: u32,
    pub(crate) license: String,
    pub(crate) url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Catalog {
    pub(crate) revision: String,
    pub(crate) packages: Vec<Package>,
    language_names: BTreeMap<String, String>,
    language_tags: BTreeMap<String, String>,
    dictionary_source_sha256: String,
    language_source_sha256: String,
    pub(crate) cached: bool,
}

impl Package {
    pub(crate) fn language_pair(&self) -> String {
        format!("{} → {}", self.source_language, self.target_language)
    }

    pub(crate) fn installable(&self) -> bool {
        self.url.starts_with("https://") && supported_archive_url(&self.url)
    }
}

impl Catalog {
    pub(crate) fn fallback() -> Self {
        let language_names = BTreeMap::from([
            ("ces".into(), "Czech".into()),
            ("deu".into(), "German".into()),
            ("eng".into(), "English".into()),
            ("fra".into(), "French".into()),
            ("lat".into(), "Latin".into()),
            ("swe".into(), "Swedish".into()),
        ]);
        let language_tags = BTreeMap::from([
            ("ces".into(), "cs".into()),
            ("deu".into(), "de".into()),
            ("eng".into(), "en".into()),
            ("fra".into(), "fr".into()),
            ("lat".into(), "la".into()),
            ("swe".into(), "sv".into()),
        ]);
        let mut catalog = Self {
            revision: "bundled fallback".into(),
            packages: Vec::new(),
            language_names,
            language_tags,
            dictionary_source_sha256: String::new(),
            language_source_sha256: String::new(),
            cached: false,
        };
        catalog.packages = [
            ("folkets-en-sv", "Folkets lexikon", "eng", "swe", 53618, "CC-BY-SA 2.5", "https://gitlab.com/koreader/stardict-dictionaries/uploads/619cbab2537b4d115d5503cdd023ce05/folkets_en-sv.tar.gz"),
            ("folkets-sv-en", "Folkets lexikon", "swe", "eng", 36513, "CC-BY-SA 2.5", "https://gitlab.com/koreader/stardict-dictionaries/uploads/53a0a9fea8cab8661cf930ddd2353a4c/folkets_sv-en.tar.gz"),
            ("academie-francaise-8e", "Dictionnaire de l’Académie française: 8ème edition", "fra", "fra", 31934, "Public Domain (copyright expired, published 1935)", "https://gitlab.com/koreader/stardict-dictionaries/uploads/b8e8ba6b8941a78762675ff2ef95d1d1/acadfran.tar.gz"),
            ("georges-lat-de", "Georges: Ausführliches lateinisch-deutsches Handwörterbuch", "lat", "deu", 54831, "Public Domain (copyright expired, published 1913)", "https://gitlab.com/koreader/stardict-dictionaries/uploads/6339585b68ac485bedb8ee67892cb974/georges_lat-de.tar.gz"),
            ("georges-de-lat", "Georges: Kleines deutsch-lateinisches Handwörterbuch", "deu", "lat", 26608, "Public Domain (copyright expired, published 1910)", "https://gitlab.com/koreader/stardict-dictionaries/uploads/a04de66c7376e436913ca288a3ca608b/georges_de-lat.tar.gz"),
            ("czech-english", "GNU/FDL Anglicko/Český slovník", "ces", "eng", 88994, "GNU/FDL", "https://github.com/Vuizur/czech-dictionary-extender/releases/download/1.0.0/czech-english-dict.tar.gz"),
            ("english-idioms", "English Idioms (eng-eng)", "eng", "eng", 8560, "GNU Public License", "https://gitlab.com/avsej/dicts-stardict-form-xdxf/raw/d636cc5e8d4a47e22ac7466f4af6d435a8a3f650/002c/stardict-comn_sdict05_idioms_eng_eng-2.4.2.tar.gz"),
        ]
        .into_iter()
        .map(|(slug, title, source_code, target_code, entries, license, url)| {
            let source_language = catalog.language_names[source_code].clone();
            let target_language = catalog.language_names[target_code].clone();
            let ifo_language = format!(
                "{}-{}",
                catalog.language_tags[source_code],
                catalog.language_tags[target_code]
            );
            Package {
                slug: slug.into(),
                title: title.into(),
                source_code: source_code.into(),
                target_code: target_code.into(),
                source_language,
                target_language,
                ifo_language,
                entries,
                license: license.into(),
                url: url.into(),
            }
        })
        .collect();
        catalog
    }
}

pub(crate) fn load_cache(root: &Path) -> Option<Catalog> {
    let root_info = fs::symlink_metadata(root).ok()?;
    if !root_info.is_dir() || root_info.file_type().is_symlink() {
        return None;
    }
    let path = root.join(CATALOG_CACHE_FILE);
    let info = fs::symlink_metadata(&path).ok()?;
    if !info.is_file() || info.file_type().is_symlink() || info.len() > MAX_CATALOG_CACHE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mut catalog: Catalog = serde_json::from_slice(&bytes).ok()?;
    validate_catalog(&catalog).ok()?;
    catalog.cached = true;
    Some(catalog)
}

fn save_cache(root: &Path, catalog: &Catalog) -> io::Result<()> {
    validate_catalog(catalog)?;
    match fs::symlink_metadata(root) {
        Ok(info) if info.is_dir() && !info.file_type().is_symlink() => (),
        Ok(_) => {
            return Err(io::Error::other(
                "Dictionary storage is not a regular directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(root)?,
        Err(error) => return Err(error),
    }
    let bytes = serde_json::to_vec(catalog)?;
    if bytes.len() as u64 > MAX_CATALOG_CACHE_BYTES {
        return Err(io::Error::other("Dictionary catalogue cache is too large"));
    }
    let stage = root.join(format!(
        ".catalog-update-{}-{}",
        std::process::id(),
        SystemTimeStamp::now()
    ));
    let outcome = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&stage, root.join(CATALOG_CACHE_FILE))
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(stage);
    }
    outcome
}

fn validate_catalog(catalog: &Catalog) -> io::Result<()> {
    if catalog.revision != KO_READER_REVISION
        || catalog.packages.len() != EXPECTED_CATALOG_PACKAGES
        || catalog.packages.len() > MAX_CATALOG_PACKAGES
        || catalog.language_names.len() > 1024
        || catalog.language_tags.len() > 1024
        || catalog.dictionary_source_sha256 != DICTIONARY_SOURCE_SHA256
        || catalog.language_source_sha256 != LANGUAGE_SOURCE_SHA256
    {
        return Err(io::Error::other(
            "Dictionary catalogue is incomplete or unsupported",
        ));
    }
    let mut slugs = std::collections::HashSet::new();
    for package in &catalog.packages {
        validate_catalog_package(package)?;
        if !slugs.insert(package.slug.as_str()) {
            return Err(io::Error::other(
                "Dictionary catalogue has duplicate package URLs",
            ));
        }
    }
    Ok(())
}

fn validate_catalog_package(package: &Package) -> io::Result<()> {
    let valid_slug = !package.slug.is_empty()
        && package.slug.len() <= 64
        && package
            .slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let valid_language = |code: &str| {
        (2..=12).contains(&code.len())
            && code.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            })
    };
    let url = url::Url::parse(&package.url).map_err(io::Error::other)?;
    if !valid_slug
        || package.title.is_empty()
        || package.title.len() > 1024
        || package.entries == 0
        || !valid_language(&package.source_code)
        || !valid_language(&package.target_code)
        || package.source_language.is_empty()
        || package.target_language.is_empty()
        || package.license.len() > 1024
        || package.url.len() > 4096
        || !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || package.ifo_language.is_empty()
        || package.ifo_language.len() > 40
        || !package
            .ifo_language
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !slug_for_url(&package.url).eq(&package.slug)
    {
        return Err(io::Error::other(
            "Dictionary catalogue contains an invalid package",
        ));
    }
    Ok(())
}

fn slug_for_url(url: &str) -> String {
    format!("dict-{:x}", md5::compute(url.as_bytes()))
}

fn supported_archive_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| {
        let path = url.path().to_ascii_lowercase();
        [
            ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.zst", ".tzst", ".zip",
        ]
        .into_iter()
        .any(|extension| path.ends_with(extension))
    })
}

fn fetch_catalog(root: &Path) -> io::Result<Catalog> {
    let (dictionary_source, language_source) = thread::scope(|scope| {
        let dictionary = scope.spawn(|| fetch_catalog_source(CATALOG_RAW_URL));
        let languages = scope.spawn(|| fetch_catalog_source(ISO_LANGUAGE_RAW_URL));
        let dictionary = dictionary
            .join()
            .map_err(|_| io::Error::other("Dictionary catalogue request stopped"))??;
        let languages = languages
            .join()
            .map_err(|_| io::Error::other("Language catalogue request stopped"))??;
        Ok::<_, io::Error>((dictionary, languages))
    })?;
    let mut catalog = parse_catalog(&dictionary_source, &language_source)?;
    catalog.cached = true;
    primitive_app::with_storage(|| save_cache(root, &catalog))
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(catalog)
}

fn fetch_catalog_source(url: &str) -> io::Result<String> {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(3)
            .http_status_as_error(false)
            .user_agent("PrimitiveOS Reader dictionary catalogue")
            .timeout_global(Some(Duration::from_secs(30)))
            .build(),
    );
    let mut response = agent
        .get(url)
        .call()
        .map_err(|_| io::Error::other("Could not refresh the dictionary catalogue over HTTPS"))?;
    if response.status().as_u16() != 200 {
        return Err(io::Error::other(format!(
            "Dictionary catalogue source returned HTTP {}",
            response.status().as_u16()
        )));
    }
    if response
        .body()
        .content_length()
        .is_some_and(|length| length > MAX_CATALOG_SOURCE_BYTES as u64)
    {
        return Err(io::Error::other("Dictionary catalogue source is too large"));
    }
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_CATALOG_SOURCE_BYTES as u64)
        .reader();
    let mut contents = String::new();
    reader
        .read_to_string(&mut contents)
        .map_err(|_| io::Error::other("Dictionary catalogue source is incomplete or invalid"))?;
    Ok(contents)
}

fn parse_catalog(dictionary_source: &str, language_source: &str) -> io::Result<Catalog> {
    if dictionary_source.len() > MAX_CATALOG_SOURCE_BYTES
        || language_source.len() > MAX_CATALOG_SOURCE_BYTES
    {
        return Err(io::Error::other("Dictionary catalogue source is too large"));
    }
    let (language_names, language_tags) = parse_language_maps(language_source)?;
    let mut packages = Vec::new();
    let mut in_catalog = false;
    let mut in_package = false;
    let mut fields = BTreeMap::<String, String>::new();
    for line in dictionary_source.lines() {
        let line = line.trim();
        if !in_catalog {
            if line == "local dictionaries = {" {
                in_catalog = true;
            }
            continue;
        }
        if !in_package {
            if line == "{" {
                in_package = true;
                fields.clear();
            } else if line == "}" {
                break;
            }
            continue;
        }
        if line == "}," || line == "}" {
            let title = required_field(&fields, "name")?.to_owned();
            let source_code = required_field(&fields, "lang_in")?.to_owned();
            let target_code = required_field(&fields, "lang_out")?.to_owned();
            let entries = required_field(&fields, "entries")?
                .parse::<u32>()
                .map_err(io::Error::other)?;
            let license = fields.get("license").cloned().unwrap_or_default();
            let url = required_field(&fields, "url")?.to_owned();
            let source_language = language_names
                .get(&source_code)
                .cloned()
                .unwrap_or_else(|| source_code.clone());
            let target_language = language_names
                .get(&target_code)
                .cloned()
                .unwrap_or_else(|| target_code.clone());
            let source_tag = language_tags
                .get(&source_code)
                .cloned()
                .unwrap_or_else(|| source_code.clone());
            let target_tag = language_tags
                .get(&target_code)
                .cloned()
                .unwrap_or_else(|| target_code.clone());
            packages.push(Package {
                slug: slug_for_url(&url),
                title,
                source_code,
                target_code,
                source_language,
                target_language,
                ifo_language: format!("{source_tag}-{target_tag}"),
                entries,
                license,
                url,
            });
            if packages.len() > MAX_CATALOG_PACKAGES {
                return Err(io::Error::other(
                    "Dictionary catalogue has too many entries",
                ));
            }
            in_package = false;
            continue;
        }
        if let Some((key, value)) = parse_lua_assignment(line) {
            if matches!(
                key.as_str(),
                "name" | "lang_in" | "lang_out" | "entries" | "license" | "url"
            ) {
                let value = if key == "entries" {
                    value.trim_end_matches(',').trim().to_owned()
                } else if value.trim() == "nil," || value.trim() == "nil" {
                    String::new()
                } else {
                    parse_lua_string(value)?
                };
                fields.insert(key, value);
            }
        }
    }
    if !in_catalog || in_package || packages.len() != EXPECTED_CATALOG_PACKAGES {
        return Err(io::Error::other(
            "Pinned dictionary catalogue could not be parsed completely",
        ));
    }
    let dictionary_source_sha256 = sha256_hex(dictionary_source.as_bytes());
    let language_source_sha256 = sha256_hex(language_source.as_bytes());
    if dictionary_source_sha256 != DICTIONARY_SOURCE_SHA256
        || language_source_sha256 != LANGUAGE_SOURCE_SHA256
    {
        return Err(io::Error::other(
            "Dictionary catalogue does not match the pinned KOReader sources",
        ));
    }
    let catalog = Catalog {
        revision: KO_READER_REVISION.into(),
        packages,
        language_names,
        language_tags,
        dictionary_source_sha256,
        language_source_sha256,
        cached: false,
    };
    validate_catalog(&catalog)?;
    Ok(catalog)
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn required_field<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> io::Result<&'a str> {
    fields
        .get(key)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or_else(|| io::Error::other(format!("Dictionary catalogue entry is missing {key}")))
}

fn parse_lua_assignment(line: &str) -> Option<(String, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    Some((key.to_owned(), value.trim()))
}

fn parse_lua_string(value: &str) -> io::Result<String> {
    let start = value
        .find('"')
        .ok_or_else(|| io::Error::other("Dictionary catalogue string is missing a quote"))?
        + 1;
    let suffix = &value[start..];
    let end = suffix
        .find('"')
        .ok_or_else(|| io::Error::other("Dictionary catalogue string is incomplete"))?;
    let string = &suffix[..end];
    if string.contains('\\') {
        return Err(io::Error::other(
            "Pinned dictionary catalogue uses an unsupported string escape",
        ));
    }
    Ok(string.to_owned())
}

fn parse_language_maps(
    source: &str,
) -> io::Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    let mut names = BTreeMap::new();
    let mut tags = BTreeMap::new();
    let mut section = 0;
    for line in source.lines() {
        let line = line.trim();
        if line == "local locale_lang_map = {" {
            section = 1;
            continue;
        }
        if line == "local iso_bcp47_map = {" {
            section = 2;
            continue;
        }
        if line == "}" {
            section = 0;
            continue;
        }
        if section == 0 {
            continue;
        }
        let Some((code, value)) = parse_lua_assignment(line) else {
            continue;
        };
        if section == 1 {
            names.insert(code, parse_lua_string(value)?);
        } else {
            tags.insert(code, parse_lua_string(value)?);
        }
    }
    if names.len() < 100 || tags.len() < 100 {
        return Err(io::Error::other(
            "Pinned language catalogue could not be parsed completely",
        ));
    }
    Ok((names, tags))
}

pub(crate) enum CatalogPoll {
    Pending,
    Finished(Result<Catalog, String>),
}

pub(crate) struct CatalogRefreshJob {
    receiver: Receiver<Result<Catalog, String>>,
    poll_at: Instant,
}

impl CatalogRefreshJob {
    pub(crate) fn start(root: PathBuf) -> crate::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("dictionary-catalog-refresh".into())
            .spawn(move || {
                let result = fetch_catalog(&root).map_err(|error| error.to_string());
                let _ = sender.send(result);
            })?;
        Ok(Self {
            receiver,
            poll_at: Instant::now() + WORKER_POLL,
        })
    }

    pub(crate) fn poll(&mut self, now: Instant) -> CatalogPoll {
        self.poll_at = now + WORKER_POLL;
        match self.receiver.try_recv() {
            Ok(result) => CatalogPoll::Finished(result),
            Err(TryRecvError::Empty) => CatalogPoll::Pending,
            Err(TryRecvError::Disconnected) => CatalogPoll::Finished(Err(
                "Dictionary catalogue refresh stopped unexpectedly".into(),
            )),
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.poll_at
    }
}

pub(crate) enum SizeCheckPoll {
    Pending,
    Finished(Result<Option<u64>, String>),
}

/// A bounded asynchronous HEAD request used to show the package size before
/// the user starts a dictionary download.
pub(crate) struct SizeCheckJob {
    package_slug: String,
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<Result<Option<u64>, String>>,
    poll_at: Instant,
}

impl SizeCheckJob {
    pub(crate) fn start(package: &Package) -> crate::Result<Self> {
        let package_slug = package.slug.clone();
        let url = package.url.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("dictionary-size-check".into())
            .spawn(move || {
                let result =
                    check_package_size(&url, &worker_cancelled).map_err(|error| error.to_string());
                let _ = sender.send(result);
            })?;
        Ok(Self {
            package_slug,
            cancelled,
            receiver,
            poll_at: Instant::now() + WORKER_POLL,
        })
    }

    pub(crate) fn package_slug(&self) -> &str {
        &self.package_slug
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.poll_at
    }

    pub(crate) fn poll(&mut self, now: Instant) -> SizeCheckPoll {
        self.poll_at = now + WORKER_POLL;
        match self.receiver.try_recv() {
            Ok(result) => SizeCheckPoll::Finished(result),
            Err(TryRecvError::Empty) => SizeCheckPoll::Pending,
            Err(TryRecvError::Disconnected) => SizeCheckPoll::Finished(Err(
                "Dictionary package size check stopped unexpectedly".into(),
            )),
        }
    }
}

impl Drop for SizeCheckJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn check_package_size(url: &str, cancelled: &AtomicBool) -> io::Result<Option<u64>> {
    check_cancelled(cancelled)?;
    let parsed = url::Url::parse(url).map_err(io::Error::other)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(io::Error::other("Dictionary packages must use HTTPS"));
    }
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(4)
            .http_status_as_error(false)
            .user_agent("PrimitiveOS Reader dictionary installer")
            .timeout_global(Some(Duration::from_secs(20)))
            .build(),
    );
    let response = agent
        .head(url)
        .call()
        .map_err(|_| io::Error::other("Could not check dictionary package size over HTTPS"))?;
    check_cancelled(cancelled)?;
    if response.status().as_u16() != 200 {
        return Err(io::Error::other(format!(
            "Dictionary source returned HTTP {} while checking its size",
            response.status().as_u16()
        )));
    }
    Ok(response.body().content_length().filter(|size| *size > 0))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Filter {
    #[default]
    All,
    SameLanguage,
    Bilingual,
}

impl Filter {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::All => Self::SameLanguage,
            Self::SameLanguage => Self::Bilingual,
            Self::Bilingual => Self::All,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::SameLanguage => "Same-language",
            Self::Bilingual => "Bilingual",
        }
    }

    pub(crate) fn accepts(self, package: &Package) -> bool {
        match self {
            Self::All => true,
            Self::SameLanguage => package.source_code == package.target_code,
            Self::Bilingual => package.source_code != package.target_code,
        }
    }
}

pub(crate) struct LanguageGroup {
    pub(crate) language: String,
    pub(crate) packages: Vec<usize>,
}

pub(crate) fn language_groups(catalog: &Catalog) -> Vec<LanguageGroup> {
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, package) in catalog.packages.iter().enumerate() {
        for language in [&package.source_language, &package.target_language] {
            let packages = groups.entry(language.clone()).or_default();
            if !packages
                .iter()
                .any(|existing| catalog.packages[*existing].title == package.title)
            {
                packages.push(index);
            }
        }
    }
    groups
        .into_iter()
        .map(|(language, packages)| LanguageGroup { language, packages })
        .collect()
}

pub(crate) fn installed(root: &Path, package: &Package) -> bool {
    let mut slugs = vec![package.slug.as_str()];
    if let Some(legacy) = legacy_slug(&package.url) {
        slugs.push(legacy);
    }
    slugs.into_iter().any(|slug| {
        fs::symlink_metadata(root.join(slug))
            .is_ok_and(|info| info.is_dir() && !info.file_type().is_symlink())
    })
}

fn legacy_slug(url: &str) -> Option<&'static str> {
    match url {
        "https://gitlab.com/koreader/stardict-dictionaries/uploads/619cbab2537b4d115d5503cdd023ce05/folkets_en-sv.tar.gz" => {
            Some("folkets-en-sv")
        }
        "https://gitlab.com/koreader/stardict-dictionaries/uploads/53a0a9fea8cab8661cf930ddd2353a4c/folkets_sv-en.tar.gz" => {
            Some("folkets-sv-en")
        }
        "https://gitlab.com/koreader/stardict-dictionaries/uploads/b8e8ba6b8941a78762675ff2ef95d1d1/acadfran.tar.gz" => {
            Some("academie-francaise-8e")
        }
        "https://gitlab.com/koreader/stardict-dictionaries/uploads/6339585b68ac485bedb8ee67892cb974/georges_lat-de.tar.gz" => {
            Some("georges-lat-de")
        }
        "https://gitlab.com/koreader/stardict-dictionaries/uploads/a04de66c7376e436913ca288a3ca608b/georges_de-lat.tar.gz" => {
            Some("georges-de-lat")
        }
        "https://github.com/Vuizur/czech-dictionary-extender/releases/download/1.0.0/czech-english-dict.tar.gz" => {
            Some("czech-english")
        }
        "https://gitlab.com/avsej/dicts-stardict-form-xdxf/raw/d636cc5e8d4a47e22ac7466f4af6d435a8a3f650/002c/stardict-comn_sdict05_idioms_eng_eng-2.4.2.tar.gz" => {
            Some("english-idioms")
        }
        _ => None,
    }
}

#[derive(Serialize, Deserialize)]
struct WorkerRequest {
    package: Package,
    dictionary_root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct WorkerReply {
    result: Result<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DownloadProgress {
    received: u64,
    total: Option<u64>,
}

enum WorkerEvent {
    Progress(DownloadProgress),
    Finished(Result<String, String>),
}

pub(crate) enum Poll {
    Pending { progress_changed: bool },
    Finished(Result<String, String>),
}

/// Child-process entry point. Network and archive work stay away from the
/// Reader event loop, and the reply never contains downloaded dictionary data.
pub fn worker() -> crate::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_WORKER_REQUEST_BYTES {
        return Err("Dictionary request is invalid".into());
    }
    let mut payload = vec![0; length];
    input.read_exact(&mut payload)?;
    drop(input);
    let request: WorkerRequest = serde_json::from_slice(&payload)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let monitor_cancelled = Arc::clone(&cancelled);
    let _monitor = thread::Builder::new()
        .name("dictionary-download-cancel".into())
        .spawn(move || {
            let mut byte = [0; 1];
            let received = io::stdin().lock().read_exact(&mut byte).is_ok();
            if !received || byte[0] != 0 {
                monitor_cancelled.store(true, Ordering::Release);
            }
        })?;
    let result = validate_package(&request.package)
        .map_err(|error| error.to_string())
        .and_then(|()| install(&request.package, &request.dictionary_root, &cancelled));
    serde_json::to_writer(io::stdout().lock(), &WorkerReply { result })?;
    Ok(())
}

fn validate_package(package: &Package) -> io::Result<()> {
    let valid_slug = !package.slug.is_empty()
        && package.slug.len() <= 64
        && package
            .slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let valid_language = |code: &str| {
        (2..=8).contains(&code.len())
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-' || byte == b'_')
    };
    let url = url::Url::parse(&package.url).map_err(io::Error::other)?;
    if !valid_slug
        || package.title.is_empty()
        || package.title.len() > 1024
        || package.entries == 0
        || !valid_language(&package.source_code)
        || !valid_language(&package.target_code)
        || package.license.len() > 1024
        || package.url.len() > 4096
        || url.scheme() != "https"
        || url.host_str().is_none()
        || package.ifo_language.len() > 40
        || !package
            .ifo_language
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(io::Error::other("Invalid dictionary catalogue entry"));
    }
    Ok(())
}

fn append_ifo_language(root: &Path, language: &str, cancelled: &AtomicBool) -> io::Result<()> {
    if language.is_empty()
        || language.len() > 40
        || !language
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(io::Error::other("Dictionary language tag is invalid"));
    }
    let mut directories = vec![root.to_owned()];
    let mut found = false;
    while let Some(directory) = directories.pop() {
        check_cancelled(cancelled)?;
        for entry in fs::read_dir(directory)? {
            check_cancelled(cancelled)?;
            let entry = entry?;
            let path = entry.path();
            let kind = fs::symlink_metadata(&path)?.file_type();
            if kind.is_dir() {
                directories.push(path);
                continue;
            }
            if !kind.is_file()
                || !path
                    .extension()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ifo"))
            {
                continue;
            }
            if fs::metadata(&path)?.len() > 64 * 1024 {
                return Err(io::Error::other("StarDict IFO file is too large"));
            }
            let contents = fs::read(&path)?;
            let mut output = OpenOptions::new().append(true).open(path)?;
            if !contents.ends_with(b"\n") {
                output.write_all(b"\n")?;
            }
            writeln!(output, "lang={language}")?;
            found = true;
        }
    }
    if !found {
        return Err(io::Error::other(
            "Dictionary archive has no StarDict IFO file",
        ));
    }
    Ok(())
}

fn install(package: &Package, root: &Path, cancelled: &AtomicBool) -> Result<String, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Dictionary download canceled".into());
    }
    primitive_app::with_storage(|| install_with_storage(package, root, cancelled))
        .map_err(|error| error.to_string())
}

fn install_with_storage(
    package: &Package,
    root: &Path,
    cancelled: &AtomicBool,
) -> io::Result<String> {
    check_cancelled(cancelled)?;
    validate_package(package)?;
    match fs::symlink_metadata(root) {
        Ok(info) if info.is_dir() && !info.file_type().is_symlink() => (),
        Ok(_) => {
            return Err(io::Error::other(
                "Dictionary storage is not a regular directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(root)?,
        Err(error) => return Err(error),
    }
    let destination = root.join(&package.slug);
    if fs::symlink_metadata(&destination).is_ok() || installed(root, package) {
        return Err(io::Error::other("This dictionary is already installed"));
    }

    let unique = format!(
        ".install-{}-{}-{}",
        package.slug,
        std::process::id(),
        SystemTimeStamp::now()
    );
    let workspace = root.join(&unique);
    let extracted = workspace.join("dictionary");
    let archive = workspace.join("package.archive");
    let prepared = root.join(format!("{unique}-ready"));
    fs::create_dir(&workspace)?;
    let outcome = (|| {
        fs::create_dir(&extracted)?;
        download(&package.url, &archive, cancelled)?;
        check_cancelled(cancelled)?;
        let archive_file = fs::File::open(&archive)?;
        extract_dictionary_archive(archive_file, &package.url, &extracted, cancelled)?;
        fs::remove_file(&archive)?;
        check_cancelled(cancelled)?;
        append_ifo_language(&extracted, &package.ifo_language, cancelled)?;
        check_cancelled(cancelled)?;
        let title = validate_star_dict(&extracted)?;
        check_cancelled(cancelled)?;
        let attribution = format!(
            "{}\n\nLanguage pair: {}\nLicense (KOReader catalogue): {}\n\nPackage source: {}\nCatalogue source (KOReader revision {}): {}\n\nThis package was downloaded by explicit user action. Its license applies to the dictionary data, not to PrimitiveOS Reader.\n",
            package.title,
            package.language_pair(),
            if package.license.is_empty() {
                "Not stated in the KOReader catalogue"
            } else {
                &package.license
            },
            &package.url,
            KO_READER_REVISION,
            CATALOG_SOURCE,
        );
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(extracted.join("ATTRIBUTION.txt"))?
            .write_all(attribution.as_bytes())?;
        check_cancelled(cancelled)?;
        fs::rename(&extracted, &prepared)?;
        fs::remove_dir_all(&workspace)?;
        check_cancelled(cancelled)?;
        fs::rename(&prepared, &destination)?;
        Ok(title)
    })();
    if outcome.is_err() {
        let _ = fs::remove_dir_all(&workspace);
        let _ = fs::remove_dir_all(&prepared);
    }
    outcome
}

fn download(url: &str, destination: &Path, cancelled: &AtomicBool) -> io::Result<()> {
    let parsed = url::Url::parse(url).map_err(io::Error::other)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(io::Error::other("Dictionary packages must use HTTPS"));
    }
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(4)
            .http_status_as_error(false)
            .user_agent("PrimitiveOS Reader dictionary installer")
            .timeout_global(Some(Duration::from_secs(90)))
            .build(),
    );
    let mut response = agent
        .get(url)
        .call()
        .map_err(|_| io::Error::other("Could not download the dictionary over HTTPS"))?;
    if response.status().as_u16() != 200 {
        return Err(io::Error::other(format!(
            "Dictionary source returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let total = response.body().content_length();
    if total.is_some_and(|length| length > MAX_COMPRESSED_BYTES as u64) {
        return Err(io::Error::other("Dictionary archive is too large"));
    }
    let mut archive = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_COMPRESSED_BYTES as u64)
        .reader();
    let mut chunk = [0; DOWNLOAD_CHUNK_BYTES];
    let mut received = 0u64;
    let mut last_reported = 0u64;
    let mut last_reported_at = Instant::now();
    loop {
        check_cancelled(cancelled)?;
        let count = reader
            .read(&mut chunk)
            .map_err(|_| io::Error::other("Dictionary archive is too large or incomplete"))?;
        if count == 0 {
            break;
        }
        archive.write_all(&chunk[..count])?;
        received = received.saturating_add(count as u64);
        let elapsed = last_reported_at.elapsed();
        if received.saturating_sub(last_reported) >= DOWNLOAD_PROGRESS_BYTES
            || elapsed >= DOWNLOAD_PROGRESS_INTERVAL
        {
            report_download_progress(received, total);
            last_reported = received;
            last_reported_at = Instant::now();
        }
    }
    if total.is_some_and(|expected| expected != received) {
        return Err(io::Error::other(
            "Dictionary archive download is incomplete",
        ));
    }
    archive.sync_all()?;
    report_download_progress(received, total);
    Ok(())
}

fn report_download_progress(received: u64, total: Option<u64>) {
    let total = total.map_or_else(|| "unknown".to_owned(), |length| length.to_string());
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "DOWNLOAD_PROGRESS {received} {total}");
    let _ = stderr.flush();
}

fn parse_download_progress(line: &str) -> Option<DownloadProgress> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "DOWNLOAD_PROGRESS" {
        return None;
    }
    let received = fields.next()?.parse().ok()?;
    let total = match fields.next()? {
        "unknown" => None,
        value => Some(value.parse().ok()?),
    };
    Some(DownloadProgress { received, total })
}

impl DownloadProgress {
    pub(crate) fn label(self) -> String {
        match self.total {
            Some(total) => format!(
                "Downloaded {} of {}",
                format_download_size(self.received),
                format_download_size(total)
            ),
            None => format!("Downloaded {}", format_download_size(self.received)),
        }
    }
}

pub(crate) fn format_download_size(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KiB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn extract_dictionary_archive(
    archive: impl Read + Seek,
    package_url: &str,
    destination: &Path,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let url = url::Url::parse(package_url).map_err(io::Error::other)?;
    let path = url.path().to_ascii_lowercase();
    if path.ends_with(".tar.gz") || path.ends_with(".tgz") {
        return extract_tar(MultiGzDecoder::new(archive), destination, cancelled);
    }
    if path.ends_with(".tar.bz2") || path.ends_with(".tbz2") {
        return extract_tar(BzDecoder::new(archive), destination, cancelled);
    }
    if path.ends_with(".tar.zst") || path.ends_with(".tzst") {
        let mut decoder = zstd::stream::read::Decoder::new(archive)?;
        decoder.window_log_max(26)?;
        return extract_tar(decoder, destination, cancelled);
    }
    if path.ends_with(".zip") {
        return extract_zip(archive, destination, cancelled);
    }
    Err(io::Error::other(
        "Dictionary archive uses an unsupported format",
    ))
}

fn extract_tar(
    mut decoder: impl Read,
    destination: &Path,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let mut total = 0u64;
    let mut entries = 0usize;
    let mut saw_entry = false;
    loop {
        check_cancelled(cancelled)?;
        let mut header = [0u8; TAR_BLOCK];
        read_tar_block(&mut decoder, &mut header, &mut total)?;
        if header.iter().all(|byte| *byte == 0) {
            let mut second = [0u8; TAR_BLOCK];
            read_tar_block(&mut decoder, &mut second, &mut total)?;
            if second.iter().any(|byte| *byte != 0) {
                return Err(io::Error::other("Dictionary TAR trailer is malformed"));
            }
            break;
        }
        entries += 1;
        if entries > MAX_ARCHIVE_ENTRIES {
            return Err(io::Error::other("Dictionary archive has too many entries"));
        }
        validate_header(&header)?;
        let name = tar_name(&header)?;
        let size = tar_octal(&header[124..136])
            .ok_or_else(|| io::Error::other("Dictionary TAR entry has an invalid size"))?;
        if size > MAX_FILE_BYTES || total.saturating_add(size) > MAX_UNCOMPRESSED_BYTES {
            return Err(io::Error::other(
                "Dictionary archive exceeds Reader's size limits",
            ));
        }
        let file_type = header[156];
        let path = safe_member_path(destination, &name)?;
        match file_type {
            b'5' if size == 0 => {
                if path != destination {
                    fs::create_dir_all(path)?;
                }
            }
            0 | b'0' => {
                if !saw_entry && name.is_empty() {
                    return Err(io::Error::other("Dictionary TAR entry has no name"));
                }
                let parent = path
                    .parent()
                    .ok_or_else(|| io::Error::other("Dictionary TAR path has no parent"))?;
                fs::create_dir_all(parent)?;
                let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
                copy_member(&mut decoder, &mut output, size, &mut total, cancelled)?;
                output.flush()?;
                saw_entry = true;
            }
            _ => {
                return Err(io::Error::other(
                    "Dictionary archive uses a link or unsupported TAR entry",
                ));
            }
        }
        let padding = (TAR_BLOCK as u64 - size % TAR_BLOCK as u64) % TAR_BLOCK as u64;
        skip_exact(&mut decoder, padding, &mut total, cancelled)?;
    }
    if !saw_entry {
        return Err(io::Error::other("Dictionary archive is empty"));
    }
    let mut trailing = [0u8; 4096];
    loop {
        let count = decoder.read(&mut trailing)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > MAX_UNCOMPRESSED_BYTES || trailing[..count].iter().any(|byte| *byte != 0) {
            return Err(io::Error::other("Dictionary TAR has invalid trailing data"));
        }
    }
    Ok(())
}

fn extract_zip(
    archive: impl Read + Seek,
    destination: &Path,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let mut archive = zip::ZipArchive::new(archive).map_err(io::Error::other)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(io::Error::other("Dictionary archive has too many entries"));
    }
    let mut total = 0u64;
    let mut saw_file = false;
    for index in 0..archive.len() {
        check_cancelled(cancelled)?;
        let mut entry = archive.by_index(index).map_err(io::Error::other)?;
        let name = entry.name().to_owned();
        let path = safe_member_path(destination, &name)?;
        let size = entry.size();
        if size > MAX_FILE_BYTES || total.saturating_add(size) > MAX_UNCOMPRESSED_BYTES {
            return Err(io::Error::other(
                "Dictionary archive exceeds Reader's size limits",
            ));
        }
        let kind = entry.unix_mode().map(|mode| mode & 0o170000).unwrap_or(0);
        if kind != 0 && !matches!(kind, 0o100000 | 0o040000) {
            return Err(io::Error::other(
                "Dictionary archive uses a link or unsupported ZIP entry",
            ));
        }
        let is_directory = entry.is_dir() || kind == 0o040000;
        if is_directory {
            if size != 0 {
                return Err(io::Error::other(
                    "Dictionary archive contains a non-empty directory entry",
                ));
            }
            if path != destination {
                fs::create_dir_all(path)?;
            }
            continue;
        }
        if path == destination {
            return Err(io::Error::other("Dictionary ZIP entry has no file name"));
        }
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Dictionary ZIP path has no parent"))?;
        fs::create_dir_all(parent)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
        copy_zip_member(&mut entry, &mut output, size, &mut total, cancelled)?;
        output.flush()?;
        saw_file = true;
    }
    if !saw_file {
        return Err(io::Error::other("Dictionary archive is empty"));
    }
    Ok(())
}

fn copy_zip_member(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected_size: u64,
    total: &mut u64,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let mut buffer = [0; 16 * 1024];
    let mut member_size = 0u64;
    loop {
        check_cancelled(cancelled)?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        member_size = member_size.saturating_add(count as u64);
        *total = total.saturating_add(count as u64);
        if member_size > expected_size
            || member_size > MAX_FILE_BYTES
            || *total > MAX_UNCOMPRESSED_BYTES
        {
            return Err(io::Error::other(
                "Dictionary archive exceeds Reader's size limits",
            ));
        }
        writer.write_all(&buffer[..count])?;
    }
    if member_size != expected_size {
        return Err(io::Error::other(
            "Dictionary ZIP entry has an invalid uncompressed size",
        ));
    }
    Ok(())
}

fn read_tar_block(
    reader: &mut impl Read,
    block: &mut [u8; TAR_BLOCK],
    total: &mut u64,
) -> io::Result<()> {
    reader.read_exact(block)?;
    *total = total.saturating_add(TAR_BLOCK as u64);
    if *total > MAX_UNCOMPRESSED_BYTES {
        return Err(io::Error::other(
            "Dictionary archive exceeds Reader's size limit",
        ));
    }
    Ok(())
}

fn copy_member(
    reader: &mut impl Read,
    writer: &mut impl Write,
    mut bytes: u64,
    total: &mut u64,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let mut buffer = [0; 16 * 1024];
    while bytes > 0 {
        check_cancelled(cancelled)?;
        let count = bytes.min(buffer.len() as u64) as usize;
        reader.read_exact(&mut buffer[..count])?;
        writer.write_all(&buffer[..count])?;
        bytes -= count as u64;
        *total = (*total).saturating_add(count as u64);
        if *total > MAX_UNCOMPRESSED_BYTES {
            return Err(io::Error::other(
                "Dictionary archive exceeds Reader's size limit",
            ));
        }
    }
    Ok(())
}

fn skip_exact(
    reader: &mut impl Read,
    mut bytes: u64,
    total: &mut u64,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let mut buffer = [0; 4096];
    while bytes > 0 {
        check_cancelled(cancelled)?;
        let count = bytes.min(buffer.len() as u64) as usize;
        reader.read_exact(&mut buffer[..count])?;
        bytes -= count as u64;
        *total = total.saturating_add(count as u64);
        if *total > MAX_UNCOMPRESSED_BYTES {
            return Err(io::Error::other(
                "Dictionary archive exceeds Reader's size limit",
            ));
        }
    }
    Ok(())
}

fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Dictionary download canceled",
        ))
    } else {
        Ok(())
    }
}

fn validate_header(header: &[u8; TAR_BLOCK]) -> io::Result<()> {
    if &header[257..263] != b"ustar\0" && &header[257..265] != b"ustar  \0" {
        return Err(io::Error::other("Dictionary archive is not USTAR"));
    }
    let checksum = tar_octal(&header[148..156])
        .ok_or_else(|| io::Error::other("Dictionary TAR checksum is invalid"))?;
    let actual = header[..148]
        .iter()
        .chain(std::iter::repeat_n(&b' ', 8))
        .chain(&header[156..])
        .map(|byte| u64::from(*byte))
        .sum::<u64>();
    if actual != checksum {
        return Err(io::Error::other("Dictionary TAR checksum does not match"));
    }
    Ok(())
}

fn tar_name(header: &[u8; TAR_BLOCK]) -> io::Result<String> {
    let field = |bytes: &[u8]| {
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        std::str::from_utf8(&bytes[..end])
            .map(str::to_owned)
            .map_err(io::Error::other)
    };
    let name = field(&header[..100])?;
    let prefix = field(&header[345..500])?;
    let name = if prefix.is_empty() {
        name
    } else if name.is_empty() {
        return Err(io::Error::other("Dictionary TAR path is invalid"));
    } else {
        format!("{prefix}/{name}")
    };
    let name = name.trim_start_matches("./");
    Ok(if name.is_empty() {
        ".".to_owned()
    } else {
        name.to_owned()
    })
}

fn tar_octal(field: &[u8]) -> Option<u64> {
    let field = field
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    let text = std::str::from_utf8(&field).ok()?.trim();
    if text.is_empty() || !text.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
        return None;
    }
    u64::from_str_radix(text, 8).ok()
}

fn safe_member_path(destination: &Path, member: &str) -> io::Result<PathBuf> {
    if member.is_empty() || member.contains('\\') || member.contains('\0') {
        return Err(io::Error::other("Dictionary TAR path is invalid"));
    }
    let path = Path::new(member);
    let mut output = destination.to_owned();
    for component in path.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(part) => output.push(part),
            _ => return Err(io::Error::other("Dictionary TAR path escapes its package")),
        }
    }
    if output == destination {
        return Ok(output);
    }
    Ok(output)
}

fn validate_star_dict(root: &Path) -> io::Result<String> {
    crate::dictionary::validate_star_dict_package(root)
}

struct SystemTimeStamp;

impl SystemTimeStamp {
    fn now() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }
}

pub(crate) struct Job {
    child: Option<Child>,
    input: Option<ChildStdin>,
    receiver: Receiver<WorkerEvent>,
    progress: Option<DownloadProgress>,
    poll_at: Instant,
    end: Instant,
    package: String,
    cancel_requested: bool,
}

impl Job {
    pub(crate) fn start(package: Package, dictionary_root: PathBuf) -> crate::Result<Self> {
        validate_package(&package)?;
        let package_slug = package.slug.clone();
        let payload = serde_json::to_vec(&WorkerRequest {
            package,
            dictionary_root,
        })?;
        if payload.len() > MAX_WORKER_REQUEST_BYTES {
            return Err("Dictionary request is too large".into());
        }
        let length = u32::try_from(payload.len())
            .map_err(|_| "Dictionary request is too large")?
            .to_be_bytes();
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--dictionary-download-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut input = child.stdin.take().expect("piped dictionary request");
        let output = child.stdout.take().expect("piped dictionary response");
        let progress_output = child.stderr.take().expect("piped dictionary progress");
        if let Err(error) = input
            .write_all(&length)
            .and_then(|()| input.write_all(&payload))
            .and_then(|()| input.flush())
        {
            drop(input);
            reap_child(child);
            return Err(error.into());
        }
        let (sender, receiver) = mpsc::channel();
        let result_sender = sender.clone();
        if let Err(error) = thread::Builder::new()
            .name("dictionary-download-result".into())
            .spawn(move || {
                let response = (|| -> std::io::Result<_> {
                    let reply: WorkerReply =
                        serde_json::from_reader(output.take(WORKER_RESPONSE_LIMIT))
                            .map_err(io::Error::other)?;
                    Ok(reply.result)
                })()
                .unwrap_or_else(|_| {
                    Err("Dictionary download ended before installation completed".into())
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
            .name("dictionary-download-progress".into())
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
            child: Some(child),
            input: Some(input),
            receiver,
            progress: None,
            poll_at: now + WORKER_POLL,
            end: now + WORKER_TIMEOUT,
            package: package_slug,
            cancel_requested: false,
        })
    }

    pub(crate) fn package(&self) -> &str {
        &self.package
    }

    pub(crate) fn progress_label(&self) -> Option<String> {
        self.progress.map(DownloadProgress::label)
    }

    pub(crate) fn cancel(&mut self) {
        if self.cancel_requested {
            return;
        }
        self.cancel_requested = true;
        if let Some(mut input) = self.input.take() {
            let _ = input.write_all(&[1]);
            let _ = input.flush();
        }
    }

    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel_requested
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.poll_at
    }

    pub(crate) fn poll(&mut self, now: Instant) -> Poll {
        self.poll_at = now + WORKER_POLL;
        let mut progress_changed = false;
        loop {
            match self.receiver.try_recv() {
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
                    return Poll::Finished(Err("Dictionary installer stopped unexpectedly".into()));
                }
                Err(TryRecvError::Empty) if now >= self.end => {
                    self.cancel();
                    return Poll::Finished(Err("Dictionary download timed out".into()));
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

fn reap_child(mut child: Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = thread::Builder::new()
        .name("dictionary-download-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        self.reap_if_finished();
    }
}
