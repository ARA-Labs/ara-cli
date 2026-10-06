//! Thin CLI authoring adapters; library planners own policy and transactions.
use crate::agent::ReadOptions;
use crate::output::AgentError;
use ara_core::write::{self, ApplyMode, EntrySelector, Fields, WriteOperation};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, clap::Args)]
pub struct AddArgs {
    #[command(subcommand)]
    pub command: AddCommand,
}
#[derive(Debug, clap::Subcommand)]
pub enum AddCommand {
    Node(NodeArgs),
    Edge(EdgeArgs),
}
#[derive(Debug, clap::Args)]
pub struct NodeArgs {
    #[arg(long = "type")]
    pub kind: String,
    #[arg(long)]
    pub parent: String,
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long = "set")]
    pub set: Vec<String>,
    #[arg(long = "depends-on")]
    pub depends_on: Vec<String>,
    #[arg(long)]
    pub provenance: Option<String>,
    #[arg(long)]
    pub no_duplicate_check: bool,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct EdgeArgs {
    pub node: String,
    #[arg(long = "depends-on")]
    pub depends_on: String,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct EditArgs {
    pub id: Option<String>,
    #[arg(long)]
    pub document: Option<String>,
    #[arg(long)]
    pub heading: Vec<String>,
    #[arg(long)]
    pub entry: Option<String>,
    #[arg(long = "set", required = true)]
    pub set: Vec<String>,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct LogicArgs {
    #[command(subcommand)]
    pub command: LogicCommand,
}
#[derive(Debug, clap::Subcommand)]
pub enum LogicCommand {
    Add(LogicAddArgs),
    Set(LogicSetArgs),
}
#[derive(Debug, clap::Args)]
pub struct LogicAddArgs {
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long = "set", required = true)]
    pub set: Vec<String>,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct LogicSetArgs {
    pub id: String,
    #[arg(long = "set", required = true)]
    pub set: Vec<String>,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct StageArgs {
    #[arg(long)]
    pub content: String,
    #[arg(long = "potential-type")]
    pub potential_type: String,
    #[arg(long)]
    pub context: Option<String>,
    #[arg(long)]
    pub provenance: String,
    #[arg(long)]
    pub timestamp: Option<String>,
    #[arg(long = "bound-to")]
    pub bound_to: Vec<String>,
    #[arg(long)]
    pub id: Option<String>,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct PromoteArgs {
    pub observation: String,
    #[arg(long)]
    pub to: String,
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub signal: String,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long = "set")]
    pub set: Vec<String>,
    #[arg(long)]
    pub document: Option<String>,
    #[arg(long)]
    pub heading: Vec<String>,
    #[arg(long)]
    pub content: Option<String>,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}
#[derive(Debug, clap::Subcommand)]
pub enum SessionCommand {
    Start(SessionStartArgs),
    Log(SessionLogArgs),
}
#[derive(Debug, clap::Args)]
pub struct SessionStartArgs {
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long)]
    pub started: Option<String>,
    #[arg(long)]
    pub summary: String,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct SessionLogArgs {
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub record: Option<String>,
    #[arg(long)]
    pub node: Vec<String>,
    #[arg(long)]
    pub timestamp: Option<String>,
    #[arg(long)]
    pub summary: Option<String>,
    #[arg(long = "type")]
    pub kind: Option<String>,
    #[arg(long)]
    pub provenance: Option<String>,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct LinkArgs {
    pub node: String,
    #[arg(long = "same-as")]
    pub same_as: String,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct ApplyArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub no_duplicate_check: bool,
    #[command(flatten)]
    pub output: ReadOptions,
}

pub fn convert_error(error: write::WriteError) -> AgentError {
    let details = serde_json::to_value(&error).ok().map(Box::new);
    AgentError {
        exit: error.exit_code(),
        code: error.code,
        message: error.message,
        id: None,
        line: error.line,
        details,
        summary: None,
    }
}
fn read_input(value: &str, used_stdin: &mut bool) -> Result<String, AgentError> {
    if let Some(literal) = value.strip_prefix("@@") {
        return Ok(format!("@{literal}"));
    }
    if value == "@-" {
        if *used_stdin {
            return Err(AgentError::semantic(
                "duplicate_stdin",
                "At most one field may consume stdin",
            ));
        }
        *used_stdin = true;
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| AgentError::io(e.to_string()))?;
        return Ok(text);
    }
    if let Some(path) = value.strip_prefix('@') {
        return std::fs::read_to_string(path).map_err(|e| AgentError::io(format!("{path}: {e}")));
    }
    Ok(value.into())
}
pub fn resolve_fields(assignments: &[String]) -> Result<Fields, AgentError> {
    resolve_fields_with_stdin(assignments, &mut false)
}
fn resolve_fields_with_stdin(
    assignments: &[String],
    stdin: &mut bool,
) -> Result<Fields, AgentError> {
    let mut fields = BTreeMap::new();
    let mut canonical = BTreeSet::new();
    for assignment in assignments {
        let (key, input) = assignment.split_once('=').ok_or_else(|| {
            AgentError::semantic(
                "invalid_assignment",
                format!("Expected key=value: `{assignment}`"),
            )
        })?;
        let normalized = key.to_ascii_lowercase().replace([' ', '-'], "_");
        let normalized = if normalized == "falsification_criteria" {
            "falsification".to_owned()
        } else {
            normalized
        };
        if key.is_empty() || !canonical.insert(normalized.clone()) {
            return Err(AgentError::semantic(
                "duplicate_field",
                format!("Repeated or empty canonical field `{key}`"),
            ));
        }
        let input = read_input(input, stdin)?;
        let value = if matches!(
            normalized.as_str(),
            "evidence"
                | "code_ref"
                | "sources"
                | "proof"
                | "tags"
                | "appears_in"
                | "related"
                | "verifies"
                | "run"
                | "procedure"
                | "metrics"
                | "expected_outcome"
                | "baselines"
                | "claims_affected"
                | "adopted_elements"
        ) {
            if input.trim_start().starts_with('[') {
                let value: Value = serde_json::from_str(&input).map_err(|error| {
                    AgentError::semantic(
                        "invalid_field_type",
                        format!("{key}: invalid JSON sequence: {error}"),
                    )
                })?;
                if !value.is_array() {
                    return Err(AgentError::semantic(
                        "invalid_field_type",
                        format!("{key}: expected JSON array"),
                    ));
                }
                value
            } else {
                Value::String(input)
            }
        } else if matches!(
            normalized.as_str(),
            "dependencies"
                | "also_depends_on"
                | "same_as"
                | "source_refs"
                | "bound_to"
                | "alternatives"
                | "artifacts"
                | "concepts"
        ) {
            let value: Value = serde_json::from_str(&input).map_err(|e| {
                AgentError::semantic(
                    "invalid_field_type",
                    format!("{key}: expected JSON array: {e}"),
                )
            })?;
            if !value.is_array() {
                return Err(AgentError::semantic(
                    "invalid_field_type",
                    format!("{key}: expected JSON array"),
                ));
            }
            value
        } else if matches!(normalized.as_str(), "promoted" | "stale" | "isolated") {
            let value: bool = input.parse().map_err(|_| {
                AgentError::semantic(
                    "invalid_field_type",
                    format!("{key}: expected true or false"),
                )
            })?;
            json!(value)
        } else {
            Value::String(input)
        };
        fields.insert(key.into(), value);
    }
    Ok(fields)
}
fn execute(
    root: &Path,
    operations: &[WriteOperation],
    mode: ApplyMode,
    format: &str,
    no_duplicate_check: bool,
) -> Result<Value, AgentError> {
    let scoring = !no_duplicate_check
        && std::env::var("ARA_NO_DUPLICATE_CHECK").as_deref() != Ok("1")
        && operations.iter().any(|op| match op {
            WriteOperation::NodeAdd { .. } => true,
            WriteOperation::ObservationPromote { to, .. } => to == "dead_end",
            _ => false,
        });
    let (report, duplicates) =
        write::execute_with_observer(root, operations, mode, |working, _| {
            if scoring {
                duplicate_candidates(working)
            } else {
                Ok(Vec::new())
            }
        })
        .map_err(convert_error)?;
    let mut value = serde_json::to_value(&report).expect("write report serialization");
    let object = value.as_object_mut().unwrap();
    object.insert("format".into(), json!(format));
    if report.operations.len() == 1 {
        let operation = &report.operations[0];
        if let Some(id) = &operation.id {
            object.insert("id".into(), json!(id));
        }
        if let Some(turn) = operation.turn {
            object.insert("turn".into(), json!(turn));
        }
        object.insert("no_op".into(), json!(operation.no_op));
    }
    let (candidates, advisories) = match duplicates {
        Ok(candidates) => (candidates, Vec::new()),
        Err(error) => (
            Vec::new(),
            vec![format!("duplicate_advisory_unavailable: {error}")],
        ),
    };
    object.insert("duplicate_candidates".into(), json!(candidates));
    if !advisories.is_empty() {
        object.insert("advisories".into(), json!(advisories));
    }
    Ok(value)
}
pub fn duplicate_candidates(
    working: &write::WorkingArtifact,
) -> Result<Vec<crate::search::DuplicatePair>, AgentError> {
    let before = if let Some(file) = working.base.files.get("trace/exploration_tree.yaml") {
        let tree =
            std::str::from_utf8(&file.bytes).map_err(|error| AgentError::io(error.to_string()))?;
        Some(
            ara_core::parse_sources(tree, None)
                .map_err(|report| {
                    AgentError::semantic("duplicate_advisory_unavailable", report.to_string())
                })?
                .0,
        )
    } else {
        None
    };
    let after = ara_core::parse_sources(
        working
            .text("trace/exploration_tree.yaml")
            .map_err(convert_error)?,
        None,
    )
    .map_err(|report| AgentError::semantic("duplicate_advisory_unavailable", report.to_string()))?
    .0;
    duplicate_candidates_from_manifests(before.as_ref(), &after)
}

pub(crate) fn duplicate_candidates_from_manifests(
    before: Option<&ara_core::Manifest>,
    after: &ara_core::Manifest,
) -> Result<Vec<crate::search::DuplicatePair>, AgentError> {
    let old_ids = before
        .into_iter()
        .flat_map(|manifest| manifest.nodes.iter().map(|node| node.id.as_str()))
        .collect::<BTreeSet<_>>();
    let convert = |node: &ara_core::Node| -> Result<crate::search::DuplicateText, AgentError> {
        Ok(crate::search::DuplicateText {
            id: node.id.to_string(),
            title: node.label.clone().unwrap_or_default(),
            body: crate::search::node_duplicate_text(node).map_err(|message| {
                AgentError::semantic("duplicate_advisory_unavailable", message)
            })?,
        })
    };
    let existing = before
        .into_iter()
        .flat_map(|manifest| &manifest.nodes)
        .map(convert)
        .collect::<Result<Vec<_>, _>>()?;
    let added = after
        .nodes
        .iter()
        .filter(|node| !old_ids.contains(node.id.as_str()))
        .map(convert)
        .collect::<Result<Vec<_>, _>>()?;
    crate::search::duplicate_pairs(
        &existing,
        &added,
        crate::search::DEFAULT_DUPLICATE_THRESHOLD,
        crate::search::DEFAULT_DUPLICATE_LIMIT,
    )
    .map_err(|error| AgentError::semantic("duplicate_advisory_unavailable", format!("{error:?}")))
}
pub fn add(root: &Path, args: &AddArgs) -> Result<Value, AgentError> {
    let (operation, no_check) = match &args.command {
        AddCommand::Node(args) => {
            let mut fields = resolve_fields(&args.set)?;
            if let Some(provenance) = &args.provenance {
                if fields
                    .keys()
                    .any(|key| key.eq_ignore_ascii_case("provenance"))
                {
                    return Err(AgentError::semantic(
                        "duplicate_field",
                        "provenance supplied twice",
                    ));
                }
                fields.insert("provenance".into(), json!(provenance));
            }
            (
                WriteOperation::NodeAdd {
                    id: args.id.clone(),
                    kind: args.kind.clone(),
                    parent: args.parent.clone(),
                    title: args.title.clone(),
                    fields,
                    depends_on: args.depends_on.clone(),
                },
                args.no_duplicate_check,
            )
        }
        AddCommand::Edge(args) => (
            WriteOperation::EdgeAdd {
                node: args.node.clone(),
                depends_on: args.depends_on.clone(),
            },
            true,
        ),
    };
    execute(
        root,
        &[operation],
        ApplyMode::Commit,
        "ara.add/v1",
        no_check,
    )
}
pub fn edit(root: &Path, args: &EditArgs) -> Result<Value, AgentError> {
    let target = match (&args.id, &args.document) {
        (Some(id), None) if args.heading.is_empty() && args.entry.is_none() => {
            EntrySelector::Id { id: id.clone() }
        }
        (None, Some(document)) => EntrySelector::Document {
            document: document.clone(),
            heading: args.heading.clone(),
            entry: args.entry.clone(),
        },
        _ => {
            return Err(AgentError::semantic(
                "invalid_selector",
                "Choose one ID or document selector",
            ));
        }
    };
    execute(
        root,
        &[WriteOperation::EntryEdit {
            target,
            set: resolve_fields(&args.set)?,
        }],
        ApplyMode::Commit,
        "ara.edit/v1",
        true,
    )
}
pub fn logic(root: &Path, args: &LogicArgs, heuristic: bool) -> Result<Value, AgentError> {
    let operation = match &args.command {
        LogicCommand::Add(args) => {
            let fields = resolve_fields(&args.set)?;
            if heuristic {
                WriteOperation::HeuristicAdd {
                    id: args.id.clone(),
                    title: args.title.clone(),
                    fields,
                }
            } else {
                WriteOperation::ClaimAdd {
                    id: args.id.clone(),
                    title: args.title.clone(),
                    fields,
                }
            }
        }
        LogicCommand::Set(args) => WriteOperation::EntryEdit {
            target: EntrySelector::Id {
                id: args.id.clone(),
            },
            set: resolve_fields(&args.set)?,
        },
    };
    execute(
        root,
        &[operation],
        ApplyMode::Commit,
        if heuristic {
            "ara.heuristic/v1"
        } else {
            "ara.claim/v1"
        },
        true,
    )
}
pub fn stage(root: &Path, args: &StageArgs) -> Result<Value, AgentError> {
    let mut stdin = false;
    let operation = WriteOperation::ObservationStage {
        id: args.id.clone(),
        content: read_input(&args.content, &mut stdin)?,
        potential_type: args.potential_type.clone(),
        context: args
            .context
            .as_deref()
            .map(|s| read_input(s, &mut stdin))
            .transpose()?,
        provenance: args.provenance.clone(),
        timestamp: args.timestamp.clone(),
        bound_to: args.bound_to.clone(),
    };
    execute(root, &[operation], ApplyMode::Commit, "ara.stage/v1", true)
}
pub fn promote(root: &Path, args: &PromoteArgs) -> Result<Value, AgentError> {
    let mut stdin = false;
    let operation = WriteOperation::ObservationPromote {
        observation: args.observation.clone(),
        to: args.to.clone(),
        id: args.id.clone(),
        title: args.title.clone(),
        fields: resolve_fields_with_stdin(&args.set, &mut stdin)?,
        signal: args.signal.clone(),
        target: args
            .document
            .as_ref()
            .map(|document| EntrySelector::Document {
                document: document.clone(),
                heading: args.heading.clone(),
                entry: None,
            }),
        content: args
            .content
            .as_deref()
            .map(|s| read_input(s, &mut stdin))
            .transpose()?,
    };
    execute(
        root,
        &[operation],
        ApplyMode::Commit,
        "ara.promote/v1",
        true,
    )
}
pub fn session(root: &Path, args: &SessionArgs) -> Result<Value, AgentError> {
    let operation = match &args.command {
        // Omitted dates, timestamps and sessions are resolved by the writer
        // under its lock from one captured clock value, never from a pre-read.
        SessionCommand::Start(args) => WriteOperation::SessionStart {
            id: None,
            date: args.date.clone(),
            started: args.started.clone(),
            summary: args.summary.clone(),
        },
        SessionCommand::Log(args) => {
            // The node lookup below reads event text only; it never chooses
            // the session, turn or timestamp.
            let artifact = crate::agent::Artifact::load_valid(root)?;
            let mut record = if let Some(input) = &args.record {
                if !args.node.is_empty() {
                    return Err(AgentError::semantic(
                        "conflicting_inputs",
                        "--record and --node are mutually exclusive",
                    ));
                }
                let mut stdin = false;
                let text = read_input(input, &mut stdin)?;
                let record: Value = serde_json::from_str(&text)
                    .map_err(|e| AgentError::semantic("invalid_record", e.to_string()))?;
                if !record.is_object() {
                    return Err(AgentError::semantic(
                        "invalid_record",
                        "Turn record must be a JSON object",
                    ));
                }
                record
            } else {
                if args.node.is_empty() {
                    return Err(AgentError::semantic(
                        "missing_record",
                        "Supply --record or --node",
                    ));
                }
                let mut events = Vec::new();
                for id in &args.node {
                    let node = artifact
                        .manifest
                        .nodes
                        .iter()
                        .find(|n| n.id.as_str() == id)
                        .ok_or_else(|| AgentError::unknown(id))?;
                    let summary = args
                        .summary
                        .as_deref()
                        .or(node.label.as_deref())
                        .ok_or_else(|| {
                            AgentError::semantic(
                                "missing_summary",
                                "Node event requires --summary or an existing title",
                            )
                        })?;
                    let provenance = args
                        .provenance
                        .as_deref()
                        .or(node.provenance.as_deref())
                        .ok_or_else(|| {
                            AgentError::semantic(
                                "missing_provenance",
                                "Node event requires provenance",
                            )
                        })?;
                    events.push(json!({"type":args.kind.as_deref().unwrap_or(crate::agent::node_kind(&node.kind)),"id":id,"routing":"direct","provenance":provenance,"summary":summary}));
                }
                json!({"events":events})
            };
            let object = record.as_object_mut().unwrap();
            object.insert("op".into(), json!("session.log"));
            if let Some(session) = &args.session {
                object.insert("session".into(), json!(session));
            }
            if let Some(timestamp) = &args.timestamp {
                object.insert("timestamp".into(), json!(timestamp));
            }
            if let Some(summary) = &args.summary {
                object.insert("summary".into(), json!(summary));
            }
            serde_json::from_value(record)
                .map_err(|e| AgentError::semantic("invalid_record", e.to_string()))?
        }
    };
    execute(
        root,
        &[operation],
        ApplyMode::Commit,
        "ara.session/v1",
        true,
    )
}
pub fn link(root: &Path, args: &LinkArgs) -> Result<Value, AgentError> {
    execute(
        root,
        &[WriteOperation::NodeLinkSameAs {
            node: args.node.clone(),
            same_as: args.same_as.clone(),
        }],
        ApplyMode::Commit,
        "ara.link/v1",
        true,
    )
}
fn read_batch(args: &ApplyArgs) -> Result<(Vec<WriteOperation>, Vec<usize>), AgentError> {
    let bytes = if args.file == Path::new("-") {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|e| AgentError::io(e.to_string()))?;
        bytes
    } else {
        std::fs::read(&args.file).map_err(|e| AgentError::io(e.to_string()))?
    };
    Ok(write::batch::parse_batch_located(&bytes)
        .map_err(convert_error)?
        .into_iter()
        .map(|(line, operation)| (operation, line))
        .unzip())
}
fn physical_error(mut error: AgentError, lines: &[usize]) -> AgentError {
    if let Some(line) = error
        .line
        .and_then(|line| line.checked_sub(1))
        .and_then(|index| lines.get(index))
        .copied()
    {
        error.line = Some(line);
        if let Some(details) = error.details.as_deref_mut().and_then(Value::as_object_mut) {
            details.insert("line".into(), json!(line));
        }
    }
    // A two-input conflict names its other operation with the same numbering.
    if let Some(details) = error.details.as_deref_mut().and_then(Value::as_object_mut)
        && let Some(line) = details
            .get("related_line")
            .and_then(Value::as_u64)
            .and_then(|line| (line as usize).checked_sub(1))
            .and_then(|index| lines.get(index))
            .copied()
    {
        details.insert("related_line".into(), json!(line));
    }
    error
}
pub fn apply(root: &Path, args: &ApplyArgs) -> Result<Value, AgentError> {
    let (operations, lines) = read_batch(args)?;
    execute(
        root,
        &operations,
        if args.dry_run {
            ApplyMode::DryRun
        } else {
            ApplyMode::Commit
        },
        "ara.apply/v1",
        args.no_duplicate_check,
    )
    .map_err(|error| physical_error(error, &lines))
}
pub fn apply_discover(explicit: Option<&Path>, args: &ApplyArgs) -> Result<Value, AgentError> {
    let (operations, lines) = read_batch(args)?;
    let initialization = operations
        .iter()
        .any(|operation| matches!(operation, WriteOperation::ArtifactInit { .. }));
    let root = if initialization {
        let root = explicit.ok_or_else(|| {
            AgentError::setup(
                "explicit_root_required",
                "artifact.init requires explicit -C for a new or empty root",
            )
        })?;
        std::env::current_dir()
            .map_err(|e| AgentError::io(e.to_string()))?
            .join(root)
    } else if args.dry_run {
        crate::context::discover(explicit)?
    } else {
        crate::context::discover_writer(explicit)?
    };
    execute(
        &root,
        &operations,
        if args.dry_run {
            ApplyMode::DryRun
        } else {
            ApplyMode::Commit
        },
        "ara.apply/v1",
        args.no_duplicate_check,
    )
    .map_err(|error| physical_error(error, &lines))
}
