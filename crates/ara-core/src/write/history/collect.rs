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
    pub redirects: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    /// Imported session ID -> source keys of the imports that brought it.
    pub imported: BTreeMap<String, BTreeSet<String>>,
    pub origins: BTreeMap<String, BTreeMap<String, BTreeSet<(String, String)>>>,
    pub imported_fields: BTreeMap<String, BTreeSet<String>>,
    pub relocation_maps: BTreeMap<String, crate::merge::IdentityMap>,
}
impl Aliases {
    /// The one local session an imported session ID was relocated to.
    pub fn session_redirect(&self, id: &str) -> Option<String> {
        let targets: BTreeSet<&String> = self
            .redirects
            .values()
            .filter_map(|originals| originals.get(id))
            .flatten()
            .collect();
        (targets.len() == 1).then(|| targets.into_iter().next().expect("one").clone())
    }
    /// Resolve an imported literal through its authenticated redirect:
    /// `(target, resolved_via, unresolved)`.
    fn literal(&self, key: &str, token: &str) -> (String, Option<String>, bool) {
        match self
            .redirects
            .get(key)
            .and_then(|originals| originals.get(token))
        {
            Some(targets) if targets.len() == 1 => (
                targets.iter().next().expect("one target").clone(),
                Some(format!("alias:{key}:{token}")),
                false,
            ),
            _ => (token.to_owned(), None, true),
        }
    }
    /// Use captured occurrence text, never its enclosing session, to decide
    /// whether a literal is original imported spelling or already relocated.
    fn occurrences(
        &self,
        path: &str,
        field: &str,
        text: &str,
        current: &[crate::query::TokenMatch<'_>],
    ) -> Option<Vec<(String, Option<String>, bool)>> {
        let unknown = || {
            current
                .iter()
                .map(|token| (token.literal.to_owned(), None, true))
                .collect()
        };
        let Some(origins) = self.origins.get(path).and_then(|fields| fields.get(field)) else {
            let imported = self.imported_fields.get(path).is_some_and(|prefixes| {
                prefixes.contains(field)
                    || field.char_indices().any(|(offset, character)| {
                        matches!(character, '.' | '[') && prefixes.contains(&field[..offset])
                    })
            });
            return imported.then(unknown);
        };
        let mut resolutions = vec![BTreeSet::new(); current.len()];
        for (key, original) in origins {
            let found = crate::query::scan_tokens(original);
            if found.len() != current.len() {
                return Some(unknown());
            }
            let mut candidate = Vec::with_capacity(found.len());
            if original == text {
                candidate.extend(found.iter().map(|token| self.literal(key, token.literal)));
            } else {
                if field.contains(".session_metadata.after.")
                    || field.contains(".session_metadata.before.")
                {
                    // Merge preserves archived transition values verbatim.
                    // A changed value there is not relocation evidence.
                    return Some(unknown());
                }
                let Some(map) = self.relocation_maps.get(key) else {
                    return Some(unknown());
                };
                if !crate::merge::relocate_scalar(original, map)
                    .is_ok_and(|relocated| relocated == text)
                {
                    return Some(unknown());
                }
                let protected = crate::merge::quoted_ranges(original);
                for (token, current_token) in found.iter().zip(current) {
                    let resolution = self.literal(key, token.literal);
                    let preserved = protected.iter().any(|range| {
                        range.start < token.range.end && token.range.start < range.end
                    });
                    let expected = if preserved || resolution.2 {
                        token.literal
                    } else {
                        resolution.0.as_str()
                    };
                    if current_token.literal != expected {
                        return Some(unknown());
                    }
                    candidate.push(if preserved || resolution.2 {
                        resolution
                    } else {
                        (resolution.0, None, false)
                    });
                }
            }
            for (possible, resolution) in resolutions.iter_mut().zip(candidate) {
                possible.insert(resolution);
            }
        }
        Some(
            resolutions
                .into_iter()
                .zip(current)
                .map(|(possible, token)| {
                    if possible.len() == 1 {
                        possible.into_iter().next().expect("one resolution")
                    } else {
                        (token.literal.to_owned(), None, true)
                    }
                })
                .collect(),
        )
    }
}

/// Collects the reference occurrences of one unit of caller text.
pub(super) struct Collector<'m> {
    doc: usize,
    unit: UnitRef,
    path: &'m str,
    aliases: &'m Aliases,
    pub out: Vec<Mention>,
}
impl<'m> Collector<'m> {
    pub fn new(doc: usize, unit: UnitRef, path: &'m str, aliases: &'m Aliases) -> Self {
        Self {
            doc,
            unit,
            path,
            aliases,
            out: Vec::new(),
        }
    }
    pub fn literal_text(&mut self, text: &str, offset: usize, field: &str) {
        let tokens = crate::query::scan_tokens(text);
        let mut resolutions = self
            .aliases
            .occurrences(self.path, field, text, &tokens)
            .map(Vec::into_iter);
        for token in tokens {
            let (target, via, unresolved) = resolutions
                .as_mut()
                .and_then(Iterator::next)
                .unwrap_or_else(|| (token.literal.to_owned(), None, false));
            if unresolved {
                self.out.push(Mention {
                    target: String::new(),
                    literal: "history.origin_unknown".to_owned(),
                    basis: Basis::Literal,
                    unit: self.unit.clone(),
                    doc: self.doc,
                    offset,
                    field: field.to_owned(),
                    via: None,
                    unresolved: true,
                });
            }
            self.out.push(Mention {
                target,
                literal: token.literal.to_owned(),
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
            YamlKind::Alias(_) => self.unknown(node, field),
        }
    }
    /// An unexpanded YAML reference must invalidate both counts, including
    /// references whose subject cannot be identified without expansion.
    pub fn unknown(&mut self, node: &YamlNode, field: &str) {
        self.out.push(Mention {
            target: String::new(),
            literal: String::new(),
            basis: Basis::Literal,
            unit: self.unit.clone(),
            doc: self.doc,
            offset: node.start,
            field: field.to_owned(),
            via: None,
            unresolved: true,
        });
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
