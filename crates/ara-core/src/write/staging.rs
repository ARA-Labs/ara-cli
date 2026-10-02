//! Immutable observations with atomic final promotion and evidence-backed stale flags.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::positions::PathPart;
use super::{EntrySelector, Fields, OperationResult, WorkingArtifact, WriteError, WriteOperation};

pub const OBSERVATIONS: &str = "staging/observations.yaml";
const SIGNALS: [&str; 4] = [
    "topic-abandonment",
    "verbal-affirmation",
    "empirical-resolution",
    "artifact-commitment",
];

fn observation_value(node: &super::positions::YamlNode) -> Result<Value, WriteError> {
    let mut result = serde_json::Map::new();
    for key in [
        "id",
        "timestamp",
        "provenance",
        "content",
        "context",
        "potential_type",
        "bound_to",
        "promoted",
        "promoted_to",
        "crystallized_via",
        "stale",
        "conflict_annotations",
    ] {
        if let Some(field) = node.get(key)? {
            result.insert(key.into(), field.to_json()?);
        }
    }
    Ok(Value::Object(result))
}

fn invalid(field: &str, message: impl Into<String>) -> WriteError {
    WriteError::semantic("write.observation", message).at(field)
}

fn entries(working: &WorkingArtifact) -> Result<Vec<Value>, WriteError> {
    if !working.exists(OBSERVATIONS) {
        return Ok(Vec::new());
    }
    let doc = working.yaml(OBSERVATIONS)?;
    doc.root
        .get("observations")?
        .ok_or_else(|| invalid("observations", "missing observations sequence"))?
        .sequence()?
        .iter()
        .map(observation_value)
        .collect()
}
fn locate(working: &WorkingArtifact, id: &str) -> Result<(usize, Value), WriteError> {
    super::sessions::validate_id(id, "O")?;
    let mut found = None;
    for (index, value) in entries(working)?.into_iter().enumerate() {
        if value.get("id").and_then(Value::as_str) == Some(id) {
            if found.is_some() {
                return Err(invalid("observation", "ambiguous observation ID"));
            }
            found = Some((index, value));
        }
    }
    found.ok_or_else(|| invalid("observation", format!("unknown observation {id}")))
}
fn selector(index: usize) -> [PathPart; 2] {
    [PathPart::from("observations"), PathPart::Index(index)]
}
fn observation_ids(working: &WorkingArtifact) -> Result<Vec<String>, WriteError> {
    entries(working)?
        .iter()
        .map(|v| {
            v.get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| invalid("observations.id", "observation ID is required"))
        })
        .collect()
}

fn inherited(fields: &Fields, provenance: &str, node: bool) -> Result<Fields, WriteError> {
    let mut result = fields.clone();
    let supplied = fields
        .iter()
        .filter(|(key, _)| super::fields::canonical(key) == "provenance")
        .collect::<Vec<_>>();
    if supplied.len() > 1 {
        return Err(invalid("fields.Provenance", "duplicate provenance aliases"));
    }
    if let Some((_, value)) = supplied.first() {
        super::sessions::validate_provenance(
            value
                .as_str()
                .ok_or_else(|| invalid("fields.Provenance", "expected provenance string"))?,
        )?;
    } else {
        result.insert(
            if node { "provenance" } else { "Provenance" }.into(),
            json!(provenance),
        );
    }
    Ok(result)
}

pub fn plan(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    match operation {
        WriteOperation::ObservationStage {
            id,
            content,
            potential_type,
            context,
            provenance,
            timestamp,
            bound_to,
        } => {
            if ![
                "claim",
                "heuristic",
                "concept",
                "constraint",
                "architecture",
                "unknown",
            ]
            .contains(&potential_type.as_str())
            {
                return Err(invalid(
                    "potential_type",
                    "unsupported potential observation type",
                ));
            }
            super::sessions::validate_provenance(provenance)?;
            super::sessions::validate_timestamp(timestamp)?;
            let mut bounds = BTreeSet::new();
            for id in bound_to {
                super::sessions::validate_id(id, "N")?;
                if !bounds.insert(id) {
                    return Err(invalid("bound_to", "duplicate bound reference"));
                }
            }
            let assigned = working.allocate_id('O', &observation_ids(working)?, id.as_deref())?;
            let mut value = json!({"id":assigned,"timestamp":timestamp,"provenance":provenance,"content":content,"potential_type":potential_type,"bound_to":bound_to,"promoted":false,"promoted_to":null,"crystallized_via":null,"stale":false});
            if let Some(context) = context {
                value
                    .as_object_mut()
                    .ok_or_else(|| invalid("context", "expected object"))?
                    .insert("context".into(), json!(context));
            }
            working.ensure_yaml(OBSERVATIONS, "observations: []\n")?;
            working.append_yaml(OBSERVATIONS, &[PathPart::from("observations")], &value)?;
            Ok(OperationResult::new("observation.stage", Some(assigned)))
        }
        WriteOperation::ObservationPromote {
            observation,
            to,
            id,
            title,
            fields,
            signal,
            target,
            content,
        } => {
            if !SIGNALS.contains(&signal.as_str()) {
                return Err(invalid("signal", "unsupported crystallization signal"));
            }
            let (index, value) = locate(working, observation)?;
            if value.get("promoted").and_then(Value::as_bool) != Some(false)
                || value.get("promoted_to").is_some_and(|v| !v.is_null())
                || value.get("crystallized_via").is_some_and(|v| !v.is_null())
            {
                return Err(invalid(
                    "observation",
                    "observation is already promoted or its final tuple is inconsistent",
                ));
            }
            let provenance = value
                .get("provenance")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("provenance", "observation provenance is missing"))?;
            super::sessions::validate_provenance(provenance)?;
            let fields = inherited(fields, provenance, to == "dead_end")?;
            let (destination, assigned) = match to.as_str() {
                "claim" | "heuristic" => {
                    if target.is_some() || content.is_some() {
                        return Err(invalid(
                            "target",
                            "claim/heuristic promotion uses typed title and fields only",
                        ));
                    }
                    let operation = if to == "claim" {
                        WriteOperation::ClaimAdd {
                            id: id.clone(),
                            title: title.clone(),
                            fields,
                        }
                    } else {
                        WriteOperation::HeuristicAdd {
                            id: id.clone(),
                            title: title.clone(),
                            fields,
                        }
                    };
                    let result = super::logic::plan(working, &operation)?;
                    let assigned = result
                        .id
                        .ok_or_else(|| invalid("id", "logic planner did not allocate target"))?;
                    let path = if to == "claim" {
                        "logic/claims.md"
                    } else {
                        "logic/solution/heuristics.md"
                    };
                    (format!("{path}:{assigned}"), Some(assigned))
                }
                "concept" | "constraint" | "architecture" => {
                    if id.is_some() {
                        return Err(invalid(
                            "id",
                            "named logic sections do not have numeric IDs",
                        ));
                    }
                    let document = match to.as_str() {
                        "concept" => "logic/concepts.md",
                        "constraint" => "logic/solution/constraints.md",
                        _ => "logic/solution/architecture.md",
                    };
                    let section = if let Some(target) = target {
                        match target {
                            EntrySelector::Document {
                                document: selected,
                                heading,
                                entry,
                            } if selected == document && heading.len() == 1 && entry.is_none() => {
                                heading[0].as_str()
                            }
                            _ => {
                                return Err(invalid(
                                    "target",
                                    "promotion target must select one heading in its canonical document",
                                ));
                            }
                        }
                    } else {
                        title.as_str()
                    };
                    if section.is_empty() || section.contains(['\n', '\r', '#']) {
                        return Err(invalid(
                            "title",
                            "promotion requires one unambiguous section name",
                        ));
                    }
                    super::documents::append_entry(
                        working,
                        document,
                        section,
                        &fields,
                        content.as_deref(),
                    )?;
                    (format!("{document}#{section}"), None)
                }
                "dead_end" => {
                    if content.is_some() {
                        return Err(invalid(
                            "content",
                            "dead-end content uses typed node fields",
                        ));
                    }
                    let parent = match target {
                        None => "root".to_owned(),
                        Some(EntrySelector::Id { id }) => {
                            super::sessions::validate_id(id, "N")?;
                            id.clone()
                        }
                        _ => {
                            return Err(invalid(
                                "target",
                                "dead-end target selects its trace parent",
                            ));
                        }
                    };
                    let result = super::node::plan(
                        working,
                        &WriteOperation::NodeAdd {
                            id: id.clone(),
                            kind: "dead_end".into(),
                            parent,
                            title: title.clone(),
                            fields,
                            depends_on: Vec::new(),
                        },
                    )?;
                    let assigned = result
                        .id
                        .ok_or_else(|| invalid("id", "node planner did not allocate target"))?;
                    (format!("trace:{assigned}"), Some(assigned))
                }
                _ => return Err(invalid("to", "unsupported promotion destination")),
            };
            let path = selector(index);
            working.replace_yaml_field(OBSERVATIONS, &path, "promoted", &json!(true))?;
            working.replace_yaml_field(OBSERVATIONS, &path, "promoted_to", &json!(destination))?;
            working.replace_yaml_field(OBSERVATIONS, &path, "crystallized_via", &json!(signal))?;
            let mut result = OperationResult::new("observation.promote", assigned);
            result.target = Some(destination);
            Ok(result)
        }
        WriteOperation::ObservationMarkStale {
            observation,
            session_days,
            reason,
            audit,
        } => {
            if reason.trim().is_empty() {
                return Err(invalid(
                    "reason",
                    "stale handling requires the caller's rationale",
                ));
            }
            super::logic::revision_context(
                &audit.session,
                audit.turn,
                &audit.signal,
                &audit.provenance,
            )?;
            let (index, value) = locate(working, observation)?;
            if value.get("promoted").and_then(Value::as_bool) != Some(false) {
                return Err(invalid(
                    "observation",
                    "promoted observations cannot become stale",
                ));
            }
            if value.get("stale").and_then(Value::as_bool) == Some(true) {
                let mut result =
                    OperationResult::new("observation.mark_stale", Some(observation.clone()));
                result.no_op = true;
                return Ok(result);
            }
            let evidence = stale_evidence(working, observation, &value, session_days, audit)?;
            working.replace_yaml_field(OBSERVATIONS, &selector(index), "stale", &json!(true))?;
            let stale_intent = working.intents.len() - 1;
            let mut notes = vec![
                reason.clone(),
                serde_json::to_string(&evidence)
                    .map_err(|error| invalid("reason", error.to_string()))?,
            ];
            if let Some(note) = &audit.note {
                notes.push(note.clone());
            }
            working.intents[stale_intent].reason = format!(
                "observation.stale:{}",
                json!({"observation":observation,"session":audit.session,"turn":audit.turn,"notes":notes})
            );
            super::records::plan(
                working,
                &WriteOperation::RecordAppend {
                    id: None,
                    document: super::records::REASONING.into(),
                    record: json!({"turn":format!("{}#{}",audit.session,audit.turn),"notes":notes}),
                },
            )?;
            Ok(OperationResult::new(
                "observation.mark_stale",
                Some(observation.clone()),
            ))
        }
        _ => Err(invalid("op", "not an observation operation")),
    }
}

fn stale_evidence(
    working: &WorkingArtifact,
    observation: &str,
    value: &Value,
    session_days: &[String],
    audit: &super::RevisionContext,
) -> Result<Value, WriteError> {
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("timestamp", "observation timestamp is required"))?;
    super::sessions::validate_timestamp(timestamp)?;
    super::sessions::validate_index(working)?;
    let mut days = BTreeSet::new();
    for day in session_days {
        super::sessions::validate_date(day)?;
        if day.as_str() <= &timestamp[..10] || !days.insert(day.clone()) {
            return Err(invalid(
                "session_days",
                "evidence must contain distinct subsequent calendar days",
            ));
        }
    }
    if days.len() < 3 {
        return Err(invalid(
            "session_days",
            "stale requires at least three distinct subsequent session-days",
        ));
    }
    let latest = days
        .last()
        .ok_or_else(|| invalid("session_days", "missing session-days"))?;
    let bounds: BTreeSet<&str> = value
        .get("bound_to")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut proven = BTreeSet::new();
    let mut session_sources = Vec::new();
    let mut last_reference = timestamp[..10].to_owned();
    for path in working.paths() {
        if !path.starts_with("trace/sessions/")
            || !path.ends_with(".yaml")
            || path == super::sessions::INDEX
        {
            continue;
        }
        let doc = working.yaml(&path)?;
        let metadata = doc
            .root
            .get("session")?
            .ok_or_else(|| invalid("session_days", "session metadata is missing"))?;
        let date = metadata
            .get("date")?
            .and_then(|n| n.scalar())
            .ok_or_else(|| invalid("session_days", "session date is missing"))?;
        if date <= &timestamp[..10] || date > &audit.session[..10] {
            continue;
        }
        if metadata
            .get("turn_count")?
            .and_then(|n| n.scalar())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
            == 0
        {
            continue;
        }
        if days.contains(date) {
            proven.insert(date.to_owned());
            session_sources.push(json!({"date":date,"document":path}));
        }
        if referenced_source(&doc.root, observation, &bounds) {
            last_reference = last_reference.max(date.to_owned());
        }
    }
    if proven != days
        || proven
            .iter()
            .filter(|d| d.as_str() > last_reference.as_str())
            .count()
            < 3
    {
        return Err(invalid(
            "session_days",
            "three session-days after the observation's most recent reference are not proven",
        ));
    }
    if &audit.session[..10] < latest.as_str() {
        return Err(invalid(
            "audit",
            "audit owner cannot precede the proven session-days",
        ));
    }
    Ok(
        json!({"operation":"observation.mark_stale","observation":observation,"session_days":session_days,"last_reference":last_reference,"bound_to":bounds,"signal":audit.signal,"provenance":audit.provenance,"session_sources":session_sources,"audit":{"session":audit.session,"turn":audit.turn,"source_refs":[format!("trace/sessions/{}.yaml",audit.session)]}}),
    )
}

fn referenced_source(node: &super::positions::YamlNode, id: &str, bounds: &BTreeSet<&str>) -> bool {
    use super::positions::YamlKind;
    match &node.kind {
        YamlKind::Scalar { value, .. } => value
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token == id || bounds.contains(token)),
        YamlKind::Sequence(values) => values
            .iter()
            .any(|value| referenced_source(value, id, bounds)),
        YamlKind::Mapping(entries) => entries
            .iter()
            .any(|(_, value)| referenced_source(value, id, bounds)),
        // Anchor definitions occur in this same source tree and are visited above.
        YamlKind::Alias(_) => false,
    }
}

/// No generic entry setter can finalize or retarget an immutable observation.
pub fn edit_pointer(
    working: &mut WorkingArtifact,
    id: &str,
    set: &Fields,
) -> Result<OperationResult, WriteError> {
    let (_, value) = locate(working, id)?;
    for (field, replacement) in set {
        if field != "crystallized_via"
            || value.get("promoted").and_then(Value::as_bool) != Some(true)
            || value.get(field) != Some(replacement)
        {
            return Err(invalid(
                field,
                "promotion metadata is a final tuple; use observation.promote",
            ));
        }
    }
    let mut result = OperationResult::new("entry.edit", Some(id.to_owned()));
    result.no_op = true;
    Ok(result)
}

pub fn annotate(
    working: &mut WorkingArtifact,
    id: &str,
    kind: &str,
    references: &[String],
    comment: &str,
) -> Result<OperationResult, WriteError> {
    if kind != "conflict" || references.is_empty() {
        return Err(invalid(
            "kind",
            "observation annotations require conflict references",
        ));
    }
    if references
        .iter()
        .any(|reference| reference.is_empty() || reference.contains(['\r', '\n']))
    {
        return Err(invalid(
            "references",
            "conflict references must be nonempty single-line locators",
        ));
    }
    let (index, value) = locate(working, id)?;
    let selector = selector(index);
    let record = json!({"references":references,"comment":comment});
    if value
        .get("conflict_annotations")
        .and_then(Value::as_array)
        .is_some_and(|a| a.contains(&record))
    {
        let mut result = OperationResult::new("entry.annotate", Some(id.to_owned()));
        result.no_op = true;
        return Ok(result);
    }
    if working.yaml(OBSERVATIONS)?.root.at(&selector)?.flow {
        return Err(invalid(
            "target",
            "flow-style observations cannot receive an unambiguous native conflict marker",
        ));
    }
    if value.get("conflict_annotations").is_none() {
        working.replace_yaml_field(OBSERVATIONS, &selector, "conflict_annotations", &json!([]))?;
    }
    let mut path = selector.to_vec();
    path.push(PathPart::from("conflict_annotations"));
    working.append_yaml(OBSERVATIONS, &path, &record)?;
    let document = working.yaml(OBSERVATIONS)?;
    let node = document.root.at(&selector)?;
    let source = working.text(OBSERVATIONS)?;
    let at = super::positions::line_start(source, node.start);
    let indent = source[at..node.start]
        .bytes()
        .take_while(|c| *c == b' ')
        .count();
    let newline = super::positions::eol(source);
    let marker = format!(
        "{}# CONFLICT: see {}{newline}",
        " ".repeat(indent),
        references.join(", ")
    );
    working.edit(
        OBSERVATIONS,
        at..at,
        &marker,
        "append native observation conflict marker",
    )?;
    Ok(OperationResult::new("entry.annotate", Some(id.to_owned())))
}

pub fn validate_references(working: &WorkingArtifact) -> Result<(), WriteError> {
    if !working
        .changed_paths()
        .iter()
        .any(|path| path == OBSERVATIONS)
    {
        return Ok(());
    }
    let previous: BTreeMap<String, Value> =
        if let Some(base) = working.base.files.get(OBSERVATIONS).filter(|f| f.existed) {
            let text = std::str::from_utf8(&base.bytes)
                .map_err(|_| invalid("document", "invalid UTF-8"))?;
            let doc = working.indexed_yaml(OBSERVATIONS, text, true)?;
            doc.root
                .get("observations")?
                .ok_or_else(|| invalid("observations", "missing observations sequence"))?
                .sequence()?
                .iter()
                .map(|n| {
                    let v = observation_value(n)?;
                    let id = v
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("id", "missing observation ID"))?
                        .to_owned();
                    Ok((id, v))
                })
                .collect::<Result<_, WriteError>>()?
        } else {
            BTreeMap::new()
        };
    let imported = super::intent::imports_history(working, OBSERVATIONS)?;
    let mut stale_audits = BTreeSet::new();
    for intent in working
        .intents
        .iter()
        .filter(|intent| intent.path == OBSERVATIONS)
    {
        if let Some(encoded) = intent.reason.strip_prefix("observation.stale:") {
            let audit: Value = serde_json::from_str(encoded)
                .map_err(|error| invalid("audit", error.to_string()))?;
            let owner = (
                audit["session"]
                    .as_str()
                    .ok_or_else(|| invalid("audit", "missing session"))?
                    .to_owned(),
                audit["turn"]
                    .as_u64()
                    .ok_or_else(|| invalid("audit", "missing turn"))?,
            );
            if !working.owned_turns.contains_key(&owner) {
                return Err(invalid(
                    "audit",
                    "stale rationale requires a new batch-owned session turn",
                ));
            }
            let notes = audit
                .get("notes")
                .ok_or_else(|| invalid("audit", "missing exact stale notes"))?;
            let reasoning = working.yaml(super::records::REASONING)?;
            let turn = format!("{}#{}", owner.0, owner.1);
            if !reasoning
                .root
                .get("entries")?
                .ok_or_else(|| invalid("audit", "missing reasoning entries"))?
                .sequence()?
                .iter()
                .any(|entry| {
                    entry
                        .get("turn")
                        .ok()
                        .flatten()
                        .and_then(super::positions::YamlNode::scalar)
                        == Some(turn.as_str())
                        && entry
                            .get("notes")
                            .ok()
                            .flatten()
                            .and_then(|notes| notes.to_json().ok())
                            .as_ref()
                            == Some(notes)
                })
            {
                return Err(invalid(
                    "audit",
                    "stale flag requires its exact caller rationale and evidence in native reasoning",
                ));
            }
            let observation = audit["observation"]
                .as_str()
                .ok_or_else(|| invalid("audit", "missing observation"))?;
            let captured: Value = serde_json::from_str(
                notes[1]
                    .as_str()
                    .ok_or_else(|| invalid("audit", "missing factual stale evidence"))?,
            )
            .map_err(|error| invalid("audit", error.to_string()))?;
            let days: Vec<String> = serde_json::from_value(captured["session_days"].clone())
                .map_err(|error| invalid("audit", error.to_string()))?;
            let context = super::RevisionContext {
                session: owner.0.clone(),
                turn: owner.1,
                signal: captured["signal"]
                    .as_str()
                    .ok_or_else(|| invalid("audit", "missing signal"))?
                    .into(),
                provenance: captured["provenance"]
                    .as_str()
                    .ok_or_else(|| invalid("audit", "missing provenance"))?
                    .into(),
                note: None,
            };
            let (_, value) = locate(working, observation)?;
            if value.get("promoted").and_then(Value::as_bool) != Some(false)
                || value.get("stale").and_then(Value::as_bool) != Some(true)
            {
                return Err(invalid(
                    "stale",
                    "audited stale flag must remain true and unpromoted",
                ));
            }
            if stale_evidence(working, observation, &value, &days, &context)? != captured {
                return Err(invalid(
                    "audit",
                    "stale evidence changed after authoring; regenerate the decision against final sessions",
                ));
            }
            stale_audits.insert(observation.to_owned());
        }
    }
    let mut nodes = None;
    for value in entries(working)? {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("id", "missing observation ID"))?;
        if value.get("stale").and_then(Value::as_bool) == Some(true)
            && previous
                .get(id)
                .and_then(|value| value.get("stale"))
                .and_then(Value::as_bool)
                != Some(true)
            && !stale_audits.contains(id)
            && !imported
        {
            return Err(invalid(
                "stale",
                "new stale flags require observation.mark_stale with an owned audit turn",
            ));
        }
        if previous
            .get(id)
            .and_then(|value| value.get("stale"))
            .and_then(Value::as_bool)
            == Some(true)
            && value.get("stale").and_then(Value::as_bool) != Some(true)
        {
            return Err(invalid(
                "stale",
                "unsetting historical stale flags requires explicit audited adjudication",
            ));
        }
        let old_annotations = previous
            .get(id)
            .and_then(|v| v.get("conflict_annotations"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if let Some(annotations) = value.get("conflict_annotations").and_then(Value::as_array) {
            for annotation in annotations.iter().skip(old_annotations) {
                let references = annotation
                    .get("references")
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("references", "conflict references must be a list"))?;
                for reference in references {
                    super::sessions::require_entry_reference(
                        working,
                        reference.as_str().ok_or_else(|| {
                            invalid("references", "conflict references must be strings")
                        })?,
                    )?;
                }
            }
        }
        if !previous.contains_key(id) {
            for target in value
                .get("bound_to")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let target = target
                    .as_str()
                    .ok_or_else(|| invalid("bound_to", "expected node ID"))?;
                if nodes.is_none() {
                    nodes = Some(super::node::cached_node_index(working)?);
                }
                if nodes
                    .as_ref()
                    .expect("initialized node index")
                    .kind(target)
                    .is_none()
                {
                    return Err(invalid("bound_to", format!("unknown bound node {target}")));
                }
            }
        }
        if value.get("promoted").and_then(Value::as_bool) == Some(true)
            && previous
                .get(id)
                .and_then(|v| v.get("promoted"))
                .and_then(Value::as_bool)
                != Some(true)
        {
            let destination = value
                .get("promoted_to")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("promoted_to", "promotion target is missing"))?;
            if let Some(node) = destination.strip_prefix("trace:") {
                if nodes.is_none() {
                    nodes = Some(super::node::cached_node_index(working)?);
                }
                if nodes.as_ref().expect("initialized node index").kind(node) != Some("dead_end") {
                    return Err(invalid(
                        "promoted_to",
                        "promotion target must exist as a dead-end node",
                    ));
                }
            } else if let Some((document, section)) = destination.split_once('#') {
                if ![
                    "logic/concepts.md",
                    "logic/solution/constraints.md",
                    "logic/solution/architecture.md",
                ]
                .contains(&document)
                    || crate::markdown::sections(working.text(document)?)
                        .iter()
                        .filter(|s| s.heading == section)
                        .count()
                        != 1
                {
                    return Err(invalid(
                        "promoted_to",
                        "promotion section is missing or ambiguous",
                    ));
                }
            } else if let Some((document, target)) = destination.split_once(':') {
                let prefix = if document == "logic/claims.md" {
                    "C"
                } else if document == "logic/solution/heuristics.md" {
                    "H"
                } else {
                    return Err(invalid("promoted_to", "invalid promotion path"));
                };
                super::sessions::validate_id(target, prefix)?;
                if crate::markdown::sections(working.text(document)?)
                    .iter()
                    .filter(|s| {
                        s.heading
                            .split_once(':')
                            .map_or(s.heading, |(id, _)| id)
                            .trim()
                            == target
                    })
                    .count()
                    != 1
                {
                    return Err(invalid(
                        "promoted_to",
                        "promoted target is missing or ambiguous",
                    ));
                }
            } else {
                return Err(invalid("promoted_to", "invalid promotion locator grammar"));
            }
            if !value
                .get("crystallized_via")
                .and_then(Value::as_str)
                .is_some_and(|s| SIGNALS.contains(&s))
            {
                return Err(invalid("crystallized_via", "invalid promotion signal"));
            }
        }
    }
    Ok(())
}
