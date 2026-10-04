use crate::write::{WorkingArtifact, WriteError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitMergeProvenance {
    pub repo_relative_root: String,
    pub head: String,
    pub theirs: String,
    pub base: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeOptions {
    pub source_key: String,
    pub label: String,
    pub time: String,
    #[serde(default)]
    pub git: Option<GitMergeProvenance>,
    #[serde(default)]
    pub predecessor: Option<String>,
    /// This destination's own stable source key (`--self-key`). It is never
    /// inferred; the first explicit use is recorded as `self_identity`.
    #[serde(default)]
    pub self_key: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportMapping {
    pub source_key: String,
    pub original: String,
    pub target: String,
    pub layer: String,
    pub path: String,
}
#[derive(Debug, Clone)]
pub(crate) struct EntryIdentity {
    pub address: String,
    pub layer: String,
    pub path: String,
    pub numeric: Option<char>,
    pub session: bool,
    pub heading: Vec<(String, Option<String>)>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct IdentityMap {
    source: BTreeMap<String, String>,
    pub references: BTreeMap<String, String>,
    pub local: BTreeMap<String, String>,
    pub tokens: BTreeMap<String, Option<String>>,
    pub concepts: BTreeMap<String, String>,
}
impl IdentityMap {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn source_target<'a>(&'a self, address: &'a str) -> &'a str {
        let target = self.source.get(address).map_or(address, String::as_str);
        self.local.get(target).map_or(target, String::as_str)
    }
}
impl From<BTreeMap<String, String>> for IdentityMap {
    fn from(source: BTreeMap<String, String>) -> Self {
        Self {
            source,
            references: BTreeMap::new(),
            local: BTreeMap::new(),
            tokens: BTreeMap::new(),
            concepts: BTreeMap::new(),
        }
    }
}
impl std::ops::Deref for IdentityMap {
    type Target = BTreeMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.source
    }
}
impl std::ops::DerefMut for IdentityMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.source
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MergeValue {
    pub present: bool,
    pub bytes: Vec<u8>,
    pub fingerprint: String,
}
impl MergeValue {
    pub(crate) fn new(value: Option<&[u8]>) -> Self {
        Self {
            present: value.is_some(),
            bytes: value.unwrap_or_default().to_vec(),
            fingerprint: super::identity::value_fingerprint(value),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "adapter", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConflictLocator {
    Document,
    Yaml { keys: Vec<String>, field: String },
    Markdown { entry: String, field: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MergeConflict {
    pub id: String,
    pub kind: String,
    pub source_key: String,
    pub source_revision: String,
    pub path: String,
    pub selector: String,
    pub field: String,
    pub base: MergeValue,
    pub ours: MergeValue,
    pub theirs: MergeValue,
    pub allowed: Vec<String>,
    pub locator: ConflictLocator,
    #[serde(default)]
    pub predecessor: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RewriteFact {
    pub path: String,
    pub selector: String,
    pub old: String,
    pub new: String,
    pub confidence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeReport {
    pub format: String,
    pub source_key: String,
    pub source_revision: String,
    pub imports: Vec<ImportMapping>,
    pub renamed: Vec<ImportMapping>,
    pub rewritten: Vec<RewriteFact>,
    pub needs_review: Vec<RewriteFact>,
    pub duplicate_candidates: Vec<serde_json::Value>,
    pub conflicts: Vec<MergeConflict>,
    pub logic_conflicts: Vec<MergeConflict>,
    pub unresolved_count: usize,
    pub changed_paths: Vec<String>,
    pub committed: bool,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitMergeProvenance>,
}
impl MergeReport {
    pub(crate) fn new(options: &MergeOptions, revision: String) -> Self {
        Self {
            format: "ara.merge/v1".into(),
            source_key: options.source_key.clone(),
            source_revision: revision,
            imports: vec![],
            renamed: vec![],
            rewritten: vec![],
            needs_review: vec![],
            duplicate_candidates: vec![],
            conflicts: vec![],
            logic_conflicts: vec![],
            unresolved_count: 0,
            changed_paths: vec![],
            committed: false,
            dry_run: false,
            git: options.git.clone(),
        }
    }
    pub fn exit_code(&self) -> u8 {
        u8::from(self.unresolved_count != 0)
    }
}
pub struct MergePlan {
    pub working: WorkingArtifact,
    pub report: MergeReport,
    pub validation: crate::write::source::ValidatedArtifact,
}
#[derive(Debug, Clone, Serialize)]
pub struct MergeError {
    pub code: String,
    pub message: String,
    pub evidence: Vec<MergeConflict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip)]
    pub exit: u8,
}
impl MergeError {
    pub fn content(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            evidence: vec![],
            field: None,
            exit: 1,
        }
    }
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.field = Some(path.into());
        self
    }
    pub fn exit_code(&self) -> u8 {
        self.exit
    }
}
impl From<WriteError> for MergeError {
    fn from(error: WriteError) -> Self {
        Self {
            exit: error.exit_code(),
            code: error.code,
            message: error.message,
            evidence: vec![],
            field: error.field,
        }
    }
}
impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for MergeError {}

// Keep the complete three-source/audit context explicit at this internal boundary.
#[allow(clippy::too_many_arguments)]
pub(crate) fn conflict(
    report: &mut MergeReport,
    path: &str,
    selector: &str,
    field: &str,
    kind: &str,
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
    locator: ConflictLocator,
) -> MergeConflict {
    let ours_only = matches!(
        kind,
        "opaque_file"
            | "external_read_only"
            | "unregistered_knowledge_path"
            | "unsupported_document_deletion"
            | "unsupported_paper_field"
    ) || (path == "PAPER.md" && matches!(locator, ConflictLocator::Document));
    let mut item = MergeConflict {
        id: String::new(),
        kind: kind.into(),
        source_key: report.source_key.clone(),
        source_revision: report.source_revision.clone(),
        path: path.into(),
        selector: selector.into(),
        field: field.into(),
        base: MergeValue::new(base),
        ours: MergeValue::new(ours),
        theirs: MergeValue::new(theirs),
        allowed: if kind.starts_with("protected") {
            vec![]
        } else if ours_only {
            vec!["ours".into()]
        } else {
            vec!["ours".into(), "theirs".into(), "base".into()]
        },
        locator,
        predecessor: None,
    };
    item.id = format!(
        "MC{}",
        super::identity::digest(&serde_json::to_vec(&item).expect("serializable conflict"))
    );
    report.conflicts.push(item.clone());
    item
}
pub(crate) fn reject_protected(report: &MergeReport) -> MergeError {
    let mut error = MergeError::content(
        "merge.protected_content",
        "protected history changed or was deleted; the entire merge is rejected",
    );
    error.evidence = report.conflicts.clone();
    error
}
