//! Offline StarDict 2.4.2 and 3.0.0 lookup from a configured directory.
//! Stream StarDict indexes and seek definition slices in plain or dictzip data.

#[cfg(any(feature = "kindle", test))]
mod spelling;
#[cfg(feature = "kindle")]
pub(crate) use spelling::eligible as spelling_eligible;
mod word_forms;
use word_forms::{Ranked, WordForms};

use std::cell::{Cell, RefCell};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use flate2::{Decompress, FlushDecompress, Status, bufread::GzDecoder, read::MultiGzDecoder};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
};
use icu_casemap::{CaseMapper, CaseMapperBorrowed};

const MAX_DIRECTORIES_DEPTH: usize = 2;
const MAX_SCAN_ENTRIES: usize = 1024;
const MAX_DICTIONARIES: usize = 16;
const MAX_INFO_BYTES: usize = 32 * 1024;
const MAX_INDEX_BYTES: usize = 16 * 1024 * 1024;
const MAX_COMPRESSED_INDEX_BYTES: u64 = 17 * 1024 * 1024;
const MAX_TOTAL_INDEX_BYTES: usize = 32 * 1024 * 1024;
const MAX_WORDS: usize = 1_000_000;
const MAX_WORD_BYTES: usize = 255;
const MAX_FIELDS: usize = 32;
const MAX_ENTRY_BYTES: usize = 64 * 1024;
const MAX_RESULTS: usize = 12;
const MAX_DZ_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DZ_DATA_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DZ_HEADER_BYTES: u64 = 128 * 1024;
const MAX_DZ_CHUNKS: usize = 16_384;
const MAX_DZ_ENTRY_CHUNKS: usize = 64;
const MAX_DZ_ENTRY_COMPRESSED_BYTES: u64 = 256 * 1024;
const MAX_DZ_LOOKUP_BYTES: u64 = 4 * 1024 * 1024;

struct LookupBudget {
    index_bytes: usize,
    dictzip_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub dictionary: String,
    pub word: String,
    /// Display-ready text. Markup is flattened and never opens its resources.
    pub text: String,
}

#[cfg(feature = "kindle")]
pub(crate) struct InstalledDictionary {
    pub(crate) path: PathBuf,
    pub(crate) title: String,
}

/// Search a Reader installation and the two standard KOReader dictionary directories.
#[derive(Clone)]
pub(crate) struct DictionarySources {
    local: PathBuf,
    shared_storage: Option<PathBuf>,
}

struct DictionaryFile {
    root: PathBuf,
    path: PathBuf,
    // Local preferences retain their original relative paths. External keys are
    // absolute-looking identifiers only; a stored key is never opened as a path.
    key: PathBuf,
}

impl DictionarySources {
    #[cfg(feature = "kindle")]
    pub(crate) fn for_reader(documents: &Path, data: &Path) -> Self {
        Self {
            local: data.join("dictionaries"),
            shared_storage: documents
                .parent()
                .filter(|_| {
                    documents
                        .file_name()
                        .is_some_and(|name| name == "documents")
                })
                .map(Path::to_owned),
        }
    }

    fn files(&self) -> io::Result<Vec<DictionaryFile>> {
        let mut roots = vec![(self.local.clone(), PathBuf::new())];
        if let Some(storage) = &self.shared_storage {
            let home = storage.join("koreader");
            let data = home.join("data");
            if [&home, &data]
                .into_iter()
                .all(|dir| fs::symlink_metadata(dir).is_ok_and(|info| info.is_dir()))
            {
                for name in ["dict", "dict_ext"] {
                    roots.push((data.join(name), Path::new("/koreader/data").join(name)));
                }
            }
        }
        let mut files = Vec::new();
        let mut entries = 0;
        for (directory, prefix) in roots {
            let found = (|| -> io::Result<()> {
                let Some(root) = dictionary_root(&directory)? else {
                    return Ok(());
                };
                for path in dictionary_files(&root, &mut entries, MAX_DICTIONARIES - files.len())? {
                    let key =
                        prefix.join(path.strip_prefix(&root).expect("discovered beneath root"));
                    files.push(DictionaryFile {
                        root: root.clone(),
                        path,
                        key,
                    });
                }
                Ok(())
            })();
            if prefix.as_os_str().is_empty() {
                found?;
            }
        }
        Ok(files)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn installed(&self) -> io::Result<Vec<InstalledDictionary>> {
        Ok(self
            .files()?
            .into_iter()
            .filter_map(|file| {
                let title = Info::load(open_dictionary_file(&file.root, &file.path).ok()?)
                    .ok()?
                    .title;
                Some(InstalledDictionary {
                    path: file.key,
                    title,
                })
            })
            .collect())
    }

    pub(crate) fn lookup(
        &self,
        word: &str,
        preferred: Option<&Path>,
    ) -> io::Result<Vec<Definition>> {
        let first: Vec<_> = preferred.iter().map(|path| path.to_path_buf()).collect();
        self.lookup_ordered(word, &[], &first, &[])
    }

    fn enabled(&self, book_order: &[PathBuf], order: &[PathBuf], disabled: &[PathBuf])
        -> io::Result<(Vec<(DictionaryFile, Info)>, Option<io::Error>)> {
        let mut first_error = None;
        let mut sources = Vec::new();
        for file in self.files()? {
            if disabled.contains(&file.key) && !book_order.contains(&file.key) { continue; }
            match open_dictionary_file(&file.root, &file.path).and_then(Info::load) {
                Ok(info) => sources.push((file, info)),
                Err(error) => { first_error.get_or_insert(error); }
            }
        }
        sources.sort_by_cached_key(|(file, info)| {
            let rank = book_order.iter().chain(order).position(|key| key == &file.key).unwrap_or(usize::MAX);
            (rank, info.title.to_lowercase(), file.key.is_absolute(), file.key.clone())
        });
        Ok((sources, first_error))
    }

    pub(crate) fn lookup_ordered(&self, word: &str, book_order: &[PathBuf], order: &[PathBuf],
                                 disabled: &[PathBuf]) -> io::Result<Vec<Definition>> {
        self.lookup_configured(word, book_order, order, disabled, false)
    }

    pub(crate) fn lookup_configured(&self, word: &str, book_order: &[PathBuf], order: &[PathBuf],
                                    disabled: &[PathBuf], with_word_forms: bool) -> io::Result<Vec<Definition>> {
        let word = word.trim();
        if word.len() > MAX_WORD_BYTES {
            return Ok(Vec::new());
        }
        let unwrapped = crate::search::without_soft_hyphens(word);
        let term = unwrapped.trim();
        if term.is_empty() {
            return Ok(Vec::new());
        }
        let (sources, mut first_error) = self.enabled(book_order, order, disabled)?;
        let folded = CaseMapper::new().fold_string(term);
        let forms = with_word_forms.then(|| WordForms::for_word(term));
        let query = Query {
            forms: forms.as_ref(),
            word,
            folded: &folded,
        };
        let mut hits = Hits::new();
        let mut budget = LookupBudget {
            index_bytes: MAX_TOTAL_INDEX_BYTES,
            dictzip_bytes: MAX_DZ_LOOKUP_BYTES,
        };
        let mut read_any = false;
        for (DictionaryFile { root, path, .. }, info) in sources {
            let aliases = synonym_targets(&root, &path, &info, query, &mut budget)
                .unwrap_or_default();
            let result = lookup_dictionary(&root, &path, &info, IndexQuery { headword: query, aliases }, &mut budget);
            match result {
                Ok(found) => {
                    read_any = true;
                    if with_word_forms {
                        let remaining = MAX_RESULTS - hits.exact.len();
                        hits.exact.extend(found.into_best().into_iter().take(remaining));
                    } else {
                        hits.extend(found);
                    }
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
            if hits.exact.len() == MAX_RESULTS || budget.index_bytes == 0 {
                break;
            }
        }
        if !read_any
            && let Some(error) = first_error
        {
            return Err(error);
        }
        Ok(hits.into_best())
    }
}

/// Find exact and Unicode caseless headwords and synonyms. Close all files before returning.
pub fn lookup(root: &Path, word: &str) -> io::Result<Vec<Definition>> {
    lookup_preferred(root, word, None)
}

fn lookup_preferred(
    root: &Path,
    word: &str,
    preferred: Option<&Path>,
) -> io::Result<Vec<Definition>> {
    DictionarySources {
        local: root.to_owned(),
        shared_storage: None,
    }
    .lookup(word, preferred)
}


fn dictionary_root(root: &Path) -> io::Result<Option<PathBuf>> {
    match fs::symlink_metadata(root) {
        Ok(info) if info.is_dir() => root.canonicalize().map(Some),
        Ok(_) => Err(invalid("The dictionary path is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn dictionary_files(root: &Path, entries: &mut usize, limit: usize) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![(0, fs::read_dir(root)?)];
    while let Some((depth, directory)) = directories.last_mut() {
        let Some(entry) = directory.next() else {
            directories.pop();
            continue;
        };
        if *entries == MAX_SCAN_ENTRIES || files.len() == limit {
            break;
        }
        *entries += 1;
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() && *depth < MAX_DIRECTORIES_DEPTH {
            let depth = *depth + 1;
            if let Ok(children) = fs::read_dir(path) {
                directories.push((depth, children));
            }
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "ifo") {
            files.push(path);
        }
    }
    files.sort_unstable();
    Ok(files)
}

struct Info {
    title: String,
    words: usize,
    synonyms: Option<usize>,
    index_size: usize,
    offset_bytes: usize,
    fields: Option<Vec<u8>>,
}

impl Info {
    fn load(file: File) -> io::Result<Self> {
        let mut bytes = Vec::new();
        file.take((MAX_INFO_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_INFO_BYTES {
            return Err(invalid("The dictionary metadata exceeds its size limit"));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("Dictionary metadata must be UTF-8"))?;
        let mut lines = text.trim_start_matches('\u{feff}').lines();
        if lines.next() != Some("StarDict's dict ifo file") {
            return Err(invalid("The dictionary metadata is not StarDict"));
        }
        let mut version = "";
        let mut title = None;
        let mut words = None;
        let mut synonyms = None;
        let mut index_size = None;
        let mut offset_bits = "32";
        let mut fields = None;
        for (key, value) in lines.filter_map(|line| line.split_once('=')) {
            let value = value.trim();
            match key {
                "version" => version = value,
                "bookname" if !value.is_empty() => {
                    title = Some(
                        value
                            .chars()
                            .filter(|ch| !ch.is_control())
                            .take(160)
                            .collect(),
                    );
                }
                "wordcount" => words = positive_number(value, MAX_WORDS),
                "synwordcount" => synonyms = positive_number(value, MAX_WORDS),
                "idxfilesize" => index_size = positive_number(value, MAX_INDEX_BYTES),
                "idxoffsetbits" => offset_bits = value,
                "sametypesequence" => {
                    if value.is_empty()
                        || value.len() > MAX_FIELDS
                        || !value.bytes().all(|b| b.is_ascii_alphabetic())
                    {
                        return Err(invalid(
                            "The dictionary lists an invalid data type sequence",
                        ));
                    }
                    fields = Some(value.as_bytes().to_vec());
                }
                _ => {}
            }
        }
        let offset_bytes = match (version, offset_bits) {
            ("2.4.2" | "3.0.0", "32") => 4,
            ("3.0.0", "64") => 8,
            _ => {
                return Err(invalid(
                    "The StarDict version or index offset width is unsupported",
                ));
            }
        };
        Ok(Self {
            title: title.ok_or_else(|| invalid("The dictionary has no title"))?,
            words: words
                .ok_or_else(|| invalid("The dictionary word count is missing or too large"))?,
            synonyms,
            index_size: index_size
                .ok_or_else(|| invalid("The dictionary index size is missing or too large"))?,
            offset_bytes,
            fields,
        })
    }
}

fn positive_number(text: &str, max: usize) -> Option<usize> {
    text.parse::<usize>()
        .ok()
        .filter(|value| (1..=max).contains(value))
}

struct Hits<T> {
    exact: Vec<T>,
    fallback: Vec<T>,
}

impl<T> Hits<T> {
    fn new() -> Self {
        Self {
            exact: Vec::new(),
            fallback: Vec::new(),
        }
    }

    fn extend(&mut self, found: Self) {
        self.exact
            .extend(found.exact.into_iter().take(MAX_RESULTS - self.exact.len()));
        self.fallback.extend(
            found
                .fallback
                .into_iter()
                .take(MAX_RESULTS - self.fallback.len()),
        );
    }

    fn into_best(self) -> Vec<T> {
        if self.exact.is_empty() {
            self.fallback
        } else {
            self.exact
        }
    }
}

#[derive(Clone, Copy)]
struct Query<'a> {
    word: &'a str,
    folded: &'a str,
    forms: Option<&'a WordForms>,
}

impl Query<'_> {
    fn caseless(self, headword: &str, mapper: CaseMapperBorrowed<'_>) -> bool {
        if headword.is_ascii() {
            headword.eq_ignore_ascii_case(self.folded)
        } else {
            let unwrapped = crate::search::without_soft_hyphens(headword);
            mapper.fold_string(&unwrapped) == self.folded
        }
    }
}

struct IndexQuery<'a> {
    headword: Query<'a>,
    aliases: Aliases,
}

struct Aliases {
    direct: Hits<usize>,
    forms: Ranked<usize>,
}

impl Default for Aliases {
    fn default() -> Self { Self { direct: Hits::new(), forms: Ranked::default() } }
}

struct IndexMatch {
    word: String,
    offset: u64,
    size: usize,
}

fn open_index_and_data(root: &Path, path: &Path, info: &Info, budget: &mut LookupBudget)
    -> io::Result<(Box<dyn Read>, Data)> {
    let (index, gzip) = open_variant(root, path, "idx", "idx.gz")?;
    let file_size = index.metadata()?.len();
    if gzip && file_size > MAX_COMPRESSED_INDEX_BYTES {
        return Err(invalid(
            "The compressed dictionary index exceeds its size limit",
        ));
    }
    if !gzip && file_size != info.index_size as u64 {
        return Err(invalid(
            "The dictionary index size does not match its metadata",
        ));
    }
    if info.index_size > budget.index_bytes {
        return Err(invalid(
            "The combined dictionary indexes exceed the lookup limit",
        ));
    }
    budget.index_bytes -= info.index_size;
    let (file, dictzip) = open_variant(root, path, "dict", "dict.dz")?;
    let data = if dictzip {
        Data::DictZip(DictZip::open(file)?)
    } else {
        Data::Plain(file)
    };
    let index: Box<dyn Read> = if gzip { Box::new(MultiGzDecoder::new(index.take(file_size))) } else { Box::new(index) };
    Ok((index,data))
}

fn read_definitions(data: &mut Data, info: &Info, budget: &mut LookupBudget,
    matches: Vec<IndexMatch>) -> io::Result<Vec<Definition>> {
    let mut definitions = Vec::new();
    for matched in matches {
        let text = if matched.size == 0 {
            // Some bilingual indexes intentionally contain headword-only entries.
            Some(String::new())
        } else {
            let bytes = data.read(matched.offset, matched.size, &mut budget.dictzip_bytes)?;
            definition(&bytes, info.fields.as_deref())
        };
        if let Some(text) = text {
            definitions.push(Definition {
                dictionary: info.title.clone(),
                word: matched.word,
                text,
            });
        }
    }
    definitions.sort_by_key(|entry| entry.text.is_empty());
    Ok(definitions)
}

fn lookup_dictionary(
    root: &Path,
    path: &Path,
    info: &Info,
    query: IndexQuery<'_>,
    budget: &mut LookupBudget,
) -> io::Result<Hits<Definition>> {
    let (index, mut data) = open_index_and_data(root, path, info, budget)?;
    let matches = scan_index(index, info, query, data.len()?)?;
    let mut read_hits = |hits| read_definitions(&mut data, info, budget, hits);
    let exact = read_hits(matches.exact)?;
    let fallback = if exact.is_empty() {
        read_hits(matches.fallback)?
    } else {
        Vec::new()
    };
    Ok(Hits { exact, fallback })
}

fn open_variant(
    root: &Path,
    path: &Path,
    plain: &str,
    compressed: &str,
) -> io::Result<(File, bool)> {
    match open_dictionary_file(root, &path.with_extension(plain)) {
        Ok(file) => Ok((file, false)),
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        Err(_) => match open_dictionary_file(root, &path.with_extension(compressed)) {
            Ok(file) => Ok((file, true)),
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            Err(_) => Err(invalid(&format!(
                "The dictionary needs a .{plain} or .{compressed} file"
            ))),
        },
    }
}

fn open_dictionary_file(root: &Path, path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid("Dictionary files must be regular files"));
    }
    let path = path.canonicalize()?;
    if !path.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The dictionary file is outside its configured folder",
        ));
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("Dictionary files must be regular files"));
    }
    Ok(file)
}

enum Data {
    Plain(File),
    DictZip(DictZip),
}

impl Data {
    fn len(&self) -> io::Result<u64> {
        match self {
            Self::Plain(file) => Ok(file.metadata()?.len()),
            Self::DictZip(data) => Ok(data.size),
        }
    }

    fn read(&mut self, offset: u64, length: usize, budget: &mut u64) -> io::Result<Vec<u8>> {
        match self {
            Self::Plain(file) => {
                let mut bytes = vec![0; length];
                file.seek(SeekFrom::Start(offset))?;
                file.read_exact(&mut bytes)?;
                Ok(bytes)
            }
            Self::DictZip(data) => data.read(offset, length, budget),
        }
    }
}

// dictzip v1 uses gzip's RA extra subfield for raw DEFLATE chunks separated
// by full-flush points. See the dictzip(1) file format and RFC 1952.
struct DictZip {
    file: BufReader<File>,
    size: u64,
    chunk_size: u64,
    offsets: Vec<u64>,
    cached: Option<usize>,
    compressed: Vec<u8>,
    decoded: Vec<u8>,
}

impl DictZip {
    fn open(file: File) -> io::Result<Self> {
        let length = file.metadata()?.len();
        if !(18..=MAX_DZ_FILE_BYTES).contains(&length) {
            return Err(invalid(
                "The dictzip file length is outside Reader's limits",
            ));
        }
        let mut file = BufReader::new(file);
        let header = GzDecoder::new((&mut file).take(MAX_DZ_HEADER_BYTES));
        let extra = header
            .header()
            .and_then(|header| header.extra())
            .ok_or_else(|| {
                invalid("The .dict.dz file has no valid dictzip random-access header")
            })?;
        let (chunk_size, chunks) = dictzip_table(extra)?;
        let header_end = MAX_DZ_HEADER_BYTES - header.into_inner().limit();
        let compressed_end = length - 8;
        file.seek(SeekFrom::End(-4))?;
        let mut size = [0; 4];
        file.read_exact(&mut size)?;
        let size = u64::from(u32::from_le_bytes(size));
        if size == 0 || size > MAX_DZ_DATA_BYTES || size.div_ceil(chunk_size) != chunks.len() as u64
        {
            return Err(invalid(
                "The dictzip data size disagrees with its chunk table",
            ));
        }
        let mut offsets = Vec::with_capacity(chunks.len() + 1);
        offsets.push(header_end);
        let mut offset = header_end;
        for chunk in chunks {
            offset += u64::from(chunk);
            offsets.push(offset);
        }
        let trailing = compressed_end
            .checked_sub(offset)
            .filter(|bytes| *bytes <= 32)
            .ok_or_else(|| invalid("The dictzip chunk table extends beyond its compressed data"))?;
        if trailing > 0 {
            // The reference writer emits an empty final block outside its indexed chunks.
            let mut bytes = [0; 32];
            let bytes = &mut bytes[..trailing as usize];
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(bytes)?;
            let mut decoder = Decompress::new(false);
            let status = decoder
                .decompress(bytes, &mut [0], FlushDecompress::Finish)
                .map_err(|_| invalid("The dictzip deflate trailer is invalid"))?;
            if status != Status::StreamEnd
                || decoder.total_in() != trailing
                || decoder.total_out() != 0
            {
                return Err(invalid("The dictzip deflate stream has an invalid end"));
            }
        }
        Ok(Self {
            file,
            size,
            chunk_size,
            offsets,
            cached: None,
            compressed: Vec::new(),
            decoded: Vec::new(),
        })
    }

    fn read(&mut self, offset: u64, length: usize, budget: &mut u64) -> io::Result<Vec<u8>> {
        let end = offset
            .checked_add(length as u64)
            .filter(|end| *end <= self.size && length > 0 && length <= MAX_ENTRY_BYTES)
            .ok_or_else(|| invalid("The dictionary definition is outside the dictzip data"))?;
        let first_chunk = (offset / self.chunk_size) as usize;
        let last_chunk = ((end - 1) / self.chunk_size) as usize;
        let compressed = self.offsets[last_chunk + 1] - self.offsets[first_chunk];
        if last_chunk - first_chunk + 1 > MAX_DZ_ENTRY_CHUNKS
            || compressed > MAX_DZ_ENTRY_COMPRESSED_BYTES
        {
            return Err(invalid(
                "The definition exceeds Reader's dictzip chunk limit",
            ));
        }
        *budget = budget
            .checked_sub(compressed)
            .ok_or_else(|| invalid("The lookup exceeds Reader's dictzip I/O limit"))?;
        let mut bytes = Vec::with_capacity(length);
        for chunk in first_chunk..=last_chunk {
            self.load_chunk(chunk)?;
            let chunk_start = chunk as u64 * self.chunk_size;
            let left = offset.saturating_sub(chunk_start) as usize;
            let right = (end - chunk_start).min(self.decoded.len() as u64) as usize;
            bytes.extend_from_slice(&self.decoded[left..right]);
        }
        Ok(bytes)
    }

    fn load_chunk(&mut self, index: usize) -> io::Result<()> {
        if self.cached == Some(index) {
            return Ok(());
        }
        self.cached = None;
        let start = self.offsets[index];
        let compressed = (self.offsets[index + 1] - start) as usize;
        let expected = (self.size - index as u64 * self.chunk_size).min(self.chunk_size) as usize;
        self.compressed.resize(compressed, 0);
        self.decoded.resize(expected + 1, 0);
        self.file.seek(SeekFrom::Start(start))?;
        self.file.read_exact(&mut self.compressed)?;
        let mut decoder = Decompress::new(false);
        let status = decoder
            .decompress(&self.compressed, &mut self.decoded, FlushDecompress::Sync)
            .map_err(|_| invalid("The dictionary has a damaged dictzip chunk"))?;
        if decoder.total_in() != compressed as u64
            || decoder.total_out() != expected as u64
            || (status == Status::StreamEnd && index + 2 != self.offsets.len())
        {
            return Err(invalid(
                "The dictzip chunk exceeds or disagrees with its declared bounds",
            ));
        }
        self.decoded.truncate(expected);
        self.cached = Some(index);
        Ok(())
    }
}

fn dictzip_table(mut extra: &[u8]) -> io::Result<(u64, Vec<u16>)> {
    let mut table = None;
    while !extra.is_empty() {
        let (header, fields) = extra
            .split_at_checked(4)
            .ok_or_else(|| invalid("The dictzip extra field is truncated"))?;
        let size = usize::from(u16::from_le_bytes([header[2], header[3]]));
        let (field, remaining) = fields
            .split_at_checked(size)
            .ok_or_else(|| invalid("The dictzip extra field has an invalid size"))?;
        extra = remaining;
        if &header[..2] != b"RA" {
            continue;
        }
        if table.is_some() || field.len() < 6 {
            return Err(invalid("The dictzip random-access table is malformed"));
        }
        let number = |index| u16::from_le_bytes([field[index], field[index + 1]]);
        let version = number(0);
        let chunk_size = u64::from(number(2));
        let count = usize::from(number(4));
        if version != 1
            || chunk_size == 0
            || !(1..=MAX_DZ_CHUNKS).contains(&count)
            || field.len() != 6 + count * 2
        {
            return Err(invalid(
                "The dictzip random-access format or chunk count is unsupported",
            ));
        }
        let chunks: Vec<_> = field[6..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|entry| u16::from_le_bytes([entry[0], entry[1]]))
            .collect();
        if chunks.contains(&0) {
            return Err(invalid(
                "The dictzip random-access table lists an empty chunk",
            ));
        }
        table = Some((chunk_size, chunks));
    }
    table.ok_or_else(|| {
        invalid("Use a dictzip-compressed .dict.dz file or an uncompressed .dict file")
    })
}

fn visit_synonyms(root: &Path, path: &Path, info: &Info, budget: &mut LookupBudget,
    mut visit: impl FnMut(&str, usize) -> io::Result<()>) -> io::Result<()> {
    let Some(count) = info.synonyms else {
        return Ok(());
    };
    let file = match open_dictionary_file(root, &path.with_extension("syn")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let size = file.metadata()?.len();
    let limit = MAX_INDEX_BYTES.min(budget.index_bytes.saturating_sub(info.index_size));
    if size > limit as u64 || size < (count * 6) as u64 {
        return Err(invalid(
            "The dictionary synonym index exceeds its limits or is truncated",
        ));
    }
    budget.index_bytes -= size as usize;
    let mut index = BufReader::new(file.take(size + 1));
    let mut word = Vec::with_capacity(MAX_WORD_BYTES + 1);
    let mut ordinal = [0; 4];
    let mut bytes_read = 0;
    for _ in 0..count {
        word.clear();
        bytes_read += (&mut index)
            .take((MAX_WORD_BYTES + 1) as u64)
            .read_until(0, &mut word)?;
        if word.pop() != Some(0) || word.is_empty() {
            return Err(invalid("A dictionary synonym is invalid or too long"));
        }
        let word =
            std::str::from_utf8(&word).map_err(|_| invalid("Dictionary synonyms must be UTF-8"))?;
        index.read_exact(&mut ordinal)?;
        bytes_read += ordinal.len();
        let target = u32::from_be_bytes(ordinal) as usize;
        if target >= info.words {
            return Err(invalid(
                "A dictionary synonym references an absent index entry",
            ));
        }
        visit(word, target)?;
    }
    if bytes_read as u64 != size || !index.fill_buf()?.is_empty() {
        return Err(invalid(
            "The dictionary synonym count does not match its index",
        ));
    }
    Ok(())
}

fn synonym_targets(
    root: &Path,
    path: &Path,
    info: &Info,
    query: Query<'_>,
    budget: &mut LookupBudget,
) -> io::Result<Aliases> {
    let mut hits = Aliases::default();
    let mapper = CaseMapper::new();
    visit_synonyms(root, path, info, budget, |word, target| {
        let matches = if word == query.word {
            &mut hits.direct.exact
        } else if hits.direct.fallback.len() < MAX_RESULTS && query.caseless(word, mapper) {
            &mut hits.direct.fallback
        } else {
            if let Some(rank) = query.forms.and_then(|forms| forms.rank(word))
                && (rank < hits.forms.rank || !hits.forms.values.contains(&target))
            {
                hits.forms.record(rank, || target);
            }
            return Ok(());
        };
        if matches.len() < MAX_RESULTS && !matches.contains(&target) {
            matches.push(target);
        }
        Ok(())
    })?;
    hits.direct.exact.sort_unstable();
    hits.direct.fallback.sort_unstable();
    hits.forms.values.sort_unstable();
    hits.forms.values.dedup();
    Ok(hits)
}

fn visit_index(index: impl Read, info: &Info, data_size: u64,
    mut visit: impl FnMut(usize, &str, u64, u32) -> io::Result<()>) -> io::Result<()> {
    let mut index = BufReader::new(index.take(info.index_size as u64 + 1));
    let mut headword = Vec::with_capacity(MAX_WORD_BYTES + 1);
    let mut position = [0; 12];
    let record_size = info.offset_bytes + 4;
    let mut bytes_read = 0;
    for ordinal in 0..info.words {
        headword.clear();
        bytes_read += (&mut index)
            .take((MAX_WORD_BYTES + 1) as u64)
            .read_until(0, &mut headword)?;
        if headword.pop() != Some(0) || headword.is_empty() {
            return Err(invalid("A dictionary headword is invalid or too long"));
        }
        let headword = std::str::from_utf8(&headword)
            .map_err(|_| invalid("Dictionary headwords must be UTF-8"))?;
        index.read_exact(&mut position[..record_size])?;
        bytes_read += record_size;
        let offset = if info.offset_bytes == 8 {
            u64::from_be_bytes(position[..8].try_into().unwrap())
        } else {
            u64::from(u32::from_be_bytes(position[..4].try_into().unwrap()))
        };
        let size = u32::from_be_bytes(position[info.offset_bytes..record_size].try_into().unwrap());
        if offset
            .checked_add(u64::from(size))
            .is_none_or(|end| end > data_size)
        {
            return Err(invalid("A dictionary definition is outside the data file"));
        }
        if size as usize > MAX_ENTRY_BYTES {
            continue;
        }
        visit(ordinal, headword, offset, size)?;
    }
    if bytes_read != info.index_size || !index.fill_buf()?.is_empty() {
        return Err(invalid(
            "The dictionary word count does not match its index",
        ));
    }
    Ok(())
}

fn scan_index(
    index: impl Read,
    info: &Info,
    query: IndexQuery<'_>,
    data_size: u64,
) -> io::Result<Hits<IndexMatch>> {
    let mut hits = Hits::new();
    let mut forms = Ranked::default();
    let mapper = CaseMapper::new();
    visit_index(index, info, data_size, |ordinal, headword, offset, size| {
        let exact = headword == query.headword.word || query.aliases.direct.exact.binary_search(&ordinal).is_ok();
        let fallback = !exact && hits.fallback.len() < MAX_RESULTS
            && (query.aliases.direct.fallback.binary_search(&ordinal).is_ok() || query.headword.caseless(headword, mapper));
        let entry = || IndexMatch { word: headword.to_owned(), offset, size: size as usize };
        if exact {
            if hits.exact.len() < MAX_RESULTS { hits.exact.push(entry()); }
        } else if fallback {
            hits.fallback.push(entry());
        } else if let Some(candidates) = query.headword.forms {
            let direct = candidates.rank(headword);
            let synonym = query.aliases.forms.values.binary_search(&ordinal)
                .ok().map(|_| query.aliases.forms.rank);
            if let Some(best) = direct.into_iter().chain(synonym).min() {
                forms.record(best, entry);
            }
        }
        Ok(())
    })?;
    if hits.exact.is_empty() && hits.fallback.is_empty() { hits.fallback = forms.values; }
    Ok(hits)
}

fn definition(mut data: &[u8], sequence: Option<&[u8]>) -> Option<String> {
    let mut text = String::new();
    for index in 0..sequence.map_or(MAX_FIELDS, <[u8]>::len) {
        if data.is_empty() && sequence.is_none() {
            break;
        }
        let (kind, last) = match sequence {
            Some(types) => (types[index], index + 1 == types.len()),
            None => {
                let (&kind, value) = data.split_first()?;
                data = value;
                (kind, false)
            }
        };
        if !kind.is_ascii_alphabetic() {
            return None;
        }
        let value = if last {
            std::mem::take(&mut data)
        } else if kind.is_ascii_uppercase() {
            let size = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
            let (value, rest) = data.get(4..)?.split_at_checked(size)?;
            data = rest;
            value
        } else {
            let end = data.iter().position(|&byte| byte == 0)?;
            let value = &data[..end];
            data = &data[end + 1..];
            value
        };
        if !matches!(kind, b'm' | b't' | b'y' | b'h' | b'g' | b'x' | b'k') {
            continue;
        }
        let value = std::str::from_utf8(value).ok()?.trim_end_matches('\0');
        let value = if matches!(kind, b'h' | b'g' | b'x' | b'k') {
            markup_text(value, kind == b'x')
        } else {
            display_text(value)
        };
        if !value.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&value);
        }
    }
    (data.is_empty() && !text.is_empty()).then_some(text)
}

fn display_text(text: &str) -> String {
    let mut output = String::new();
    for line in text.lines() {
        let mut words = line.split_whitespace().peekable();
        if words.peek().is_none() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        for (index, word) in words.enumerate() {
            if index > 0 {
                output.push(' ');
            }
            output.extend(word.chars().filter(|ch| !ch.is_control()));
        }
    }
    output
}

#[derive(Default)]
struct PlainText {
    xdxf: bool,
    headword_start: Cell<Option<usize>>,
    text: RefCell<String>,
    hidden: Cell<Option<&'static str>>,
}

impl TokenSink for PlainText {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(text) if self.hidden.get().is_none() => {
                self.text.borrow_mut().push_str(&text)
            }
            Token::TagToken(tag) => {
                let name: &str = &tag.name;
                if let Some(hidden) = self.hidden.get() {
                    if tag.kind == TagKind::EndTag && name == hidden {
                        self.hidden.set(None);
                    }
                } else if tag.kind == TagKind::StartTag && !tag.self_closing {
                    if name == "script" {
                        self.hidden.set(Some("script"));
                        return TokenSinkResult::RawData(RawKind::ScriptData);
                    }
                    if name == "style" {
                        self.hidden.set(Some("style"));
                        return TokenSinkResult::RawData(RawKind::Rawtext);
                    }
                }
                if self.xdxf && self.hidden.get().is_none() {
                    match (name, tag.kind) {
                        ("k", TagKind::StartTag) if !tag.self_closing => {
                            self.headword_start.set(Some(self.text.borrow().len()));
                            return TokenSinkResult::Continue;
                        }
                        ("k", TagKind::EndTag) => {
                            if let Some(start) = self.headword_start.take() {
                                self.text.borrow_mut().truncate(start);
                            }
                            return TokenSinkResult::Continue;
                        }
                        ("tr", _) => {
                            if !tag.self_closing {
                                let bracket = if tag.kind == TagKind::StartTag { '[' } else { ']' };
                                self.text.borrow_mut().push(bracket);
                            }
                            return TokenSinkResult::Continue;
                        }
                        _ => {}
                    }
                }
                if self.hidden.get().is_none()
                    && matches!(
                        name,
                        "br" | "p"
                            | "div"
                            | "dd"
                            | "dt"
                            | "li"
                            | "tr"
                            | "hr"
                            | "k"
                            | "def"
                            | "example"
                            | "blockquote"
                    )
                {
                    self.text.borrow_mut().push('\n');
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

fn markup_text(text: &str, xdxf: bool) -> String {
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from_slice(text));
    let tokenizer = Tokenizer::new(PlainText { xdxf, ..Default::default() }, Default::default());
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    display_text(&tokenizer.sink.text.into_inner())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use flate2::{Compress, Compression, Crc, FlushCompress, write::GzEncoder};

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-stardict-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) fn write_dictionary(
        root: &Path,
        name: &str,
        sequence: &str,
        entries: &[(&str, &[u8])],
    ) {
        let mut index = Vec::new();
        let mut data = Vec::new();
        for &(word, value) in entries {
            index.extend_from_slice(word.as_bytes());
            index.push(0);
            index.extend_from_slice(&(data.len() as u32).to_be_bytes());
            index.extend_from_slice(&(value.len() as u32).to_be_bytes());
            data.extend_from_slice(value);
        }
        let info = format!(
            "StarDict's dict ifo file\nversion=2.4.2\nbookname={name}\nwordcount={}\nidxfilesize={}\n{sequence}",
            entries.len(),
            index.len()
        );
        fs::write(root.join(format!("{name}.ifo")), info).unwrap();
        fs::write(root.join(format!("{name}.idx")), index).unwrap();
        fs::write(root.join(format!("{name}.dict")), data).unwrap();
    }

    fn write_synonyms(root: &Path, name: &str, entries: &[(&str, u32)]) {
        let path = root.join(format!("{name}.ifo"));
        let mut info = fs::read_to_string(&path).unwrap();
        info.push_str(&format!("synwordcount={}\n", entries.len()));
        fs::write(path, info).unwrap();
        let mut index = Vec::new();
        for &(word, ordinal) in entries {
            index.extend_from_slice(word.as_bytes());
            index.push(0);
            index.extend_from_slice(&ordinal.to_be_bytes());
        }
        fs::write(root.join(format!("{name}.syn")), index).unwrap();
    }

    #[test]
    fn accepts_soft_hyphens_on_either_side_and_keeps_literal_headword_priority() {
        let root = TestDirectory::new();
        write_dictionary(
            &root.0,
            "Terms",
            "sametypesequence=m\n",
            &[
                ("international", b"Ordinary"),
                ("interna\u{ad}tional", b"Literal"),
            ],
        );
        write_synonyms(&root.0, "Terms", &[("over\u{ad}seas", 0)]);
        let answers = lookup(&root.0, "interna\u{ad}tional").unwrap();
        assert_eq!(answers[0].word, "interna\u{ad}tional");
        let answers = lookup(&root.0, "international").unwrap();
        assert_eq!(answers[0].word, "international");
        let answers = lookup(&root.0, "INTERNA\u{ad}TIONAL").unwrap();
        assert_eq!(answers.len(), 2);
        let answers = lookup(&root.0, "OVERSEAS").unwrap();
        assert_eq!(answers[0].word, "international");
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn dictzip(bytes: &[u8], chunk_size: u16) -> Vec<u8> {
        let mut raw = Vec::new();
        let mut lengths: Vec<u16> = Vec::new();
        let mut encoder = Compress::new(Compression::fast(), false);
        let mut buffer = vec![0; u16::MAX as usize];
        for chunk in bytes.chunks(usize::from(chunk_size)) {
            let start = encoder.total_out();
            let input_start = encoder.total_in();
            encoder
                .compress(chunk, &mut buffer, FlushCompress::Full)
                .unwrap();
            let length = (encoder.total_out() - start) as usize;
            assert_eq!(encoder.total_in() - input_start, chunk.len() as u64);
            lengths.push(length as u16);
            raw.extend_from_slice(&buffer[..length]);
        }
        let start = encoder.total_out();
        assert_eq!(
            encoder
                .compress(&[], &mut buffer, FlushCompress::Finish)
                .unwrap(),
            Status::StreamEnd,
        );
        raw.extend_from_slice(&buffer[..(encoder.total_out() - start) as usize]);
        let mut extra = b"ZZ\0\0RA".to_vec();
        extra.extend_from_slice(&(6 + lengths.len() as u16 * 2).to_le_bytes());
        extra.extend_from_slice(&1u16.to_le_bytes());
        extra.extend_from_slice(&chunk_size.to_le_bytes());
        extra.extend_from_slice(&(lengths.len() as u16).to_le_bytes());
        for length in lengths {
            extra.extend_from_slice(&length.to_le_bytes());
        }
        let mut result = vec![0x1f, 0x8b, 8, 12, 0, 0, 0, 0, 0, 3];
        result.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        result.extend_from_slice(&extra);
        result.extend_from_slice(b"dictionary.dict\0");
        result.extend_from_slice(&raw);
        let mut crc = Crc::new();
        crc.update(bytes);
        result.extend_from_slice(&crc.sum().to_le_bytes());
        result.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        result
    }

    fn pack_dictionary(root: &Path, name: &str, chunk_size: u16) {
        let index = root.join(format!("{name}.idx"));
        let data = root.join(format!("{name}.dict"));
        let plain = fs::read(&data).unwrap();
        let packed = dictzip(&plain, chunk_size);
        let mut decoded = Vec::new();
        MultiGzDecoder::new(packed.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, plain);
        fs::write(root.join(format!("{name}.dict.dz")), packed).unwrap();
        fs::write(
            root.join(format!("{name}.idx.gz")),
            gzip(&fs::read(&index).unwrap()),
        )
        .unwrap();
        fs::remove_file(index).unwrap();
        fs::remove_file(data).unwrap();
    }

    #[test]
    fn reads_gzip_indexes_and_cross_chunk_dictzip_definitions() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[
                ("lantern", b"Thirteen chars"),
                ("tide", b"Water moving beyond the harbor."),
                ("zebra", b"Last entry."),
            ],
        );
        pack_dictionary(&temp.0, "Shore", 16);
        assert_eq!(
            lookup(&temp.0, "tide").unwrap()[0].text,
            "Water moving beyond the harbor."
        );
        assert_eq!(lookup(&temp.0, "ZEBRA").unwrap()[0].text, "Last entry.");
        assert!(lookup(&temp.0, "missing").unwrap().is_empty());
    }

    #[test]
    fn indexed_dictzip_reads_only_the_requested_chunks() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[
                ("earlier", b"0123456789abcdef"),
                ("later", b"Later definition."),
            ],
        );
        pack_dictionary(&temp.0, "Shore", 16);
        let path = temp.0.join("Shore.dict.dz");
        let data = DictZip::open(File::open(&path).unwrap()).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes[data.offsets[0] as usize] = 0xff;
        fs::write(path, bytes).unwrap();
        assert_eq!(
            lookup(&temp.0, "later").unwrap()[0].text,
            "Later definition."
        );
        assert!(lookup(&temp.0, "earlier").is_err());
    }

    #[test]
    fn validates_gzip_index_length_crc_and_dictzip_table() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[("word", b"Definition.")],
        );
        let idx = fs::read(temp.0.join("Shore.idx")).unwrap();
        pack_dictionary(&temp.0, "Shore", 16);
        let packed = temp.0.join("Shore.idx.gz");
        let mut extra = idx.clone();
        extra.push(0);
        fs::write(&packed, gzip(&extra)).unwrap();
        assert!(lookup(&temp.0, "word").is_err());
        let mut corrupt = gzip(&idx);
        let crc = corrupt.len() - 8;
        corrupt[crc] ^= 1;
        fs::write(&packed, corrupt).unwrap();
        assert!(lookup(&temp.0, "word").is_err());
        fs::write(&packed, gzip(&idx)).unwrap();
        fs::write(temp.0.join("Shore.dict.dz"), gzip(b"Definition.")).unwrap();
        let error = lookup(&temp.0, "word").unwrap_err();
        assert!(error.to_string().contains("random-access"));
    }

    #[test]
    fn prioritizes_exact_matches_and_supports_unicode_case_folding() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[
                ("lantern", b"A light carried to the shore."),
                ("Lantern", b"A name on the harbor chart."),
                ("Straße", "A street near the harbour.".as_bytes()),
            ],
        );
        let result = lookup(&temp.0, "lantern").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "A light carried to the shore.");
        let result = lookup(&temp.0, "STRASSE").unwrap();
        assert_eq!(result[0].word, "Straße");
        assert_eq!(result[0].dictionary, "Shore");
        assert!(lookup(&temp.0, "absent").unwrap().is_empty());
    }

    #[test]
    fn preferred_dictionary_changes_the_order_of_exact_matches() {
        let temp = TestDirectory::new();
        for (name, meaning) in [
            ("Harbor", b"One.".as_slice()),
            ("Shore", b"Two.".as_slice()),
        ] {
            write_dictionary(&temp.0, name, "sametypesequence=m\n", &[("word", meaning)]);
        }
        assert_eq!(lookup(&temp.0, "word").unwrap()[0].dictionary, "Harbor");
        let rows = lookup_preferred(&temp.0, "word", Some(Path::new("Shore.ifo"))).unwrap();
        assert_eq!(rows[0].dictionary, "Shore");
        assert_eq!(rows[1].dictionary, "Harbor");
        let fallback =
            lookup_preferred(&temp.0, "word", Some(Path::new("../unavailable.ifo"))).unwrap();
        assert_eq!(fallback[0].dictionary, "Harbor");
    }

    #[test]
    fn an_alias_can_refer_to_a_headword_only_entry_in_the_same_dictionary() {
        let temp = TestDirectory::new();
        write_dictionary(&temp.0, "Bilingual", "sametypesequence=m\n",
            &[("alias", b""), ("lamp", b"A portable light.")]);
        write_synonyms(&temp.0, "Bilingual", &[("lamp", 0), ("lamp", 1)]);
        let hits = lookup(&temp.0, "lamp").unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].text, "A portable light.");
        assert_eq!(hits[1].word, "alias");
        assert!(hits[1].text.is_empty());
    }

    #[test]
    fn xdxf_headwords_are_shown_once_and_transcriptions_are_bracketed() {
        let entry = b"<k>Quay</k>\n<tr>ki:</tr> A dock &amp; landing.";
        assert_eq!(definition(entry, Some(b"x")).as_deref(), Some("[ki:] A dock & landing."));
        assert_eq!(definition(b"<tr><td>First</td></tr>", Some(b"h")).as_deref(), Some("First"));
        assert_eq!(definition(b"<k>Unclosed headword", Some(b"x")).as_deref(), Some("Unclosed headword"));
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn search_reader_and_existing_koreader_dictionaries_in_preference_order() {
        let temp = TestDirectory::new();
        let docs = temp.0.join("documents");
        let data = temp.0.join("reader/data");
        let local = data.join("dictionaries");
        let shared = temp.0.join("koreader/data");
        for (root, definition) in [
            (local.clone(), "Reader"),
            (shared.join("dict"), "KOReader"),
            (shared.join("dict_ext"), "Extended"),
        ] {
            fs::create_dir_all(&root).unwrap();
            write_dictionary(
                &root,
                "Same",
                "sametypesequence=m\n",
                &[("word", definition.as_bytes())],
            );
        }
        let sources = DictionarySources::for_reader(&docs, &data);
        let dictionaries = sources.installed().unwrap();
        assert_eq!(dictionaries.len(), 3);
        assert_eq!(dictionaries[0].path, Path::new("Same.ifo"));
        assert_ne!(dictionaries[1].path, dictionaries[2].path);
        for (file, first) in dictionaries.iter().zip(["Reader", "KOReader", "Extended"]) {
            let result = sources.lookup("word", Some(&file.path)).unwrap();
            assert_eq!(result.len(), 3);
            assert_eq!(result[0].text, first);
        }
        fs::remove_dir_all(&local).unwrap();
        assert_eq!(sources.lookup("word", None).unwrap()[0].text, "KOReader");
        assert!(!local.exists());
        assert!(
            DictionarySources::for_reader(&temp.0.join("custom"), &data)
                .installed()
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(all(unix, feature = "kindle"))]
    #[test]
    fn shared_dictionary_discovery_skips_redirected_or_inaccessible_sources() {
        let temp = TestDirectory::new();
        let data = temp.0.join("reader/data");
        let local = data.join("dictionaries");
        fs::create_dir_all(&local).unwrap();
        write_dictionary(
            &local,
            "Home",
            "sametypesequence=m\n",
            &[("word", b"Reader")],
        );
        let other = temp.0.join("elsewhere/data/dict");
        fs::create_dir_all(&other).unwrap();
        write_dictionary(
            &other,
            "Other",
            "sametypesequence=m\n",
            &[("word", b"Other")],
        );
        std::os::unix::fs::symlink(temp.0.join("elsewhere"), temp.0.join("koreader")).unwrap();
        let sources = DictionarySources::for_reader(&temp.0.join("documents"), &data);
        assert_eq!(sources.installed().unwrap().len(), 1);
        fs::remove_file(temp.0.join("koreader")).unwrap();
        let shared = temp.0.join("koreader/data");
        fs::create_dir_all(&shared).unwrap();
        fs::write(shared.join("dict_ext"), b"unavailable").unwrap();
        std::os::unix::fs::symlink(&other, shared.join("dict")).unwrap();
        assert_eq!(sources.lookup("word", None).unwrap().len(), 1);
    }

    #[test]
    fn follows_synonym_ordinals_through_plain_and_compressed_dictionaries() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Harbor",
            "sametypesequence=m\n",
            &[("lamp", b"First."), ("lantern", b"Second.")],
        );
        write_synonyms(
            &temp.0,
            "Harbor",
            &[("firelight", 0), ("firelight", 1), ("Straße", 1)],
        );
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[("torch", b"Third.")],
        );
        write_synonyms(&temp.0, "Shore", &[("firelight", 0), ("lantern", 0)]);
        pack_dictionary(&temp.0, "Shore", 8);

        let names = |rows: Vec<Definition>| {
            rows.into_iter()
                .map(|definition| (definition.word, definition.text))
                .collect::<Vec<_>>()
        };
        let first = ("lamp".into(), "First.".into());
        let second = ("lantern".into(), "Second.".into());
        let third = ("torch".into(), "Third.".into());
        assert_eq!(
            names(lookup(&temp.0, "firelight").unwrap()),
            vec![first.clone(), second.clone(), third.clone()]
        );
        assert_eq!(
            names(lookup_preferred(&temp.0, "firelight", Some(Path::new("Shore.ifo"))).unwrap()),
            vec![third.clone(), first, second.clone()]
        );
        assert_eq!(
            names(lookup(&temp.0, "STRASSE").unwrap()),
            vec![second.clone()]
        );
        for query in ["lantern", "LANTERN"] {
            assert_eq!(
                names(lookup_preferred(&temp.0, query, Some(Path::new("Shore.ifo"))).unwrap()),
                vec![third.clone(), second.clone()]
            );
        }
    }

    #[test]
    fn malformed_or_missing_synonyms_leave_the_headword_available() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[("lamp", b"Still here.")],
        );
        write_synonyms(&temp.0, "Shore", &[("firelight", 0), ("lantern", 0)]);
        let path = temp.0.join("Shore.syn");
        let valid = fs::read(&path).unwrap();
        let mut invalid_target = valid.clone();
        *invalid_target.last_mut().unwrap() = 1;
        let mut invalid_utf8 = valid.clone();
        invalid_utf8[valid.len() - 5 - "lantern".len()] = 0xff;
        let mut extra = valid.clone();
        extra.push(0);
        for bytes in [
            &valid[..valid.len() - 1],
            &invalid_target,
            &invalid_utf8,
            &extra,
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(lookup(&temp.0, "firelight").unwrap().is_empty());
            assert_eq!(lookup(&temp.0, "lamp").unwrap()[0].text, "Still here.");
        }
        fs::remove_file(&path).unwrap();
        assert!(lookup(&temp.0, "firelight").unwrap().is_empty());
        assert_eq!(lookup(&temp.0, "lamp").unwrap()[0].text, "Still here.");
    }

    #[test]
    fn each_dictionary_can_supply_its_own_headword_or_inflected_synonym() {
        let temp = TestDirectory::new();
        write_dictionary(&temp.0,"North","sametypesequence=m\n",&[("moving",b"Travelling.")]);
        write_dictionary(&temp.0,"South","sametypesequence=m\n",&[("move",b"Change position."),("run",b"Proceed quickly."),("transport",b"Carry from place to place.")]);
        write_synonyms(&temp.0,"South",&[("carry",2)]);
        let sources = DictionarySources {local: temp.0.clone(), shared_storage: None};
        let query = |word,enabled| sources.lookup_configured(word,&[],&[],&[],enabled).unwrap()
            .into_iter().map(|r|r.word).collect::<Vec<_>>();
        assert_eq!(query("moving",true), ["moving","move"]);
        assert_eq!(query("moving",false), ["moving"]);
        assert_eq!(query("running",true), ["run"]);
        assert_eq!(query("carried",true), ["transport"]);
        assert!(query("carried",false).is_empty());
        assert!(query("đang",true).is_empty());
    }

    #[test]
    fn parses_tagged_and_shared_fields_to_safe_text() {
        let temp = TestDirectory::new();
        let nested = temp.0.join("travel");
        fs::create_dir(&nested).unwrap();
        write_dictionary(&nested, "Words", "", &[(
            "tide", b"t/taid/\0h<b>Rise &amp; fall</b><br>of the sea.<script>hidden</script><img src='https://example.org/p'>\0",
        )]);
        write_dictionary(
            &nested,
            "More",
            "sametypesequence=tg\n",
            &[("bank", b"/bank/\0<b>Ground</b> by a river.")],
        );
        assert_eq!(
            lookup(&temp.0, "tide").unwrap()[0].text,
            "/taid/\nRise & fall\nof the sea."
        );
        assert_eq!(
            lookup(&temp.0, "bank").unwrap()[0].text,
            "/bank/\nGround by a river."
        );
    }

    #[test]
    fn rejects_index_records_outside_the_data_file() {
        let temp = TestDirectory::new();
        write_dictionary(
            &temp.0,
            "Shore",
            "sametypesequence=m\n",
            &[("tide", b"A short definition.")],
        );
        fs::write(temp.0.join("Shore.dict"), b"tiny").unwrap();
        assert_eq!(
            lookup(&temp.0, "tide").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[cfg(unix)]
    #[test]
    fn ignores_symlinked_folders_and_rejects_symlinked_data() {
        let root = TestDirectory::new();
        let outside = TestDirectory::new();
        write_dictionary(
            &outside.0,
            "Shore",
            "sametypesequence=m\n",
            &[("tide", b"Definition outside the configured root.")],
        );
        std::os::unix::fs::symlink(&outside.0, root.0.join("other")).unwrap();
        assert!(lookup(&root.0, "tide").unwrap().is_empty());
        for ext in ["ifo", "idx"] {
            fs::copy(
                outside.0.join(format!("Shore.{ext}")),
                root.0.join(format!("Shore.{ext}")),
            )
            .unwrap();
        }
        std::os::unix::fs::symlink(outside.0.join("Shore.dict"), root.0.join("Shore.dict"))
            .unwrap();
        assert_eq!(
            lookup(&root.0, "tide").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
