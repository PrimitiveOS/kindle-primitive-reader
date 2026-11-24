//! Read a checkpointed copy of KOReader's fixed-page visit journal.
use super::{History, MAX_BYTES, MAX_VISIT_SECONDS, MAX_VISITS, Visit, invalid, valid_wall};
use rusqlite::{Connection, config::DbConfig, params};
use std::{collections::BTreeMap, io};

pub(crate) const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SOURCE_VISITS: i64 = 20_000;
const STOCK_SCHEMA_VERSION: i64 = 20_221_111;
const MAX_LABEL_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Record {
    page: u64,
    start: i64,
    seconds: u32,
}

#[derive(Debug)]
pub(crate) struct Records {
    visits: Vec<Record>,
    pub(crate) title: String,
    pub(crate) authors: String,
    pub(crate) other_page_counts: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Preview {
    pub(crate) new_visits: usize,
    pub(crate) seconds: u64,
    pub(crate) duplicate: usize,
    pub(crate) conflicting: usize,
    pub(crate) first_start: Option<i64>,
    pub(crate) last_start: Option<i64>,
    pub(crate) can_store: bool,
}

pub(crate) fn read(mut bytes: Vec<u8>, checksum: [u8; 16], pages: usize) -> io::Result<Records> {
    if bytes.len() as u64 > MAX_SOURCE_BYTES
        || bytes.len() < 100
        || !bytes.starts_with(b"SQLite format 3\0")
    {
        return Err(invalid_database());
    }
    // SQLite's memory snapshot API uses rollback-mode headers even for a clean WAL copy.
    match &mut bytes[18..20] {
        [1, 1] => {}
        header @ [2, 2] => header.fill(1),
        _ => return Err(invalid_database()),
    }
    let mut db = Connection::open_in_memory().map_err(db_error)?;
    db.deserialize_read_exact("main", bytes.as_slice(), bytes.len(), true)
        .map_err(db_error)?;
    drop(bytes);
    db.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(db_error)?;
    db.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(db_error)?;
    db.execute_batch("PRAGMA query_only=ON; PRAGMA temp_store=MEMORY;")
        .map_err(db_error)?;
    let mut rounds = 0;
    db.progress_handler(
        1000,
        Some(move || {
            rounds += 1;
            rounds > 5000
        }),
    );
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_error)?;
    let tables: i64 = db
        .query_row(
            "SELECT count(*) FROM pragma_table_list WHERE schema='main' AND type='table'
             AND name IN ('book', 'page_stat_data')",
            [],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if version != STOCK_SCHEMA_VERSION || tables != 2 {
        return Err(invalid_database());
    }
    read_visits(&db, checksum, pages)
}

fn read_visits(db: &Connection, checksum: [u8; 16], pages: usize) -> io::Result<Records> {
    let pages = i64::try_from(pages).map_err(|_| invalid_database())?;
    let md5 = format!("{:x}", md5::Digest(checksum));
    let mut book = db
        .prepare(
            "SELECT id, pages, title, authors FROM main.book WHERE md5 = ?1 COLLATE NOCASE LIMIT 2",
        )
        .map_err(db_error)?;
    let mut matching = book.query([md5]).map_err(db_error)?;
    let (id, page_count, title, authors) = match matching.next().map_err(db_error)? {
        Some(row) => (
            row.get::<_, i64>(0).map_err(db_error)?,
            row.get::<_, i64>(1).map_err(db_error)?,
            read_label(row.get(2).map_err(db_error)?)?,
            read_label(row.get(3).map_err(db_error)?)?,
        ),
        None => return Err(io::Error::other("The database has no match for this PDF")),
    };
    if matching.next().map_err(db_error)?.is_some() {
        return Err(io::Error::other("Several KOReader books match this PDF"));
    }
    if id <= 0 || page_count != pages || pages <= 0 {
        return Err(io::Error::other(
            "KOReader saved a different page count for this PDF",
        ));
    }

    let mut source = db
        .prepare(
            "SELECT page, start_time, duration, total_pages FROM main.page_stat_data
                  WHERE id_book = ?1 ORDER BY page, start_time LIMIT ?2",
        )
        .map_err(db_error)?;
    let mut rows = source
        .query(params![id, MAX_SOURCE_VISITS + 1])
        .map_err(db_error)?;
    let mut records = Records {
        visits: Vec::new(),
        title,
        authors,
        other_page_counts: 0,
    };
    let mut previous = None;
    while let Some(row) = rows.next().map_err(db_error)? {
        if records.visits.len() + records.other_page_counts == MAX_SOURCE_VISITS as usize {
            return Err(io::Error::other(
                "This book exceeds the KOReader visit import limit",
            ));
        }
        let page: u64 = row.get(0).map_err(db_error)?;
        let start: i64 = row.get(1).map_err(db_error)?;
        let seconds: u32 = row.get(2).map_err(db_error)?;
        let saved_pages: u64 = row.get(3).map_err(db_error)?;
        let key = (page, start);
        if page == 0
            || page > saved_pages
            || !valid_wall(start)
            || !(1..=MAX_VISIT_SECONDS).contains(&seconds)
            || previous == Some(key)
        {
            return Err(invalid_database());
        }
        previous = Some(key);
        if saved_pages == pages as u64 {
            records.visits.push(Record {
                page: page - 1,
                start,
                seconds,
            });
        } else {
            records.other_page_counts += 1;
        }
    }
    Ok(records)
}

fn read_label(value: Option<String>) -> io::Result<String> {
    let value = value.as_deref().unwrap_or_default();
    let label = value.trim_matches('\0');
    if label.len() > MAX_LABEL_BYTES || label.chars().any(|c| c.is_control() && !c.is_whitespace())
    {
        return Err(invalid_database());
    }
    Ok(label.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn invalid_database() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "This is not a supported KOReader statistics database",
    )
}

fn db_error(_: rusqlite::Error) -> io::Error {
    invalid_database()
}

impl History {
    fn import_rows(&self, book: u64, records: &[Record]) -> Option<Vec<Visit>> {
        if self.read_only || self.data.visits.len() + records.len() > MAX_VISITS {
            return None;
        }
        self.next_id.checked_add(records.len() as u64)?;
        let rows = records
            .iter()
            .enumerate()
            .map(|(index, record)| Visit {
                book,
                page: record.page,
                start: Some(record.start),
                seconds: record.seconds,
                id: self.next_id + index as u64,
            })
            .collect::<Vec<_>>();
        let current_len = serde_json::to_vec(&self.data).ok()?.len();
        let added_len = serde_json::to_vec(&rows).ok()?.len().saturating_sub(2);
        let comma = usize::from(!rows.is_empty() && !self.data.visits.is_empty());
        (current_len + added_len + comma <= MAX_BYTES as usize).then_some(rows)
    }

    fn plan_koreader_import(&self, book: u64, records: &Records) -> (Preview, Vec<Record>) {
        let mut existing = BTreeMap::new();
        for visit in &self.data.visits {
            if visit.book == book
                && let Some(start) = visit.start
            {
                existing
                    .entry((visit.page, start))
                    .and_modify(|seconds| {
                        if *seconds != Some(visit.seconds) {
                            *seconds = None;
                        }
                    })
                    .or_insert(Some(visit.seconds));
            }
        }
        let mut preview = Preview::default();
        let mut new = Vec::new();
        for &visit in &records.visits {
            match existing.get(&(visit.page, visit.start)) {
                Some(&Some(value)) if value == visit.seconds => preview.duplicate += 1,
                Some(_) => preview.conflicting += 1,
                None => {
                    new.push(visit);
                    preview.seconds += u64::from(visit.seconds);
                    preview.first_start = Some(
                        preview
                            .first_start
                            .map_or(visit.start, |first| first.min(visit.start)),
                    );
                    preview.last_start = Some(
                        preview
                            .last_start
                            .map_or(visit.start, |last| last.max(visit.start)),
                    );
                }
            }
        }
        preview.new_visits = new.len();
        (preview, new)
    }

    pub(crate) fn preview_koreader_import(&self, book: u64, records: &Records) -> Preview {
        let (mut preview, new) = self.plan_koreader_import(book, records);
        preview.can_store = self.import_rows(book, &new).is_some();
        preview
    }

    pub(crate) fn import_koreader_visits(
        &mut self,
        book: u64,
        records: &Records,
        expected: Preview,
    ) -> io::Result<()> {
        if self.read_only {
            return Err(invalid());
        }
        let (mut current, new) = self.plan_koreader_import(book, records);
        let admissible = self.import_rows(book, &new);
        current.can_store = admissible.is_some();
        if current != expected {
            return Err(io::Error::other(
                "Reading statistics changed. Preview the import again.",
            ));
        }
        let rows = admissible.ok_or_else(invalid)?;
        if rows.is_empty() {
            return Ok(());
        }
        let before = self.data.visits.len();
        self.data.visits.extend(rows);
        if let Err(error) = self.persist() {
            self.data.visits.truncate(before);
            return Err(error);
        }
        self.next_id += current.new_visits as u64;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_import_capacity_detects_full_rows_bytes_and_ids_before_apply() {
        let (mut history, _, _, dir) = super::super::tests::fixture("import-capacity");
        let records = Records {
            visits: vec![Record {
                page: 0,
                start: 1_769_990_394,
                seconds: 20,
            }],
            title: String::new(),
            authors: String::new(),
            other_page_counts: 0,
        };
        assert!(history.preview_koreader_import(1, &records).can_store);
        history.next_id = u64::MAX;
        let blocked = history.preview_koreader_import(1, &records);
        assert_eq!(blocked.new_visits, 1);
        assert!(!blocked.can_store);
        assert!(
            history
                .import_koreader_visits(1, &records, blocked)
                .is_err()
        );
        history.next_id = 1;
        history.data.visits.resize(
            MAX_VISITS,
            Visit {
                book: 2,
                page: 1,
                start: None,
                seconds: 20,
                id: 1,
            },
        );
        assert!(!history.preview_koreader_import(1, &records).can_store);
        history.data.visits.clear();
        history
            .data
            .titles
            .insert(2, "x".repeat(MAX_BYTES as usize));
        assert!(!history.preview_koreader_import(1, &records).can_store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
