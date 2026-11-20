//! Offline StarDict 2.4.2 and 3.0.0 lookup from a configured directory.
//! Stream StarDict indexes and seek definition slices in plain or dictzip data.

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

#[cfg(feature = "kindle")]
pub(crate) fn installed(root: &Path) -> io::Result<Vec<InstalledDictionary>> {
    let Some(root) = dictionary_root(root)? else {
        return Ok(Vec::new());
    };
    Ok(dictionary_files(&root)?
        .into_iter()
        .filter_map(|path| {
            let title = Info::load(open_dictionary_file(&root, &path).ok()?)
                .ok()?
                .title;
            Some(InstalledDictionary {
                path: path.strip_prefix(&root).ok()?.to_owned(),
                title,
            })
        })
        .collect())
}

/// Find exact and Unicode caseless headwords, then try optional synonym indexes
/// if there is no headword definition. Close all files before returning.
pub fn lookup(root: &Path, word: &str) -> io::Result<Vec<Definition>> {
    lookup_preferred(root, word, None)
}

pub(crate) fn lookup_preferred(
    root: &Path,
    word: &str,
    preferred: Option<&Path>,
) -> io::Result<Vec<Definition>> {
    let word = word.trim();
    if word.len() > MAX_WORD_BYTES {
        return Ok(Vec::new());
    }
    let unwrapped = crate::search::without_soft_hyphens(word);
    let term = unwrapped.trim();
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let Some(root) = dictionary_root(root)? else {
        return Ok(Vec::new());
    };
    let mut files = dictionary_files(&root)?;
    if let Some(preferred) = preferred
        && let Some(index) = files
            .iter()
            .position(|path| path.strip_prefix(&root).ok() == Some(preferred))
    {
        files[..=index].rotate_right(1);
    }
    let folded = CaseMapper::new().fold_string(term);
    let query = Query {
        word,
        folded: &folded,
    };
    let mut hits = Hits::new();
    let mut budget = LookupBudget {
        index_bytes: MAX_TOTAL_INDEX_BYTES,
        dictzip_bytes: MAX_DZ_LOOKUP_BYTES,
    };
    let mut valid = Vec::new();
    let mut first_error = None;
    for path in files {
        let result = (|| {
            let info = Info::load(open_dictionary_file(&root, &path)?)?;
            let found = lookup_dictionary(
                &root,
                &path,
                &info,
                IndexQuery::Headwords(query),
                &mut budget,
            )?;
            Ok((info, found))
        })();
        match result {
            Ok((info, found)) => {
                valid.push((path, info));
                hits.extend(found);
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
        if hits.exact.len() == MAX_RESULTS || budget.index_bytes == 0 {
            break;
        }
    }
    if valid.is_empty()
        && let Some(error) = first_error
    {
        return Err(error);
    }
    if hits.is_empty() {
        for (path, info) in valid {
            let Ok(targets) = synonym_targets(&root, &path, &info, query, &mut budget) else {
                // A malformed optional sidecar does not invalidate the base dictionary.
                continue;
            };
            if !targets.is_empty()
                && let Ok(found) = lookup_dictionary(
                    &root,
                    &path,
                    &info,
                    IndexQuery::Synonyms(targets),
                    &mut budget,
                )
            {
                hits.extend(found);
            }
            if hits.exact.len() == MAX_RESULTS || budget.index_bytes == 0 {
                break;
            }
        }
    }
    Ok(hits.into_best())
}

fn dictionary_root(root: &Path) -> io::Result<Option<PathBuf>> {
    match fs::symlink_metadata(root) {
        Ok(info) if info.is_dir() => root.canonicalize().map(Some),
        Ok(_) => Err(invalid("The dictionary path is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn dictionary_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![(0, fs::read_dir(root)?)];
    let mut entries = 0;
    while let Some((depth, directory)) = directories.last_mut() {
        let Some(entry) = directory.next() else {
            directories.pop();
            continue;
        };
        if entries == MAX_SCAN_ENTRIES || files.len() == MAX_DICTIONARIES {
            break;
        }
        entries += 1;
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

    fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.fallback.is_empty()
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

enum IndexQuery<'a> {
    Headwords(Query<'a>),
    Synonyms(Hits<usize>),
}

struct IndexMatch {
    word: String,
    offset: u64,
    size: usize,
}

fn lookup_dictionary(
    root: &Path,
    path: &Path,
    info: &Info,
    query: IndexQuery<'_>,
    budget: &mut LookupBudget,
) -> io::Result<Hits<Definition>> {
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
    let mut data = if dictzip {
        Data::DictZip(DictZip::open(file)?)
    } else {
        Data::Plain(file)
    };
    let data_size = data.len()?;
    let matches = if gzip {
        scan_index(
            MultiGzDecoder::new(index.take(file_size)),
            info,
            query,
            data_size,
        )?
    } else {
        scan_index(index, info, query, data_size)?
    };
    let mut read_hits = |matches: Vec<IndexMatch>| -> io::Result<Vec<Definition>> {
        let mut definitions = Vec::new();
        for matched in matches {
            let bytes = data.read(matched.offset, matched.size, &mut budget.dictzip_bytes)?;
            if let Some(text) = definition(&bytes, info.fields.as_deref()) {
                definitions.push(Definition {
                    dictionary: info.title.clone(),
                    word: matched.word,
                    text,
                });
            }
        }
        Ok(definitions)
    };
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

fn synonym_targets(
    root: &Path,
    path: &Path,
    info: &Info,
    query: Query<'_>,
    budget: &mut LookupBudget,
) -> io::Result<Hits<usize>> {
    let mut hits = Hits::new();
    let Some(count) = info.synonyms else {
        return Ok(hits);
    };
    let file = match open_dictionary_file(root, &path.with_extension("syn")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(hits),
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
    let mapper = CaseMapper::new();
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
        let matches = if word == query.word {
            &mut hits.exact
        } else if hits.fallback.len() < MAX_RESULTS && query.caseless(word, mapper) {
            &mut hits.fallback
        } else {
            continue;
        };
        if matches.len() < MAX_RESULTS && !matches.contains(&target) {
            matches.push(target);
        }
    }
    if bytes_read as u64 != size || !index.fill_buf()?.is_empty() {
        return Err(invalid(
            "The dictionary synonym count does not match its index",
        ));
    }
    hits.exact.sort_unstable();
    hits.fallback.sort_unstable();
    Ok(hits)
}

fn scan_index(
    index: impl Read,
    info: &Info,
    query: IndexQuery<'_>,
    data_size: u64,
) -> io::Result<Hits<IndexMatch>> {
    let mut index = BufReader::new(index.take(info.index_size as u64 + 1));
    let mut hits = Hits::new();
    let mut headword = Vec::with_capacity(MAX_WORD_BYTES + 1);
    let mut position = [0; 12];
    let record_size = info.offset_bytes + 4;
    let mut bytes_read = 0;
    let mapper = CaseMapper::new();
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
        if size == 0 || size as usize > MAX_ENTRY_BYTES {
            continue;
        }
        let (exact, fallback) = match &query {
            IndexQuery::Headwords(query) => {
                let exact = headword == query.word;
                let fallback =
                    !exact && hits.fallback.len() < MAX_RESULTS && query.caseless(headword, mapper);
                (exact, fallback)
            }
            IndexQuery::Synonyms(targets) => (
                targets.exact.binary_search(&ordinal).is_ok(),
                targets.fallback.binary_search(&ordinal).is_ok(),
            ),
        };
        let matches = if exact {
            &mut hits.exact
        } else if fallback {
            &mut hits.fallback
        } else {
            continue;
        };
        if matches.len() < MAX_RESULTS {
            matches.push(IndexMatch {
                word: headword.to_owned(),
                offset,
                size: size as usize,
            });
        }
    }
    if bytes_read != info.index_size || !index.fill_buf()?.is_empty() {
        return Err(invalid(
            "The dictionary word count does not match its index",
        ));
    }
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
            markup_text(value)
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

fn markup_text(text: &str) -> String {
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from_slice(text));
    let tokenizer = Tokenizer::new(PlainText::default(), Default::default());
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    display_text(&tokenizer.sink.text.into_inner())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
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

    fn write_dictionary(root: &Path, name: &str, sequence: &str, entries: &[(&str, &[u8])]) {
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
            vec![third, first, second.clone()]
        );
        assert_eq!(
            names(lookup(&temp.0, "STRASSE").unwrap()),
            vec![second.clone()]
        );
        for query in ["lantern", "LANTERN"] {
            assert_eq!(
                names(lookup_preferred(&temp.0, query, Some(Path::new("Shore.ifo"))).unwrap()),
                vec![second.clone()]
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
