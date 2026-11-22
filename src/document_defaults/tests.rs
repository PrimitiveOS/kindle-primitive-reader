use super::*;
use crate::fixed_text::PdfTextRect;

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "reader-document-defaults-{}-{time}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

#[test]
fn fixed_templates_exclude_geometry_and_reuse_only_the_current_books_geometry() {
    let source = FixedPreferences {
        zoom: 3,
        turns: 1,
        crop: Some(PdfTextRect {
            x0: 0.1,
            y0: 0.2,
            x1: 0.8,
            y1: 0.9,
        }),
        offset_x: 72.,
        offset_y: -42.,
        scan_contrast: true,
        ..Default::default()
    };
    let template = FixedDefaults::from_view(source);
    let value = serde_json::to_value(template).unwrap();
    for key in ["crop", "offset_x", "offset_y"] {
        assert!(value.get(key).is_none());
    }
    let new = template.apply(FixedPreferences::default());
    assert_eq!(new.crop, None);
    assert_eq!((new.offset_x, new.offset_y), (0., 0.));
    let current = FixedPreferences {
        offset_x: -19.,
        offset_y: 23.,
        crop: source.crop,
        ..Default::default()
    };
    let reset = template.apply(current);
    assert_eq!(
        (reset.crop, reset.offset_x, reset.offset_y),
        (current.crop, current.offset_x, current.offset_y)
    );
    assert_eq!((reset.zoom, reset.turns, reset.scan_contrast), (3, 1, true));
}

#[test]
fn defaults_are_optional_distinct_atomic_and_retained_across_reopening() {
    let root = TestDirectory::new();
    let path = root.0.join(FILE);
    let mut store = Store::load(path.clone());
    assert!(!path.exists());
    assert!(store.problem.is_none());
    let mut templates = store.templates.clone();
    templates.reflow = Some(BookPreferences {
        font_size: Some(59.),
        scroll: true,
        ..Default::default()
    });
    templates.fixed = Some(FixedDefaults::from_view(FixedPreferences {
        zoom: 2,
        right_to_left: true,
        ..Default::default()
    }));
    let mut invalid = templates.clone();
    invalid.reflow.as_mut().unwrap().font_size = Some(f32::NAN);
    assert!(store.save(invalid).is_err());
    assert!(!path.exists());
    store.save(templates.clone()).unwrap();
    assert_eq!(Store::load(path.clone()).templates, templates);
    let bytes = fs::read(&path).unwrap();
    validate_backup(&bytes).unwrap();
    // Serde JSON may turn nonfinite floats into null. Validate every typed
    // floating field before serialization and preserve the existing file/model.
    for field in 0..5 {
        let mut invalid = templates.clone();
        let prefs = invalid.reflow.as_mut().unwrap();
        match field {
            0 => prefs.font_size = Some(f32::NAN),
            1 => prefs.line_height = Some(f32::INFINITY),
            2 => prefs.word_spacing = f32::NAN,
            3 => prefs.letter_spacing = f32::NEG_INFINITY,
            _ => prefs.font_weight_delta = Some(f32::NAN),
        }
        assert!(store.save(invalid).is_err());
        assert_eq!(store.templates, templates);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    // A failure at the final destination must not commit new in-memory defaults.
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let mut changed = templates.clone();
    changed.reflow = None;
    assert!(store.save(changed).is_err());
    assert_eq!(store.templates, templates);
    assert!(path.is_dir());
}

#[test]
fn unreadable_future_oversized_or_symlinked_defaults_are_preserved_read_only() {
    let root = TestDirectory::new();
    let path = root.0.join(FILE);
    for bytes in [
        b"{bad".to_vec(),
        br#"{"version":2,"reflow":null,"fixed":null}"#.to_vec(),
        vec![b' '; MAX_BYTES as usize + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        let mut store = Store::load(path.clone());
        assert!(store.problem.is_some());
        assert!(store.save(Templates::default()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::remove_file(&path).unwrap();
    let target = root.0.join("outside.json");
    fs::write(&target, b"preserve").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let mut store = Store::load(path);
    assert!(store.problem.is_some());
    assert!(store.save(Templates::default()).is_err());
    assert_eq!(fs::read(target).unwrap(), b"preserve");
}

#[test]
fn backup_and_profile_validators_share_exact_ranges_without_resolving_font_files() {
    for prefs in [
        BookPreferences {
            font_size: Some(11.),
            ..Default::default()
        },
        BookPreferences {
            line_height: Some(4.),
            ..Default::default()
        },
        BookPreferences {
            font_file: Some("../Font.ttf".into()),
            ..Default::default()
        },
        BookPreferences {
            font_gamma_index: Some(255),
            ..Default::default()
        },
        BookPreferences {
            word_spacing: f32::NAN,
            ..Default::default()
        },
    ] {
        assert!(validate_preferences(&prefs).is_err());
    }
    validate_preferences(&BookPreferences {
        font_file: Some("Not installed yet.otf".into()),
        ..Default::default()
    })
    .unwrap();
    for json in [
        r#"{"version":1,"fixed":{"crop":{"x0":0,"x1":1,"y0":0,"y1":1}}}"#,
        r#"{"version":1,"fixed":{"zoom":255}}"#,
        r#"{"version":1,"fixed":{"turns":4}}"#,
        r#"{"version":1,"fixed":{"offset_x":2}}"#,
        r#"{"version":1,"fixed":{"width_fit":true,"height_fit":true}}"#,
        r#"{"version":1,"fixed":{"scan_white_level":1}}"#,
    ] {
        assert!(validate_backup(json.as_bytes()).is_err(), "{json}");
    }
    validate_backup(br#"{"version":1,"reflow":null,"fixed":null}"#).unwrap();
}
