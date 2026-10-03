//! Strict JSONL decoding and one-snapshot ordered batch planning.
use super::{EntrySelector, Fields, OperationResult, WorkingArtifact, WriteError, WriteOperation};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::collections::BTreeMap;

struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("one JSON value with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    items.push(value);
                }
                Ok(UniqueValue(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!(
                            "duplicate JSON object key `{key}`"
                        )));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

pub fn parse_batch(bytes: &[u8]) -> Result<Vec<WriteOperation>, WriteError> {
    Ok(parse_batch_located(bytes)?
        .into_iter()
        .map(|(_, operation)| operation)
        .collect())
}
/// Preserve physical JSONL line numbers when a CLI needs planning errors on
/// original lines, including ignored blank lines.
pub fn parse_batch_located(bytes: &[u8]) -> Result<Vec<(usize, WriteOperation)>, WriteError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| WriteError::semantic("write.batch_encoding", "JSONL input must be UTF-8"))?;
    let mut operations = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_number = index + 1;
        let decode = (|| {
            let mut deserializer = serde_json::Deserializer::from_str(line);
            let UniqueValue(value) = serde_path_to_error::deserialize::<_, UniqueValue>(
                &mut deserializer,
            )
            .map_err(|e| {
                decode_error(
                    "write.batch_json",
                    "",
                    &e.path().to_string(),
                    &e.inner().to_string(),
                )
            })?;
            deserializer.end().map_err(|e| {
                WriteError::semantic(
                    "write.batch_json",
                    format!("one operation per JSONL line: {e}"),
                )
            })?;
            if !value.is_object() {
                return Err(WriteError::semantic(
                    "write.batch_shape",
                    "each JSONL line must be one operation object",
                ));
            }
            let tag = value
                .get("op")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            serde_path_to_error::deserialize::<_, WriteOperation>(value).map_err(|e| {
                decode_error(
                    "write.batch_operation",
                    &tag,
                    &e.path().to_string(),
                    &e.inner().to_string(),
                )
            })
        })();
        match decode {
            Ok(operation) => operations.push((line_number, operation)),
            Err(mut error) => {
                error.line = Some(line_number);
                return Err(error);
            }
        }
    }
    Ok(operations)
}
fn decode_error(code: &str, tag: &str, path: &str, message: &str) -> WriteError {
    let mut field = if path.is_empty() || path == "." {
        String::new()
    } else {
        path.to_owned()
    };
    let named = message
        .strip_prefix("duplicate JSON object key `")
        .and_then(|rest| rest.split('`').next())
        .or_else(|| {
            message
                .strip_prefix("missing field `")
                .and_then(|rest| rest.split('`').next())
        })
        .or_else(|| {
            message
                .strip_prefix("unknown field `")
                .and_then(|rest| rest.split('`').next())
        });
    if let Some(name) = named {
        if !field.is_empty() {
            field.push('.');
        }
        field.push_str(name);
    }
    if field.is_empty() {
        field = if message.contains("unknown variant") {
            "op".into()
        } else {
            "operation".into()
        };
    }
    WriteError::semantic(
        code,
        if tag.is_empty() {
            message.to_owned()
        } else {
            format!("{tag}: {message}")
        },
    )
    .at(field)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Namespace {
    Node,
    Claim,
    Heuristic,
    Observation,
    Session,
    Taste,
    Native,
    Any,
}
#[derive(Clone)]
struct Binding {
    id: String,
    kind: Namespace,
}

pub fn plan_batch(
    working: &mut WorkingArtifact,
    operations: &[WriteOperation],
) -> Result<(Vec<OperationResult>, BTreeMap<String, String>), WriteError> {
    let mut results = Vec::with_capacity(operations.len());
    let mut bindings: BTreeMap<String, Binding> = BTreeMap::new();
    for (index, operation) in operations.iter().enumerate() {
        let result = (|| {
            let mut operation = operation.clone();
            let binding = take_binding(&mut operation)?;
            if let Some((name, _)) = &binding
                && bindings.contains_key(name)
            {
                return Err(WriteError::semantic(
                    "write.binding_duplicate",
                    format!("creation binding `${name}` is already defined"),
                )
                .at("id"));
            }
            substitute(&mut operation, &bindings)?;
            let result = super::plan_operation(working, &operation)?;
            if let Some((name, kind)) = binding {
                let id = result
                    .id
                    .as_ref()
                    .or_else(|| {
                        if kind == Namespace::Native {
                            result.target.as_ref()
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| {
                        WriteError::semantic(
                            "write.binding_result",
                            "creation did not return its native identity",
                        )
                    })?
                    .clone();
                bindings.insert(name, Binding { id, kind });
            }
            Ok(result)
        })();
        match result {
            Ok(result) => results.push(result),
            Err(mut error) => {
                error.line = Some(index + 1);
                return Err(error);
            }
        }
    }
    // Revision mutations are attached only to turns explicitly authored in this
    // batch. Neither operation ordering nor a historical record grants access.
    let revisions = std::mem::take(&mut working.revisions);
    for pending in revisions {
        if !working
            .owned_turns
            .contains_key(&(pending.session.clone(), pending.turn))
        {
            return Err(WriteError::semantic("write.revision_turn","logic revision requires exactly one session.log owning the referenced new turn in this batch").at("turn"));
        }
        super::sessions::append_revision(working, &pending.session, pending.turn, &pending.record)?;
    }
    Ok((
        results,
        bindings
            .into_iter()
            .map(|(name, binding)| (format!("${name}"), binding.id))
            .collect(),
    ))
}

fn take_binding(operation: &mut WriteOperation) -> Result<Option<(String, Namespace)>, WriteError> {
    let pair = match operation {
        WriteOperation::NodeAdd { id, .. } => (id, Namespace::Node),
        WriteOperation::ClaimAdd { id, .. } => (id, Namespace::Claim),
        WriteOperation::HeuristicAdd { id, .. } => (id, Namespace::Heuristic),
        WriteOperation::ObservationStage { id, .. } => (id, Namespace::Observation),
        WriteOperation::SessionStart { id, .. } => (id, Namespace::Session),
        WriteOperation::RecordAppend { id, document, .. } if document == "trace/taste_log.yaml" => {
            (id, Namespace::Taste)
        }
        WriteOperation::ObservationPromote { id, to, .. } => (
            id,
            match to.as_str() {
                "claim" => Namespace::Claim,
                "heuristic" => Namespace::Heuristic,
                "dead_end" => Namespace::Node,
                _ => Namespace::Native,
            },
        ),
        _ => return Ok(None),
    };
    let Some(name) = pair.0.as_deref().and_then(|id| id.strip_prefix('$')) else {
        return Ok(None);
    };
    if name.is_empty()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || name.as_bytes()[0].is_ascii_digit()
    {
        return Err(WriteError::semantic("write.binding_name","binding names must begin with a letter/underscore and contain only letters, digits, underscores").at("id"));
    }
    let name = name.to_owned();
    *pair.0 = None;
    Ok(Some((name, pair.1)))
}
fn reference(
    text: &mut String,
    namespace: Namespace,
    bindings: &BTreeMap<String, Binding>,
    field: &str,
) -> Result<(), WriteError> {
    let Some(name) = text.strip_prefix('$') else {
        return Ok(());
    };
    let (name, suffix) = if namespace == Namespace::Session {
        name.split_once('#').map_or((name, None), |(name, suffix)| {
            (name, Some(suffix.to_owned()))
        })
    } else {
        (name, None)
    };
    let bound = bindings.get(name).ok_or_else(|| {
        WriteError::semantic(
            "write.binding_unknown",
            format!("unknown or forward binding `${name}`"),
        )
        .at(field)
    })?;
    if namespace != Namespace::Any && namespace != bound.kind {
        return Err(WriteError::semantic(
            "write.binding_kind",
            format!(
                "`${name}` is a {:?} binding, expected {namespace:?}",
                bound.kind
            ),
        )
        .at(field));
    }
    *text = if let Some(suffix) = suffix {
        format!("{}#{suffix}", bound.id)
    } else {
        bound.id.clone()
    };
    Ok(())
}
fn selector(
    target: &mut EntrySelector,
    bindings: &BTreeMap<String, Binding>,
    field: &str,
) -> Result<(), WriteError> {
    if let EntrySelector::Id { id } = target
        && let Some(bound) = id
            .strip_prefix('$')
            .and_then(|name| bindings.get(name))
            .filter(|bound| bound.kind == Namespace::Native)
    {
        let (document, heading) = bound.id.split_once('#').ok_or_else(|| {
            WriteError::semantic(
                "write.binding_kind",
                "native binding has no section locator",
            )
            .at(field)
        })?;
        *target = EntrySelector::Document {
            document: document.into(),
            heading: vec![heading.into()],
            entry: None,
        };
        return Ok(());
    }
    match target {
        EntrySelector::Id { id } => reference(id, Namespace::Any, bindings, field),
        EntrySelector::Document {
            entry: Some(id), ..
        } => reference(id, Namespace::Any, bindings, field),
        _ => Ok(()),
    }
}
fn refs(
    values: &mut [String],
    kind: Namespace,
    bindings: &BTreeMap<String, Binding>,
    field: &str,
) -> Result<(), WriteError> {
    for (index, value) in values.iter_mut().enumerate() {
        reference(value, kind, bindings, &format!("{field}[{index}]"))?;
    }
    Ok(())
}
fn json_reference(
    value: &mut Value,
    key: &str,
    kind: Namespace,
    bindings: &BTreeMap<String, Binding>,
    field: &str,
) -> Result<(), WriteError> {
    if let Some(Value::String(text)) = value.get_mut(key) {
        reference(text, kind, bindings, &format!("{field}.{key}"))?;
    }
    Ok(())
}
fn field_references(
    fields: &mut Fields,
    bindings: &BTreeMap<String, Binding>,
) -> Result<(), WriteError> {
    for (key, value) in fields {
        let namespace = match key.to_ascii_lowercase().as_str() {
            "dependencies" | "merged into" => Some(Namespace::Claim),
            "also_depends_on" | "same_as" | "parent" => Some(Namespace::Node),
            _ => None,
        };
        if let Some(namespace) = namespace {
            match value {
                Value::String(text) => reference(text, namespace, bindings, key)?,
                Value::Array(values) => {
                    for (index, value) in values.iter_mut().enumerate() {
                        if let Value::String(text) = value {
                            reference(text, namespace, bindings, &format!("{key}[{index}]"))?;
                        }
                    }
                }
                _ => {}
            }
        }
        // Evidence/Proof contain prose as well as refs. Substitute ONLY exact
        // provisional reference elements, never words embedded in quotations.
        if matches!(key.to_ascii_lowercase().as_str(), "evidence" | "proof") {
            match value {
                Value::Array(values) => {
                    for (index, value) in values.iter_mut().enumerate() {
                        if let Value::String(text) = value
                            && text.starts_with('$')
                            && !text.contains(char::is_whitespace)
                        {
                            reference(text, Namespace::Any, bindings, &format!("{key}[{index}]"))?;
                        }
                    }
                }
                Value::String(text)
                    if text.starts_with('$') && !text.contains(char::is_whitespace) =>
                {
                    reference(text, Namespace::Any, bindings, key)?
                }
                _ => {}
            }
        }
    }
    Ok(())
}
fn substitute(
    operation: &mut WriteOperation,
    bindings: &BTreeMap<String, Binding>,
) -> Result<(), WriteError> {
    match operation {
        WriteOperation::NodeAdd {
            parent,
            depends_on,
            fields,
            ..
        } => {
            reference(parent, Namespace::Node, bindings, "parent")?;
            refs(depends_on, Namespace::Node, bindings, "depends_on")?;
            field_references(fields, bindings)?;
        }
        WriteOperation::EdgeAdd { node, depends_on } => {
            reference(node, Namespace::Node, bindings, "node")?;
            reference(depends_on, Namespace::Node, bindings, "depends_on")?;
        }
        WriteOperation::NodeLinkSameAs { node, same_as } => {
            reference(node, Namespace::Node, bindings, "node")?;
            reference(same_as, Namespace::Node, bindings, "same_as")?;
        }
        WriteOperation::ClaimAdd { fields, .. } | WriteOperation::HeuristicAdd { fields, .. } => {
            field_references(fields, bindings)?
        }
        WriteOperation::EntryEdit { target, set }
        | WriteOperation::LogicRevise { target, set, .. } => {
            selector(target, bindings, "target")?;
            field_references(set, bindings)?;
        }
        WriteOperation::ObservationStage { bound_to, .. } => {
            refs(bound_to, Namespace::Node, bindings, "bound_to")?
        }
        WriteOperation::ObservationPromote {
            observation,
            target,
            fields,
            ..
        } => {
            reference(observation, Namespace::Observation, bindings, "observation")?;
            if let Some(target) = target {
                selector(target, bindings, "target")?;
            }
            field_references(fields, bindings)?;
        }
        WriteOperation::ObservationMarkStale {
            observation, audit, ..
        } => {
            reference(observation, Namespace::Observation, bindings, "observation")?;
            reference(
                &mut audit.session,
                Namespace::Session,
                bindings,
                "audit.session",
            )?;
        }
        WriteOperation::SessionLog {
            session,
            events,
            claims_touched,
            logic_revisions,
            ..
        } => {
            reference(session, Namespace::Session, bindings, "session")?;
            for (index, event) in events.iter_mut().enumerate() {
                json_reference(
                    event,
                    "id",
                    Namespace::Any,
                    bindings,
                    &format!("events[{index}]"),
                )?;
            }
            for (index, claim) in claims_touched.iter_mut().enumerate() {
                json_reference(
                    claim,
                    "id",
                    Namespace::Claim,
                    bindings,
                    &format!("claims_touched[{index}]"),
                )?;
            }
            for (index, revision) in logic_revisions.iter_mut().enumerate() {
                json_reference(
                    revision,
                    "entry",
                    Namespace::Any,
                    bindings,
                    &format!("logic_revisions[{index}]"),
                )?;
            }
        }
        WriteOperation::RecordAppend {
            document, record, ..
        } => {
            if document == "trace/taste_log.yaml" {
                json_reference(record, "target", Namespace::Node, bindings, "record")?;
            } else if document == "trace/pm_reasoning_log.yaml" {
                json_reference(record, "turn", Namespace::Session, bindings, "record")?;
            }
        }
        WriteOperation::EntryRename {
            target, references, ..
        }
        | WriteOperation::EntryRemove {
            target, references, ..
        } => {
            selector(target, bindings, "target")?;
            for (index, edit) in references.iter_mut().enumerate() {
                selector(
                    &mut edit.target,
                    bindings,
                    &format!("references[{index}].target"),
                )?;
            }
        }
        WriteOperation::EntryTasteAppend { target, .. } => selector(target, bindings, "target")?,
        WriteOperation::EntryAnnotate {
            target, references, ..
        } => {
            selector(target, bindings, "target")?;
            refs(references, Namespace::Any, bindings, "references")?;
        }
        WriteOperation::PaperEdit {
            frontmatter, audit, ..
        } => {
            if let Some(context) = audit {
                reference(
                    &mut context.session,
                    Namespace::Session,
                    bindings,
                    "audit.session",
                )?;
            }
            if let Some(Value::Array(claims)) = frontmatter.get_mut("claims_summary") {
                for (index, value) in claims.iter_mut().enumerate() {
                    if let Value::String(text) = value {
                        reference(
                            text,
                            Namespace::Claim,
                            bindings,
                            &format!("frontmatter.claims_summary[{index}]"),
                        )?;
                    }
                }
            }
        }
        _ => {}
    }
    match operation {
        WriteOperation::LogicRevise { session, .. } => {
            reference(session, Namespace::Session, bindings, "session")?
        }
        WriteOperation::EntryRename {
            session: Some(session),
            ..
        }
        | WriteOperation::EntryRemove {
            session: Some(session),
            ..
        } => reference(session, Namespace::Session, bindings, "session")?,
        _ => {}
    }
    if let WriteOperation::EntryRemove {
        redirect: Some(target),
        ..
    } = operation
    {
        selector(target, bindings, "redirect")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_keys_reject_at_physical_line() {
        let error=parse_batch(b"\n{\"op\":\"node.add\",\"type\":\"question\",\"parent\":\"root\",\"title\":\"a\",\"fields\":{\"x\":1,\"x\":2}}\n").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.message.contains("duplicate"));
    }
    #[test]
    fn malformed_or_multiple_objects_reject() {
        for text in [
            "{} {}",
            "{\"op\":\"unknown\"}",
            "[]",
            "{\"op\":\"edge.add\",\"node\":\"N01\",\"depends_on\":\"N02\",\"extra\":true}",
        ] {
            assert!(parse_batch(text.as_bytes()).is_err());
        }
    }
    #[test]
    fn literal_prose_dollars_and_ats_are_not_substituted() {
        let mut op=parse_batch(br#"{"op":"node.add","id":"$new","type":"question","parent":"root","title":"$missing","fields":{"description":"@literal $missing"}}"#).unwrap().remove(0);
        take_binding(&mut op).unwrap();
        substitute(&mut op, &BTreeMap::new()).unwrap();
        let WriteOperation::NodeAdd { title, fields, .. } = op else {
            panic!()
        };
        assert_eq!(title, "$missing");
        assert_eq!(fields["description"], "@literal $missing");
    }
    #[test]
    fn wrong_kind_and_forward_binding_reject() {
        let mut references = BTreeMap::new();
        references.insert(
            "claim".into(),
            Binding {
                id: "C01".into(),
                kind: Namespace::Claim,
            },
        );
        assert!(reference(&mut "$claim".into(), Namespace::Node, &references, "parent").is_err());
        assert!(reference(&mut "$later".into(), Namespace::Any, &references, "target").is_err());
    }
    #[test]
    fn empty_input_is_noop() {
        assert!(parse_batch(b"\n \t\r\n").unwrap().is_empty());
    }
}
