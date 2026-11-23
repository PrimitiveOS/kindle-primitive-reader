use super::*;
#[test]
fn live_defaults_keep_three_states_and_resolve_book_conflicts_without_deleting_global_choices() {
    let mut data = Data::default();
    data.make_global(11, "paragraph-align-left", true).unwrap();
    assert!(data.enabled(11, "paragraph-align-left"));
    assert!(data.enabled(12, "paragraph-align-left"));
    data.choose(12, "paragraph-align-left", Some(false))
        .unwrap();
    assert!(!data.enabled(12, "paragraph-align-left"));
    data.choose(11, "paragraph-align-justify", Some(true))
        .unwrap();
    assert!(data.enabled(11, "paragraph-align-justify"));
    assert!(!data.enabled(11, "paragraph-align-left"));
    assert!(data.global.contains("paragraph-align-left"));
    data.choose(11, "paragraph-align-left", None).unwrap();
    assert!(data.enabled(11, "paragraph-align-left"));
    assert!(!data.enabled(11, "paragraph-align-justify"));
    data.make_global(11, "paragraph-align-justify", true)
        .unwrap();
    assert!(!data.global.contains("paragraph-align-left"));
    assert!(data.enabled(12, "paragraph-align-justify"));
    assert_eq!(
        data.books[&12].choices.get("paragraph-align-left"),
        Some(&false)
    );
    let restored: Data = serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
    assert_eq!(restored, data);
}
#[test]
fn master_off_retains_css_and_unknown_ids_without_changing_immutable_snapshots() {
    let mut data = Data::default();
    data.global.insert("future-preset".into());
    data.global.insert("link-bold".into());
    let book = data.books.entry(3).or_default();
    book.custom = Some("p{font-weight:300!important}".into());
    book.custom_enabled = true;
    data.validate().unwrap();
    let snapshot = data.snapshot(3).unwrap();
    assert!(!snapshot.is_empty());
    data.books.get_mut(&3).unwrap().enabled = false;
    assert!(data.snapshot(3).unwrap().is_empty());
    assert!(data.books[&3].custom.is_some());
    assert!(snapshot.css().contains("font-weight:300"));
    assert_ne!(snapshot.digest, Snapshot::default().digest);
    assert!(data.global.contains("future-preset"));
}
#[test]
fn configurations_reject_bad_versions_ids_and_disabled_invalid_css_without_truncation() {
    let mut data = Data::default();
    data.version = 9;
    assert!(data.validate().is_err());
    data.version = 1;
    data.global.insert("../bad".into());
    assert!(data.validate().is_err());
    data.global.clear();
    data.books.entry(1).or_default().custom = Some("p{display:none}".into());
    assert!(data.validate().is_err());
    data.books.get_mut(&1).unwrap().custom = Some(" ".repeat(MAX_BOOK_CSS + 1));
    assert!(data.validate().is_err());
}
#[cfg(feature = "kindle")]
#[test]
fn atomic_store_failures_and_unknown_files_preserve_the_existing_bytes_and_model() {
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Directory(std::env::temp_dir().join(format!(
            "reader-style-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    std::fs::create_dir(&root.0).unwrap();
    let path = root.0.join(FILE);
    let mut store = storage::Store::load(path.clone());
    let mut next = Data::default();
    next.make_global(1, "link-bold", true).unwrap();
    store.save(next.clone()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(storage::Store::load(path.clone()).data, next);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let mut changed = next.clone();
    changed.global.clear();
    assert!(store.save(changed).is_err());
    assert_eq!(store.data, next);
    assert!(path.is_dir());
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, b"{\"version\":9,\"global\":[],\"books\":{}}").unwrap();
    let mut unreadable = storage::Store::load(path.clone());
    assert!(unreadable.problem.is_some());
    let old = std::fs::read(&path).unwrap();
    assert!(unreadable.save(Data::default()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), old);
    std::fs::remove_file(&path).unwrap();
    let outside = root.0.join("retained.json");
    std::fs::write(&outside, &bytes).unwrap();
    std::os::unix::fs::symlink(&outside, &path).unwrap();
    assert!(storage::Store::load(path).problem.is_some());
    assert_eq!(std::fs::read(outside).unwrap(), bytes);
}
