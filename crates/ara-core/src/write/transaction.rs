//! Persistent-inode cooperative writer locking and same-filesystem synced
//! rename transactions. Ordinary failures restore exact preimages; durable
//! crash recovery is supplied by the transaction observer.
use super::{
    WorkingArtifact, WriteError,
    source::{allowed_write_path, digest, safe_relative},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct ArtifactLock {
    file: File,
}
impl ArtifactLock {
    pub fn acquire(root: &Path) -> Result<Self, WriteError> {
        reject_symlink(root)?;
        let mut created = Vec::new();
        create_parents(root, &mut created)?;
        for directory in created.iter().rev() {
            sync_directory(directory)?;
            if let Some(parent) = directory.parent() {
                sync_directory(parent)?;
            }
        }
        let operational = root.join(".ara");
        reject_symlink(&operational)?;
        fs::create_dir(&operational)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(e)
                }
            })
            .map_err(|e| io(&operational, e))?;
        let path = operational.join("lock");
        reject_symlink(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| io(&path, e))?;
        if !file.metadata().map_err(|e| io(&path, e))?.is_file() {
            return Err(WriteError::semantic(
                "write.path",
                ".ara/lock is not a regular file",
            ));
        }
        file.lock().map_err(|e| io(&path, e))?;
        Ok(Self { file })
    }
}
impl Drop for ArtifactLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Journal implementors keep preimages durable until completed succeeds.
/// The completion callback is after all destination and directory syncs. Its
/// failure is an uncertain completion, NOT an ordinary rollback opportunity.
pub trait TransactionObserver {
    fn staged(&mut self, _candidates: &[(String, PathBuf)]) -> Result<(), WriteError> {
        Ok(())
    }
    fn prepared(&mut self, working: &WorkingArtifact) -> Result<(), WriteError>;
    fn before_rename(&mut self, _path: &str) -> Result<(), WriteError> {
        Ok(())
    }
    fn after_rename(&mut self, _path: &str) -> Result<(), WriteError> {
        Ok(())
    }
    fn completed(&mut self) -> Result<(), WriteError>;
    fn rolled_back(&mut self) -> Result<(), WriteError>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionBoundary {
    TemporaryWrite,
    TemporarySync,
    BeforeRename,
    AfterRename,
    DirectorySync,
    RollbackWrite,
    RollbackSync,
    RollbackRename,
}
/// Deterministic fault/concurrency injection. Production uses the no-op default.
pub trait TransactionHooks {
    fn boundary(&mut self, _kind: TransactionBoundary, _path: &str) -> Result<(), WriteError> {
        Ok(())
    }
}
pub struct NoHooks;
impl TransactionHooks for NoHooks {}

pub fn commit(working: &WorkingArtifact) -> Result<(), WriteError> {
    let mut observer = super::journal::DurableJournal::new(working)?;
    commit_with(working, &mut observer)
}
pub fn commit_with<O: TransactionObserver>(
    working: &WorkingArtifact,
    observer: &mut O,
) -> Result<(), WriteError> {
    commit_with_hooks(working, observer, &mut NoHooks)
}
pub fn commit_with_hooks<O: TransactionObserver, H: TransactionHooks>(
    working: &WorkingArtifact,
    observer: &mut O,
    hooks: &mut H,
) -> Result<(), WriteError> {
    let paths = working.changed_paths();
    if paths.is_empty() && working.created_dirs.is_empty() {
        return Ok(());
    }
    for path in &paths {
        if !allowed_write_path(path) {
            return Err(WriteError::semantic(
                "write.path",
                format!("destination `{path}` lies outside knowledge write boundary"),
            ));
        }
        checked_destination(&working.base.root, path)?;
        if !matches!(
            path.as_str(),
            "PAPER.md" | ".gitignore" | "evidence/README.md" | "rubric/requirements.md"
        ) && !path.starts_with("logic/")
            && !path.starts_with("trace/")
            && !path.starts_with("staging/")
        {
            let current = working.is_allowed_document(path)?;
            let previous = working
                .base
                .files
                .get("PAPER.md")
                .filter(|f| f.existed)
                .map(|paper| std::str::from_utf8(&paper.bytes))
                .transpose()
                .map_err(|_| WriteError::semantic("write.encoding", "base PAPER is not UTF-8"))?
                .map(crate::knowledge_paths)
                .transpose()
                .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?
                .is_some_and(|paths| paths.contains(path));
            if !current && !previous {
                return Err(WriteError::semantic(
                    "write.path",
                    format!("destination `{path}` is not registered as mutable knowledge"),
                ));
            }
        }
        if !working.base.files.contains_key(path) {
            return Err(WriteError::semantic(
                "write.snapshot",
                format!("destination `{path}` has no captured preimage"),
            ));
        }
    }
    for directory in &working.created_dirs {
        safe_relative(directory)?;
        let ancestor = paths.iter().any(|path| {
            Path::new(path)
                .ancestors()
                .skip(1)
                .any(|parent| parent == Path::new(directory))
        });
        if !super::source::allowed_init_directory(directory) && !ancestor {
            return Err(WriteError::semantic(
                "write.path",
                format!(
                    "directory `{directory}` is outside approved initialization/target structure"
                ),
            ));
        }
    }
    recheck(working)?;
    let mut directories = Vec::new();
    let mut temps = Vec::new();
    let mut applied = Vec::new();
    let mut prepared = false;
    let result = (|| {
        for directory in &working.created_dirs {
            safe_relative(directory)?;
            create_parents(&working.base.root.join(directory), &mut directories)?;
        }
        for path in &paths {
            let destination = checked_destination(&working.base.root, path)?;
            if let Some(parent) = destination.parent() {
                create_parents(parent, &mut directories)?;
            }
            if let Some(bytes) = working.files.get(path) {
                hooks.boundary(TransactionBoundary::TemporaryWrite, path)?;
                let temp = temporary_path(&destination);
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)
                    .map_err(|e| io(&temp, e))?;
                temps.push((path.clone(), temp.clone()));
                file.write_all(bytes).map_err(|e| io(&temp, e))?;
                if let Some(permissions) = &working.base.files[path].permissions {
                    file.set_permissions(permissions.clone())
                        .map_err(|e| io(&temp, e))?;
                }
                hooks.boundary(TransactionBoundary::TemporarySync, path)?;
                file.sync_all().map_err(|e| io(&temp, e))?;
            }
        }
        // Recheck immediately before publishing the prepared journal and the
        // first destination rename; staging has not touched any source bytes.
        recheck(working)?;
        observer.staged(&temps)?;
        prepared = true;
        observer.prepared(working)?;
        for path in &paths {
            let destination = checked_destination(&working.base.root, path)?;
            hooks.boundary(TransactionBoundary::BeforeRename, path)?;
            observer.before_rename(path)?;
            if working.deleted_paths.contains(path) {
                fs::remove_file(&destination).map_err(|e| io(&destination, e))?;
            } else {
                let (_, temp) = temps
                    .iter()
                    .find(|(candidate, _)| candidate == path)
                    .ok_or_else(|| {
                        WriteError::io(format!("missing synced candidate for `{path}`"))
                    })?;
                fs::rename(temp, &destination).map_err(|e| io(&destination, e))?;
            }
            applied.push(path.clone());
            observer.after_rename(path)?;
            hooks.boundary(TransactionBoundary::AfterRename, path)?;
        }
        for directory in affected_directories(working, &paths, &directories) {
            hooks.boundary(
                TransactionBoundary::DirectorySync,
                &directory.to_string_lossy(),
            )?;
            sync_directory(&directory)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for path in applied.iter().rev() {
            if let Err(error) = restore(working, path, hooks) {
                failures.push(format!(
                    "{path}: {error}; original={}, intended={}",
                    working.base.files[path].digest,
                    working
                        .files
                        .get(path)
                        .map_or_else(|| "absent".into(), |bytes| digest(bytes))
                ));
            }
        }
        cleanup_temps(&temps, &mut failures);
        for directory in directories.iter().rev() {
            if let Err(error) = fs::remove_dir(directory)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                failures.push(format!("{}: {error}", directory.display()));
            }
        }
        if failures.is_empty()
            && prepared
            && let Err(error) = observer.rolled_back()
        {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            return Err(error);
        }
        return Err(WriteError::io(format!(
            "transaction failed ({error}); rollback incomplete: {}",
            failures.join("; ")
        )));
    }
    let mut failures = Vec::new();
    cleanup_temps(&temps, &mut failures);
    if !failures.is_empty() {
        return Err(WriteError::io(format!(
            "source changes applied but temporary cleanup failed: {}",
            failures.join("; ")
        )));
    }
    observer.completed().map_err(|error|WriteError::io(format!("source renames and syncs completed; durable completion uncertain: {error}; recovery evidence retained")))
}

/// Verify existence and digests of ALL loaded sources, not only destinations.
/// Direct editors can still race after this check; only cooperating writers are
/// serialized by ArtifactLock.
pub fn recheck(working: &WorkingArtifact) -> Result<(), WriteError> {
    for (path, preimage) in &working.base.files {
        let destination = checked_destination(&working.base.root, path)?;
        match fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                if !preimage.existed || !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(stale(path));
                }
                #[cfg(unix)]
                if let Some(original) = &preimage.permissions {
                    use std::os::unix::fs::PermissionsExt;
                    if original.mode() != metadata.permissions().mode() {
                        return Err(stale(path));
                    }
                }
                let bytes = fs::read(&destination).map_err(|e| io(&destination, e))?;
                if digest(&bytes) != preimage.digest {
                    return Err(stale(path));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if preimage.existed {
                    return Err(stale(path));
                }
            }
            Err(error) => return Err(io(&destination, error)),
        }
    }
    Ok(())
}
fn restore<H: TransactionHooks>(
    working: &WorkingArtifact,
    path: &str,
    hooks: &mut H,
) -> Result<(), WriteError> {
    let preimage = &working.base.files[path];
    let destination = checked_destination(&working.base.root, path)?;
    if !preimage.existed {
        match fs::remove_file(&destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(&destination, error)),
        };
        if let Some(parent) = destination.parent() {
            sync_directory(parent)?;
        }
        return Ok(());
    }
    hooks.boundary(TransactionBoundary::RollbackWrite, path)?;
    let temp = temporary_path(&destination);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| io(&temp, e))?;
        file.write_all(&preimage.bytes).map_err(|e| io(&temp, e))?;
        if let Some(permissions) = &preimage.permissions {
            file.set_permissions(permissions.clone())
                .map_err(|e| io(&temp, e))?;
        }
        hooks.boundary(TransactionBoundary::RollbackSync, path)?;
        file.sync_all().map_err(|e| io(&temp, e))?;
        hooks.boundary(TransactionBoundary::RollbackRename, path)?;
        fs::rename(&temp, &destination).map_err(|e| io(&destination, e))?;
        if let Some(parent) = destination.parent() {
            sync_directory(parent)?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}
fn cleanup_temps(temps: &[(String, PathBuf)], failures: &mut Vec<String>) {
    for (_, path) in temps {
        if let Err(error) = fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
}
fn affected_directories(
    working: &WorkingArtifact,
    paths: &[String],
    created: &[PathBuf],
) -> BTreeSet<PathBuf> {
    let mut result: BTreeSet<PathBuf> = paths
        .iter()
        .filter_map(|path| working.base.root.join(path).parent().map(Path::to_path_buf))
        .collect();
    for directory in created {
        result.insert(directory.clone());
        if let Some(parent) = directory.parent() {
            result.insert(parent.to_path_buf());
        }
    }
    result
}
pub fn sync_directory(path: &Path) -> Result<(), WriteError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| io(path, e))
}
pub fn checked_destination(root: &Path, path: &str) -> Result<PathBuf, WriteError> {
    safe_relative(path)?;
    reject_symlink(root)?;
    let mut destination = root.to_path_buf();
    for component in Path::new(path).components() {
        destination.push(component);
        reject_symlink(&destination)?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&destination)
        && !metadata.is_file()
    {
        return Err(WriteError::semantic(
            "write.path",
            format!("destination `{path}` is not a regular file"),
        ));
    }
    Ok(destination)
}
fn reject_symlink(path: &Path) -> Result<(), WriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(WriteError::semantic(
            "write.path",
            format!("symlink `{}` is not a safe write path", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(path, error)),
    }
}
fn create_parents(path: &Path, created: &mut Vec<PathBuf>) -> Result<(), WriteError> {
    reject_symlink(path)?;
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        return Err(WriteError::semantic(
            "write.path",
            format!("{} is not a directory", path.display()),
        ));
    }
    if let Some(parent) = path.parent() {
        create_parents(parent, created)?;
    }
    fs::create_dir(path).map_err(|e| io(path, e))?;
    created.push(path.to_path_buf());
    Ok(())
}
fn temporary_path(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(
        ".ara-write-{}-{}",
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    ))
}
fn stale(path: &str) -> WriteError {
    WriteError::semantic(
        "write.concurrent_edit",
        format!("source `{path}` changed outside the writer lock; no source changes committed"),
    )
}
fn io(path: &Path, error: std::io::Error) -> WriteError {
    WriteError::io(format!("{}: {error}", path.display()))
}
