//! Strictly typed append records; earlier records remain untouched source bytes.
use serde::Deserialize;
use serde_json::{Value, json};

use super::positions::PathPart;
use super::{OperationResult, WorkingArtifact, WriteError, WriteOperation};

pub const REASONING: &str = "trace/pm_reasoning_log.yaml";
pub const TASTE: &str = "trace/taste_log.yaml";

fn invalid(field: &str, message: impl Into<String>) -> WriteError {
    WriteError::semantic("write.record", message).at(field)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reasoning {
    turn: String,
    notes: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Taste {
    #[serde(default)]
    id: Option<String>,
    timestamp: String,
    target: String,
    tag: String,
    object: String,
    comment: String,
}

fn append(working: &mut WorkingArtifact, document: &str, record: &Value) -> Result<(), WriteError> {
    working.ensure_yaml(document, "entries: []\n")?;
    working.append_yaml(document, &[PathPart::from("entries")], record)
}

pub fn plan(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    let WriteOperation::RecordAppend {
        id,
        document,
        record,
    } = operation
    else {
        return Err(invalid("op", "not a record append operation"));
    };
    match document.as_str() {
        REASONING => {
            if id.is_some() {
                return Err(invalid("id", "reasoning records have no allocated ID"));
            }
            let typed =
                Reasoning::deserialize(record).map_err(|e| invalid("record", e.to_string()))?;
            super::sessions::validate_turn_reference(&typed.turn)?;
            // Concrete turn references may be supplied before their session.log in a batch.
            let _ = typed.notes;
            append(working, document, record)?;
            Ok(OperationResult::new("record.append", None))
        }
        TASTE => {
            let mut record = record.clone();
            if let Some(object) = record.as_object_mut()
                && !object.contains_key("timestamp")
            {
                object.insert("timestamp".into(), json!(working.clock_time()?));
            }
            let record = &record;
            let typed = Taste::deserialize(record).map_err(|e| invalid("record", e.to_string()))?;
            super::sessions::validate_timestamp(&typed.timestamp)?;
            super::sessions::validate_id(&typed.target, "N")?;
            if !["endorse", "uncertain", "reject"].contains(&typed.tag.as_str()) {
                return Err(invalid("record.tag", "unsupported taste attitude"));
            }
            if !["claim", "evidence", "framing", "priority"].contains(&typed.object.as_str()) {
                return Err(invalid("record.object", "unsupported taste object"));
            }
            if super::node::node_kind(working, &typed.target)?.as_deref() == Some("question") {
                return Err(invalid(
                    "record.target",
                    "taste targets must be non-question trace nodes",
                ));
            }
            if id.is_some() && typed.id.is_some() && id != &typed.id {
                return Err(invalid("id", "record ID differs from operation ID"));
            }
            let mut ids = Vec::new();
            if working.exists(TASTE) {
                let doc = working.yaml(TASTE)?;
                for entry in doc
                    .root
                    .get("entries")?
                    .ok_or_else(|| invalid("entries", "missing taste entries sequence"))?
                    .sequence()?
                {
                    let existing = entry
                        .get("id")?
                        .and_then(|node| node.scalar())
                        .ok_or_else(|| invalid("entries.id", "taste ID is required"))?;
                    ids.push(existing.to_owned());
                }
            }
            let assigned = working.allocate_id('T', &ids, id.as_deref().or(typed.id.as_deref()))?;
            let mut value = record.clone();
            value
                .as_object_mut()
                .ok_or_else(|| invalid("record", "record must be an object"))?
                .insert("id".into(), json!(assigned));
            let _ = typed.comment;
            append(working, document, &value)?;
            Ok(OperationResult::new("record.append", Some(assigned)))
        }
        _ => Err(invalid(
            "document",
            "record append is limited to reasoning and taste logs",
        )),
    }
}

/// Internal-only complete before/after archive. Missing keys stay missing; null,
/// empty and absent values are distinct and no explanatory research is invented.
pub fn append_archive(
    working: &mut WorkingArtifact,
    turn: &str,
    session: &str,
    before: Value,
    after: Value,
) -> Result<(), WriteError> {
    super::sessions::validate_turn_reference(turn)?;
    super::sessions::validate_session_id(session)?;
    if !before.is_object() || !after.is_object() {
        return Err(invalid(
            "session_metadata",
            "archive endpoints must be objects",
        ));
    }
    append(
        working,
        REASONING,
        &json!({"turn":turn,"session_metadata":{"session":session,"before":before,"after":after}}),
    )
}

pub fn validate_references(working: &WorkingArtifact) -> Result<(), WriteError> {
    let mut nodes = None;
    for path in working.changed_paths() {
        if path != REASONING && path != TASTE {
            continue;
        }
        let doc = working.yaml(&path)?;
        let entries = doc
            .root
            .get("entries")?
            .ok_or_else(|| invalid("entries", "missing entries sequence"))?
            .sequence()?;
        let previous = if working.base.files.get(&path).is_some_and(|f| f.existed) {
            let text = std::str::from_utf8(&working.base.files[&path].bytes)
                .map_err(|_| invalid("document", "invalid UTF-8"))?;
            working
                .indexed_yaml(&path, text, true)?
                .root
                .get("entries")?
                .map(|n| n.sequence().map(|a| a.len()))
                .transpose()?
                .unwrap_or(0)
        } else {
            0
        };
        for entry in entries.iter().skip(previous) {
            let value = entry.to_json()?;
            if path == REASONING {
                let turn = value
                    .get("turn")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("turn", "record turn is required"))?;
                super::sessions::require_turn(working, turn)?;
            } else {
                let target = value
                    .get("target")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("target", "taste target is required"))?;
                if nodes.is_none() {
                    nodes = Some(super::node::cached_node_index(working)?);
                }
                match nodes.as_ref().expect("initialized node index").kind(target) {
                    None => {
                        return Err(invalid("target", format!("unknown taste target {target}")));
                    }
                    Some("question") => {
                        return Err(invalid(
                            "target",
                            "taste targets must be non-question trace nodes",
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
