//! Exact checkout preimages and staged edits. Existing documents are never
//! normalized and reserialized. Only changed documents are allocated as candidates.
pub use super::positions::{PathPart, YamlDocument, YamlKind, YamlNode};
use super::positions::{eol, field_range, line_start};
use super::{
    WriteError,
    intent::{Intent, YamlDelta, validate_bytes, validate_yaml},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
pub const INIT_DIRECTORIES: &[&str] = &[
    "logic",
    "logic/solution",
    "trace",
    "trace/sessions",
    "staging",
    "src",
    "evidence",
    "evidence/tables",
    "evidence/figures",
];
pub fn allowed_init_directory(path: &str) -> bool {
    INIT_DIRECTORIES.contains(&path)
}
const CANONICAL_SOURCES: &[&str] = &[
    "PAPER.md",
    ".gitignore",
    "trace/exploration_tree.yaml",
    "logic/claims.md",
    "logic/problem.md",
    "logic/concepts.md",
    "logic/experiments.md",
    "logic/related_work.md",
    "logic/solution/heuristics.md",
    "logic/solution/constraints.md",
    "logic/solution/architecture.md",
    "staging/observations.yaml",
    "trace/sessions/session_index.yaml",
    "trace/pm_reasoning_log.yaml",
    "trace/taste_log.yaml",
    "trace/logic_mutations.yaml",
    "trace/aliases.yaml",
    "trace/merge_log.yaml",
    "rubric/requirements.md",
];

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub bytes: Vec<u8>,
    pub existed: bool,
    pub permissions: Option<fs::Permissions>,
    pub digest: String,
}
#[derive(Debug, Clone)]
pub struct ArtifactSnapshot {
    pub root: PathBuf,
    pub files: BTreeMap<String, FileSnapshot>,
    pub identity_paths: BTreeSet<String>,
}
#[derive(Debug, Clone)]
pub struct PendingRevision {
    pub session: String,
    pub turn: u64,
    pub record: Value,
}
#[derive(Debug, Clone)]
pub struct OwnedMarkdownHeading {
    pub heading: String,
    pub level: usize,
    pub range: Range<usize>,
    pub body_range: Range<usize>,
    pub path: Vec<String>,
}

/// Normalized views retained from the guarded validation pass. These describe
/// the validated bytes, not subsequent mutations of the working candidate.
#[derive(Debug)]
pub struct ValidatedArtifact {
    pub diagnostics: Vec<crate::report::Diagnostic>,
    pub base_manifest: Option<crate::Manifest>,
    pub candidate_manifest: crate::Manifest,
}
#[derive(Debug)]
struct CachedLedger {
    raw: Vec<u8>,
    parsed: Arc<crate::merge::Ledger>,
}
#[derive(Debug)]
pub struct WorkingArtifact {
    pub base: ArtifactSnapshot,
    /// Candidate bytes for changed destinations, not copies of every preimage.
    pub files: BTreeMap<String, Vec<u8>>,
    pub deleted_paths: BTreeSet<String>,
    pub created_dirs: BTreeSet<String>,
    pub revisions: Vec<PendingRevision>,
    pub owned_turns: BTreeMap<(String, u64), String>,
    pub intents: Vec<Intent>,
    yaml_cache: RefCell<BTreeMap<String, Arc<YamlDocument>>>,
    pub(super) node_kinds_cache: RefCell<Option<Arc<super::node::NodeKinds>>>,
    markdown_cache: RefCell<BTreeMap<String, Arc<Vec<OwnedMarkdownHeading>>>>,
    ledger_cache: RefCell<Option<CachedLedger>>,
}

impl ArtifactSnapshot {
    pub fn load(root: &Path) -> Result<Self, WriteError> {
        let metadata = match fs::symlink_metadata(root) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(WriteError::io(format!("{}: {e}", root.display()))),
        };
        if metadata
            .as_ref()
            .is_some_and(|m| !m.is_dir() || m.file_type().is_symlink())
        {
            return Err(WriteError::semantic(
                "write.path",
                "artifact root must be a real directory, not a symlink",
            ));
        }
        let root = if metadata.is_some() {
            fs::canonicalize(root).map_err(|e| WriteError::io(e.to_string()))?
        } else if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| WriteError::io(e.to_string()))?
                .join(root)
        };
        let mut result = Self {
            root,
            files: BTreeMap::new(),
            identity_paths: BTreeSet::new(),
        };
        for path in CANONICAL_SOURCES {
            result.capture(path)?;
        }
        for directory in ["logic", "trace", "staging", "rubric"] {
            result.walk(directory)?;
        }
        if let Some(paper) = result.files.get("PAPER.md").filter(|f| f.existed) {
            let paper = std::str::from_utf8(&paper.bytes)
                .map_err(|_| WriteError::semantic("write.encoding", "PAPER.md is not UTF-8"))?;
            for path in crate::knowledge_paths(paper)
                .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?
            {
                if !result.files.contains_key(&path) {
                    result.capture(&path)?;
                }
            }
        }
        Ok(result)
    }
    /// Complete merge source: every nonprivate regular file, including opaque
    /// extensions. Capture does not grant permission to write those files.
    pub fn load_complete(root: &Path) -> Result<Self, WriteError> {
        let mut snapshot = Self::load(root)?;
        snapshot.inventory_all(true)?;
        Ok(snapshot)
    }
    /// Resolve external identity terminals without copying code/evidence bodies.
    pub fn load_with_identities(root: &Path) -> Result<Self, WriteError> {
        let mut snapshot = Self::load(root)?;
        snapshot.inventory_all(false)?;
        Ok(snapshot)
    }
    fn inventory_all(&mut self, capture: bool) -> Result<(), WriteError> {
        let mut pending = vec![self.root.clone()];
        while let Some(directory) = pending.pop() {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && directory == self.root =>
                {
                    continue;
                }
                Err(error) => {
                    return Err(WriteError::io(format!("{}: {error}", directory.display())));
                }
            };
            for entry in entries {
                let entry = entry.map_err(|error| WriteError::io(error.to_string()))?;
                let name = entry.file_name().into_string().map_err(|_| {
                    WriteError::semantic("write.path", "artifact paths must be UTF-8")
                })?;
                if matches!(name.as_str(), ".git" | ".ara") || is_temporary_path(&name) {
                    continue;
                }
                let absolute = entry.path();
                let relative = absolute
                    .strip_prefix(&self.root)
                    .expect("inventory descendant")
                    .to_str()
                    .ok_or_else(|| {
                        WriteError::semantic("write.path", "artifact paths must be UTF-8")
                    })?
                    .to_owned();
                safe_relative(&relative)?;
                let kind = entry
                    .file_type()
                    .map_err(|error| WriteError::io(format!("{relative}: {error}")))?;
                if kind.is_symlink() {
                    return Err(WriteError::semantic(
                        "write.path",
                        format!("artifact symlink `{relative}` is not a safe source"),
                    ));
                }
                if kind.is_dir() {
                    pending.push(absolute);
                } else if kind.is_file() {
                    self.identity_paths.insert(relative.clone());
                    if capture && !self.files.contains_key(&relative) {
                        self.capture(&relative)?;
                    }
                } else {
                    return Err(WriteError::semantic(
                        "write.path",
                        format!("artifact entry `{relative}` is not a regular file"),
                    ));
                }
            }
        }
        Ok(())
    }
    fn walk(&mut self, path: &str) -> Result<(), WriteError> {
        let absolute = self.root.join(path);
        let metadata = match fs::symlink_metadata(&absolute) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(WriteError::io(format!("{path}: {e}"))),
        };
        if metadata.file_type().is_symlink() {
            return Err(WriteError::semantic(
                "write.path",
                format!("knowledge symlink `{path}` is not a safe source"),
            ));
        }
        if !metadata.is_dir() {
            return Err(WriteError::semantic(
                "write.path",
                format!("knowledge directory `{path}` is not a directory"),
            ));
        }
        let mut entries = fs::read_dir(absolute)
            .map_err(|e| WriteError::io(format!("{path}: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| WriteError::io(e.to_string()))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| WriteError::semantic("write.path", "knowledge paths must be UTF-8"))?;
            if is_temporary_path(&name) {
                continue;
            }
            let child = format!("{path}/{name}");
            let ty = entry
                .file_type()
                .map_err(|e| WriteError::io(e.to_string()))?;
            if ty.is_dir() {
                self.walk(&child)?;
            } else if !self.files.contains_key(&child) {
                self.capture(&child)?;
            }
        }
        Ok(())
    }
    pub fn capture(&mut self, path: &str) -> Result<(), WriteError> {
        safe_relative(path)?;
        let absolute = super::transaction::checked_destination(&self.root, path)?;
        let metadata = match fs::symlink_metadata(&absolute) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(WriteError::io(format!("{path}: {e}"))),
        };
        if metadata
            .as_ref()
            .is_some_and(|m| !m.is_file() || m.file_type().is_symlink())
        {
            return Err(WriteError::semantic(
                "write.path",
                format!("source `{path}` must be a regular file, not a symlink"),
            ));
        }
        let bytes = if metadata.is_some() {
            fs::read(&absolute).map_err(|e| WriteError::io(format!("{path}: {e}")))?
        } else {
            Vec::new()
        };
        let snapshot = FileSnapshot {
            digest: digest(&bytes),
            bytes,
            existed: metadata.is_some(),
            permissions: metadata.map(|m| m.permissions()),
        };
        self.files.insert(path.into(), snapshot);
        Ok(())
    }
}

impl WorkingArtifact {
    pub fn new(base: ArtifactSnapshot) -> Self {
        Self {
            base,
            files: BTreeMap::new(),
            deleted_paths: BTreeSet::new(),
            created_dirs: BTreeSet::new(),
            revisions: Vec::new(),
            owned_turns: BTreeMap::new(),
            intents: Vec::new(),
            yaml_cache: RefCell::new(BTreeMap::new()),
            node_kinds_cache: RefCell::new(None),
            markdown_cache: RefCell::new(BTreeMap::new()),
            ledger_cache: RefCell::new(None),
        }
    }
    pub fn bytes(&self, path: &str) -> Result<&[u8], WriteError> {
        if self.deleted_paths.contains(path) {
            return Err(WriteError::semantic(
                "write.missing",
                format!("source `{path}` is deleted in candidate"),
            ));
        }
        if let Some(bytes) = self.files.get(path) {
            return Ok(bytes);
        }
        self.base
            .files
            .get(path)
            .filter(|file| file.existed)
            .map(|file| file.bytes.as_slice())
            .ok_or_else(|| {
                WriteError::semantic("write.missing", format!("source `{path}` does not exist"))
            })
    }
    pub fn text(&self, path: &str) -> Result<&str, WriteError> {
        std::str::from_utf8(self.bytes(path)?).map_err(|_| {
            WriteError::semantic(
                "write.encoding",
                format!("source `{path}` is not valid UTF-8"),
            )
        })
    }
    pub fn exists(&self, path: &str) -> bool {
        !self.deleted_paths.contains(path)
            && (self.files.contains_key(path)
                || self.base.files.get(path).is_some_and(|f| f.existed))
    }
    pub fn paths(&self) -> Vec<String> {
        self.base
            .files
            .iter()
            .filter(|(p, f)| f.existed && !self.deleted_paths.contains(*p))
            .map(|(p, _)| p.clone())
            .chain(self.files.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn changed_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter(|(path, bytes)| {
                !self
                    .base
                    .files
                    .get(*path)
                    .is_some_and(|f| f.existed && f.bytes == **bytes)
            })
            .map(|(path, _)| path.clone())
            .chain(self.deleted_paths.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// Native, read-only preflight for missing target and explicit seed
    /// directories. Pure merge planning does not call this method.
    pub fn plan_missing_directories(&mut self) -> Result<(), WriteError> {
        let mut candidates = self.created_dirs.clone();
        for path in self.changed_paths() {
            if self.deleted_paths.contains(&path) {
                continue;
            }
            for parent in Path::new(&path)
                .ancestors()
                .skip(1)
                .filter(|p| !p.as_os_str().is_empty())
            {
                candidates.insert(parent.to_string_lossy().into_owned());
            }
        }
        for relative in candidates {
            safe_relative(&relative)?;
            let path = self.base.root.join(&relative);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    self.created_dirs.remove(&relative);
                }
                Ok(_) => {
                    return Err(WriteError::semantic(
                        "write.path",
                        format!("directory `{relative}` must be a real directory"),
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.created_dirs.insert(relative);
                }
                Err(error) => return Err(WriteError::io(format!("{}: {error}", path.display()))),
            }
        }
        Ok(())
    }
    pub fn delete(&mut self, path: &str, reason: &str) -> Result<(), WriteError> {
        if !self.is_allowed_document(path)? || path == "PAPER.md" {
            return Err(WriteError::semantic(
                "write.immutable",
                "only mutable knowledge documents can be deleted",
            ));
        }
        let before = self.bytes(path)?;
        let before_digest = digest(before);
        let len = before.len();
        self.files.remove(path);
        self.deleted_paths.insert(path.into());
        self.invalidate_path(path);
        self.intents.push(Intent {
            path: path.into(),
            range: 0..len,
            before_digest,
            after_digest: "absent".into(),
            reason: reason.into(),
        });
        Ok(())
    }
    pub fn is_allowed_document(&self, path: &str) -> Result<bool, WriteError> {
        if allowed_document(path) {
            return Ok(true);
        }
        if !self.exists("PAPER.md") {
            return Ok(false);
        }
        Ok(crate::knowledge_paths(self.text("PAPER.md")?)
            .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?
            .iter()
            .any(|registered| registered == path))
    }
    pub fn allocate_id(
        &self,
        prefix: char,
        ids: &[String],
        requested: Option<&str>,
    ) -> Result<String, WriteError> {
        let mut reserved = BTreeSet::new();
        if self.exists("trace/merge_log.yaml") {
            let document = self.yaml("trace/merge_log.yaml")?;
            if document.root.get("format")?.and_then(YamlNode::scalar) != Some("ara.merge-log/v1") {
                return Err(WriteError::semantic(
                    "write.merge_history",
                    "Cannot allocate safely from unsupported merge history",
                ));
            }
            let records = document
                .root
                .get("records")?
                .ok_or_else(|| {
                    WriteError::semantic("write.merge_history", "Merge history records are missing")
                })?
                .sequence()?;
            for record in records {
                if record.get("kind")?.and_then(YamlNode::scalar) != Some("revision") {
                    continue;
                }
                let mappings = record
                    .get("mappings")?
                    .ok_or_else(|| {
                        WriteError::semantic(
                            "write.merge_history",
                            "Merge revision mappings are missing",
                        )
                    })?
                    .sequence()?;
                for mapping in mappings {
                    let target = mapping
                        .get("target")?
                        .and_then(YamlNode::scalar)
                        .ok_or_else(|| {
                            WriteError::semantic(
                                "write.merge_history",
                                "Merge mapping target must be a scalar identity",
                            )
                        })?;
                    reserve_numeric(prefix, target, &mut reserved)?;
                }
            }
        }
        if self.exists("trace/logic_mutations.yaml") {
            let document = self.yaml("trace/logic_mutations.yaml")?;
            let records = document
                .root
                .get("mutations")?
                .ok_or_else(|| {
                    WriteError::semantic("write.redirect", "Mutation records are missing")
                })?
                .sequence()?;
            for record in records {
                let from = record
                    .get("from")?
                    .and_then(YamlNode::scalar)
                    .ok_or_else(|| {
                        WriteError::semantic("write.redirect", "Mutation source locator is missing")
                    })?;
                if let Some((document, identity)) = from.split_once(':')
                    && matches!(
                        (prefix, document),
                        ('C', "logic/claims.md")
                            | ('H', "logic/solution/heuristics.md")
                            | ('E', "logic/experiments.md")
                            | ('R', "rubric/requirements.md")
                    )
                {
                    reserve_numeric(prefix, identity, &mut reserved)?;
                }
            }
        }
        allocate_with_reserved(prefix, ids, requested, &reserved)
    }
    /// Pure staging boundary for merge/import snapshots. The native caller must
    /// recheck the absent destination under its checkout lock at commit.
    pub fn stage_create(&mut self, path: &str, bytes: &[u8]) -> Result<(), WriteError> {
        safe_relative(path)?;
        if self.exists(path) || self.deleted_paths.contains(path) {
            return Err(WriteError::semantic(
                "write.exists",
                format!("source `{path}` already exists or was deleted in this plan"),
            ));
        }
        if path.ends_with(".md") || path.ends_with(".yaml") || path.ends_with(".yml") {
            let text = std::str::from_utf8(bytes).map_err(|_| {
                WriteError::semantic(
                    "write.encoding",
                    "new structured knowledge source must be UTF-8",
                )
            })?;
            self.check_syntax(path, text)?;
        }
        self.base
            .files
            .entry(path.into())
            .or_insert_with(|| FileSnapshot {
                bytes: Vec::new(),
                existed: false,
                permissions: None,
                digest: digest(&[]),
            });
        self.files.insert(path.into(), bytes.to_vec());
        self.intents.push(Intent {
            path: path.into(),
            range: 0..0,
            before_digest: digest(&[]),
            after_digest: digest(bytes),
            reason: "create exact imported knowledge source".into(),
        });
        Ok(())
    }
    pub fn stage_replace(
        &mut self,
        path: &str,
        bytes: &[u8],
        reason: &str,
    ) -> Result<(), WriteError> {
        if path.ends_with(".md") || path.ends_with(".yaml") || path.ends_with(".yml") {
            let text = std::str::from_utf8(bytes).map_err(|_| {
                WriteError::semantic(
                    "write.encoding",
                    "structured knowledge candidate must be UTF-8",
                )
            })?;
            return self.replace_document(path, text, reason);
        }
        safe_relative(path)?;
        let before = self.bytes(path)?;
        if before == bytes {
            return Ok(());
        }
        let len = before.len();
        let before_digest = digest(before);
        self.invalidate_path(path);
        self.files.insert(path.into(), bytes.to_vec());
        self.intents.push(Intent {
            path: path.into(),
            range: 0..len,
            before_digest,
            after_digest: digest(bytes),
            reason: reason.into(),
        });
        Ok(())
    }
    pub fn create(&mut self, path: &str, content: &str) -> Result<(), WriteError> {
        safe_relative(path)?;
        if self.exists(path) {
            return Err(WriteError::semantic(
                "write.exists",
                format!("source `{path}` already exists"),
            ));
        }
        self.capture_destination(path)?;
        self.check_syntax(path, content)?;
        self.intents.push(Intent {
            path: path.into(),
            range: 0..0,
            before_digest: digest(&[]),
            after_digest: digest(content.as_bytes()),
            reason: "create complete caller-supplied source".into(),
        });
        self.files.insert(path.into(), content.as_bytes().to_vec());
        Ok(())
    }
    fn capture_destination(&mut self, path: &str) -> Result<(), WriteError> {
        if !self.base.files.contains_key(path) {
            self.base.capture(path)?;
        }
        if self.base.files.get(path).is_some_and(|f| f.existed) {
            return Err(WriteError::semantic(
                "write.concurrent_edit",
                format!("destination `{path}` appeared outside snapshot"),
            ));
        }
        Ok(())
    }
    pub fn replace_document(
        &mut self,
        path: &str,
        content: &str,
        reason: &str,
    ) -> Result<(), WriteError> {
        let len = self.bytes(path)?.len();
        self.edit(path, 0..len, content, reason)
    }
    pub fn edit(
        &mut self,
        path: &str,
        range: Range<usize>,
        replacement: &str,
        reason: &str,
    ) -> Result<(), WriteError> {
        safe_relative(path)?;
        let before = self.bytes(path)?;
        let text = std::str::from_utf8(before)
            .map_err(|_| WriteError::semantic("write.encoding", "target is not UTF-8"))?;
        if range.start > range.end
            || range.end > before.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            return Err(WriteError::semantic(
                "write.intent",
                "invalid structural byte range",
            ));
        }
        if before[range.clone()] == *replacement.as_bytes() {
            return Ok(());
        }
        let mut candidate = Vec::with_capacity(before.len() - range.len() + replacement.len());
        candidate.extend_from_slice(&before[..range.start]);
        candidate.extend_from_slice(replacement.as_bytes());
        candidate.extend_from_slice(&before[range.end..]);
        self.accept_candidate(path, range, replacement, candidate, reason, None)
    }
    fn accept_candidate(
        &mut self,
        path: &str,
        range: Range<usize>,
        replacement: &str,
        candidate: Vec<u8>,
        reason: &str,
        indexed: Option<YamlDocument>,
    ) -> Result<(), WriteError> {
        if path == "trace/exploration_tree.yaml" {
            self.node_kinds_cache.borrow_mut().take();
        }
        let before = self.bytes(path)?;
        validate_bytes(before, &candidate, range.clone(), replacement.as_bytes())?;
        let before_digest = digest(before);
        let after_digest = digest(&candidate);
        if let Some(document) = indexed {
            self.markdown_cache.borrow_mut().remove(path);
            self.yaml_cache
                .borrow_mut()
                .insert(path.into(), Arc::new(document));
        } else {
            let text = std::str::from_utf8(&candidate)
                .map_err(|_| WriteError::semantic("write.encoding", "candidate is not UTF-8"))?;
            self.check_syntax(path, text)?;
        }
        self.files.insert(path.into(), candidate);
        self.intents.push(Intent {
            path: path.into(),
            range,
            before_digest,
            after_digest,
            reason: reason.into(),
        });
        Ok(())
    }
    fn check_syntax(&mut self, path: &str, text: &str) -> Result<(), WriteError> {
        if path == "trace/exploration_tree.yaml" {
            self.node_kinds_cache.borrow_mut().take();
        }
        self.markdown_cache.borrow_mut().remove(path);
        if path == "trace/merge_log.yaml" {
            self.merge_ledger_bytes(text.as_bytes())
                .map_err(|error| WriteError::semantic(&error.code, error.message))?;
            self.yaml_cache.borrow_mut().remove(path);
            return Ok(());
        }
        if path.ends_with(".yaml") || path.ends_with(".yml") {
            let document = YamlDocument::parse(text).map_err(|error| error.at(path))?;
            self.yaml_cache
                .borrow_mut()
                .insert(path.into(), Arc::new(document));
        }
        Ok(())
    }
    pub(crate) fn merge_ledger(
        &self,
    ) -> Result<Arc<crate::merge::Ledger>, crate::merge::MergeError> {
        self.merge_ledger_bytes(self.bytes("trace/merge_log.yaml")?)
    }
    fn merge_ledger_bytes(
        &self,
        raw: &[u8],
    ) -> Result<Arc<crate::merge::Ledger>, crate::merge::MergeError> {
        // Public candidate/preimage maps can bypass invalidation and retain a
        // stale digest. Only exact current byte equality authorizes reuse.
        if let Some(cached) = self.ledger_cache.borrow().as_ref()
            && cached.raw == raw
        {
            return Ok(Arc::clone(&cached.parsed));
        }
        let parsed = Arc::new(crate::merge::decode_ledger_bytes(raw)?);
        *self.ledger_cache.borrow_mut() = Some(CachedLedger {
            raw: raw.to_vec(),
            parsed: Arc::clone(&parsed),
        });
        Ok(parsed)
    }
    pub fn yaml(&self, path: &str) -> Result<Arc<YamlDocument>, WriteError> {
        if let Some(document) = self.yaml_cache.borrow().get(path) {
            return Ok(Arc::clone(document));
        }
        let document =
            Arc::new(YamlDocument::parse(self.text(path)?).map_err(|error| error.at(path))?);
        self.yaml_cache
            .borrow_mut()
            .insert(path.into(), Arc::clone(&document));
        Ok(document)
    }
    pub fn headings(&self, path: &str) -> Result<Arc<Vec<OwnedMarkdownHeading>>, WriteError> {
        if let Some(headings) = self.markdown_cache.borrow().get(path) {
            return Ok(Arc::clone(headings));
        }
        let headings = Arc::new(
            crate::markdown::headings(self.text(path)?)
                .into_iter()
                .map(|heading| OwnedMarkdownHeading {
                    heading: heading.heading.into(),
                    level: heading.level,
                    range: heading.range,
                    body_range: heading.body_range,
                    path: heading.path.into_iter().map(String::from).collect(),
                })
                .collect(),
        );
        self.markdown_cache
            .borrow_mut()
            .insert(path.into(), Arc::clone(&headings));
        Ok(headings)
    }
    /// External merge composers using the public candidate map must invalidate
    /// a changed path before querying its cached structural source index.
    pub fn invalidate_path(&self, path: &str) {
        if path == "trace/exploration_tree.yaml" {
            self.node_kinds_cache.borrow_mut().take();
        }
        self.yaml_cache.borrow_mut().remove(path);
        self.markdown_cache.borrow_mut().remove(path);
    }
    pub fn ensure_yaml(&mut self, path: &str, empty: &str) -> Result<(), WriteError> {
        if !self.exists(path) {
            self.create(path, empty)?;
        }
        Ok(())
    }
    pub fn replace_yaml_field(
        &mut self,
        path: &str,
        selector: &[PathPart],
        key: &str,
        value: &Value,
    ) -> Result<(), WriteError> {
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(WriteError::semantic(
                "write.field",
                "invalid YAML field name",
            ));
        }
        let before = self.yaml(path)?;
        let mapping = before.root.at(selector)?;
        let entries = mapping.mapping()?;
        if let Some(current) = mapping.get(key)? {
            current.editable()?;
            if current.to_json()? == *value {
                return Ok(());
            }
        }
        if mapping.flow {
            let text = self.text(path)?;
            let encoded = serde_json::to_string(value)
                .map_err(|error| WriteError::semantic("write.value", error.to_string()))?;
            let parsed = YamlDocument::parse(&encoded)?.root.semantic();
            let (range, replacement) = if let Some(current) = mapping.get(key)? {
                (current.start..current.end, encoded)
            } else {
                let close = mapping
                    .end
                    .checked_sub(1)
                    .filter(|index| text.as_bytes().get(*index) == Some(&b'}'))
                    .ok_or_else(|| {
                        WriteError::semantic(
                            "write.unsupported_source",
                            "ambiguous flow mapping closing delimiter",
                        )
                    })?;
                (
                    close..close,
                    format!(
                        "{}{key}: {encoded}",
                        if entries.is_empty() { "" } else { ", " }
                    ),
                )
            };
            let candidate = splice(text, range.clone(), &replacement);
            let after = YamlDocument::parse(&candidate)?;
            validate_yaml(
                &before,
                &after,
                selector,
                YamlDelta::Field(key.into(), parsed),
            )
            .map_err(|error| error.at(format!("{path}.{key}")))?;
            return self.accept_candidate(
                path,
                range,
                &replacement,
                candidate.into_bytes(),
                "replace one typed YAML field",
                Some(after),
            );
        }
        let text = self.text(path)?;
        let newline = eol(text);
        let key_node = entries
            .iter()
            .find(|(k, _)| k.scalar() == Some(key))
            .map(|(k, _)| k);
        let indent = entries
            .first()
            .map(|(k, _)| k.start - line_start(text, k.start))
            .unwrap_or(mapping.start - line_start(text, mapping.start));
        let nested = if value.as_object().is_some_and(|map| !map.is_empty()) {
            format!("{newline}{}", " ".repeat(indent + 2))
        } else {
            String::new()
        };
        let fragment = format!(
            "{key}: {nested}{}{newline}",
            render_yaml(value, indent + 2, newline)
        );
        let parsed = YamlDocument::parse(&format!("{}{fragment}", " ".repeat(indent)))?
            .root
            .get(key)?
            .ok_or_else(|| WriteError::semantic("write.intent", "rendered field missing"))?
            .semantic();
        let (range, replacement) = if let Some(range) = field_range(text, mapping, key)? {
            let key_node = key_node
                .ok_or_else(|| WriteError::semantic("write.intent", "field key missing"))?;
            // A sequence item's first key shares its line with '-'. Retain that
            // prefix rather than erase the surrounding sequence marker.
            let prefix = &text[range.start..key_node.start];
            let current = mapping
                .get(key)?
                .ok_or_else(|| WriteError::semantic("write.intent", "field value missing"))?;
            let replacement = retain_field_comments(
                text,
                range.clone(),
                current,
                key_node.start - line_start(text, key_node.start),
                format!("{prefix}{fragment}"),
            );
            (range, replacement)
        } else {
            let at = boundary(text, mapping.end);
            let prefix = if at > 0 && !text[..at].ends_with('\n') {
                newline
            } else {
                ""
            };
            (at..at, format!("{prefix}{}{fragment}", " ".repeat(indent)))
        };
        let candidate = splice(text, range.clone(), &replacement);
        let after = YamlDocument::parse(&candidate)?;
        validate_yaml(
            &before,
            &after,
            selector,
            YamlDelta::Field(key.into(), parsed),
        )
        .map_err(|error| error.at(format!("{path}.{key}")))?;
        self.accept_candidate(
            path,
            range,
            &replacement,
            candidate.into_bytes(),
            "replace one typed YAML field",
            Some(after),
        )
    }
    pub fn append_yaml(
        &mut self,
        path: &str,
        selector: &[PathPart],
        value: &Value,
    ) -> Result<(), WriteError> {
        self.append_yaml_many(path, selector, std::slice::from_ref(value))
    }
    pub fn append_yaml_many(
        &mut self,
        path: &str,
        selector: &[PathPart],
        values: &[Value],
    ) -> Result<(), WriteError> {
        if values.is_empty() {
            return Ok(());
        }
        let before = self.yaml(path)?;
        let target = before.root.at(selector)?;
        let items = target.sequence()?;
        let text = self.text(path)?;
        let newline = eol(text);
        let parent_flow = selector
            .split_last()
            .map(|(_, parent)| before.root.at(parent).map(|node| node.flow))
            .transpose()?
            .unwrap_or(false);
        let expand =
            target.flow && !parent_flow && items.is_empty() && values.iter().all(Value::is_object);
        let (range, replacement, fragment) = if target.flow && !expand {
            let close = target
                .end
                .checked_sub(1)
                .filter(|index| text.as_bytes().get(*index) == Some(&b']'))
                .ok_or_else(|| {
                    WriteError::semantic(
                        "write.unsupported_source",
                        "ambiguous flow list closing delimiter",
                    )
                })?;
            let mut encoded = String::new();
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    encoded.push_str(", ");
                }
                encoded.push_str(
                    &serde_json::to_string(value)
                        .map_err(|error| WriteError::semantic("write.value", error.to_string()))?,
                );
            }
            let fragment = format!("[{encoded}]\n");
            let replacement = if items.is_empty() {
                encoded
            } else {
                format!(", {encoded}")
            };
            (close..close, replacement, fragment)
        } else {
            let indent = if expand {
                if let Some((PathPart::Key(key), parent)) = selector.split_last() {
                    let mapping = before.root.at(parent)?;
                    mapping
                        .mapping()?
                        .iter()
                        .find(|(name, _)| name.scalar() == Some(key))
                        .map(|(key, _)| key.start - line_start(text, key.start) + 2)
                        .ok_or_else(|| {
                            WriteError::semantic("write.intent", "list field position missing")
                        })?
                } else {
                    target.start - line_start(text, target.start)
                }
            } else if let Some(first) = items.first() {
                let line = line_start(text, first.start);
                text[line..first.start]
                    .find('-')
                    .map_or(first.start - line, |index| index)
            } else {
                target.start - line_start(text, target.start)
            };
            let padding = " ".repeat(indent);
            let mut fragment = String::new();
            for value in values {
                fragment.push_str(&padding);
                fragment.push_str("- ");
                fragment.push_str(&render_yaml(value, indent + 2, newline));
                fragment.push_str(newline);
            }
            let replacement = if expand {
                let body = if matches!(text.as_bytes().get(target.end), Some(b'\r' | b'\n')) {
                    fragment.strip_suffix(newline).unwrap_or(&fragment)
                } else {
                    &fragment
                };
                format!("{newline}{body}")
            } else {
                let at = boundary(text, target.end);
                let prefix = if at > 0 && !text[..at].ends_with('\n') {
                    newline
                } else {
                    ""
                };
                format!("{prefix}{fragment}")
            };
            let range = if expand {
                target.start..target.end
            } else {
                let at = boundary(text, target.end);
                at..at
            };
            (range, replacement, fragment)
        };
        let mut expected = YamlDocument::parse(&fragment)?.root.semantic();
        let parsed = match &mut expected.value {
            super::positions::SourceKind::Sequence(items) => std::mem::take(items),
            _ => {
                return Err(WriteError::semantic(
                    "write.intent",
                    "rendered append group is not a sequence",
                ));
            }
        };
        let candidate = splice(text, range.clone(), &replacement);
        let after = YamlDocument::parse(&candidate)?;
        validate_yaml(&before, &after, selector, YamlDelta::AppendMany(parsed))
            .map_err(|error| error.at(format!("{path}:{selector:?}")))?;
        self.accept_candidate(
            path,
            range,
            &replacement,
            candidate.into_bytes(),
            "append ordered typed YAML entries",
            Some(after),
        )
    }
    pub fn include_operational_ignore(&mut self) -> Result<(), WriteError> {
        let existing = if self.exists(".gitignore") {
            self.text(".gitignore")?
        } else {
            ""
        };
        if existing
            .lines()
            .any(|line| matches!(line.trim(), ".ara/" | "/.ara/"))
        {
            return Ok(());
        }
        let newline = eol(existing);
        let content = format!(
            "{existing}{}.ara/{newline}",
            if existing.is_empty() || existing.ends_with('\n') {
                ""
            } else {
                newline
            }
        );
        if self.exists(".gitignore") {
            self.replace_document(
                ".gitignore",
                &content,
                "operational artifact-root .ara ignore rule",
            )
        } else {
            self.create(".gitignore", &content)
        }
    }
    pub fn validate(&self) -> Result<ValidatedArtifact, WriteError> {
        let tree = self.text("trace/exploration_tree.yaml")?;
        let claims = self.text("logic/claims.md").ok();
        let redirects = super::logic::claim_redirects_from_source(self)?;
        let (candidate_manifest, candidate) =
            match crate::parse::parse_sources_detailed_with_claim_redirects(
                tree, claims, &redirects,
            ) {
                crate::parse::ParseOutcome::Normalized(manifest, report) => (manifest, report),
                crate::parse::ParseOutcome::Fatal(report) => {
                    return Err(WriteError::semantic(
                        "write.syntax",
                        format!(
                            "fatal candidate syntax: {}",
                            report
                                .errors()
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("; ")
                        ),
                    ));
                }
            };
        let base_tree = self
            .base
            .files
            .get("trace/exploration_tree.yaml")
            .filter(|f| f.existed)
            .map(|f| std::str::from_utf8(&f.bytes))
            .transpose()
            .map_err(|_| WriteError::semantic("write.encoding", "base tree is not UTF-8"))?;
        let base_claims = self
            .base
            .files
            .get("logic/claims.md")
            .filter(|f| f.existed)
            .and_then(|f| std::str::from_utf8(&f.bytes).ok());
        let base_redirects = super::logic::claim_redirects_from_snapshot(&self.base)?;
        let (base_manifest, base_errors) = if let Some(tree) = base_tree {
            match crate::parse::parse_sources_detailed_with_claim_redirects(
                tree,
                base_claims,
                &base_redirects,
            ) {
                crate::parse::ParseOutcome::Normalized(manifest, report) => {
                    (Some(manifest), report.errors().to_vec())
                }
                crate::parse::ParseOutcome::Fatal(_) => {
                    return Err(WriteError::semantic(
                        "write.syntax",
                        "fatal base syntax rejects authoring",
                    ));
                }
            }
        } else {
            (None, Vec::new())
        };
        let mut allowed = BTreeMap::new();
        for error in base_errors {
            *allowed
                .entry((error.code.to_string(), error.path, error.message))
                .or_insert(0usize) += 1;
        }
        for error in candidate.errors() {
            let key = (
                error.code.to_string(),
                error.path.clone(),
                error.message.clone(),
            );
            match allowed.get_mut(&key) {
                Some(count) if *count > 0 => *count -= 1,
                _ => {
                    return Err(
                        WriteError::semantic("write.validation", error.to_string()).at(&error.path)
                    );
                }
            }
        }
        super::node::validate_references(self)?;
        super::logic::validate_references(self)?;
        super::staging::validate_references(self)?;
        super::sessions::validate_authored(self)?;
        super::records::validate_references(self)?;
        Ok(ValidatedArtifact {
            diagnostics: candidate
                .errors()
                .iter()
                .chain(candidate.warnings())
                .cloned()
                .collect(),
            base_manifest,
            candidate_manifest,
        })
    }
}

pub fn safe_relative(path: &str) -> Result<(), WriteError> {
    if path.is_empty()
        || path.contains('\\')
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(WriteError::semantic(
            "write.path",
            format!("path `{path}` must be artifact-relative without traversal"),
        ));
    }
    Ok(())
}
pub fn allowed_document(path: &str) -> bool {
    safe_relative(path).is_ok()
        && (matches!(
            path,
            "PAPER.md"
                | "logic/problem.md"
                | "logic/claims.md"
                | "logic/concepts.md"
                | "logic/experiments.md"
                | "logic/related_work.md"
                | "rubric/requirements.md"
        ) || (path.starts_with("logic/solution/") && path.ends_with(".md")))
}
pub fn allowed_write_path(path: &str) -> bool {
    safe_relative(path).is_ok()
        && (matches!(
            path,
            "PAPER.md" | ".gitignore" | "evidence/README.md" | "rubric/requirements.md"
        ) || path.starts_with("logic/")
            || path.starts_with("trace/")
            || path.starts_with("staging/")
            || (path.ends_with(".md")
                && !path.split('/').any(|component| component.starts_with('.'))
                && !matches!(path.split('/').next(), Some("src" | "evidence"))))
}
/// Exact reserved same-directory transaction temp namespace. Unauthenticated
/// pre-prepared crash leftovers are excluded, never prefix-deleted.
pub fn is_temporary_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_prefix(".ara-write-")
        .and_then(|suffix| suffix.split_once('-'))
        .is_some_and(|(pid, nonce)| {
            !pid.is_empty()
                && !nonce.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
                && nonce.bytes().all(|b| b.is_ascii_digit())
        })
}
pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn allocate_id(
    prefix: char,
    ids: &[String],
    requested: Option<&str>,
) -> Result<String, WriteError> {
    allocate_with_reserved(prefix, ids, requested, &BTreeSet::new())
}
fn reserve_numeric(
    prefix: char,
    identity: &str,
    reserved: &mut BTreeSet<u64>,
) -> Result<(), WriteError> {
    if let Some(digits) = identity
        .strip_prefix(prefix)
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
    {
        reserved.insert(digits.parse().map_err(|_| {
            WriteError::semantic(
                "write.id_overflow",
                format!("historical identity `{identity}` overflows"),
            )
        })?);
    }
    Ok(())
}
fn allocate_with_reserved(
    prefix: char,
    ids: &[String],
    requested: Option<&str>,
    reserved: &BTreeSet<u64>,
) -> Result<String, WriteError> {
    let mut suffixes = BTreeMap::new();
    let mut maximum = 0u64;
    for id in ids {
        let digits = id
            .strip_prefix(prefix)
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| {
                WriteError::semantic("write.id", format!("invalid source identity `{id}`"))
            })?;
        let numeric = digits.parse::<u64>().map_err(|_| {
            WriteError::semantic(
                "write.id_overflow",
                format!("numeric suffix overflows in `{id}`"),
            )
        })?;
        if let Some(previous) = suffixes.insert(numeric, id) {
            return Err(WriteError::semantic(
                "write.ambiguous_id",
                format!("source identities `{previous}` and `{id}` have the same numeric suffix"),
            ));
        }
        maximum = maximum.max(numeric);
    }
    if let Some(maximum_reserved) = reserved.last() {
        maximum = maximum.max(*maximum_reserved);
    }
    if let Some(requested) = requested {
        let digits = requested
            .strip_prefix(prefix)
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| {
                WriteError::semantic(
                    "write.id",
                    format!("requested identity `{requested}` is not a {prefix} ID"),
                )
            })?;
        let numeric = digits
            .parse::<u64>()
            .map_err(|_| WriteError::semantic("write.id_overflow", "requested ID overflows"))?;
        if suffixes.contains_key(&numeric) || reserved.contains(&numeric) {
            return Err(WriteError::semantic(
                "write.id_collision",
                format!("requested identity `{requested}` is already reserved"),
            ));
        }
        return Ok(requested.into());
    }
    let next = maximum.checked_add(1).ok_or_else(|| {
        WriteError::semantic("write.id_overflow", "ID allocation suffix overflows")
    })?;
    Ok(format!("{prefix}{next:02}"))
}
/// Render LF prose as literal blocks with explicit indentation and exact
/// chomping. CR/control text uses JSON quoting to avoid YAML normalization.
pub fn render_yaml(value: &Value, indent: usize, newline: &str) -> String {
    match value {
        Value::String(text)
            if text.contains('\n')
                && !text
                    .chars()
                    .any(|c| c == '\r' || (c.is_control() && c != '\n' && c != '\t')) =>
        {
            let trailing = text.bytes().rev().take_while(|b| *b == b'\n').count();
            let chomping = match trailing {
                0 => "-",
                1 => "",
                _ => "+",
            };
            let content = text.strip_suffix('\n').unwrap_or(text);
            let prefix = " ".repeat(indent.max(2));
            let mut output = format!("|2{chomping}{newline}");
            for (index, line) in content.split('\n').enumerate() {
                if index > 0 {
                    output.push_str(newline);
                }
                output.push_str(&prefix);
                output.push_str(line);
            }
            output
        }
        Value::Object(map) if !map.is_empty() => {
            let mut output = String::new();
            for (index, (key, value)) in map.iter().enumerate() {
                if index > 0 {
                    output.push_str(newline);
                    output.push_str(&" ".repeat(indent));
                }
                output.push_str(key);
                output.push_str(": ");
                if value.as_object().is_some_and(|map| !map.is_empty()) {
                    output.push_str(newline);
                    output.push_str(&" ".repeat(indent + 2));
                }
                output.push_str(&render_yaml(value, indent + 2, newline));
            }
            output
        }
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
            let mut output = String::new();
            for item in items {
                output.push_str(newline);
                output.push_str(&" ".repeat(indent));
                output.push_str("- ");
                output.push_str(&render_yaml(item, indent + 2, newline));
            }
            output
        }
        Value::Array(items) if !items.is_empty() => {
            serde_json::to_string(items).expect("JSON value serialization is infallible")
        }
        _ => serde_json::to_string(value).expect("JSON value serialization is infallible"),
    }
}
fn boundary(text: &str, end: usize) -> usize {
    if end >= text.len() {
        text.len()
    } else {
        line_start(text, end)
    }
}
fn splice(text: &str, range: Range<usize>, replacement: &str) -> String {
    let mut output = String::with_capacity(text.len() - range.len() + replacement.len());
    output.push_str(&text[..range.start]);
    output.push_str(replacement);
    output.push_str(&text[range.end..]);
    output
}
fn retain_field_comments(
    text: &str,
    range: Range<usize>,
    value: &YamlNode,
    key_indent: usize,
    mut replacement: String,
) -> String {
    let header_end = super::positions::line_end(text, range.start);
    let header_content_end = header_end
        - text[range.start..header_end]
            .bytes()
            .rev()
            .take_while(|b| matches!(b, b'\r' | b'\n'))
            .count();
    let block = matches!(
        value.style,
        Some(
            yaml_rust2::scanner::TScalarStyle::Literal | yaml_rust2::scanner::TScalarStyle::Folded
        )
    );
    let search = if block { value.start } else { value.end };
    if search <= header_content_end
        && let Some(hash) = text[search..header_content_end].find('#')
    {
        let hash = search + hash;
        let start = search + text[search..hash].trim_end().len();
        let suffix = &text[start..header_content_end];
        let newline = replacement.find('\n').unwrap_or(replacement.len());
        let at = if newline > 0 && replacement.as_bytes()[newline - 1] == b'\r' {
            newline - 1
        } else {
            newline
        };
        replacement.insert_str(at, suffix);
    }
    let mut offset = range.start;
    for line in text[range].split_inclusive('\n') {
        let content = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - content.len();
        let quoted = matches!(
            value.style,
            Some(
                yaml_rust2::scanner::TScalarStyle::SingleQuoted
                    | yaml_rust2::scanner::TScalarStyle::DoubleQuoted
            )
        );
        if content.starts_with('#')
            && indent <= key_indent
            && !(quoted && offset >= value.start && offset < value.end)
        {
            replacement.push_str(line);
        }
        offset += line.len();
    }
    replacement
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_rejects_numeric_aliases_and_overflow() {
        assert!(allocate_id('N', &["N1".into(), "N01".into()], None).is_err());
        assert!(allocate_id('N', &[format!("N{}", u64::MAX)], None).is_err());
        assert_eq!(
            allocate_id('N', &["N03".into(), "N01".into()], None).unwrap(),
            "N04"
        );
    }
    #[test]
    fn allocation_reserves_retired_merge_targets_without_reading_opaque_prose() {
        let history = "format: ara.merge-log/v1\nrecords:\n  - kind: revision\n    mappings:\n      - {target: C02}\n      - {target: C03}\n      - {target: C03}\n      - {target: N09}\n      - {target: O07}\n      - {target: H04}\n      - {target: E05}\n      - {target: T06}\n      - {target: 'notes.md:C99'}\n    unrelated: {target: C98}\n  - kind: note\n    mappings: [{target: C97}]\n";
        let mutations = "mutations:\n  - {from: 'logic/claims.md:C08', to: null}\n  - {from: 'notes.md:C99', to: null}\n";
        let files = [
            ("trace/merge_log.yaml", history),
            ("trace/logic_mutations.yaml", mutations),
        ]
        .into_iter()
        .map(|(path, text)| {
            (
                path.into(),
                FileSnapshot {
                    bytes: text.as_bytes().to_vec(),
                    existed: true,
                    permissions: None,
                    digest: digest(text.as_bytes()),
                },
            )
        })
        .collect();
        let working = WorkingArtifact::new(ArtifactSnapshot {
            root: PathBuf::from("/virtual/source"),
            files,
            identity_paths: BTreeSet::new(),
        });
        assert_eq!(
            working.allocate_id('C', &["C02".into()], None).unwrap(),
            "C09"
        );
        assert_eq!(
            working
                .allocate_id('C', &["C02".into()], Some("C03"))
                .unwrap_err()
                .code,
            "write.id_collision"
        );
        assert_eq!(
            working
                .allocate_id('C', &["C02".into()], Some("C008"))
                .unwrap_err()
                .code,
            "write.id_collision"
        );
        for (prefix, expected) in [
            ('N', "N10"),
            ('O', "O08"),
            ('H', "H05"),
            ('E', "E06"),
            ('T', "T07"),
        ] {
            assert_eq!(working.allocate_id(prefix, &[], None).unwrap(), expected);
        }
    }
    #[test]
    fn unsafe_merge_history_cannot_silently_release_retired_ids() {
        let history = "format: ara.merge-log/v999\nrecords: []\n";
        let files = BTreeMap::from([(
            "trace/merge_log.yaml".into(),
            FileSnapshot {
                bytes: history.as_bytes().to_vec(),
                existed: true,
                permissions: None,
                digest: digest(history.as_bytes()),
            },
        )]);
        let working = WorkingArtifact::new(ArtifactSnapshot {
            root: PathBuf::from("/virtual/source"),
            files,
            identity_paths: BTreeSet::new(),
        });
        assert_eq!(
            working.allocate_id('N', &[], None).unwrap_err().code,
            "write.merge_history"
        );
    }
    #[test]
    fn strings_roundtrip_crlf_and_trailing_newlines() {
        for input in [
            "é\r\n你好\n\n",
            "  x  ",
            "",
            "one\n",
            "  first\nsecond\n\n",
            "\n\n",
        ] {
            let rendered = format!("{}\n", render_yaml(&Value::String(input.into()), 0, "\n"));
            let parsed = YamlDocument::parse(&rendered).unwrap();
            assert_eq!(parsed.root.scalar(), Some(input));
        }
    }
    #[test]
    fn selected_field_updates_keep_inline_and_surrounding_comments() {
        let original = "flag: false # caller explanation\r\n# surrounding context\r\nunknown: {nested: kept}\r\n";
        let snapshot = ArtifactSnapshot {
            root: PathBuf::from("/virtual/source"),
            files: BTreeMap::from([(
                "doc.yaml".into(),
                FileSnapshot {
                    bytes: original.as_bytes().to_vec(),
                    existed: true,
                    permissions: None,
                    digest: digest(original.as_bytes()),
                },
            )]),
            identity_paths: BTreeSet::new(),
        };
        let mut working = WorkingArtifact::new(snapshot);
        working
            .replace_yaml_field("doc.yaml", &[], "flag", &Value::Bool(true))
            .unwrap();
        assert_eq!(
            working.text("doc.yaml").unwrap(),
            "flag: true # caller explanation\r\n# surrounding context\r\nunknown: {nested: kept}\r\n"
        );
    }
    #[test]
    fn rejected_path_escape() {
        for path in ["../PAPER.md", "/tmp/x", "logic/../trace/x", "logic\\x"] {
            assert!(safe_relative(path).is_err());
        }
    }
}
