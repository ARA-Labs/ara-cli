//! Private crash-recovery records for the guarded writer.
//!
//! Commit-mode writers call `recover` under `ArtifactLock` before snapshots.
//! Read-only dry runs and readers only probe through `pending_prepared`, and
//! must reject a pending transaction without reading its mixed source state.
//! The prepared-to-committed rename is the commit point: a completed-hook error
//! must not trigger an in-process rollback, since that rename may have succeeded.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions, Permissions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{WorkingArtifact, WriteError, source, transaction::TransactionObserver};

const PREPARED: &str = "active.json.prepared";
const COMMITTED: &str = "active.json.committed";
const TEMP: &str = "active.json.tmp";
const PREIMAGES: &str = "active.preimages";
const FORMAT: &str = "ara.transaction/v1";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    entries: Vec<Entry>,
    created_dirs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    existed: bool,
    preimage_digest: Option<String>,
    permissions: Option<SavedPermissions>,
    candidate_existed: bool,
    candidate_digest: Option<String>,
    candidate_temporary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedPermissions {
    readonly: bool,
    #[cfg(unix)]
    mode: u32,
}

impl SavedPermissions {
    fn capture(permissions: &Permissions) -> Self {
        Self {
            readonly: permissions.readonly(),
            #[cfg(unix)]
            mode: {
                use std::os::unix::fs::PermissionsExt;
                permissions.mode() & 0o7777
            },
        }
    }

    fn restore(&self, file: &File) -> Result<(), WriteError> {
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            Permissions::from_mode(self.mode)
        };
        #[cfg(not(unix))]
        let permissions = {
            let mut permissions = file.metadata().map_err(io_error)?.permissions();
            permissions.set_readonly(self.readonly);
            permissions
        };
        file.set_permissions(permissions).map_err(io_error)
    }

    fn valid(&self) -> bool {
        #[cfg(unix)]
        {
            self.mode & !0o7777 == 0 && self.readonly == (self.mode & 0o222 == 0)
        }
        #[cfg(not(unix))]
        {
            true
        }
    }
}

/// Transaction observer used by every ordinary guarded commit.
/// Construction precedes staging so newly provisioned directories are recorded.
pub struct DurableJournal {
    root: PathBuf,
    paths: Vec<String>,
    created_dirs: Vec<String>,
    record: Option<Record>,
    candidates: BTreeMap<String, String>,
}

impl DurableJournal {
    pub fn new(working: &WorkingArtifact) -> Result<Self, WriteError> {
        require_directory(&working.base.root, false)?;
        let paths = working.changed_paths();
        let mut created_dirs = BTreeSet::new();
        let mut target_ancestors = BTreeSet::new();
        for path in &paths {
            validate_target(path)?;
            target_ancestors.extend(
                Path::new(path)
                    .ancestors()
                    .skip(1)
                    .filter(|path| !path.as_os_str().is_empty()),
            );
            if let Some(directory) = Path::new(path).parent() {
                capture_missing_directories(&working.base.root, directory, &mut created_dirs)?;
            }
        }
        for directory in &working.created_dirs {
            validate_relative(directory)?;
            if !source::allowed_init_directory(directory)
                && !target_ancestors.contains(Path::new(directory))
            {
                return Err(corrupt(format!(
                    "unapproved initialization directory {directory}"
                )));
            }
            capture_missing_directories(
                &working.base.root,
                Path::new(directory),
                &mut created_dirs,
            )?;
        }
        Ok(Self {
            root: working.base.root.clone(),
            paths,
            created_dirs: created_dirs.into_iter().collect(),
            record: None,
            candidates: BTreeMap::new(),
        })
    }
}

impl TransactionObserver for DurableJournal {
    fn staged(&mut self, candidates: &[(String, PathBuf)]) -> Result<(), WriteError> {
        if self.record.is_some() || !self.candidates.is_empty() {
            return Err(corrupt("candidate staging was already recorded"));
        }
        for (target, absolute) in candidates {
            let relative = absolute
                .strip_prefix(&self.root)
                .ok()
                .and_then(Path::to_str)
                .ok_or_else(|| corrupt("candidate temporary lies outside artifact"))?;
            validate_candidate_temporary(target, relative)?;
            if !self.paths.contains(target)
                || self
                    .candidates
                    .insert(target.clone(), relative.into())
                    .is_some()
            {
                return Err(corrupt(
                    "candidate temporary has no unique transaction target",
                ));
            }
        }
        Ok(())
    }
    fn prepared(&mut self, working: &WorkingArtifact) -> Result<(), WriteError> {
        if self.record.is_some()
            || (self.paths.is_empty() && self.created_dirs.is_empty())
            || self.root != working.base.root
            || self.paths != working.changed_paths()
        {
            return Err(corrupt(
                "journal observer does not match the staged transaction",
            ));
        }
        let directory = provision_namespace(&self.root)?;
        let namespace = list_namespace(&directory)?;
        if namespace.prepared || namespace.committed || namespace.temporary || namespace.preimages {
            return Err(corrupt(
                "a prior transaction must be recovered before preparation",
            ));
        }
        let preimages = directory.join(PREIMAGES);
        create_private_directory(&preimages)?;
        let mut entries = Vec::with_capacity(self.paths.len());
        for (index, path) in self.paths.iter().enumerate() {
            let original = working.base.files.get(path);
            let existed = original.is_some_and(|file| file.existed);
            let preimage_digest = original
                .filter(|file| file.existed)
                .map(|file| file.digest.clone());
            let current = current_digest(&self.root, path)?;
            if current != preimage_digest {
                return Err(external(
                    path,
                    "preimage changed before journal preparation",
                ));
            }
            let permissions = if existed {
                Some(SavedPermissions::capture(
                    original
                        .and_then(|file| file.permissions.as_ref())
                        .ok_or_else(|| {
                            corrupt(format!("missing preimage permissions for {path}"))
                        })?,
                ))
            } else {
                None
            };
            if let Some(original) = original.filter(|file| file.existed) {
                if source::digest(&original.bytes) != original.digest {
                    return Err(corrupt(format!(
                        "inconsistent captured preimage for {path}"
                    )));
                }
                let mut backup = create_private_file(&preimages.join(backup_name(index)))?;
                backup.write_all(&original.bytes).map_err(io_error)?;
                backup.sync_all().map_err(io_error)?;
            }
            let candidate = if working.deleted_paths.contains(path) {
                None
            } else {
                Some(
                    working
                        .files
                        .get(path)
                        .ok_or_else(|| corrupt(format!("missing candidate bytes for {path}")))?,
                )
            };
            let candidate_temporary = self.candidates.get(path).cloned();
            if candidate.is_some() != candidate_temporary.is_some() {
                return Err(corrupt(format!(
                    "missing or unexpected staged candidate temporary for {path}"
                )));
            }
            if let (Some(candidate), Some(temporary)) = (candidate, &candidate_temporary) {
                validate_ancestors(&self.root, temporary)?;
                if file_digest(&self.root.join(temporary), false)? != source::digest(candidate) {
                    return Err(corrupt(format!(
                        "staged candidate digest differs for {path}"
                    )));
                }
            }
            entries.push(Entry {
                path: path.clone(),
                existed,
                preimage_digest,
                permissions,
                candidate_existed: candidate.is_some(),
                candidate_digest: candidate.map(|bytes| source::digest(bytes)),
                candidate_temporary,
            });
        }
        let record = Record {
            format: FORMAT.into(),
            entries,
            created_dirs: self.created_dirs.clone(),
        };
        validate_record(&record)?;
        sync_directory(&preimages)?;
        let bytes = serde_json::to_vec(&record).map_err(|error| corrupt(error.to_string()))?;
        let mut manifest = create_private_file(&directory.join(TEMP))?;
        manifest.write_all(&bytes).map_err(io_error)?;
        manifest.sync_all().map_err(io_error)?;
        fs::rename(directory.join(TEMP), directory.join(PREPARED)).map_err(io_error)?;
        sync_directory(&directory)?;
        self.record = Some(record);
        Ok(())
    }

    fn completed(&mut self) -> Result<(), WriteError> {
        let record = self
            .record
            .as_ref()
            .ok_or_else(|| corrupt("journal was not prepared"))?;
        inspect_targets(&self.root, record, true)?;
        let directory = transaction_directory(&self.root)?
            .ok_or_else(|| corrupt("prepared journal disappeared"))?;
        let namespace = list_namespace(&directory)?;
        if !namespace.prepared || namespace.committed || namespace.temporary {
            return Err(corrupt("prepared journal marker changed during commit"));
        }
        fs::rename(directory.join(PREPARED), directory.join(COMMITTED)).map_err(io_error)?;
        sync_directory(&directory)?;
        self.record = None;
        Ok(())
    }

    fn rolled_back(&mut self) -> Result<(), WriteError> {
        // Recovery verifies every preimage/candidate before changing anything.
        // A failed rollback must never call this hook or remove its evidence.
        recover(&self.root)?;
        self.record = None;
        Ok(())
    }
}

/// Detect an interrupted writer with one transactions-directory listing.
/// No lock or marker-content reads are taken by this read-side probe.
pub fn pending_prepared(root: &Path) -> Result<bool, WriteError> {
    let Some(directory) = transaction_directory(root)? else {
        return Ok(false);
    };
    Ok(list_namespace(&directory)?.prepared)
}

/// Recover a transaction under the caller's exclusive `ArtifactLock`.
/// All evidence and current digests are checked before any recovery mutation.
pub fn recover(root: &Path) -> Result<(), WriteError> {
    let Some(directory) = transaction_directory(root)? else {
        return Ok(());
    };
    let namespace = list_namespace(&directory)?;
    let marker = if namespace.prepared {
        Some(PREPARED)
    } else if namespace.committed {
        Some(COMMITTED)
    } else {
        None
    };
    let Some(marker) = marker else {
        // Preparation cannot rename a destination until the marker is durable.
        // Only fixed private filenames from that interrupted preparation exist.
        clean_payloads(&directory, namespace)?;
        return Ok(());
    };
    if namespace.temporary || !namespace.preimages {
        return Err(corrupt(
            "transaction marker has incomplete or ambiguous payloads",
        ));
    }
    let mut bytes = Vec::new();
    open_regular(&directory.join(marker), true)?
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|error| corrupt(format!("unreadable transaction manifest: {error}")))?;
    validate_record(&record)?;
    validate_payloads(&directory, &record)?;
    let current = inspect_targets(root, &record, marker == COMMITTED)?;
    let temporaries = inspect_candidate_temporaries(root, &record, marker == COMMITTED)?;
    if marker == PREPARED {
        for temporary in temporaries {
            fs::remove_file(root.join(&temporary)).map_err(io_error)?;
            sync_directory(root.join(&temporary).parent().expect("candidate parent"))?;
        }
        for (index, (entry, current)) in record.entries.iter().zip(current).enumerate() {
            if entry.existed {
                if current == entry.preimage_digest {
                    let file = open_regular(&root.join(&entry.path), false)?;
                    entry
                        .permissions
                        .as_ref()
                        .expect("validated permissions")
                        .restore(&file)?;
                    file.sync_all().map_err(io_error)?;
                } else {
                    restore_preimage(root, &directory, index, entry)?;
                }
            } else if current.is_some() {
                fs::remove_file(root.join(&entry.path)).map_err(io_error)?;
            }
            if let Some(parent) = root.join(&entry.path).parent()
                && metadata(parent)?.is_some()
            {
                sync_directory(parent)?;
            }
        }
        for relative in record.created_dirs.iter().rev() {
            let path = root.join(relative);
            if metadata(&path)?.is_none() {
                // A previous rollback may have removed this directory without
                // syncing its parent before crashing. Persist that absence too.
                let mut parent = path.parent();
                while let Some(directory) = parent {
                    if metadata(directory)?.is_some() {
                        sync_directory(directory)?;
                        break;
                    }
                    parent = directory.parent();
                }
                continue;
            }
            require_directory(&path, false)?;
            fs::remove_dir(&path).map_err(|error| {
                recovery(format!(
                    "cannot remove transaction-created directory {relative}: {error}"
                ))
            })?;
            sync_directory(path.parent().expect("relative directory has parent"))?;
        }
    }
    // Persist the coherent state before retiring its marker. Payload cleanup can
    // be resumed independently after a crash, once no prepared marker remains.
    fs::remove_file(directory.join(marker)).map_err(io_error)?;
    sync_directory(&directory)?;
    clean_payloads(&directory, namespace)?;
    Ok(())
}

#[derive(Default)]
struct Namespace {
    prepared: bool,
    committed: bool,
    temporary: bool,
    preimages: bool,
}

fn list_namespace(directory: &Path) -> Result<Namespace, WriteError> {
    let mut namespace = Namespace::default();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let kind = entry.file_type().map_err(io_error)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(corrupt("non-UTF-8 entry in private transaction namespace"));
        };
        match name {
            PREPARED | COMMITTED | TEMP if kind.is_file() && !kind.is_symlink() => match name {
                PREPARED => namespace.prepared = true,
                COMMITTED => namespace.committed = true,
                _ => namespace.temporary = true,
            },
            PREIMAGES if kind.is_dir() && !kind.is_symlink() => namespace.preimages = true,
            _ => {
                return Err(corrupt(format!(
                    "unexpected or unsafe journal entry {name}"
                )));
            }
        }
    }
    if namespace.prepared && namespace.committed {
        return Err(corrupt("both prepared and committed markers are present"));
    }
    Ok(namespace)
}

fn validate_record(record: &Record) -> Result<(), WriteError> {
    if record.format != FORMAT || (record.entries.is_empty() && record.created_dirs.is_empty()) {
        return Err(corrupt("unsupported or empty transaction manifest"));
    }
    let mut prior: Option<&str> = None;
    let mut ancestors = BTreeSet::new();
    for entry in &record.entries {
        validate_recorded_target(&entry.path)?;
        if prior.is_some_and(|path| path >= entry.path.as_str()) {
            return Err(corrupt(
                "transaction targets are duplicated or out of order",
            ));
        }
        prior = Some(&entry.path);
        let mut parent = Path::new(&entry.path).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            ancestors.insert(path);
            parent = path.parent();
        }
        if entry.existed != entry.preimage_digest.is_some()
            || entry.existed != entry.permissions.is_some()
            || entry.candidate_existed != entry.candidate_digest.is_some()
            || entry.candidate_existed != entry.candidate_temporary.is_some()
            || entry
                .preimage_digest
                .iter()
                .chain(entry.candidate_digest.iter())
                .any(|digest| !valid_digest(digest))
            || entry
                .permissions
                .as_ref()
                .is_some_and(|permissions| !permissions.valid())
        {
            return Err(corrupt(format!(
                "inconsistent transaction metadata for {}",
                entry.path
            )));
        }
        if let Some(temporary) = &entry.candidate_temporary {
            validate_candidate_temporary(&entry.path, temporary)?;
        }
    }
    let mut prior: Option<&str> = None;
    for directory in &record.created_dirs {
        validate_relative(directory)?;
        if prior.is_some_and(|path| path >= directory.as_str())
            || (!ancestors.contains(Path::new(directory))
                && !source::allowed_init_directory(directory))
        {
            return Err(corrupt("invalid transaction-created directory"));
        }
        prior = Some(directory);
    }
    Ok(())
}

fn validate_payloads(directory: &Path, record: &Record) -> Result<(), WriteError> {
    let preimages = directory.join(PREIMAGES);
    let payloads = list_payloads(&preimages)?;
    for (index, entry) in record.entries.iter().enumerate() {
        let name = backup_name(index);
        if entry.existed {
            if !payloads.contains(&name)
                || file_digest(&preimages.join(&name), true)?
                    != *entry.preimage_digest.as_ref().expect("validated digest")
            {
                return Err(corrupt(format!(
                    "missing or corrupt preimage for {}",
                    entry.path
                )));
            }
        } else if payloads.contains(&name) {
            return Err(corrupt(format!(
                "unexpected preimage for new file {}",
                entry.path
            )));
        }
    }
    for name in payloads {
        let index = payload_index(&name).ok_or_else(|| corrupt("invalid payload name"))?;
        if !record.entries.get(index).is_some_and(|entry| entry.existed) {
            return Err(corrupt("payload does not belong to a transaction preimage"));
        }
    }
    Ok(())
}

fn inspect_targets(
    root: &Path,
    record: &Record,
    committed: bool,
) -> Result<Vec<Option<String>>, WriteError> {
    let mut current = if committed {
        Vec::new()
    } else {
        Vec::with_capacity(record.entries.len())
    };
    for entry in &record.entries {
        let digest = current_digest(root, &entry.path)?;
        let valid = if committed {
            digest == entry.candidate_digest
        } else {
            digest == entry.preimage_digest || digest == entry.candidate_digest
        };
        if !valid {
            return Err(external(
                &entry.path,
                "current bytes match neither the recoverable preimage nor the expected committed candidate",
            ));
        }
        if !committed {
            current.push(digest);
        }
    }
    for directory in &record.created_dirs {
        validate_ancestors(root, directory)?;
        let absolute = root.join(directory);
        if metadata(&absolute)?.is_some() {
            require_directory(&absolute, false)?;
        } else if committed {
            return Err(external(
                directory,
                "committed initialization directory was removed",
            ));
        }
    }
    Ok(current)
}
fn validate_candidate_temporary(target: &str, temporary: &str) -> Result<(), WriteError> {
    validate_relative(temporary)?;
    if Path::new(target).parent() != Path::new(temporary).parent()
        || !source::is_temporary_path(temporary)
    {
        return Err(corrupt(
            "candidate temporary is not an exact same-directory transaction name",
        ));
    }
    Ok(())
}
fn inspect_candidate_temporaries(
    root: &Path,
    record: &Record,
    committed: bool,
) -> Result<Vec<String>, WriteError> {
    let mut present = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in &record.entries {
        let Some(temporary) = &entry.candidate_temporary else {
            continue;
        };
        if !seen.insert(temporary) {
            return Err(corrupt("candidate temporary belongs to multiple targets"));
        }
        validate_ancestors(root, temporary)?;
        let absolute = root.join(temporary);
        if metadata(&absolute)?.is_none() {
            continue;
        }
        if committed
            || file_digest(&absolute, false)?
                != *entry
                    .candidate_digest
                    .as_ref()
                    .expect("validated candidate digest")
        {
            return Err(external(
                temporary,
                "candidate temporary is not the authenticated staged content",
            ));
        }
        present.push(temporary.clone());
    }
    Ok(present)
}

fn restore_preimage(
    root: &Path,
    directory: &Path,
    index: usize,
    entry: &Entry,
) -> Result<(), WriteError> {
    let preimages = directory.join(PREIMAGES);
    let temporary = preimages.join(restore_name(index));
    if metadata(&temporary)?.is_some() {
        open_regular(&temporary, true)?;
        fs::remove_file(&temporary).map_err(io_error)?;
    }
    let mut original = open_regular(&preimages.join(backup_name(index)), true)?;
    let mut restored = create_private_file(&temporary)?;
    io::copy(&mut original, &mut restored).map_err(io_error)?;
    restored.sync_all().map_err(io_error)?;
    validate_ancestors(root, &entry.path)?;
    fs::rename(&temporary, root.join(&entry.path)).map_err(|error| {
        recovery(format!(
            "cannot restore {}: {error}; prepared evidence retained",
            entry.path
        ))
    })?;
    entry
        .permissions
        .as_ref()
        .expect("validated permissions")
        .restore(&restored)?;
    restored.sync_all().map_err(io_error)?;
    Ok(())
}

fn clean_payloads(directory: &Path, namespace: Namespace) -> Result<(), WriteError> {
    if namespace.temporary {
        open_regular(&directory.join(TEMP), true)?;
        fs::remove_file(directory.join(TEMP)).map_err(io_error)?;
    }
    if namespace.preimages {
        let preimages = directory.join(PREIMAGES);
        for name in list_payloads(&preimages)? {
            fs::remove_file(preimages.join(name)).map_err(io_error)?;
        }
        fs::remove_dir(preimages).map_err(io_error)?;
    }
    sync_directory(directory)
}

fn list_payloads(directory: &Path) -> Result<BTreeSet<String>, WriteError> {
    require_directory(directory, true)?;
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| corrupt("non-UTF-8 preimage name"))?;
        if payload_index(&name).is_none() {
            return Err(corrupt(format!("unexpected journal payload {name}")));
        }
        open_regular(&entry.path(), true)?;
        names.insert(name);
    }
    Ok(names)
}

fn backup_name(index: usize) -> String {
    format!("preimage.{index:08}.bin")
}

fn restore_name(index: usize) -> String {
    format!("restore.{index:08}.bin")
}

fn payload_index(name: &str) -> Option<usize> {
    let number = name
        .strip_prefix("preimage.")
        .or_else(|| name.strip_prefix("restore."))?
        .strip_suffix(".bin")?;
    if number.len() != 8 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

fn valid_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn validate_target(path: &str) -> Result<(), WriteError> {
    validate_relative(path)?;
    if !source::allowed_write_path(path) {
        return Err(corrupt(format!(
            "journal target {path} is outside the writer allowlist"
        )));
    }
    Ok(())
}

/// Recorded targets also include `rubric/` documents, which earlier binaries
/// wrote natively. Recovery only verifies a committed journal, or rolls a
/// prepared one back to its authenticated preimages, so accepting them never
/// writes new rubric content. New journals still use `validate_target`.
fn validate_recorded_target(path: &str) -> Result<(), WriteError> {
    if path.starts_with("rubric/")
        && path.ends_with(".md")
        && !path.split('/').any(|part| part.starts_with('.'))
    {
        return validate_relative(path);
    }
    validate_target(path)
}

fn validate_relative(path: &str) -> Result<(), WriteError> {
    source::safe_relative(path).map_err(|_| corrupt(format!("unsafe journal path {path}")))?;
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
        || path.chars().any(char::is_control)
    {
        return Err(corrupt(format!("noncanonical journal path {path}")));
    }
    Ok(())
}

fn current_digest(root: &Path, relative: &str) -> Result<Option<String>, WriteError> {
    validate_recorded_target(relative)?;
    validate_ancestors(root, relative)?;
    let path = root.join(relative);
    if metadata(&path)?.is_none() {
        return Ok(None);
    }
    file_digest(&path, false).map(Some)
}

fn validate_ancestors(root: &Path, relative: &str) -> Result<(), WriteError> {
    require_directory(root, false)?;
    let mut path = root.to_path_buf();
    let mut components = Path::new(relative).components().peekable();
    while let Some(component) = components.next() {
        if components.peek().is_none() {
            break;
        }
        path.push(component);
        if metadata(&path)?.is_none() {
            return Ok(());
        }
        require_directory(&path, false)?;
    }
    Ok(())
}

fn capture_missing_directories(
    root: &Path,
    directory: &Path,
    missing: &mut BTreeSet<String>,
) -> Result<(), WriteError> {
    let mut directory = Some(directory);
    while let Some(relative) = directory.filter(|path| !path.as_os_str().is_empty()) {
        let absolute = root.join(relative);
        match metadata(&absolute)? {
            Some(_) => require_directory(&absolute, false)?,
            None => {
                missing.insert(relative.to_string_lossy().into_owned());
            }
        }
        directory = relative.parent();
    }
    Ok(())
}

fn file_digest(path: &Path, private: bool) -> Result<String, WriteError> {
    let mut file = open_regular(path, private)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn open_regular(path: &Path, private: bool) -> Result<File, WriteError> {
    let before = metadata(path)?
        .ok_or_else(|| corrupt(format!("journal file {} is missing", path.display())))?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(corrupt(format!(
            "{} is not a regular non-symlink file",
            path.display()
        )));
    }
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if before.nlink() != 1 || before.permissions().mode() & 0o077 != 0 {
            return Err(corrupt(
                "journal payload is not a private, singly linked file",
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = private;
    let file = File::open(path).map_err(io_error)?;
    let opened = file.metadata().map_err(io_error)?;
    if !opened.is_file() {
        return Err(corrupt("journal file changed while being opened"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Err(corrupt("journal file changed while being opened"));
        }
    }
    Ok(file)
}

fn transaction_directory(root: &Path) -> Result<Option<PathBuf>, WriteError> {
    if metadata(root)?.is_none() {
        return Ok(None);
    }
    require_directory(root, false)?;
    let private = root.join(".ara");
    if metadata(&private)?.is_none() {
        return Ok(None);
    }
    require_directory(&private, false)?;
    let directory = private.join("transactions");
    if metadata(&directory)?.is_none() {
        return Ok(None);
    }
    require_directory(&directory, true)?;
    Ok(Some(directory))
}

fn provision_namespace(root: &Path) -> Result<PathBuf, WriteError> {
    let private = root.join(".ara");
    if metadata(&private)?.is_none() {
        create_private_directory(&private)?;
    }
    require_directory(&private, false)?;
    // The lock adapter may have just provisioned .ara. Its own directory entry
    // must be durable, even when it existed by the time preparation began.
    sync_directory(root)?;
    let directory = private.join("transactions");
    if metadata(&directory)?.is_none() {
        create_private_directory(&directory)?;
        sync_directory(&private)?;
    }
    require_directory(&directory, true)?;
    Ok(directory)
}

fn require_directory(path: &Path, private: bool) -> Result<(), WriteError> {
    let metadata = metadata(path)?
        .ok_or_else(|| corrupt(format!("directory {} is missing", path.display())))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(corrupt(format!("directory {} is unsafe", path.display())));
    }
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(corrupt("transaction directory must be private"));
        }
    }
    #[cfg(not(unix))]
    let _ = private;
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), WriteError> {
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = builder;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(io_error)
}

fn create_private_file(path: &Path) -> Result<File, WriteError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(io_error)
}

fn sync_directory(path: &Path) -> Result<(), WriteError> {
    require_directory(path, false)?;
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>, WriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: io::Error) -> WriteError {
    WriteError::io(format!("durable journal: {error}"))
}

fn corrupt(message: impl Into<String>) -> WriteError {
    let mut error = WriteError::io(message);
    error.code = "write.journal_corrupt".into();
    error
}

fn external(path: &str, message: &str) -> WriteError {
    let mut error = WriteError::io(format!(
        "{path}: {message}; transaction evidence retained for manual repair"
    ));
    error.code = "write.recovery_external_edit".into();
    error.field = Some(path.into());
    error
}

fn recovery(message: impl Into<String>) -> WriteError {
    let mut error = WriteError::io(message);
    error.code = "write.recovery_failed".into();
    error
}
