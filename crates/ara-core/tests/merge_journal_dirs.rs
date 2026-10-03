//! Directory-only initialization must use the same durable transaction journal.
#![cfg(feature = "native")]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use ara_core::write::{
    self, ApplyMode, ArtifactLock, ArtifactSnapshot, WorkingArtifact, WriteError, WriteOperation,
    journal::{self, DurableJournal},
    transaction::{self, TransactionBoundary, TransactionHooks, TransactionObserver},
};
use serde_json::Value;
use tempfile::TempDir;

const PAPER: &str = "# Journal artifact\n";
const SEEDS: &[(&str, &str)] = &[
    ("PAPER.md", PAPER),
    (".gitignore", ".ara/\n"),
    (
        "trace/exploration_tree.yaml",
        "tree: []\r\n# exact retained comment\r\n",
    ),
    ("trace/sessions/session_index.yaml", "sessions: []\n"),
    ("trace/pm_reasoning_log.yaml", "entries: []\n"),
    ("staging/observations.yaml", "observations: []\n"),
    (
        "logic/claims.md",
        "# Claims\r\n\r\n<!-- exact retained spacing  -->\r\n",
    ),
    ("logic/problem.md", "# Problem\n"),
    ("logic/solution/heuristics.md", "# Heuristics\n"),
    ("evidence/README.md", "# Evidence Index\n"),
];
const MISSING: &[&str] = &["evidence/figures", "evidence/tables", "src"];

fn fixture() -> (TempDir, PathBuf) {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path().join("artifact");
    for (path, bytes) in SEEDS {
        let destination = root.join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, bytes).unwrap();
    }
    (temporary, root)
}

fn init_operation() -> WriteOperation {
    WriteOperation::ArtifactInit {
        profile: "research-manager".into(),
        paper: Some(PAPER.into()),
        documents: BTreeMap::new(),
        missing_only: true,
    }
}

fn plan(root: &Path) -> WorkingArtifact {
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    let result = write::plan_operation(&mut working, &init_operation()).unwrap();
    assert!(!result.no_op);
    assert_eq!(working.changed_paths(), Vec::<String>::new());
    assert_eq!(
        working.created_dirs,
        MISSING
            .iter()
            .map(|path| (*path).to_owned())
            .collect::<BTreeSet<_>>()
    );
    working
}

fn assert_seed_bytes(root: &Path) {
    for (path, bytes) in SEEDS {
        assert_eq!(
            fs::read(root.join(path)).unwrap(),
            bytes.as_bytes(),
            "{path}"
        );
    }
}

fn prepared(root: &Path) -> PathBuf {
    root.join(".ara/transactions/active.json.prepared")
}

struct FailDirectorySync;
impl TransactionHooks for FailDirectorySync {
    fn boundary(&mut self, kind: TransactionBoundary, _path: &str) -> Result<(), WriteError> {
        if kind == TransactionBoundary::DirectorySync {
            return Err(WriteError::io("injected directory-only sync failure"));
        }
        Ok(())
    }
}

#[test]
fn missing_only_initialization_commits_missing_directories_without_rewriting_seeds() {
    let (_temporary, root) = fixture();
    let report = write::execute(&root, &[init_operation()], ApplyMode::Commit).unwrap();
    assert!(report.committed);
    assert_eq!(report.changed_paths, Vec::<String>::new());
    assert!(!report.operations[0].no_op);
    for directory in MISSING {
        assert!(root.join(directory).is_dir(), "{directory}");
    }
    assert_seed_bytes(&root);
    assert!(!journal::pending_prepared(&root).unwrap());
    let _lock = ArtifactLock::acquire(&root).unwrap();
    journal::recover(&root).unwrap();
    for directory in MISSING {
        assert!(root.join(directory).is_dir(), "{directory}");
    }
    assert_seed_bytes(&root);
}

#[test]
fn directory_only_sync_failure_rolls_back_exact_original_directory_existence() {
    let (_temporary, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = plan(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    let error = transaction::commit_with_hooks(&working, &mut observer, &mut FailDirectorySync)
        .unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert!(
        error
            .message
            .contains("injected directory-only sync failure")
    );
    for directory in MISSING {
        assert!(!root.join(directory).exists(), "{directory}");
    }
    assert_seed_bytes(&root);
    assert!(!journal::pending_prepared(&root).unwrap());
}

#[test]
fn prepared_directory_only_crash_removes_only_recorded_new_directories() {
    let (_temporary, root) = fixture();
    let lock = ArtifactLock::acquire(&root).unwrap();
    let working = plan(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    for directory in &working.created_dirs {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    observer.prepared(&working).unwrap();
    assert!(journal::pending_prepared(&root).unwrap());
    drop(observer);
    drop(lock);
    let _lock = ArtifactLock::acquire(&root).unwrap();
    journal::recover(&root).unwrap();
    for directory in MISSING {
        assert!(!root.join(directory).exists(), "{directory}");
    }
    assert!(root.join("evidence").is_dir());
    assert!(root.join("logic/solution").is_dir());
    assert_seed_bytes(&root);
    assert!(!journal::pending_prepared(&root).unwrap());
}

#[test]
fn directory_only_recovery_captures_implicit_missing_ancestors() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path().join("artifact");
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(&root).unwrap());
    working.created_dirs.insert("evidence/figures".into());
    let mut observer = DurableJournal::new(&working).unwrap();
    fs::create_dir_all(root.join("evidence/figures")).unwrap();
    observer.prepared(&working).unwrap();
    journal::recover(&root).unwrap();
    assert!(!root.join("evidence/figures").exists());
    assert!(!root.join("evidence").exists());
    assert!(root.join(".ara/lock").is_file());
}

#[test]
fn corrupt_directory_only_record_cannot_remove_unapproved_directories() {
    let (_temporary, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = plan(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    for directory in &working.created_dirs {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    observer.prepared(&working).unwrap();
    fs::create_dir(root.join("src/unapproved")).unwrap();
    let mut record: Value = serde_json::from_slice(&fs::read(prepared(&root)).unwrap()).unwrap();
    record["created_dirs"] = serde_json::json!(["src/unapproved"]);
    fs::write(prepared(&root), serde_json::to_vec(&record).unwrap()).unwrap();
    let error = journal::recover(&root).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.code, "write.journal_corrupt");
    assert!(root.join("src/unapproved").is_dir());
    for directory in MISSING {
        assert!(root.join(directory).is_dir());
    }
    assert_seed_bytes(&root);
    assert!(prepared(&root).is_file());
}

#[test]
fn foreign_contents_block_directory_only_rollback_and_preserve_evidence() {
    let (_temporary, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = plan(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    for directory in &working.created_dirs {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    observer.prepared(&working).unwrap();
    fs::write(root.join("src/external.txt"), b"external content\n").unwrap();
    let evidence = fs::read(prepared(&root)).unwrap();
    let error = journal::recover(&root).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.code, "write.recovery_failed");
    assert_eq!(
        fs::read(root.join("src/external.txt")).unwrap(),
        b"external content\n"
    );
    assert_eq!(fs::read(prepared(&root)).unwrap(), evidence);
    assert_seed_bytes(&root);
}

#[test]
fn committed_directory_removal_fails_closed_instead_of_recreating_external_state() {
    let (_temporary, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = plan(&root);
    transaction::commit(&working).unwrap();
    fs::remove_dir(root.join("src")).unwrap();
    let error = journal::recover(&root).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.code, "write.recovery_external_edit");
    assert!(!root.join("src").exists());
    assert!(
        root.join(".ara/transactions/active.json.committed")
            .is_file()
    );
    assert_seed_bytes(&root);
}
