//! YAML access, token rules and reference collection for the session history.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::Basis;
use crate::write::positions::{YamlKind, YamlNode};
use crate::write::sessions::{timestamp_key, validate_date, validate_session_id};

/// Row keys that are generated, enumerated or derived audit values rather
/// than caller-authored reference text.
pub(super) const EXCLUDED_KEYS: [&str; 12] = [
    "turn",
    "type",
    "routing",
    "provenance",
    "signal",
    "field",
    "before",
    "after",
    "target",
    "turn_count",
    "events_count",
    "session_metadata",
];
/// Session metadata keys that are identity, clock or counter values.
pub(super) const METADATA_KEYS: [&str; 10] = [
    "id",
    "date",
    "timestamp",
    "started",
    "last_turn",
    "turn_count",
    "closed",
    "status",
    "ended",
    "closed_at",
];

/// Where a reference occurrence belongs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum UnitRef {
    Turn(String, u64),
    Session(String),
    Unattributed,
}

/// One explicit-reference occurrence.
#[derive(Debug, Clone)]
pub(super) struct Mention {
    pub target: String,
    pub literal: String,
    pub basis: Basis,
    pub unit: UnitRef,
    pub doc: usize,
    pub offset: usize,
    pub field: String,
    pub via: Option<String>,
    pub unresolved: bool,
}

pub(super) fn get<'n>(node: &'n YamlNode, key: &str) -> Option<&'n YamlNode> {
    match &node.kind {
        YamlKind::Mapping(entries) => {
            let mut found = entries
                .iter()
                .filter(|(k, _)| k.scalar() == Some(key))
                .map(|(_, v)| v);
            let first = found.next();
            if found.next().is_some() { None } else { first }
        }
        _ => None,
    }
}
pub(super) fn scalar<'n>(node: &'n YamlNode, key: &str) -> Option<&'n str> {
    get(node, key).and_then(YamlNode::scalar)
}
/// The written date of a date or timestamp, when it is valid.
pub(super) fn written_date(text: &str) -> Option<String> {
    let date = text.get(..10)?;
    validate_date(date).ok()?;
    if text.len() > 10 {
        timestamp_key(text).ok()?;
    }
    Some(date.to_owned())
}
pub(super) fn turn_reference(text: &str) -> Option<(String, u64)> {
    let (session, number) = text.split_once('#')?;
    validate_session_id(session).ok()?;
    if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let number = number.parse::<u64>().ok().filter(|n| *n > 0)?;
    Some((session.to_owned(), number))
}
fn id_shaped(token: &str) -> bool {
    let b = token.as_bytes();
    b.len() >= 2 && b[0].is_ascii_uppercase() && b[1..].iter().all(u8::is_ascii_digit)
}
fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| id_shaped(token))
}
pub(super) fn sequence_strings(node: Option<&YamlNode>) -> Vec<(&str, usize)> {
    match node.map(|n| &n.kind) {
        Some(YamlKind::Sequence(items)) => items
            .iter()
            .filter_map(|item| item.scalar().map(|s| (s, item.start)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether a reasoning entry is a stale-evidence record (its own evidence
/// never counts as reference activity).
pub(super) fn stale_record(entry: &YamlNode) -> bool {
    sequence_strings(get(entry, "notes"))
        .iter()
        .any(|(note, _)| {
            serde_json::from_str::<Value>(note).is_ok_and(|value| {
                value.get("operation").and_then(Value::as_str) == Some("observation.mark_stale")
            })
        })
}

/// Authenticated merge redirects decoded from `trace/aliases.yaml`.
#[derive(Default)]
pub(super) struct Aliases {
    /// `(source_key, original)` -> targets.
    pub redirects: BTreeMap<(String, String), BTreeSet<String>>,
    /// Imported session ID -> source keys of the imports that brought it.
    pub imported: BTreeMap<String, BTreeSet<String>>,
}
impl Aliases {
    /// The one local session an imported session ID was relocated to.
    pub fn session_redirect(&self, id: &str) -> Option<String> {
        let targets: BTreeSet<&String> = self
            .redirects
            .iter()
            .filter(|((_, original), _)| original == id)
            .flat_map(|(_, targets)| targets)
            .collect();
        (targets.len() == 1).then(|| targets.into_iter().next().expect("one").clone())
    }
    /// Resolve an imported literal through its authenticated redirect:
    /// `(target, resolved_via, unresolved)`.
    fn literal(&self, import_keys: &[String], token: &str) -> (String, Option<String>, bool) {
        if import_keys.is_empty() {
            return (token.to_owned(), None, false);
        }
        let mut targets = BTreeSet::new();
        for key in import_keys {
            match self.redirects.get(&(key.clone(), token.to_owned())) {
                Some(found) if found.len() == 1 => targets.extend(found.iter().cloned()),
                _ => return (token.to_owned(), None, true),
            }
        }
        if targets.len() == 1 {
            let target = targets.into_iter().next().expect("one target");
            let via = format!("alias:{}:{token}", import_keys.join(","));
            (target, Some(via), false)
        } else {
            (token.to_owned(), None, true)
        }
    }
}

/// Collects the reference occurrences of one unit of caller text.
pub(super) struct Collector<'m> {
    doc: usize,
    unit: UnitRef,
    import_keys: Vec<String>,
    aliases: &'m Aliases,
    pub out: Vec<Mention>,
}
impl<'m> Collector<'m> {
    pub fn new(
        doc: usize,
        unit: UnitRef,
        import_keys: &BTreeSet<String>,
        aliases: &'m Aliases,
    ) -> Self {
        Self {
            doc,
            unit,
            import_keys: import_keys.iter().cloned().collect(),
            aliases,
            out: Vec::new(),
        }
    }
    pub fn literal_text(&mut self, text: &str, offset: usize, field: &str) {
        for token in tokens(text) {
            let (target, via, unresolved) = self.aliases.literal(&self.import_keys, token);
            self.out.push(Mention {
                target,
                literal: token.to_owned(),
                basis: Basis::Literal,
                unit: self.unit.clone(),
                doc: self.doc,
                offset,
                field: field.to_owned(),
                via,
                unresolved,
            });
        }
    }
    /// Walk caller-authored values: `id`/`entry` scalars are typed fields,
    /// everything else is literal text; generated keys are skipped.
    pub fn walk(&mut self, node: &YamlNode, field: &str, key: Option<&str>) {
        match &node.kind {
            YamlKind::Scalar { value, .. } => {
                if matches!(key, Some("id") | Some("entry")) && id_shaped(value) {
                    self.out.push(Mention {
                        target: value.clone(),
                        literal: value.clone(),
                        basis: Basis::Structured,
                        unit: self.unit.clone(),
                        doc: self.doc,
                        offset: node.start,
                        field: field.to_owned(),
                        via: None,
                        unresolved: false,
                    });
                } else {
                    self.literal_text(value, node.start, field);
                }
            }
            YamlKind::Sequence(items) => {
                for (index, item) in items.iter().enumerate() {
                    self.walk(item, &format!("{field}[{index}]"), key);
                }
            }
            YamlKind::Mapping(entries) => {
                for (name, value) in entries {
                    let Some(name) = name.scalar() else { continue };
                    if EXCLUDED_KEYS.contains(&name) {
                        continue;
                    }
                    let path = if field.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{field}.{name}")
                    };
                    self.walk(value, &path, Some(name));
                }
            }
            YamlKind::Alias(_) => {}
        }
    }
    /// Walk every key of a mapping except `skip` and the generated keys.
    pub fn walk_mapping(&mut self, node: &YamlNode, prefix: &str, skip: &[&str]) {
        if let YamlKind::Mapping(entries) = &node.kind {
            for (name, value) in entries {
                let Some(name) = name.scalar() else { continue };
                if skip.contains(&name) || EXCLUDED_KEYS.contains(&name) {
                    continue;
                }
                let path = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}.{name}")
                };
                self.walk(value, &path, Some(name));
            }
        }
    }
}
