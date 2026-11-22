//! Library-format gate for DOM boundary locations. An older version1 decoder
//! must reject the envelope before it can ignore Anchor.boundary and use a
//! misleading fallback caret. Text-only legacy bytes keep their original form.
use super::*;

pub(super) const VERSION: u32 = 2;

fn anchors(state: &BookState) -> impl Iterator<Item = &Anchor> {
    state
        .position
        .iter()
        .chain(state.bookmarks.iter().map(|mark| &mark.position))
        .chain(
            state
                .custom_contents
                .entries()
                .iter()
                .map(|mark| &mark.position),
        )
        .chain(
            state
                .hidden_flows
                .points()
                .iter()
                .map(|point| &point.position),
        )
        .filter_map(|position| position.anchor.as_ref())
        .chain(
            state
                .highlights
                .iter()
                .flat_map(|mark| [&mark.selection.start, &mark.selection.end]),
        )
}
pub(super) fn present(states: &[BookState]) -> bool {
    states
        .iter()
        .any(|state| anchors(state).any(|anchor| anchor.boundary.is_some()))
}
pub(super) fn version(states: &[BookState]) -> u32 {
    if present(states) {
        VERSION
    } else {
        BACKUP_VERSION
    }
}
pub(super) fn validate_version(version: u32, states: &[BookState]) -> io::Result<()> {
    if !matches!(version, BACKUP_VERSION | VERSION) {
        return Err(invalid_state(
            "Reader library format version is unsupported",
        ));
    }
    if version == BACKUP_VERSION && present(states) {
        return Err(invalid_state(
            "Saved locations do not match the declared library format",
        ));
    }
    validate(states)
}
pub(super) fn validate(states: &[BookState]) -> io::Result<()> {
    for state in states {
        for anchor in anchors(state).filter(|anchor| anchor.boundary.is_some()) {
            if !valid_anchor(anchor)
                || anchor.block > 64 * 1024 * 1024
                || anchor.offset > 64 * 1024 * 1024
            {
                return Err(invalid_state(
                    "Reader data contains an invalid saved document location",
                ));
            }
        }
        if state.highlights.iter().any(|mark| {
            mark.selection.start.boundary.is_some() || mark.selection.end.boundary.is_some()
        }) {
            return Err(invalid_state(
                "A document location without text cannot be a highlight endpoint",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-boundary-state-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn state(boundary: bool) -> BookState {
        let anchor: Anchor = serde_json::from_value(
            json!({"book":1,"chapter":"chapter.xhtml","revision":2,"block":3,"offset":4}),
        )
        .unwrap();
        let mut position = ReadingPosition {
            chapter: 0,
            page: 0,
            anchor: Some(anchor),
        };
        if boundary {
            position.anchor.as_mut().unwrap().boundary = Some(crate::SourceBoundary {
                origin: 25,
                relative: 0,
            });
        }
        serde_json::from_value(json!({"id":1,"path":"book.epub","title":"Boundary","position":position,"bookmarks":[],"highlights":[],"pdf_highlights":[]})).unwrap()
    }

    #[test]
    fn legacy_library_bytes_match_the_old_codec_and_boundary_payloads_are_version_gated() {
        let scratch = Scratch::new();
        let profile = scratch.0.join("library.json");
        let legacy = vec![state(false)];
        let before = state_json(
            &SavedLibraryRef {
                version: 1,
                books: &legacy,
                collections: &[],
            },
            MAX_STATE_BYTES,
        )
        .unwrap();
        assert_eq!(
            encode_library(&legacy, &[], MAX_STATE_BYTES).unwrap(),
            before,
            "new code must preserve legacy bytes"
        );
        let mut target = Library::open(&scratch.0, &profile).unwrap();
        let boundary = vec![state(true)];
        let bytes = encode_library(&boundary, &[], MAX_STATE_BYTES).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 2);
        assert!(
            value["version"].as_u64() != Some(u64::from(BACKUP_VERSION)),
            "old v1 decoder rejects before ignoring new Anchor field"
        );
        assert!(target.preview_backup(&bytes).is_ok());
        fs::write(&profile, &bytes).unwrap();
        assert!(Library::open(&scratch.0, &profile).is_ok());
        let mut forged = value.clone();
        forged["version"] = json!(1);
        let forged = serde_json::to_vec(&forged).unwrap();
        assert!(target.preview_backup(&forged).is_err());
        fs::write(&profile, &forged).unwrap();
        assert!(Library::open(&scratch.0, &profile).is_err());
        fs::write(&profile, serde_json::to_vec(&boundary).unwrap()).unwrap();
        assert!(
            Library::open(&scratch.0, &profile).is_err(),
            "legacy arrays must not carry boundary locations"
        );
        target.recent = legacy;
        assert_eq!(target.bytes().unwrap(), before);
    }

    #[test]
    fn malformed_boundary_components_and_text_endpoints_are_rejected_before_load_merge_or_write() {
        let scratch = Scratch::new();
        let path = scratch.0.join("library.json");
        let target = Library::open(&scratch.0, &path).unwrap();
        let before = target.bytes().unwrap();
        for change in [0, 1, 2, 3, 4] {
            let mut book = state(true);
            let anchor = book.position.as_mut().unwrap().anchor.as_mut().unwrap();
            match change {
                0 => anchor.boundary.as_mut().unwrap().relative = 10001,
                1 => anchor.boundary.as_mut().unwrap().origin = usize::MAX,
                2 => anchor.illustration = Some(0),
                3 => anchor.chapter = "bad\nchapter.xhtml".into(),
                _ => anchor.offset = usize::MAX,
            }
            let bad = state_json(
                &SavedLibraryRef {
                    version: 2,
                    books: &[book.clone()],
                    collections: &[],
                },
                MAX_STATE_BYTES,
            )
            .unwrap();
            assert!(target.preview_backup(&bad).is_err());
            assert_eq!(target.bytes().unwrap(), before);
            fs::write(&path, &bad).unwrap();
            assert!(Library::open(&scratch.0, &path).is_err());
            assert!(encode_library(&[book], &[], MAX_STATE_BYTES).is_err());
        }
        let mut book = state(true);
        let start = book.position.as_ref().unwrap().anchor.clone().unwrap();
        let end = Anchor {
            offset: 8,
            ..start.clone()
        };
        book.highlights.push(Highlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            selection: TextSelection {
                start,
                end,
                text: "Unrelated fallback must not be exported".into(),
            },
            note: None,
        });
        assert!(encode_library(&[book], &[], MAX_STATE_BYTES).is_err());
    }
}
