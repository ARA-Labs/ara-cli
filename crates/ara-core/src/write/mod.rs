//! Native, source-preserving authoring and guarded checkout transactions.
//! Policy permissions implement the agent-interface proposal; upstream approval is separate.

pub mod batch;
pub mod bookkeeping;
pub mod clock;
pub mod documents;
pub mod fields;
pub mod intent;
pub mod journal;
pub mod logic;
pub mod node;
pub mod positions;
mod positions_tree;
pub mod records;
pub mod sessions;
pub mod source;
pub mod staging;
pub mod transaction;

use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use source::{ArtifactSnapshot, WorkingArtifact};
use std::{collections::BTreeMap, path::Path};
pub use transaction::ArtifactLock;

pub type Fields = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum EntrySelector {
    Id {
        id: String,
    },
    Document {
        document: String,
        #[serde(default)]
        heading: Vec<String>,
        #[serde(default)]
        entry: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceEdit {
    pub target: EntrySelector,
    pub field: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionContext {
    /// Omitted only with the batch's sole summarized `session.log` anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u64>,
    pub signal: String,
    pub provenance: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum WriteOperation {
    #[serde(rename = "node.add")]
    NodeAdd {
        #[serde(default)]
        id: Option<String>,
        #[serde(rename = "type")]
        kind: String,
        parent: String,
        title: String,
        #[serde(default)]
        fields: Fields,
        #[serde(default)]
        depends_on: Vec<String>,
    },
    #[serde(rename = "edge.add")]
    EdgeAdd { node: String, depends_on: String },
    #[serde(rename = "node.link_same_as")]
    NodeLinkSameAs { node: String, same_as: String },
    #[serde(rename = "claim.add")]
    ClaimAdd {
        #[serde(default)]
        id: Option<String>,
        title: String,
        fields: Fields,
    },
    #[serde(rename = "heuristic.add")]
    HeuristicAdd {
        #[serde(default)]
        id: Option<String>,
        title: String,
        fields: Fields,
    },
    #[serde(rename = "entry.edit")]
    EntryEdit { target: EntrySelector, set: Fields },
    #[serde(rename = "observation.stage")]
    ObservationStage {
        #[serde(default)]
        id: Option<String>,
        content: String,
        potential_type: String,
        #[serde(default)]
        context: Option<String>,
        provenance: String,
        /// Omitted: the writer's captured `batch_time`.
        #[serde(default)]
        timestamp: Option<String>,
        #[serde(default)]
        bound_to: Vec<String>,
    },
    #[serde(rename = "observation.promote")]
    ObservationPromote {
        observation: String,
        to: String,
        #[serde(default)]
        id: Option<String>,
        title: String,
        #[serde(default)]
        fields: Fields,
        signal: String,
        #[serde(default)]
        target: Option<EntrySelector>,
        #[serde(default)]
        content: Option<String>,
    },
    #[serde(rename = "observation.mark_stale")]
    ObservationMarkStale {
        observation: String,
        session_days: Vec<String>,
        reason: String,
        audit: RevisionContext,
    },
    #[serde(rename = "session.start")]
    SessionStart {
        #[serde(default)]
        id: Option<String>,
        /// Omitted: the calendar date written in `started`.
        #[serde(default)]
        date: Option<String>,
        /// Omitted: the writer's captured `batch_time`.
        #[serde(default)]
        started: Option<String>,
        summary: String,
    },
    #[serde(rename = "session.log")]
    SessionLog {
        /// Omitted: select the one open session on the log's written date, or
        /// create one with `summary` when none is open on that date.
        #[serde(default)]
        session: Option<String>,
        /// Omitted: the writer's captured `batch_time`.
        #[serde(default)]
        timestamp: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        events: Vec<Value>,
        #[serde(default)]
        ai_actions: Vec<Value>,
        #[serde(default)]
        claims_touched: Vec<Value>,
        #[serde(default)]
        logic_revisions: Vec<Value>,
        #[serde(default)]
        key_context: Vec<Value>,
        #[serde(default)]
        open_threads: Option<Vec<String>>,
        #[serde(default)]
        ai_suggestions_pending: Option<Vec<String>>,
    },
    #[serde(rename = "document.create")]
    DocumentCreate { document: String, content: String },
    #[serde(rename = "document.replace")]
    DocumentReplace {
        document: String,
        #[serde(default)]
        heading: Vec<String>,
        expected: String,
        content: String,
    },
    #[serde(rename = "paper.edit")]
    PaperEdit {
        #[serde(default)]
        frontmatter: Fields,
        #[serde(default)]
        heading: Vec<String>,
        #[serde(default)]
        expected: Option<String>,
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        audit: Option<RevisionContext>,
    },
    #[serde(rename = "record.append")]
    RecordAppend {
        #[serde(default)]
        id: Option<String>,
        document: String,
        record: Value,
    },
    #[serde(rename = "logic.revise")]
    LogicRevise {
        target: EntrySelector,
        set: Fields,
        /// `session`/`turn` may be omitted only with the batch's sole
        /// summarized `session.log` anchor, which must precede this line.
        #[serde(default)]
        session: Option<String>,
        #[serde(default)]
        turn: Option<u64>,
        signal: String,
        provenance: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        expected: Option<String>,
    },
    #[serde(rename = "entry.rename")]
    EntryRename {
        target: EntrySelector,
        name: String,
        expected: String,
        #[serde(default)]
        references: Vec<ReferenceEdit>,
        #[serde(default)]
        session: Option<String>,
        #[serde(default)]
        turn: Option<u64>,
        #[serde(default)]
        signal: Option<String>,
        #[serde(default)]
        provenance: Option<String>,
    },
    #[serde(rename = "entry.remove")]
    EntryRemove {
        target: EntrySelector,
        expected: String,
        #[serde(default)]
        references: Vec<ReferenceEdit>,
        #[serde(default)]
        session: Option<String>,
        #[serde(default)]
        turn: Option<u64>,
        #[serde(default)]
        signal: Option<String>,
        #[serde(default)]
        provenance: Option<String>,
        #[serde(default)]
        redirect: Option<EntrySelector>,
    },
    #[serde(rename = "entry.taste_append")]
    EntryTasteAppend {
        target: EntrySelector,
        record: Value,
    },
    #[serde(rename = "entry.annotate")]
    EntryAnnotate {
        target: EntrySelector,
        kind: String,
        references: Vec<String>,
        comment: String,
    },
    #[serde(rename = "artifact.init")]
    ArtifactInit {
        profile: String,
        #[serde(default)]
        paper: Option<String>,
        #[serde(default)]
        documents: BTreeMap<String, String>,
        #[serde(default)]
        missing_only: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    Commit,
    DryRun,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OperationResult {
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<u64>,
    /// Owning session, reported when the writer resolved or derived it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// True when an omitted-`session` log created its session.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub session_created: bool,
    pub no_op: bool,
}
impl OperationResult {
    pub fn new(operation: &str, id: Option<String>) -> Self {
        Self {
            operation: operation.into(),
            id,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteReport {
    pub committed: bool,
    pub dry_run: bool,
    pub operations: Vec<OperationResult>,
    pub bindings: BTreeMap<String, String>,
    pub changed_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub created_directories: Vec<String>,
    pub diagnostics: Vec<crate::report::Diagnostic>,
    /// Other open sessions left untouched when the writer selected or created
    /// the owner of an omitted-`session` log. Close them deliberately.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open_sessions: Vec<String>,
}

/// The other input of a two-input conflict: its operation line (same
/// numbering as [`WriteError::line`]) and field.
#[derive(Debug, Clone, Serialize)]
pub struct RelatedLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_line: Option<usize>,
    pub related_field: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// A second input location for conflicts between two inputs.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub related: Option<Box<RelatedLocation>>,
    #[serde(skip)]
    exit: u8,
}
impl WriteError {
    pub fn semantic(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            line: None,
            field: None,
            related: None,
            exit: 1,
        }
    }
    pub fn io(message: impl Into<String>) -> Self {
        Self {
            code: "write.io".into(),
            message: message.into(),
            line: None,
            field: None,
            related: None,
            exit: 2,
        }
    }
    pub fn at(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }
    /// Name the other input location of a two-input conflict.
    pub fn related(mut self, line: usize, field: impl Into<String>) -> Self {
        self.related = Some(Box::new(RelatedLocation {
            related_line: Some(line),
            related_field: field.into(),
        }));
        self
    }
    /// Name another field of the failing operation's own line as the other
    /// location; the batch planner fills in the line.
    pub fn related_here(mut self, field: impl Into<String>) -> Self {
        self.related = Some(Box::new(RelatedLocation {
            related_line: None,
            related_field: field.into(),
        }));
        self
    }
    pub fn related_line(&self) -> Option<usize> {
        self.related.as_ref().and_then(|r| r.related_line)
    }
    pub fn related_field(&self) -> Option<&str> {
        self.related.as_ref().map(|r| r.related_field.as_str())
    }
    pub fn exit_code(&self) -> u8 {
        self.exit
    }
}
impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for WriteError {}

pub fn execute(
    root: &Path,
    operations: &[WriteOperation],
    mode: ApplyMode,
) -> Result<WriteReport, WriteError> {
    execute_with_observer(root, operations, mode, |_, _| ()).map(|(report, ())| report)
}

/// Observe the final guarded candidate while the cooperating-writer lock is
/// held. An advisory may return its own Result; it cannot veto a valid commit.
pub fn execute_with_observer<T>(
    root: &Path,
    operations: &[WriteOperation],
    mode: ApplyMode,
    observer: impl FnOnce(&WorkingArtifact, &[OperationResult]) -> T,
) -> Result<(WriteReport, T), WriteError> {
    execute_at(root, operations, mode, clock::system_utc, observer)
}

/// Acquire the cooperating-writer lock, recover any prepared transaction, and
/// only then read `clock` once. Writers outside `apply` (merge audits) use
/// this so their omitted timestamps come from the same locked boundary.
pub fn lock_and_capture(
    root: &Path,
    clock: impl FnOnce() -> Result<String, WriteError>,
) -> Result<(ArtifactLock, String), WriteError> {
    let lock = ArtifactLock::acquire(root)?;
    journal::recover(root)?;
    let batch_time = clock()?;
    clock::validate_batch_time(&batch_time)?;
    Ok((lock, batch_time))
}

/// The guarded batch entry point with an injectable clock. Commit mode reads
/// `clock` once after lock and recovery; a dry run reads one tentative value
/// without creating a lock. Every omitted timestamp/date in the batch uses
/// that single value.
pub fn execute_at<T>(
    root: &Path,
    operations: &[WriteOperation],
    mode: ApplyMode,
    clock: impl FnOnce() -> Result<String, WriteError>,
    observer: impl FnOnce(&WorkingArtifact, &[OperationResult]) -> T,
) -> Result<(WriteReport, T), WriteError> {
    let (lock, batch_time) = if mode == ApplyMode::Commit {
        let (lock, batch_time) = lock_and_capture(root, clock)?;
        (Some(lock), batch_time)
    } else {
        if journal::pending_prepared(root)? {
            return Err(WriteError::io(
                "artifact has a prepared transaction; run a commit-mode writer to recover before dry-run planning",
            ));
        }
        let batch_time = clock()?;
        clock::validate_batch_time(&batch_time)?;
        (None, batch_time)
    };
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root)?);
    working.batch_time = Some(batch_time);
    let (results, bindings) = batch::plan_batch(&mut working, operations)?;
    let diagnostics = if operations.is_empty() {
        Vec::new()
    } else {
        working.validate()?.diagnostics
    };
    working.plan_missing_directories()?;
    if !working.changed_paths().is_empty() || !working.created_dirs.is_empty() {
        working.include_operational_ignore()?;
    }
    let advisory = observer(&working, &results);
    if mode == ApplyMode::Commit
        && (!working.changed_paths().is_empty() || !working.created_dirs.is_empty())
    {
        transaction::commit(&working)?;
    }
    drop(lock);
    Ok((
        WriteReport {
            committed: mode == ApplyMode::Commit,
            dry_run: mode == ApplyMode::DryRun,
            operations: results,
            bindings,
            changed_paths: working.changed_paths(),
            created_directories: working.created_dirs.iter().cloned().collect(),
            diagnostics,
            open_sessions: working
                .owner
                .as_ref()
                .map(|owner| owner.open_sessions.clone())
                .unwrap_or_default(),
        },
        advisory,
    ))
}

pub fn plan_operation(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    match operation {
        WriteOperation::NodeAdd { .. }
        | WriteOperation::EdgeAdd { .. }
        | WriteOperation::NodeLinkSameAs { .. } => node::plan(working, operation),
        WriteOperation::ClaimAdd { .. }
        | WriteOperation::HeuristicAdd { .. }
        | WriteOperation::EntryEdit { .. }
        | WriteOperation::LogicRevise { .. }
        | WriteOperation::EntryRename { .. }
        | WriteOperation::EntryRemove { .. }
        | WriteOperation::EntryTasteAppend { .. }
        | WriteOperation::EntryAnnotate { .. } => logic::plan(working, operation),
        WriteOperation::ObservationStage { .. }
        | WriteOperation::ObservationPromote { .. }
        | WriteOperation::ObservationMarkStale { .. } => staging::plan(working, operation),
        WriteOperation::SessionStart { .. } | WriteOperation::SessionLog { .. } => {
            sessions::plan(working, operation)
        }
        WriteOperation::DocumentCreate { .. }
        | WriteOperation::DocumentReplace { .. }
        | WriteOperation::PaperEdit { .. }
        | WriteOperation::ArtifactInit { .. } => documents::plan(working, operation),
        WriteOperation::RecordAppend { .. } => records::plan(working, operation),
    }
}
