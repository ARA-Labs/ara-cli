//! Thin CLI authoring adapters; library planners own policy and transactions.
use crate::agent::ReadOptions;
use crate::output::AgentError;
use ara_core::write::{self, ApplyMode, EntrySelector, Fields, WriteOperation};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

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
    Set(LogicSetArgs),
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
