use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "reader-backup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn state(&self) -> PathBuf {
        self.0.join("library.json")
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "private subprocess entry point used by complete_backup_actual_process_exit_recovers_before_restart"]
fn complete_backup_crash_child() {
    let state = PathBuf::from(std::env::var_os("READER_BACKUP_CRASH_STATE").unwrap());
    let at: usize = std::env::var("READER_BACKUP_CRASH_AT")
        .unwrap()
        .parse()
        .unwrap();
    let _ = transaction_observed(
        &state,
        &[
            (Key::Library, b"new library".to_vec()),
            (Key::Controls, b"new controls".to_vec()),
            (Key::Pace(9), b"new pace".to_vec()),
        ],
        || Ok(()),
        |step| {
            if step == at {
                std::process::exit(73);
            }
        },
    );
    panic!("The interruption point was not reached");
}

#[test]
fn complete_backup_actual_process_exit_recovers_before_restart() {
    for at in 0..=4 {
        let root = Root::new();
        fs::write(root.state(), b"old library").unwrap();
        fs::write(root.0.join("reading-controls.json"), b"old controls").unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "reading_backup::tests::complete_backup_crash_child",
                "--ignored",
                "--exact",
                "--test-threads=1",
            ])
            .env("READER_BACKUP_CRASH_STATE", root.state())
            .env("READER_BACKUP_CRASH_AT", at.to_string())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        recover(&root.state()).unwrap();
        let committed = at == 4;
        assert_eq!(
            fs::read(root.state()).unwrap(),
            if committed {
                b"new library"
            } else {
                b"old library"
            }
        );
        assert_eq!(
            fs::read(root.0.join("reading-controls.json")).unwrap(),
            if committed {
                b"new controls"
            } else {
                b"old controls"
            }
        );
        assert_eq!(
            root.0.join("reading-pace/0000000000000009.json").exists(),
            committed
        );
        recover(&root.state()).unwrap();
        assert!(!root.0.join(DIRECTORY).exists());
    }
}

#[test]
fn complete_backup_process_interruption_recovers_every_precommit_step_and_keeps_committed_data() {
    for interruption in 0..=4 {
        let root = Root::new();
        fs::write(root.state(), b"old library").unwrap();
        fs::write(root.0.join("reading-controls.json"), b"old controls").unwrap();
        fs::write(root.0.join("unrelated-user-file"), b"Unchanged source").unwrap();
        let updates = vec![
            (Key::Library, b"new library".to_vec()),
            (Key::Controls, b"new controls".to_vec()),
            (Key::Pace(12), b"new pace".to_vec()),
        ];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            transaction_observed(
                &root.state(),
                &updates,
                || Ok(()),
                |step| {
                    if step == interruption {
                        panic!("Simulated process loss after step {step}");
                    }
                },
            )
        }));
        assert!(result.is_err());
        assert!(root.0.join(DIRECTORY).is_dir());
        recover(&root.state()).unwrap();
        recover(&root.state()).unwrap();
        let committed = interruption == 4;
        assert_eq!(
            fs::read(root.state()).unwrap(),
            if committed {
                b"new library"
            } else {
                b"old library"
            }
        );
        assert_eq!(
            fs::read(root.0.join("reading-controls.json")).unwrap(),
            if committed {
                b"new controls"
            } else {
                b"old controls"
            }
        );
        assert_eq!(
            root.0.join("reading-pace/000000000000000c.json").exists(),
            committed
        );
        assert_eq!(
            fs::read(root.0.join("unrelated-user-file")).unwrap(),
            b"Unchanged source"
        );
        assert!(!root.0.join(DIRECTORY).exists());
    }
}

#[test]
fn complete_backup_reload_failure_rolls_back_every_file_and_removes_new_components() {
    let root = Root::new();
    fs::write(root.state(), b"original library").unwrap();
    let result: io::Result<()> = transaction(
        &root.state(),
        &[
            (Key::Library, b"changed library".to_vec()),
            (Key::Profiles, b"[]".to_vec()),
            (Key::Pace(5), b"new pace".to_vec()),
        ],
        || Err(io::Error::other("Injected reload failure")),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Injected reload failure")
    );
    assert_eq!(fs::read(root.state()).unwrap(), b"original library");
    assert!(!root.0.join("reading-profiles.json").exists());
    assert!(!root.0.join("reading-pace/0000000000000005.json").exists());
    assert!(!root.0.join(DIRECTORY).exists());
}

#[test]
fn complete_backup_refuses_changed_recovery_slots_before_mutating_other_targets() {
    let root = Root::new();
    fs::write(root.state(), b"original library").unwrap();
    fs::write(root.0.join("reading-controls.json"), b"original controls").unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = transaction_observed(
            &root.state(),
            &[
                (Key::Library, b"new library".to_vec()),
                (Key::Controls, b"new controls".to_vec()),
            ],
            || Ok(()),
            |step| {
                if step == 1 {
                    panic!("Simulated process loss");
                }
            },
        );
    }));
    fs::write(root.0.join(DIRECTORY).join("old-1"), b"Corrupted slot").unwrap();
    assert!(recover(&root.state()).is_err());
    assert_eq!(fs::read(root.state()).unwrap(), b"new library");
    assert_eq!(
        fs::read(root.0.join("reading-controls.json")).unwrap(),
        b"original controls"
    );
    assert!(root.0.join(DIRECTORY).exists());
}

#[test]
fn complete_backup_enum_targets_reject_network_paths_symlinks_duplicates_and_oversized_inputs() {
    let legacy = br#"{"version":1,"books":[],"collections":[]}"#;
    let envelope = Envelope::parse(legacy).unwrap();
    assert!(envelope.components.is_empty());
    let base = serde_json::json!({"format":"primitiveos-reader-backup","version":2,
        "library":{"version":1,"books":[],"collections":[]},"components":[]});
    for key in [
        "../outside",
        "opds",
        "progress_sync",
        "online",
        "credentials",
    ] {
        let mut value = base.clone();
        value["components"] = serde_json::json!([{"key":{"kind":key},"data":{}}]);
        assert!(Envelope::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut duplicate = base.clone();
    duplicate["components"] = serde_json::json!([
        {"key":{"kind":"controls"},"data":{}},{"key":{"kind":"controls"},"data":{}}]);
    assert!(Envelope::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let mut future = base.clone();
    future["version"] = 99.into();
    assert!(Envelope::parse(&serde_json::to_vec(&future).unwrap()).is_err());
    let root = Root::new();
    fs::write(root.state(), b"old library").unwrap();
    let result = transaction(&root.state(), &[(Key::Controls, vec![b'x'; 4097])], || {
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read(root.state()).unwrap(), b"old library");
    assert!(recover(&root.0.join("reading-controls.json")).is_err());
    assert!(recover(&root.0.join("reading-pace")).is_err());
    for reserved in [
        "READING-CONTROLS.JSON",
        "Reading-Controls.Json",
        "DOCUMENT-DEFAULTS.JSON",
        "Reading-History.Json",
        ".READING-BACKUP-TRANSACTION",
        "READING-PACE",
    ] {
        assert!(recover(&root.0.join(reserved)).is_err(), "{reserved}");
    }
    #[cfg(unix)]
    {
        let outside = Root::new();
        fs::write(outside.state(), b"Source publication").unwrap();
        std::os::unix::fs::symlink(outside.state(), root.0.join("reading-controls.json")).unwrap();
        assert!(
            transaction(
                &root.state(),
                &[(Key::Controls, b"new controls".to_vec())],
                || Ok(())
            )
            .is_err()
        );
        assert_eq!(fs::read(outside.state()).unwrap(), b"Source publication");
    }
}
