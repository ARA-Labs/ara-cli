#![cfg(all(feature = "native", unix))]
use ara_core::write::{
    ArtifactLock, ArtifactSnapshot, WorkingArtifact, WriteError,
    transaction::{self, TransactionBoundary, TransactionHooks, TransactionObserver},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "ara-writer-transaction-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("logic/solution")).unwrap();
        fs::create_dir_all(root.join("trace")).unwrap();
        fs::write(root.join("trace/exploration_tree.yaml"), "tree: []\n").unwrap();
        fs::write(root.join("logic/solution/a.md"), "# A\n\nOriginal α\n").unwrap();
        fs::write(
            root.join("logic/solution/b.md"),
            "# B\r\n\r\nOriginal β\r\n",
        )
        .unwrap();
        Self(root)
    }
    fn staged(&self) -> WorkingArtifact {
        let mut working = WorkingArtifact::new(ArtifactSnapshot::load(&self.0).unwrap());
        working
            .replace_document(
                "logic/solution/a.md",
                "# A\n\nCandidate\n",
                "selected complete body",
            )
            .unwrap();
        working
            .replace_document(
                "logic/solution/b.md",
                "# B\n\nCandidate\n",
                "selected complete body",
            )
            .unwrap();
        working.create("logic/solution/new.md", "# New\n").unwrap();
        working.include_operational_ignore().unwrap();
        working
    }
    fn assert_original(&self) {
        assert_eq!(
            fs::read(self.0.join("logic/solution/a.md")).unwrap(),
            "# A\n\nOriginal α\n".as_bytes()
        );
        assert_eq!(
            fs::read(self.0.join("logic/solution/b.md")).unwrap(),
            "# B\r\n\r\nOriginal β\r\n".as_bytes()
        );
        assert!(!self.0.join("logic/solution/new.md").exists());
        assert!(!self.0.join(".gitignore").exists());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Observer;
impl TransactionObserver for Observer {
    fn prepared(&mut self, _: &WorkingArtifact) -> Result<(), WriteError> {
        Ok(())
    }
    fn completed(&mut self) -> Result<(), WriteError> {
        Ok(())
    }
    fn rolled_back(&mut self) -> Result<(), WriteError> {
        Ok(())
    }
}
struct Fault {
    kind: TransactionBoundary,
    nth: usize,
    seen: usize,
    rollback: bool,
}
impl TransactionHooks for Fault {
    fn boundary(&mut self, kind: TransactionBoundary, path: &str) -> Result<(), WriteError> {
        if self.rollback && kind == TransactionBoundary::RollbackRename {
            return Err(WriteError::io(format!("injected rollback rename {path}")));
        }
        if kind == self.kind {
            self.seen += 1;
            if self.seen == self.nth {
                return Err(WriteError::io(format!("injected {kind:?} {path}")));
            }
        }
        Ok(())
    }
}
#[test]
fn every_ordinary_boundary_failure_restores_bytes_and_existence() {
    for kind in [
        TransactionBoundary::TemporaryWrite,
        TransactionBoundary::TemporarySync,
        TransactionBoundary::BeforeRename,
        TransactionBoundary::AfterRename,
        TransactionBoundary::DirectorySync,
    ] {
        for nth in 1..=2 {
            let fixture = Fixture::new();
            let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
            let working = fixture.staged();
            let mut fault = Fault {
                kind,
                nth,
                seen: 0,
                rollback: false,
            };
            let error =
                transaction::commit_with_hooks(&working, &mut Observer, &mut fault).unwrap_err();
            assert_eq!(error.exit_code(), 2);
            fixture.assert_original();
            assert!(
                !fs::read_dir(fixture.0.join("logic/solution"))
                    .unwrap()
                    .any(|entry| entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".ara-write-"))
            );
        }
    }
}
#[test]
fn failed_rollback_lists_uncertain_path_and_both_digests() {
    let fixture = Fixture::new();
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let working = fixture.staged();
    let mut fault = Fault {
        kind: TransactionBoundary::AfterRename,
        nth: 2,
        seen: 0,
        rollback: true,
    };
    let error = transaction::commit_with_hooks(&working, &mut Observer, &mut fault).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert!(error.message.contains("rollback incomplete"));
    assert!(error.message.contains("logic/solution/a.md"));
    assert!(error.message.contains("original=sha256:"));
    assert!(error.message.contains("intended=sha256:"));
}
#[test]
fn noncooperating_edit_is_detected_after_staging_without_overwriting_it() {
    let fixture = Fixture::new();
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let working = fixture.staged();
    struct External(PathBuf, bool);
    impl TransactionHooks for External {
        fn boundary(&mut self, kind: TransactionBoundary, _: &str) -> Result<(), WriteError> {
            if !self.1 && kind == TransactionBoundary::TemporarySync {
                fs::write(self.0.join("logic/solution/b.md"), "External editor\n").unwrap();
                self.1 = true;
            }
            Ok(())
        }
    }
    let error = transaction::commit_with_hooks(
        &working,
        &mut Observer,
        &mut External(fixture.0.clone(), false),
    )
    .unwrap_err();
    assert_eq!(error.code, "write.concurrent_edit");
    assert_eq!(
        fs::read_to_string(fixture.0.join("logic/solution/b.md")).unwrap(),
        "External editor\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("logic/solution/a.md")).unwrap(),
        "# A\n\nOriginal α\n"
    );
    assert!(!fixture.0.join("logic/solution/new.md").exists());
}
#[test]
fn persistent_lock_inode_survives_multiple_writes() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let inode = fs::metadata(fixture.0.join(".ara/lock")).unwrap().ino();
    drop(lock);
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    assert_eq!(
        fs::metadata(fixture.0.join(".ara/lock")).unwrap().ino(),
        inode
    );
}
#[test]
fn ordinary_commit_retains_permissions_and_operational_ignore_contents() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fs::set_permissions(
        fixture.0.join("logic/solution/a.md"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    fs::write(fixture.0.join(".gitignore"), "# user rules\r\n*.tmp").unwrap();
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let working = fixture.staged();
    transaction::commit_with(&working, &mut Observer).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.0.join(".gitignore")).unwrap(),
        "# user rules\r\n*.tmp\r\n.ara/\r\n"
    );
    assert_eq!(
        fs::metadata(fixture.0.join("logic/solution/a.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("logic/solution/new.md")).unwrap(),
        "# New\n"
    );
}
#[test]
fn newly_created_directories_are_removed_on_recoverable_failure() {
    let fixture = Fixture::new();
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let mut working = fixture.staged();
    working
        .create("logic/solution/sub/new.md", "# Nested\n")
        .unwrap();
    working.created_dirs.insert("evidence/figures".into());
    let mut fault = Fault {
        kind: TransactionBoundary::AfterRename,
        nth: 2,
        seen: 0,
        rollback: false,
    };
    let error = transaction::commit_with_hooks(&working, &mut Observer, &mut fault).unwrap_err();
    assert!(error.message.contains("injected AfterRename"));
    fixture.assert_original();
    assert!(!fixture.0.join("logic/solution/sub").exists());
    assert!(!fixture.0.join("evidence").exists());
}
#[test]
fn destination_parent_symlink_cannot_escape_artifact() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = Fixture::new();
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let mut working = fixture.staged();
    working
        .stage_create("logic/solution/link/new.md", b"# Escaped\n")
        .unwrap();
    symlink(&outside.0, fixture.0.join("logic/solution/link")).unwrap();
    assert!(transaction::commit_with(&working, &mut Observer).is_err());
    assert!(!outside.0.join("new.md").exists());
    fixture.assert_original();
}

#[test]
fn appearance_of_optional_source_is_detected_without_deleting_external_content() {
    let fixture = Fixture::new();
    let _lock = ArtifactLock::acquire(&fixture.0).unwrap();
    let working = fixture.staged();
    struct Appearing(PathBuf, bool);
    impl TransactionHooks for Appearing {
        fn boundary(&mut self, kind: TransactionBoundary, _: &str) -> Result<(), WriteError> {
            if !self.1 && kind == TransactionBoundary::TemporarySync {
                fs::write(
                    self.0.join("logic/claims.md"),
                    "# Claims\n\n## C01: external source\n",
                )
                .unwrap();
                self.1 = true;
            }
            Ok(())
        }
    }
    let error = transaction::commit_with_hooks(
        &working,
        &mut Observer,
        &mut Appearing(fixture.0.clone(), false),
    )
    .unwrap_err();
    assert_eq!(error.code, "write.concurrent_edit");
    fixture.assert_original();
    assert_eq!(
        fs::read_to_string(fixture.0.join("logic/claims.md")).unwrap(),
        "# Claims\n\n## C01: external source\n"
    );
}

#[test]
fn flow_mapping_field_edits_keep_unrelated_source_values_and_exact_bytes() {
    use ara_core::write::source::PathPart;
    let fixture = Fixture::new();
    let path = "trace/pm_reasoning_log.yaml";
    let text = "entries: [{turn: '2026-10-01_001#1', notes: [original], opaque: {value: '雪 = literal'}}] # retained\n";
    fs::write(fixture.0.join(path), text).unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(&fixture.0).unwrap());
    let selector = [PathPart::from("entries"), PathPart::Index(0)];
    working
        .replace_yaml_field(
            path,
            &selector,
            "notes",
            &serde_json::json!(["full caller note\n@literal=$binding\n"]),
        )
        .unwrap();
    working
        .replace_yaml_field(
            path,
            &selector,
            "summary",
            &serde_json::json!("complete supplied text"),
        )
        .unwrap();
    let source = working.text(path).unwrap();
    assert!(source.contains("opaque: {value: '雪 = literal'}"));
    assert!(source.ends_with("] # retained\n"));
    let parsed = working.yaml(path).unwrap().root.to_json().unwrap();
    assert_eq!(
        parsed["entries"][0]["notes"],
        serde_json::json!(["full caller note\n@literal=$binding\n"])
    );
    assert_eq!(parsed["entries"][0]["summary"], "complete supplied text");
    assert_eq!(fs::read_to_string(fixture.0.join(path)).unwrap(), text);
}
