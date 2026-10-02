#![cfg(feature = "native")]

use std::{
    fs,
    path::{Path, PathBuf},
};

use ara_core::write::{
    ArtifactLock, ArtifactSnapshot, WorkingArtifact, WriteError,
    journal::{self, DurableJournal},
    transaction::{self, TransactionBoundary, TransactionHooks, TransactionObserver},
};
use serde_json::Value;
use tempfile::TempDir;

const PROBLEM: &str = "logic/problem.md";
const CONCEPTS: &str = "logic/concepts.md";
const NEW_FILE: &str = "logic/solution/new.md";
const ORIGINAL_PROBLEM: &[u8] =
    b"# Problem\r\n\r\nOriginal A.\r\n<!-- retain exact spacing  -->\r\n";
const ORIGINAL_CONCEPTS: &[u8] = b"# Concepts\n\nOriginal B.\n";

fn fixture() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("artifact");
    fs::create_dir_all(root.join("trace")).unwrap();
    fs::create_dir_all(root.join("logic")).unwrap();
    fs::write(root.join("trace/exploration_tree.yaml"), b"tree: []\n").unwrap();
    fs::write(root.join(PROBLEM), ORIGINAL_PROBLEM).unwrap();
    fs::write(root.join(CONCEPTS), ORIGINAL_CONCEPTS).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join(PROBLEM), fs::Permissions::from_mode(0o640)).unwrap();
    }
    (directory, root)
}

fn working(root: &Path) -> WorkingArtifact {
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    for (path, before, after) in [
        (PROBLEM, "Original A", "Candidate A"),
        (CONCEPTS, "Original B", "Candidate B"),
    ] {
        let start = working.text(path).unwrap().find(before).unwrap();
        working
            .edit(
                path,
                start..start + before.len(),
                after,
                "journal acceptance mutation",
            )
            .unwrap();
    }
    working
}

fn marker(root: &Path, state: &str) -> PathBuf {
    root.join(".ara/transactions")
        .join(format!("active.json.{state}"))
}

fn temporary(working: &WorkingArtifact, path: &str) -> PathBuf {
    let ordinal = working
        .changed_paths()
        .iter()
        .position(|target| target == path)
        .unwrap();
    working
        .base
        .root
        .join(path)
        .with_file_name(format!(".ara-write-900001-{ordinal}"))
}
fn prepare(observer: &mut DurableJournal, working: &WorkingArtifact) {
    let mut candidates = Vec::new();
    for path in working.changed_paths() {
        let Some(bytes) = working.files.get(&path) else {
            continue;
        };
        let temporary = temporary(working, &path);
        fs::create_dir_all(temporary.parent().unwrap()).unwrap();
        fs::write(&temporary, bytes).unwrap();
        fs::File::open(&temporary).unwrap().sync_all().unwrap();
        candidates.push((path, temporary));
    }
    observer.staged(&candidates).unwrap();
    observer.prepared(working).unwrap();
}
fn install_candidate(working: &WorkingArtifact, path: &str) {
    let destination = working.base.root.join(path);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    if working.deleted_paths.contains(path) {
        fs::remove_file(destination).unwrap();
    } else {
        fs::rename(temporary(working, path), destination).unwrap();
    }
}

fn assert_originals(root: &Path) {
    assert_eq!(fs::read(root.join(PROBLEM)).unwrap(), ORIGINAL_PROBLEM);
    assert_eq!(fs::read(root.join(CONCEPTS)).unwrap(), ORIGINAL_CONCEPTS);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join(PROBLEM))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o640
        );
    }
}

struct FailRename {
    fail_before: usize,
    seen: usize,
    fail_rollback: bool,
}

impl TransactionHooks for FailRename {
    fn boundary(&mut self, kind: TransactionBoundary, _path: &str) -> Result<(), WriteError> {
        if kind == TransactionBoundary::BeforeRename {
            let index = self.seen;
            self.seen += 1;
            if index == self.fail_before {
                return Err(WriteError::io("injected destination rename failure"));
            }
        }
        if kind == TransactionBoundary::RollbackWrite && self.fail_rollback {
            return Err(WriteError::io("injected rollback write failure"));
        }
        Ok(())
    }
}

#[test]
fn failures_before_first_and_middle_rename_restore_exact_preimages() {
    for fail_before in [0, 1] {
        let (_directory, root) = fixture();
        let _lock = ArtifactLock::acquire(&root).unwrap();
        let working = working(&root);
        let mut journal = DurableJournal::new(&working).unwrap();
        let mut failure = FailRename {
            fail_before,
            seen: 0,
            fail_rollback: false,
        };
        let error =
            transaction::commit_with_hooks(&working, &mut journal, &mut failure).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(
            error
                .message
                .contains("injected destination rename failure")
        );
        assert_originals(&root);
        assert!(!journal::pending_prepared(&root).unwrap());
        assert!(!marker(&root, "prepared").exists());
    }
}

#[test]
fn incomplete_rollback_preserves_evidence_and_a_later_writer_recovers() {
    let (_directory, root) = fixture();
    let lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    let mut journal = DurableJournal::new(&working).unwrap();
    let mut failure = FailRename {
        fail_before: 1,
        seen: 0,
        fail_rollback: true,
    };
    let error = transaction::commit_with_hooks(&working, &mut journal, &mut failure).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert!(error.message.contains("rollback incomplete"));
    assert!(journal::pending_prepared(&root).unwrap());
    assert_eq!(
        fs::read(root.join(CONCEPTS)).unwrap(),
        working.files[CONCEPTS]
    );
    assert_eq!(fs::read(root.join(PROBLEM)).unwrap(), ORIGINAL_PROBLEM);
    drop(journal);
    drop(lock);
    let _lock = ArtifactLock::acquire(&root).unwrap();
    journal::recover(&root).unwrap();
    assert_originals(&root);
    assert!(!journal::pending_prepared(&root).unwrap());
}

#[test]
fn prepared_crash_restores_permissions_and_removes_new_files_and_directories() {
    let (_directory, root) = fixture();
    let lock = ArtifactLock::acquire(&root).unwrap();
    let mut working = working(&root);
    working
        .create(NEW_FILE, "# New recipe\n\nCandidate only.\n")
        .unwrap();
    let mut observer = DurableJournal::new(&working).unwrap();
    // Staging may create parents, but the observer captured their absence first.
    fs::create_dir_all(root.join("logic/solution")).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, PROBLEM);
    install_candidate(&working, NEW_FILE);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join(PROBLEM), fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(journal::pending_prepared(&root).unwrap());
    drop(observer);
    drop(lock);
    let _lock = ArtifactLock::acquire(&root).unwrap();
    journal::recover(&root).unwrap();
    assert_originals(&root);
    assert!(!root.join(NEW_FILE).exists());
    assert!(!root.join("logic/solution").exists());
    assert!(!journal::pending_prepared(&root).unwrap());
    journal::recover(&root).unwrap();
    assert_originals(&root);
}

#[test]
fn committed_transaction_keeps_candidates_and_deleted_files_absent() {
    let (_directory, root) = fixture();
    let lock = ArtifactLock::acquire(&root).unwrap();
    let mut working = working(&root);
    working
        .delete(CONCEPTS, "journal acceptance deletion")
        .unwrap();
    transaction::commit(&working).unwrap();
    assert!(!marker(&root, "prepared").exists());
    assert!(marker(&root, "committed").exists());
    assert!(!journal::pending_prepared(&root).unwrap());
    drop(lock);
    let _lock = ArtifactLock::acquire(&root).unwrap();
    journal::recover(&root).unwrap();
    assert_eq!(
        fs::read(root.join(PROBLEM)).unwrap(),
        working.files[PROBLEM]
    );
    assert!(!root.join(CONCEPTS).exists());
    assert!(!marker(&root, "committed").exists());
}

#[test]
fn prepared_deletion_is_rolled_back_to_the_exact_original_file() {
    let (_directory, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let mut working = working(&root);
    working
        .delete(CONCEPTS, "journal acceptance deletion")
        .unwrap();
    let mut observer = DurableJournal::new(&working).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, CONCEPTS);
    journal::recover(&root).unwrap();
    assert_originals(&root);
}

#[test]
fn zero_byte_preimages_remain_present_but_zero_byte_new_files_are_removed() {
    let (_directory, root) = fixture();
    fs::write(root.join(CONCEPTS), b"").unwrap();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(&root).unwrap());
    working
        .edit(
            CONCEPTS,
            0..0,
            "Candidate only.\n",
            "empty preimage mutation",
        )
        .unwrap();
    working.create(NEW_FILE, "").unwrap();
    let mut observer = DurableJournal::new(&working).unwrap();
    fs::create_dir_all(root.join("logic/solution")).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, CONCEPTS);
    install_candidate(&working, NEW_FILE);
    journal::recover(&root).unwrap();
    assert!(root.join(CONCEPTS).is_file());
    assert_eq!(fs::read(root.join(CONCEPTS)).unwrap(), b"");
    assert!(!root.join(NEW_FILE).exists());
    assert_eq!(fs::read(root.join(PROBLEM)).unwrap(), ORIGINAL_PROBLEM);
}

#[test]
fn external_digest_mismatch_blocks_all_recovery_mutations_and_preserves_evidence() {
    let (_directory, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, CONCEPTS);
    fs::write(root.join(PROBLEM), b"external editor bytes\n").unwrap();
    let evidence = fs::read(marker(&root, "prepared")).unwrap();
    let error = journal::recover(&root).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.code, "write.recovery_external_edit");
    assert_eq!(
        fs::read(root.join(CONCEPTS)).unwrap(),
        working.files[CONCEPTS]
    );
    assert_eq!(
        fs::read(root.join(PROBLEM)).unwrap(),
        b"external editor bytes\n"
    );
    assert_eq!(fs::read(marker(&root, "prepared")).unwrap(), evidence);
    assert!(journal::pending_prepared(&root).unwrap());
}

#[test]
fn committed_external_edit_is_not_silently_kept_or_rolled_back() {
    let (_directory, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    transaction::commit(&working).unwrap();
    fs::write(root.join(PROBLEM), ORIGINAL_PROBLEM).unwrap();
    let error = journal::recover(&root).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.code, "write.recovery_external_edit");
    assert_eq!(fs::read(root.join(PROBLEM)).unwrap(), ORIGINAL_PROBLEM);
    assert_eq!(
        fs::read(root.join(CONCEPTS)).unwrap(),
        working.files[CONCEPTS]
    );
    assert!(marker(&root, "committed").exists());
}

#[test]
fn corrupt_target_paths_never_escape_the_artifact_or_touch_reserved_bodies() {
    for path in [
        "../outside.txt",
        "logic/../outside.txt",
        "logic//problem.md",
        ".ara/lock",
        "src/environment.md",
        "evidence/body.md",
    ] {
        let (directory, root) = fixture();
        let outside = directory.path().join("outside.txt");
        fs::write(&outside, b"outside bytes\n").unwrap();
        let _lock = ArtifactLock::acquire(&root).unwrap();
        let working = working(&root);
        let mut observer = DurableJournal::new(&working).unwrap();
        prepare(&mut observer, &working);
        install_candidate(&working, PROBLEM);
        let mut record: Value =
            serde_json::from_slice(&fs::read(marker(&root, "prepared")).unwrap()).unwrap();
        record["entries"][0]["path"] = Value::String(path.into());
        fs::write(
            marker(&root, "prepared"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let error = journal::recover(&root).unwrap_err();
        assert_eq!(error.exit_code(), 2, "{path}");
        assert_eq!(error.code, "write.journal_corrupt", "{path}");
        assert_eq!(fs::read(&outside).unwrap(), b"outside bytes\n");
        assert_eq!(
            fs::read(root.join(PROBLEM)).unwrap(),
            working.files[PROBLEM]
        );
        assert!(marker(&root, "prepared").exists());
    }
}

#[test]
fn corrupt_preimage_is_detected_before_any_file_is_rolled_back() {
    let (_directory, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, PROBLEM);
    install_candidate(&working, CONCEPTS);
    fs::write(
        root.join(".ara/transactions/active.preimages/preimage.00000001.bin"),
        b"corrupt backup",
    )
    .unwrap();
    let error = journal::recover(&root).unwrap_err();
    assert_eq!(error.exit_code(), 2);
    assert_eq!(error.code, "write.journal_corrupt");
    assert_eq!(
        fs::read(root.join(PROBLEM)).unwrap(),
        working.files[PROBLEM]
    );
    assert_eq!(
        fs::read(root.join(CONCEPTS)).unwrap(),
        working.files[CONCEPTS]
    );
    assert!(journal::pending_prepared(&root).unwrap());
}

#[test]
fn simultaneous_state_markers_fail_closed_instead_of_choosing_one() {
    let (_directory, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    prepare(&mut observer, &working);
    fs::copy(marker(&root, "prepared"), marker(&root, "committed")).unwrap();
    assert_eq!(journal::pending_prepared(&root).unwrap_err().exit_code(), 2);
    assert_eq!(journal::recover(&root).unwrap_err().exit_code(), 2);
    assert_originals(&root);
    assert!(marker(&root, "prepared").exists());
    assert!(marker(&root, "committed").exists());
}

#[cfg(unix)]
#[test]
fn symlinked_journal_payload_and_target_ancestor_are_never_followed() {
    use std::os::unix::fs::symlink;
    for target_ancestor in [false, true] {
        let (directory, root) = fixture();
        let outside = directory.path().join("outside.txt");
        fs::write(&outside, b"outside bytes\n").unwrap();
        let _lock = ArtifactLock::acquire(&root).unwrap();
        let working = working(&root);
        let mut observer = DurableJournal::new(&working).unwrap();
        prepare(&mut observer, &working);
        if target_ancestor {
            fs::rename(root.join("logic"), root.join("real_logic")).unwrap();
            symlink(root.join("real_logic"), root.join("logic")).unwrap();
        } else {
            let backup = root.join(".ara/transactions/active.preimages/preimage.00000000.bin");
            fs::remove_file(&backup).unwrap();
            symlink(&outside, backup).unwrap();
        }
        let error = journal::recover(&root).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert_eq!(error.code, "write.journal_corrupt");
        assert_eq!(fs::read(&outside).unwrap(), b"outside bytes\n");
        assert!(marker(&root, "prepared").exists());
        if target_ancestor {
            assert_eq!(
                fs::read(root.join("real_logic/problem.md")).unwrap(),
                ORIGINAL_PROBLEM
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn recovery_restarts_after_a_preimage_rename_before_permission_restoration() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, CONCEPTS);
    // A recovery process may die after restoring bytes but before restoring mode.
    fs::write(root.join(PROBLEM), ORIGINAL_PROBLEM).unwrap();
    fs::set_permissions(root.join(PROBLEM), fs::Permissions::from_mode(0o600)).unwrap();
    let partial = root.join(".ara/transactions/active.preimages/restore.00000000.bin");
    fs::write(&partial, b"interrupted recovery temp").unwrap();
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o600)).unwrap();
    journal::recover(&root).unwrap();
    assert_originals(&root);
    assert!(!partial.exists());
    assert!(!journal::pending_prepared(&root).unwrap());
}

#[test]
fn prepared_unrenamed_candidates_are_authenticated_before_directory_cleanup() {
    for foreign in [false, true] {
        let (_owner, root) = fixture();
        let _lock = ArtifactLock::acquire(&root).unwrap();
        let mut working = working(&root);
        working
            .create(NEW_FILE, "# New\n\nUnrenamed staged source.\n")
            .unwrap();
        let mut observer = DurableJournal::new(&working).unwrap();
        prepare(&mut observer, &working);
        let ours = temporary(&working, NEW_FILE);
        let unrelated = root.join("logic/solution/.ara-write-900002-9");
        if foreign {
            fs::write(&unrelated, b"foreign writer bytes").unwrap();
        }
        let result = journal::recover(&root);
        assert_originals(&root);
        assert!(!root.join(NEW_FILE).exists());
        assert!(!ours.exists());
        if foreign {
            assert!(result.is_err());
            assert_eq!(fs::read(&unrelated).unwrap(), b"foreign writer bytes");
            assert!(journal::pending_prepared(&root).unwrap());
            fs::remove_file(&unrelated).unwrap();
            journal::recover(&root).unwrap();
        } else {
            result.unwrap();
        }
        assert!(!root.join("logic/solution").exists());
        assert!(!journal::pending_prepared(&root).unwrap());
    }
}

#[test]
fn foreign_replacement_of_a_staged_candidate_blocks_every_recovery_change() {
    let (_owner, root) = fixture();
    let _lock = ArtifactLock::acquire(&root).unwrap();
    let working = working(&root);
    let mut observer = DurableJournal::new(&working).unwrap();
    prepare(&mut observer, &working);
    install_candidate(&working, CONCEPTS);
    let candidate = temporary(&working, PROBLEM);
    fs::write(&candidate, b"unrelated content occupying recorded name").unwrap();
    let record = fs::read(marker(&root, "prepared")).unwrap();
    assert!(journal::recover(&root).is_err());
    assert_eq!(
        fs::read(root.join(CONCEPTS)).unwrap(),
        working.files[CONCEPTS]
    );
    assert_eq!(
        fs::read(candidate).unwrap(),
        b"unrelated content occupying recorded name"
    );
    assert_eq!(fs::read(marker(&root, "prepared")).unwrap(), record);
}
