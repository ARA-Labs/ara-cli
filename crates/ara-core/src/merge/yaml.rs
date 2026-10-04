//! Complete lossless YAML layer reconciliation. Policy remains an upstream-review proposal.
#![allow(clippy::too_many_arguments)]
use super::{
    rewrite,
    types::{
        ConflictLocator, EntryIdentity, IdentityMap, MergeConflict, MergeError, MergeReport,
        MergeValue, conflict, reject_protected,
    },
};
use crate::write::{
    ArtifactSnapshot, WorkingArtifact,
    positions::{YamlDocument, YamlKind, YamlNode, field_range, line_start},
    source::render_yaml,
};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::{Arc, OnceLock},
};

const INDEX: &str = "trace/sessions/session_index.yaml";
const HISTORY: [&str; 5] = [
    "events_logged",
    "ai_actions",
    "claims_touched",
    "logic_revisions",
    "key_context",
];
const PROMOTION: [&str; 3] = ["promoted", "promoted_to", "crystallized_via"];

pub(crate) struct Inventory {
    pub entries: Vec<EntryIdentity>,
    pub paths: BTreeSet<String>,
    docs: BTreeMap<String, Document>,
    records: BTreeMap<String, Record>,
}
impl Inventory {
    pub(crate) fn retain_preimages(&self, working: &mut WorkingArtifact) {
        for (path, document) in &self.docs {
            working.retain_base_yaml(path, &document.text, Arc::clone(&document.parsed));
        }
    }
}
struct Document {
    text: String,
    parsed: Arc<YamlDocument>,
    kind: Kind,
    order: Vec<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Tree,
    Observations,
    Taste,
    Reasoning,
    Mutations,
    Session,
    Index,
}
impl Kind {
    fn root(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Observations => "observations",
            Self::Taste | Self::Reasoning => "entries",
            Self::Mutations => "mutations",
            Self::Session => "session",
            Self::Index => "sessions",
        }
    }
    fn layer(self) -> &'static str {
        match self {
            Self::Tree => "trace",
            Self::Observations => "staging",
            Self::Taste => "taste",
            Self::Reasoning => "reasoning",
            Self::Mutations => "mutation",
            Self::Session | Self::Index => "session",
        }
    }
}
struct Record {
    path: String,
    parent: Option<String>,
    span: Range<usize>,
    end: usize,
    indent: usize,
    flow: bool,
    mapping_end: usize,
    fields: BTreeMap<String, Field>,
    children: Option<YamlNode>,
}
#[derive(Clone)]
struct Field {
    node: YamlNode,
    span: Range<usize>,
    raw: String,
    column: usize,
    semantic: OnceLock<Option<Value>>,
}
impl Field {
    fn semantic(&self) -> Option<&Value> {
        self.semantic
            .get_or_init(|| self.node.to_json().ok())
            .as_ref()
    }
}
fn field_names<'a>(
    a: &'a BTreeMap<String, Field>,
    b: &'a BTreeMap<String, Field>,
    c: &'a BTreeMap<String, Field>,
) -> impl Iterator<Item = &'a String> {
    let mut keys = [
        a.keys().peekable(),
        b.keys().peekable(),
        c.keys().peekable(),
    ];
    std::iter::from_fn(move || {
        let name = keys
            .iter_mut()
            .filter_map(|keys| keys.peek().copied())
            .min()?;
        for keys in &mut keys {
            if keys.peek().is_some_and(|key| *key == name) {
                keys.next();
            }
        }
        Some(name)
    })
}
fn kind(path: &str) -> Option<Kind> {
    Some(match path {
        "trace/exploration_tree.yaml" => Kind::Tree,
        "staging/observations.yaml" => Kind::Observations,
        "trace/taste.yaml" | "trace/taste_log.yaml" => Kind::Taste,
        "trace/reasoning.yaml" | "trace/pm_reasoning_log.yaml" => Kind::Reasoning,
        "trace/logic_mutations.yaml" => Kind::Mutations,
        INDEX => Kind::Index,
        _ if path.starts_with("trace/sessions/")
            && path.ends_with(".yaml")
            && !path[15..].contains('/') =>
        {
            Kind::Session
        }
        _ => return None,
    })
}
fn fields(text: &str, node: &YamlNode) -> Result<BTreeMap<String, Field>, MergeError> {
    fields_except(text, node, &[])
}
fn fields_except(
    text: &str,
    node: &YamlNode,
    excluded: &[&str],
) -> Result<BTreeMap<String, Field>, MergeError> {
    let mut result = BTreeMap::new();
    for (key, value) in node.mapping()? {
        let name = key
            .scalar()
            .ok_or_else(|| MergeError::content("merge.yaml", "complex YAML entry key"))?;
        if name == "children" || excluded.contains(&name) {
            continue;
        }
        let span = if node.flow {
            key.start..value.end
        } else {
            field_range(text, node, name)?
                .ok_or_else(|| MergeError::content("merge.yaml", "missing indexed field"))?
        };
        let end = if node.flow {
            value.end
        } else {
            span.end.max(value.end)
        };
        let raw = text[value.start..end].to_owned();
        if result
            .insert(
                name.into(),
                Field {
                    node: value.clone(),
                    span,
                    raw,
                    // Flow values cannot contain indentation-sensitive block values.
                    // Avoid scanning a potentially multi-megabyte single-line prefix.
                    column: if node.flow {
                        0
                    } else {
                        value.start - line_start(text, value.start)
                    },
                    semantic: OnceLock::new(),
                },
            )
            .is_some()
        {
            return Err(MergeError::content(
                "merge.yaml",
                format!("duplicate entry field {name}"),
            ));
        }
    }
    Ok(result)
}
fn record(
    text: &str,
    path: &str,
    node: &YamlNode,
    parent: Option<String>,
    span: Range<usize>,
) -> Result<Record, MergeError> {
    let indent = if node.flow {
        0
    } else {
        let line = line_start(text, node.start);
        let prefix = &text[line..node.start];
        prefix
            .find('-')
            .unwrap_or_else(|| prefix.len().saturating_sub(2))
    };
    let children = node.get("children")?.map(|n| YamlNode {
        kind: match &n.kind {
            YamlKind::Sequence(items) => YamlKind::Sequence(
                items
                    .iter()
                    .map(|item| YamlNode {
                        kind: YamlKind::Scalar {
                            value: String::new(),
                            plain: true,
                        },
                        start: item.start,
                        end: item.end,
                        flow: item.flow,
                        anchor: item.anchor,
                        tag: item.tag.clone(),
                        style: item.style,
                    })
                    .collect(),
            ),
            _ => n.kind.clone(),
        },
        start: n.start,
        end: n.end,
        flow: n.flow,
        anchor: n.anchor,
        tag: n.tag.clone(),
        style: n.style,
    });
    Ok(Record {
        path: path.into(),
        parent,
        span,
        end: if node.flow {
            node.end
        } else {
            boundary(text, node.end)
        },
        indent,
        flow: node.flow,
        mapping_end: node.end,
        fields: fields(text, node)?,
        children,
    })
}
fn collect(
    inv: &mut Inventory,
    path: &str,
    items: &[YamlNode],
    flow: bool,
    kind: Kind,
    text: &str,
    order: &mut Vec<String>,
) -> Result<(), MergeError> {
    let mut stack = vec![(items, flow, 0, None::<String>)];
    while let Some((items, flow, next, parent)) = stack.last_mut() {
        if *next == items.len() {
            stack.pop();
            continue;
        }
        let ordinal = *next;
        *next += 1;
        let nodes = *items;
        let node = &nodes[ordinal];
        if !matches!(node.kind, YamlKind::Mapping(_)) {
            return Err(MergeError::content(
                "merge.yaml_record",
                format!(
                    "{path}: record {ordinal} is not a mapping: {:?}",
                    &text[node.start..node.end]
                ),
            )
            .at(path));
        }
        let id = node
            .get("id")?
            .and_then(YamlNode::scalar)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{path}#{}/{ordinal}", kind.root()));
        if !matches!(kind, Kind::Reasoning | Kind::Mutations)
            && node.get("id")?.and_then(YamlNode::scalar).is_none()
        {
            return Err(MergeError::content(
                "merge.identity",
                format!("entry in {path} has no ID"),
            ));
        }
        let start = if *flow {
            node.start
        } else {
            line_start(text, node.start)
        };
        let end = if *flow {
            node.end
        } else {
            nodes.get(ordinal + 1).map_or_else(
                || boundary(text, node.end),
                |next| line_start(text, next.start),
            )
        };
        let entry = record(text, path, node, parent.clone(), start..end)?;
        let numeric = if matches!(kind, Kind::Reasoning | Kind::Mutations | Kind::Index) {
            None
        } else {
            id.chars()
                .next()
                .filter(|prefix| matches!(prefix, 'N' | 'O' | 'T'))
        };
        if kind != Kind::Index {
            inv.entries.push(EntryIdentity {
                address: id.clone(),
                layer: kind.layer().into(),
                path: path.into(),
                numeric,
                session: false,
                heading: Vec::new(),
            });
        }
        let key = if kind == Kind::Index {
            format!("index:{id}")
        } else {
            id.clone()
        };
        if inv.records.insert(key.clone(), entry).is_some() {
            return Err(MergeError::content(
                "merge.identity",
                format!("duplicate YAML identity {key}"),
            ));
        }
        order.push(key);
        if let Some(children) = node.get("children")? {
            stack.push((children.sequence()?, children.flow, 0, Some(id)));
        }
    }
    Ok(())
}
pub(crate) fn inventory(snapshot: &ArtifactSnapshot) -> Result<Inventory, MergeError> {
    inventory_with(snapshot, |path, text| {
        YamlDocument::parse(text)
            .map(Arc::new)
            .map_err(|error| MergeError::from(error).at(path))
    })
}
pub(crate) fn inventory_cached(
    snapshot: &ArtifactSnapshot,
    working: &WorkingArtifact,
) -> Result<Inventory, MergeError> {
    inventory_with(snapshot, |path, _| {
        working.yaml(path).map_err(MergeError::from)
    })
}
fn inventory_with(
    snapshot: &ArtifactSnapshot,
    mut parse: impl FnMut(&str, &str) -> Result<Arc<YamlDocument>, MergeError>,
) -> Result<Inventory, MergeError> {
    let mut inv = Inventory {
        entries: vec![],
        paths: BTreeSet::new(),
        docs: BTreeMap::new(),
        records: BTreeMap::new(),
    };
    for (path, file) in &snapshot.files {
        let Some(kind) = kind(path).filter(|_| file.existed) else {
            continue;
        };
        let text = std::str::from_utf8(&file.bytes)
            .map_err(|_| {
                MergeError::content("merge.encoding", format!("{path} is not UTF-8")).at(path)
            })?
            .to_owned();
        let parsed = parse(path, &text)?;
        let mut doc = Document {
            text,
            parsed,
            kind,
            order: vec![],
        };
        let tree = doc
            .parsed
            .root
            .get(kind.root())
            .map_err(|error| MergeError::from(error).at(path))?;
        let single = if kind == Kind::Tree {
            doc.parsed
                .root
                .get("root")
                .map_err(|error| MergeError::from(error).at(path))?
        } else {
            None
        };
        if tree.is_some() && single.is_some() {
            return Err(MergeError::content(
                "merge.yaml",
                "ambiguous tree and single-root dialect",
            )
            .at(path));
        }
        let root = tree.or(single).ok_or_else(|| {
            MergeError::content("merge.yaml", format!("{path} has no {}", kind.root())).at(path)
        })?;
        if kind == Kind::Session {
            let id = root
                .get("id")
                .map_err(|e| MergeError::from(e).at(path))?
                .and_then(YamlNode::scalar)
                .ok_or_else(|| MergeError::content("merge.session", "missing session ID").at(path))?
                .to_owned();
            crate::write::sessions::validate_session_id(&id)
                .map_err(|e| MergeError::from(e).at(path))?;
            if path != &format!("trace/sessions/{id}.yaml") {
                return Err(MergeError::content(
                    "merge.session",
                    "session filename and ID disagree",
                )
                .at(path));
            }
            validate_session_source(&doc, root, &id).map_err(|error| error.at(path))?;
            inv.entries.push(EntryIdentity {
                address: id.clone(),
                layer: "session".into(),
                path: path.clone(),
                numeric: None,
                session: true,
                heading: Vec::new(),
            });
            for name in HISTORY {
                if let Some(rows) = doc.parsed.root.get(name)? {
                    for ordinal in 0..rows.sequence()?.len() {
                        inv.entries.push(EntryIdentity {
                            address: format!("{path}#{name}/{ordinal}"),
                            layer: "session_occurrence".into(),
                            path: path.clone(),
                            numeric: None,
                            session: false,
                            heading: Vec::new(),
                        });
                    }
                }
            }
            inv.records.insert(
                id.clone(),
                record(&doc.text, path, root, None, 0..doc.text.len())
                    .map_err(|error| error.at(path))?,
            );
            doc.order.push(id);
        } else if kind == Kind::Tree && matches!(root.kind, YamlKind::Mapping(_)) {
            collect(
                &mut inv,
                path,
                std::slice::from_ref(root),
                root.flow,
                kind,
                &doc.text,
                &mut doc.order,
            )
            .map_err(|error| error.at(path))?;
        } else if kind == Kind::Tree
            && matches!(root.kind, YamlKind::Scalar { .. })
            && root.to_json().is_ok_and(|value| value.is_null())
        {
            // Empty dialect roots have no allocated entries.
        } else {
            collect(
                &mut inv,
                path,
                root.sequence()
                    .map_err(|error| MergeError::from(error).at(path))?,
                root.flow,
                kind,
                &doc.text,
                &mut doc.order,
            )
            .map_err(|error| error.at(path))?;
        }
        for id in &doc.order {
            if let Some(record) = inv.records.get(id)
                && let Some(rows) = record.fields.get("conflict_annotations")
            {
                for ordinal in 0..rows.node.sequence()?.len() {
                    inv.entries.push(EntryIdentity {
                        address: format!("{id}#conflict_annotations/{ordinal}"),
                        layer: "annotation_occurrence".into(),
                        path: path.clone(),
                        numeric: None,
                        session: false,
                        heading: Vec::new(),
                    });
                }
            }
        }
        inv.entries.push(EntryIdentity {
            address: path.clone(),
            layer: "document".into(),
            path: path.clone(),
            numeric: None,
            session: false,
            heading: Vec::new(),
        });
        inv.paths.insert(path.clone());
        inv.docs.insert(path.clone(), doc);
    }
    // Legacy indexes may omit derived metadata, but identities must still be complete.
    let sessions: BTreeSet<_> = inv
        .entries
        .iter()
        .filter(|e| e.session)
        .map(|e| e.address.as_str())
        .collect();
    let indexed: BTreeSet<_> = inv
        .docs
        .get(INDEX)
        .map(|d| {
            d.order
                .iter()
                .map(|s| s.trim_start_matches("index:"))
                .collect()
        })
        .unwrap_or_default();
    if sessions != indexed {
        return Err(MergeError::content(
            "merge.session_index",
            format!(
                "session index differs from actual source: missing {:?}, dangling {:?}",
                sessions.difference(&indexed).collect::<Vec<_>>(),
                indexed.difference(&sessions).collect::<Vec<_>>()
            ),
        )
        .at(INDEX));
    }
    Ok(inv)
}
fn boundary(text: &str, end: usize) -> usize {
    if end >= text.len() {
        return text.len();
    }
    let next = crate::write::positions::line_end(text, end);
    let suffix = text[end..next].trim_start_matches([' ', '\t', '\r', '\n']);
    if suffix.is_empty() || suffix.starts_with('#') {
        next
    } else {
        line_start(text, end)
    }
}
fn bytes(field: Option<&Field>) -> Option<&[u8]> {
    field.map(|f| f.raw.as_bytes())
}
fn known(name: &str) -> bool {
    if matches!(name, "from_selector" | "to_selector") {
        return true;
    }
    if matches!(
        name,
        "thinking"
            | "artifacts"
            | "concepts"
            | "source_refs"
            | "support_level"
            | "rationale"
            | "exploration"
            | "outcome"
            | "prior_direction"
            | "new_direction"
            | "reason"
            | "why_failed"
    ) {
        return true;
    }
    matches!(
        name,
        "id" | "type"
            | "title"
            | "provenance"
            | "timestamp"
            | "description"
            | "choice"
            | "alternatives"
            | "evidence"
            | "result"
            | "hypothesis"
            | "failure_mode"
            | "lesson"
            | "from"
            | "to"
            | "trigger"
            | "status"
            | "parent"
            | "also_depends_on"
            | "same_as"
            | "content"
            | "context"
            | "potential_type"
            | "bound_to"
            | "promoted"
            | "promoted_to"
            | "crystallized_via"
            | "stale"
            | "target"
            | "tag"
            | "object"
            | "comment"
            | "turn"
            | "notes"
            | "session_metadata"
            | "date"
            | "started"
            | "last_turn"
            | "turn_count"
            | "summary"
            | "open_threads"
            | "ai_suggestions_pending"
            | "events_count"
            | "claims_touched"
            | "path"
            | "routing"
            | "action"
            | "entry"
            | "field"
            | "before"
            | "after"
            | "signal"
            | "note"
            | "excerpt"
            | "files_changed"
            | "conflict_annotations"
            | "kind"
            | "references"
    )
}
fn known_entry(kind: Kind, name: &str) -> bool {
    match kind {
        Kind::Tree => matches!(
            name,
            "id" | "type"
                | "title"
                | "provenance"
                | "timestamp"
                | "description"
                | "choice"
                | "alternatives"
                | "evidence"
                | "result"
                | "hypothesis"
                | "failure_mode"
                | "lesson"
                | "from"
                | "to"
                | "trigger"
                | "status"
                | "parent"
                | "also_depends_on"
                | "same_as"
                | "thinking"
                | "artifacts"
                | "concepts"
                | "source_refs"
                | "support_level"
                | "rationale"
                | "exploration"
                | "outcome"
                | "prior_direction"
                | "new_direction"
                | "reason"
                | "why_failed"
        ),
        Kind::Observations => matches!(
            name,
            "id" | "timestamp"
                | "provenance"
                | "content"
                | "context"
                | "potential_type"
                | "bound_to"
                | "promoted"
                | "promoted_to"
                | "crystallized_via"
                | "stale"
                | "conflict_annotations"
        ),
        Kind::Taste => matches!(
            name,
            "id" | "timestamp" | "target" | "tag" | "object" | "comment"
        ),
        Kind::Reasoning => matches!(name, "turn" | "notes" | "session_metadata"),
        Kind::Mutations => matches!(
            name,
            "action"
                | "from"
                | "to"
                | "from_selector"
                | "to_selector"
                | "before"
                | "after"
                | "session"
                | "turn"
                | "signal"
                | "provenance"
                | "historical_references"
        ),
        Kind::Session => matches!(
            name,
            "id" | "date"
                | "timestamp"
                | "started"
                | "last_turn"
                | "turn_count"
                | "summary"
                | "closed"
                | "status"
                | "ended"
                | "closed_at"
        ),
        Kind::Index => matches!(
            name,
            "id" | "date"
                | "timestamp"
                | "path"
                | "summary"
                | "turn_count"
                | "events_count"
                | "claims_touched"
                | "open_threads"
        ),
    }
}
fn known_row(name: &str) -> bool {
    matches!(
        name,
        "id" | "turn"
            | "type"
            | "routing"
            | "provenance"
            | "summary"
            | "action"
            | "files_changed"
            | "entry"
            | "field"
            | "before"
            | "after"
            | "signal"
            | "note"
            | "excerpt"
            | "kind"
            | "references"
            | "comment"
    )
}
fn equal(a: Option<&Field>, b: Option<&Field>, name: &str) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) if a.raw == b.raw && a.node.same_source_value(&b.node) => true,
        (Some(a), Some(b)) if known(name) => match (a.semantic(), b.semantic()) {
            (Some(a), Some(b)) => a == b,
            _ => a.raw == b.raw,
        },
        (Some(a), Some(b)) => a.node.same_source_value(&b.node),
        _ => false,
    }
}
fn equal_mapped(a: Option<&Field>, b: Option<&Field>, name: &str, map: &IdentityMap) -> bool {
    if !known(name) || !structured_field("", name) || matches!(name, "id" | "concepts") {
        return equal(a, b, name);
    }
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) if a.raw == b.raw && a.node.same_source_value(&b.node) => true,
        (Some(a), Some(b)) => match (a.semantic(), b.semantic()) {
            (Some(a), Some(b)) => rewrite::references_equal(a, b, map),
            _ => a.raw == b.raw,
        },
        _ => false,
    }
}
fn closed_session(fields: &BTreeMap<String, Field>) -> bool {
    fields
        .get("closed")
        .is_some_and(|f| f.node.to_json().is_ok_and(|v| v == json!(true)))
        || fields
            .get("status")
            .is_some_and(|f| f.node.scalar() == Some("closed"))
        || ["ended", "closed_at"].iter().any(|name| {
            fields
                .get(*name)
                .is_some_and(|f| f.node.to_json().is_ok_and(|v| !v.is_null()))
        })
}
fn quiet_report(report: &MergeReport) -> MergeReport {
    MergeReport::new(
        &super::types::MergeOptions {
            source_key: report.source_key.clone(),
            label: String::new(),
            time: String::new(),
            git: None,
            predecessor: None,
            self_key: None,
        },
        report.source_revision.clone(),
    )
}
pub(crate) fn structured_field(path: &str, name: &str) -> bool {
    matches!(
        name,
        "id" | "parent"
            | "target"
            | "promoted_to"
            | "bound_to"
            | "also_depends_on"
            | "same_as"
            | "turn"
            | "session"
            | "from_selector"
            | "to_selector"
            | "entry"
            | "claims_touched"
            | "path"
            | "files_changed"
            | "historical_references"
            | "references"
            | "concepts"
            | "source_refs"
    ) || path == "trace/logic_mutations.yaml" && matches!(name, "from" | "to")
}
fn rewrite_field(
    raw: &str,
    node: &YamlNode,
    path: &str,
    id: &str,
    name: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<String, MergeError> {
    if name == "concepts" {
        // Concepts are literal names in their own namespace, even when a name
        // has the spelling of a claim ID. Rewrite only resolved concept scalars.
        let mut patches = Vec::new();
        let mut stack = vec![node];
        while let Some(value) = stack.pop() {
            match &value.kind {
                YamlKind::Sequence(items) => stack.extend(items.iter().rev()),
                YamlKind::Scalar { .. } => {
                    let Some(name) = value.scalar() else { continue };
                    let normalized = super::identity::normalize_local(name);
                    let Some(original) = map
                        .concepts
                        .get(name)
                        .or_else(|| map.concepts.get(&normalized))
                    else {
                        continue;
                    };
                    let target = map.source_target(original);
                    let qualified = normalized.starts_with("logic/concepts.md#");
                    let replacement = if qualified && target == original {
                        name.to_owned()
                    } else if qualified {
                        let display = super::markdown::display_address(target);
                        if name.starts_with("logic/concepts.md:") {
                            display.replacen("logic/concepts.md#", "logic/concepts.md:", 1)
                        } else {
                            display
                        }
                    } else if let Ok(crate::write::EntrySelector::Document { heading, .. }) =
                        serde_json::from_str(target)
                    {
                        heading.last().cloned().unwrap_or_else(|| name.into())
                    } else {
                        target
                            .strip_prefix("logic/concepts.md#")
                            .unwrap_or(name)
                            .to_owned()
                    };
                    if replacement != name {
                        let span = value.start - node.start..value.end - node.start;
                        let rewritten = scalar_value(&raw[span.clone()], value, &replacement);
                        patches.push((span, rewritten));
                        report.rewritten.push(super::types::RewriteFact {
                            path: path.into(),
                            selector: id.into(),
                            old: name.into(),
                            new: replacement,
                            confidence: "certain".into(),
                        });
                    }
                }
                _ => {}
            }
        }
        patches.sort_by_key(|(span, _)| span.start);
        let mut output = String::new();
        let mut cursor = 0;
        for (span, value) in patches {
            output.push_str(&raw[cursor..span.start]);
            output.push_str(&value);
            cursor = span.end;
        }
        output.push_str(&raw[cursor..]);
        return Ok(output);
    }
    if name == "thinking" {
        let mut review = quiet_report(report);
        let _ = rewrite::yaml_value(raw, path, id, map, &mut review, false)?;
        report.needs_review.extend(review.needs_review);
        return Ok(raw.into());
    }
    if name == "artifacts" {
        let mut output = String::new();
        let mut cursor = 0;
        for artifact in node.sequence()? {
            if let Some(pointer) = artifact.get("pointer")? {
                let start = pointer.start - node.start;
                let end = pointer.end - node.start;
                output.push_str(&raw[cursor..start]);
                output.push_str(&rewrite::yaml_value(
                    &raw[start..end],
                    path,
                    id,
                    map,
                    report,
                    true,
                )?);
                cursor = end;
            }
        }
        output.push_str(&raw[cursor..]);
        return Ok(output);
    }
    rewrite::yaml_value(
        raw,
        path,
        id,
        map,
        report,
        structured_field(path, name)
            || name == "evidence" && matches!(node.kind, YamlKind::Sequence(_)),
    )
}
#[derive(Default)]
struct Edits {
    patches: BTreeMap<String, Vec<(Range<usize>, String)>>,
    appends: BTreeMap<(String, usize), (Range<usize>, String)>,
}
impl Edits {
    fn patch(&mut self, path: &str, range: Range<usize>, text: String) {
        self.patches
            .entry(path.into())
            .or_default()
            .push((range, text));
    }
    fn append(
        &mut self,
        path: &str,
        list: &YamlNode,
        text: &str,
        fragment: &str,
        indent: usize,
    ) -> Result<(), MergeError> {
        let items = list.sequence()?;
        if list.flow {
            let raw = fragment
                .trim()
                .strip_prefix("- ")
                .unwrap_or(fragment.trim());
            if YamlDocument::parse(raw).is_ok_and(|document| {
                document.root.flow
                    || matches!(document.root.kind, YamlKind::Scalar { .. })
                        && !matches!(
                            document.root.style,
                            Some(
                                yaml_rust2::scanner::TScalarStyle::Literal
                                    | yaml_rust2::scanner::TScalarStyle::Folded
                            )
                        )
            }) {
                let at = list.end - 1;
                let value = self
                    .appends
                    .entry((path.into(), at))
                    .or_insert((at..at, String::new()));
                if !items.is_empty() || !value.1.is_empty() {
                    value.1.push_str(", ");
                }
                value.1.push_str(raw);
                return Ok(());
            }
        }
        let (range, prefix) = if list.flow {
            if !items.is_empty() {
                return Err(MergeError::content(
                    "merge.unsupported_source",
                    "cannot losslessly append a block entry into a nonempty flow list",
                ));
            }
            (list.start..list.end, "\n".to_owned())
        } else {
            let at = boundary(text, list.end);
            (
                at..at,
                if at > 0 && !text[..at].ends_with('\n') {
                    "\n".into()
                } else {
                    String::new()
                },
            )
        };
        let value = self
            .appends
            .entry((path.into(), range.start))
            .or_insert((range, prefix));
        value.1.push_str(&reindent(fragment, indent));
        if !value.1.ends_with('\n') {
            value.1.push('\n');
        }
        Ok(())
    }
    fn finish(mut self, working: &mut WorkingArtifact) -> Result<(), MergeError> {
        for ((path, _), (range, text)) in self.appends {
            self.patches.entry(path).or_default().push((range, text));
        }
        for (path, mut patches) in self.patches {
            patches.sort_by_key(|(r, _)| (r.start, r.end));
            let text = working.text(&path)?;
            let mut candidate = String::with_capacity(
                text.len() + patches.iter().map(|(_, s)| s.len()).sum::<usize>(),
            );
            let mut cursor = 0;
            for (range, value) in patches {
                if range.start < cursor {
                    return Err(MergeError::content(
                        "merge.intent",
                        "overlapping surgical YAML patches",
                    ));
                }
                candidate.push_str(&text[cursor..range.start]);
                candidate.push_str(&value);
                cursor = range.end;
            }
            candidate.push_str(&text[cursor..]);
            // stage_replace parses and caches the exact candidate before accepting it.
            if candidate != text {
                working.stage_replace(
                    &path,
                    candidate.as_bytes(),
                    "compose exact lossless YAML merge spans",
                )?;
            }
        }
        Ok(())
    }
}
fn reindent(fragment: &str, indent: usize) -> String {
    let original = fragment
        .lines()
        .find(|l| !l.trim().is_empty())
        .map_or(0, |l| l.len() - l.trim_start().len());
    let mut result = String::new();
    for line in fragment.split_inclusive('\n') {
        if line.trim().is_empty() {
            result.push_str(line);
        } else {
            result.push_str(&" ".repeat(indent));
            result.push_str(&line[original.min(line.len() - line.trim_start().len())..]);
        }
    }
    result
}
fn field_text(prefix: &str, value: &str, indent: usize) -> String {
    let value = value.trim_end_matches(['\r', '\n']);
    if YamlDocument::parse(&format!("value: {value}\n")).is_ok() {
        return format!("{prefix}: {value}\n");
    }
    let mut lines = value.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let tail: Vec<_> = lines.collect();
    let source_indent = tail
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(2);
    let mut text = format!("{prefix}:\n{}{}", " ".repeat(indent + 2), first);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    for line in tail {
        if !line.trim().is_empty() {
            text.push_str(&" ".repeat(indent + 2));
        }
        let n = line.len() - line.trim_start_matches(' ').len();
        text.push_str(&line[source_indent.min(n)..]);
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
fn field_patch(
    edits: &mut Edits,
    doc: &Document,
    entry: &Record,
    name: &str,
    value: Option<&str>,
) -> Result<(), MergeError> {
    let old = entry.fields.get(name);
    let indent = entry
        .fields
        .values()
        .next()
        .map(|f| {
            let key = doc.text[f.span.start..f.node.start]
                .split_once(':')
                .map_or("", |(key, _)| key);
            key.len() - key.trim_start_matches([' ', '-']).len()
        })
        .unwrap_or(entry.indent + 2);
    match (old, value) {
        (Some(old), Some(value)) => {
            let key = old.span.start;
            let prefix = doc.text[key..old.node.start]
                .split_once(':')
                .map(|(p, _)| p)
                .ok_or_else(|| MergeError::content("merge.yaml", "field has no source key"))?;
            edits.patch(
                &entry.path,
                old.span.clone(),
                field_text(prefix, value, indent),
            );
        }
        (Some(old), None) => edits.patch(&entry.path, old.span.clone(), String::new()),
        (None, Some(value)) => {
            let at = entry
                .children
                .as_ref()
                .map_or(entry.end, |n| line_start(&doc.text, n.start));
            let prefix = if at > 0 && !doc.text[..at].ends_with('\n') {
                "\n"
            } else {
                ""
            };
            edits.patch(
                &entry.path,
                at..at,
                format!(
                    "{prefix}{}",
                    field_text(&format!("{}{name}", " ".repeat(indent)), value, indent)
                ),
            );
        }
        (None, None) => {}
    }
    Ok(())
}
fn ours_only(report: &mut MergeReport, mut item: MergeConflict) -> MergeConflict {
    item.allowed = vec!["ours".into()];
    item.id.clear();
    item.id = format!(
        "MC{}",
        super::identity::digest(&serde_json::to_vec(&item).expect("serializable conflict"))
    );
    if let Some(stored) = report.conflicts.last_mut() {
        stored.allowed.clone_from(&item.allowed);
        stored.id.clone_from(&item.id);
    }
    item
}
fn report_field(
    report: &mut MergeReport,
    entry: &Record,
    address: &str,
    name: &str,
    kind: &str,
    b: Option<&Field>,
    o: Option<&Field>,
    t: Option<&Field>,
) -> MergeConflict {
    let keys = if entry.fields.contains_key("session") {
        vec!["root".into()]
    } else {
        vec![address.into()]
    };
    let item = conflict(
        report,
        &entry.path,
        address,
        name,
        kind,
        bytes(b),
        bytes(o),
        bytes(t),
        ConflictLocator::Yaml {
            keys,
            field: name.into(),
        },
    );
    if kind == "opaque_yaml" {
        ours_only(report, item)
    } else {
        item
    }
}
fn protected(
    base: &Inventory,
    ours: &Inventory,
    theirs: &Inventory,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let mut bad = false;
    let mut quiet = quiet_report(report);
    for (path, doc) in &base.docs {
        let destination = map.get(path).unwrap_or(path);
        if !ours.docs.contains_key(destination) || !theirs.docs.contains_key(path) {
            conflict(
                report,
                destination,
                destination,
                "file",
                "protected_document_delete",
                Some(doc.text.as_bytes()),
                ours.docs.get(destination).map(|d| d.text.as_bytes()),
                theirs.docs.get(path).map(|d| d.text.as_bytes()),
                ConflictLocator::Document,
            );
            bad = true;
        }
    }
    for (id, b) in &base.records {
        let doc = &base.docs[&b.path];
        if doc.kind == Kind::Index {
            continue;
        }
        let destination = map.source_target(id);
        let o = ours.records.get(destination);
        let t = theirs.records.get(id);
        if o.is_none() || t.is_none() {
            conflict(
                report,
                map.get(&b.path).map_or(b.path.as_str(), String::as_str),
                destination,
                "entry",
                "protected_delete",
                Some(base.docs[&b.path].text[b.span.clone()].as_bytes()),
                o.map(|e| ours.docs[&e.path].text[e.span.clone()].as_bytes()),
                t.map(|e| theirs.docs[&e.path].text[e.span.clone()].as_bytes()),
                ConflictLocator::Document,
            );
            bad = true;
            continue;
        }
        let o = o.unwrap();
        let t = t.unwrap();
        let parent = b.parent.as_ref().map(|p| map.get(p).unwrap_or(p));
        if o.parent.as_ref() != parent
            || t.parent != b.parent
            || o.path != *map.get(&b.path).unwrap_or(&b.path)
            || t.path != b.path
        {
            conflict(
                report,
                &o.path,
                destination,
                "parent",
                "protected_parent",
                Some(base.docs[&b.path].text[b.span.clone()].as_bytes()),
                Some(ours.docs[&o.path].text[o.span.clone()].as_bytes()),
                Some(theirs.docs[&t.path].text[t.span.clone()].as_bytes()),
                ConflictLocator::Document,
            );
            bad = true;
        }
        let names = field_names(&b.fields, &o.fields, &t.fields);
        for name in names {
            if doc.kind == Kind::Session
                && matches!(name.as_str(), "summary" | "last_turn" | "turn_count")
            {
                continue;
            }
            if doc.kind == Kind::Observations && PROMOTION.contains(&name.as_str()) {
                continue;
            }
            if doc.kind == Kind::Tree && matches!(name.as_str(), "also_depends_on" | "same_as")
                || doc.kind == Kind::Observations
                    && matches!(name.as_str(), "stale" | "conflict_annotations")
            {
                continue;
            }
            let key = if known_entry(doc.kind, name) {
                name.as_str()
            } else {
                "opaque"
            };
            let selector = if doc.kind == Kind::Mutations {
                match name.as_str() {
                    "from" => b.fields.get("from_selector"),
                    "to" => b.fields.get("to_selector"),
                    _ => None,
                }
                .map(|field| &field.node)
            } else {
                None
            };
            let bf = if key == "opaque"
                || doc.kind == Kind::Mutations && matches!(name.as_str(), "before" | "after")
            {
                b.fields.get(name).map(Cow::Borrowed)
            } else {
                mapped_field(
                    b.fields.get(name),
                    &o.path,
                    destination,
                    name,
                    map,
                    &mut quiet,
                    selector,
                )?
            };
            let same_source = if doc.kind == Kind::Mutations
                && matches!(
                    name.as_str(),
                    "from" | "to" | "from_selector" | "to_selector"
                ) {
                equal(b.fields.get(name), t.fields.get(name), key)
            } else {
                equal_mapped(b.fields.get(name), t.fields.get(name), key, map)
            };
            if !equal_mapped(bf.as_deref(), o.fields.get(name), key, map) || !same_source {
                report_field(
                    report,
                    o,
                    destination,
                    name,
                    "protected_field",
                    b.fields.get(name),
                    o.fields.get(name),
                    t.fields.get(name),
                );
                bad = true;
            }
        }
    }
    if bad {
        Err(reject_protected(report))
    } else {
        Ok(())
    }
}
fn scalar_value(raw: &str, node: &YamlNode, value: &str) -> String {
    use yaml_rust2::scanner::TScalarStyle;
    let token = match node.style {
        Some(TScalarStyle::SingleQuoted) => format!("'{}'", value.replace('\'', "''")),
        Some(TScalarStyle::DoubleQuoted) => {
            serde_json::to_string(value).expect("serializable scalar")
        }
        _ => value.into(),
    };
    format!("{token}{}", &raw[node.end - node.start..])
}
fn typed_heading(document: &str, ordinal: usize, heading: &str) -> bool {
    let prefix = match document {
        "logic/claims.md" => 'C',
        "logic/solution/heuristics.md" => 'H',
        "logic/experiments.md" => 'E',
        _ => return false,
    };
    ordinal == 1
        && super::identity::numeric_prefix(heading.split_once(':').map_or(heading, |(id, _)| id))
            == Some(prefix)
}
fn heading_prefix<'a>(
    heading: &'a str,
    map: &'a IdentityMap,
    historical: bool,
) -> Option<(&'a str, &'a str)> {
    let prefix = heading
        .split_once(':')
        .map_or(heading, |(prefix, _)| prefix);
    super::identity::numeric_prefix(prefix)?;
    let stable = map.get(prefix)?;
    let target = if historical {
        stable.as_str()
    } else {
        map.source_target(prefix)
    };
    (target != prefix).then_some((prefix, target))
}
fn heading_value(
    raw: &str,
    node: &YamlNode,
    path: &str,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    historical: bool,
) -> Option<String> {
    let heading = node.scalar()?;
    let (prefix, target) = heading_prefix(heading, map, historical)?;
    report.rewritten.push(super::types::RewriteFact {
        path: path.into(),
        selector: id.into(),
        old: prefix.into(),
        new: target.into(),
        confidence: "certain".into(),
    });
    let start = usize::from(matches!(
        node.style,
        Some(
            yaml_rust2::scanner::TScalarStyle::SingleQuoted
                | yaml_rust2::scanner::TScalarStyle::DoubleQuoted
        )
    ));
    if raw[start..].starts_with(prefix) {
        Some(format!(
            "{}{target}{}",
            &raw[..start],
            &raw[start + prefix.len()..]
        ))
    } else {
        Some(scalar_value(
            raw,
            node,
            &format!("{target}{}", &heading[prefix.len()..]),
        ))
    }
}
fn mutation_pointer(
    raw: &str,
    node: &YamlNode,
    selector: Option<&YamlNode>,
    path: &str,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    historical: bool,
) -> Result<String, MergeError> {
    if let Some(value) = node.scalar()
        && let Some((scope, tail)) = value.split_once(':').or_else(|| value.split_once('#'))
    {
        if let Some(selector) = selector
            && selector.get("document")?.and_then(YamlNode::scalar) == Some(scope)
            && let Some(headings) = selector.get("heading")?
        {
            let components: Vec<_> = headings
                .sequence()?
                .iter()
                .map(|node| {
                    node.scalar().ok_or_else(|| {
                        MergeError::content(
                            "merge.mutation",
                            "heading selector must contain strings",
                        )
                    })
                })
                .collect::<Result<_, _>>()?;
            if !components.is_empty() && components.join("/") == tail {
                let mut changed = false;
                let mut mapped = Vec::with_capacity(components.len());
                for (ordinal, component) in components.into_iter().enumerate() {
                    if typed_heading(scope, ordinal, component)
                        && let Some((prefix, target)) = heading_prefix(component, map, historical)
                    {
                        changed = true;
                        mapped.push(std::borrow::Cow::Owned(format!(
                            "{target}{}",
                            &component[prefix.len()..]
                        )));
                        continue;
                    }
                    mapped.push(std::borrow::Cow::Borrowed(component));
                }
                if changed {
                    let delimiter = &value[scope.len()..scope.len() + 1];
                    let mapped = format!("{scope}{delimiter}{}", mapped.join("/"));
                    report.rewritten.push(super::types::RewriteFact {
                        path: path.into(),
                        selector: id.into(),
                        old: value.into(),
                        new: mapped.clone(),
                        confidence: "certain".into(),
                    });
                    return Ok(scalar_value(raw, node, &mapped));
                }
                return Ok(raw.into());
            }
        }
        let native = super::identity::normalize_local(value);
        if let Some(target) = map.get(&native)
            && target != &native
        {
            let delimiter = &value[scope.len()..scope.len() + 1];
            let target = if historical {
                target.as_str()
            } else if super::identity::numeric_prefix(target).is_some() {
                map.source_target(&native)
            } else {
                target.as_str()
            };
            let mapped = if let Some((document, entry)) = target.split_once('#') {
                format!("{document}{delimiter}{entry}")
            } else {
                format!("{scope}{delimiter}{target}")
            };
            report.rewritten.push(super::types::RewriteFact {
                path: path.into(),
                selector: id.into(),
                old: value.into(),
                new: mapped.clone(),
                confidence: "certain".into(),
            });
            return Ok(scalar_value(raw, node, &mapped));
        }
    }
    if historical {
        rewrite::yaml_value_historical(raw, path, id, map, report, true)
    } else {
        rewrite::yaml_value(raw, path, id, map, report, true)
    }
}
fn selector_value(
    raw: &str,
    node: &YamlNode,
    path: &str,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    historical: bool,
) -> Result<String, MergeError> {
    if !matches!(node.kind, YamlKind::Mapping(_)) {
        return Ok(raw.into());
    }
    let mut patches = Vec::new();
    let document = node.get("document")?.and_then(YamlNode::scalar);
    for (key, value) in node.mapping()? {
        if key.scalar() == Some("heading") {
            for (ordinal, component) in value.sequence()?.iter().enumerate() {
                if document.is_none_or(|document| {
                    component
                        .scalar()
                        .is_none_or(|heading| !typed_heading(document, ordinal, heading))
                }) {
                    continue;
                }
                let range = component.start - node.start..component.end - node.start;
                if let Some(mapped) = heading_value(
                    &raw[range.clone()],
                    component,
                    path,
                    id,
                    map,
                    report,
                    historical,
                ) {
                    patches.push((range, mapped));
                }
            }
            continue;
        }
        if !key
            .scalar()
            .is_some_and(|key| matches!(key, "id" | "entry" | "document"))
        {
            continue;
        }
        let range = value.start - node.start..value.end - node.start;
        let source = &raw[range.clone()];
        let mapped = if historical {
            rewrite::yaml_value_historical(source, path, id, map, report, true)?
        } else {
            rewrite::yaml_value(source, path, id, map, report, true)?
        };
        if mapped != source {
            patches.push((range, mapped));
        }
    }
    patches.sort_by_key(|(range, _)| range.start);
    let mut output = String::new();
    let mut at = 0;
    for (range, value) in patches {
        output.push_str(&raw[at..range.start]);
        output.push_str(&value);
        at = range.end;
    }
    output.push_str(&raw[at..]);
    Ok(output)
}
fn mapped_field<'a>(
    field: Option<&'a Field>,
    path: &str,
    id: &str,
    name: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    selector: Option<&YamlNode>,
) -> Result<Option<Cow<'a, Field>>, MergeError> {
    field
        .map(|f| {
            if !known(name) && !matches!(name, "session" | "historical_references") {
                return Ok(Cow::Borrowed(f));
            }
            let historical =
                path == "trace/logic_mutations.yaml" && matches!(name, "from" | "from_selector");
            let raw = if path == "trace/logic_mutations.yaml" && matches!(name, "from" | "to") {
                mutation_pointer(&f.raw, &f.node, selector, path, id, map, report, historical)?
            } else if matches!(name, "from_selector" | "to_selector")
                || name == "entry" && matches!(f.node.kind, YamlKind::Mapping(_))
            {
                selector_value(&f.raw, &f.node, path, id, map, report, historical)?
            } else if historical {
                rewrite::yaml_value_historical(
                    &f.raw,
                    path,
                    id,
                    map,
                    report,
                    structured_field(path, name),
                )?
            } else {
                rewrite_field(&f.raw, &f.node, path, id, name, map, report)?
            };
            if raw == f.raw {
                return Ok(Cow::Borrowed(f));
            }
            let source = if matches!(f.node.kind, YamlKind::Mapping(_) | YamlKind::Sequence(_))
                && !f.node.flow
            {
                let mut lines = raw.split_inclusive('\n');
                let mut value = format!("value:\n  {}", lines.next().unwrap_or(""));
                for line in lines {
                    let n = line.len() - line.trim_start_matches(' ').len();
                    value.push_str("  ");
                    value.push_str(&line[f.column.min(n)..]);
                }
                value
            } else {
                format!("value: {raw}")
            };
            let parsed = YamlDocument::parse(&source)?;
            let node = parsed
                .root
                .get("value")?
                .ok_or_else(|| MergeError::content("merge.yaml", "missing rewritten value"))?
                .clone();
            Ok(Cow::Owned(Field {
                node,
                span: f.span.clone(),
                raw,
                column: f.column,
                semantic: OnceLock::new(),
            }))
        })
        .transpose()
}
fn merge_mutable(
    edits: &mut Edits,
    doc: &Document,
    b: &Record,
    o: &Record,
    t: &Record,
    id: &str,
    name: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let incoming = mapped_field(t.fields.get(name), &o.path, id, name, map, report, None)?;
    let mut quiet = quiet_report(report);
    let prior = mapped_field(b.fields.get(name), &o.path, id, name, map, &mut quiet, None)?;
    let bf = prior.as_deref();
    let of = o.fields.get(name);
    let tf = incoming.as_deref();
    if equal_mapped(bf, tf, name, map) || equal_mapped(of, tf, name, map) {
        return Ok(());
    }
    if equal_mapped(bf, of, name, map) {
        field_patch(edits, doc, o, name, tf.map(|f| f.raw.as_str()))?;
    } else {
        report_field(
            report,
            o,
            id,
            name,
            "mutable_field",
            b.fields.get(name),
            of,
            t.fields.get(name),
        );
    }
    Ok(())
}
fn append_set(
    edits: &mut Edits,
    doc: &Document,
    b: &Record,
    o: &Record,
    t: &Record,
    id: &str,
    name: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let rows = |r: &Record| -> Result<Vec<Value>, MergeError> {
        r.fields
            .get(name)
            .map(|f| {
                f.node.to_json().and_then(|value| match value {
                    Value::Array(rows) => Ok(rows),
                    _ => Err(crate::write::WriteError::semantic(
                        "merge.yaml",
                        "append field requires sequence",
                    )),
                })
            })
            .transpose()
            .map(|v| v.unwrap_or_default())
            .map_err(Into::into)
    };
    let bv = rows(b)?;
    let ov = rows(o)?;
    let tv = rows(t)?;
    let mut quiet = quiet_report(report);
    let mapped_base = mapped_field(b.fields.get(name), &o.path, id, name, map, &mut quiet, None)?
        .map(|f| f.node.to_json())
        .transpose()?
        .and_then(|value| match value {
            Value::Array(rows) => Some(rows),
            _ => None,
        })
        .unwrap_or_default();
    let mapped_theirs = mapped_field(t.fields.get(name), &o.path, id, name, map, &mut quiet, None)?
        .map(|f| f.node.to_json())
        .transpose()?
        .and_then(|value| match value {
            Value::Array(rows) => Some(rows),
            _ => None,
        })
        .unwrap_or_default();
    let prefix = |candidate: &[Value], base: &[Value]| {
        candidate.len() >= base.len()
            && candidate
                .iter()
                .zip(base)
                .all(|(a, b)| rewrite::references_equal(a, b, map))
    };
    if !mapped_base.iter().all(|base| {
        ov.iter()
            .any(|row| rewrite::references_equal(row, base, map))
    }) || tv.len() < bv.len()
        || !prefix(&mapped_theirs, &mapped_base)
    {
        report_field(
            report,
            o,
            id,
            name,
            "protected_list",
            b.fields.get(name),
            o.fields.get(name),
            t.fields.get(name),
        );
        return Err(reject_protected(report));
    }
    let Some(field) = t.fields.get(name) else {
        return Ok(());
    };
    let mut seen = ov;
    let mut added = Vec::new();
    for (ordinal, row) in field.node.sequence()?.iter().enumerate().skip(bv.len()) {
        let raw = row_value(field, row, ordinal);
        let raw = rewrite::yaml_value(&raw, &o.path, id, map, report, true)?;
        let parsed = YamlDocument::parse(&format!("value: {raw}"))?;
        let value = parsed
            .root
            .get("value")?
            .ok_or_else(|| MergeError::content("merge.yaml", "missing appended value"))?
            .to_json()?;
        if !seen
            .iter()
            .any(|prior| rewrite::references_equal(prior, &value, map))
        {
            seen.push(value);
            added.push(raw);
        }
    }
    append_values(edits, doc, o, name, &added)
}
fn promotion_bytes(fields: &BTreeMap<String, Field>) -> Vec<u8> {
    let tuple: BTreeMap<&str, &str> = PROMOTION
        .into_iter()
        .filter_map(|name| fields.get(name).map(|field| (name, field.raw.as_str())))
        .collect();
    serde_json::to_vec(&tuple).expect("serializable exact promotion tuple")
}
pub(crate) fn rewrite_choice(
    text: &str,
    path: &str,
    selector: &str,
    name: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<String, MergeError> {
    if name == "$promotion" {
        let mut tuple: BTreeMap<String, String> = serde_json::from_str(text)
            .map_err(|error| MergeError::content("merge.promotion", error.to_string()))?;
        if tuple.keys().any(|name| !PROMOTION.contains(&name.as_str())) {
            return Err(MergeError::content(
                "merge.promotion",
                "unexpected promotion tuple field",
            ));
        }
        for (name, raw) in &mut tuple {
            let document = YamlDocument::parse(raw)?;
            *raw = rewrite_field(raw, &document.root, path, selector, name, map, report)?;
        }
        return serde_json::to_string(&tuple)
            .map_err(|error| MergeError::content("merge.promotion", error.to_string()));
    }
    let document = YamlDocument::parse(text)?;
    rewrite_field(text, &document.root, path, selector, name, map, report)
}
fn promotion(
    edits: &mut Edits,
    doc: &Document,
    b: &Record,
    o: &Record,
    t: &Record,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    pending: &[MergeConflict],
) -> Result<(), MergeError> {
    let mut incoming = BTreeMap::new();
    for name in PROMOTION {
        if let Some(field) = mapped_field(t.fields.get(name), &o.path, id, name, map, report, None)?
        {
            incoming.insert(name.to_owned(), field.into_owned());
        }
    }
    let tuple = |r: &BTreeMap<String, Field>| -> Result<Value, MergeError> {
        let mut v = serde_json::Map::new();
        for name in PROMOTION {
            if let Some(f) = r.get(name) {
                v.insert(name.into(), f.node.to_json()?);
            }
        }
        Ok(Value::Object(v))
    };
    let mut prior = BTreeMap::new();
    let mut quiet = quiet_report(report);
    for name in PROMOTION {
        if let Some(field) =
            mapped_field(b.fields.get(name), &o.path, id, name, map, &mut quiet, None)?
        {
            prior.insert(name.to_owned(), field.into_owned());
        }
    }
    let bv = tuple(&prior)?;
    let ov = tuple(&o.fields)?;
    let tv = tuple(&incoming)?;
    let valid = |v: &Value| {
        if v.get("promoted") == Some(&json!(true)) {
            v.get("promoted_to")
                .and_then(Value::as_str)
                .is_some_and(promotion_target)
                && v.get("crystallized_via")
                    .and_then(Value::as_str)
                    .is_some_and(promotion_signal)
        } else {
            v.get("promoted").is_none_or(|p| p == &json!(false))
                && v.get("promoted_to").is_none_or(Value::is_null)
                && v.get("crystallized_via").is_none_or(Value::is_null)
        }
    };
    let same_ours = rewrite::references_equal(&ov, &bv, map);
    let same_theirs = rewrite::references_equal(&tv, &bv, map);
    let same_sides = rewrite::references_equal(&ov, &tv, map);
    if bv.get("promoted") == Some(&json!(true))
        && pending.iter().any(|conflict| {
            conflict.kind == "promotion"
                && conflict.path == o.path
                && conflict.selector == id
                && conflict.source_key == report.source_key
                && conflict.source_revision == report.source_revision
                && conflict.ours == super::types::MergeValue::new(Some(&promotion_bytes(&o.fields)))
                && conflict.theirs
                    == super::types::MergeValue::new(Some(&promotion_bytes(&t.fields)))
                && serde_json::from_slice::<BTreeMap<String, String>>(&conflict.base.bytes)
                    .ok()
                    .and_then(|tuple| tuple.get("promoted").cloned())
                    .and_then(|raw| YamlDocument::parse(&raw).ok())
                    .and_then(|doc| doc.root.to_json().ok())
                    == Some(json!(false))
        })
    {
        return Ok(());
    }
    let regression = bv.get("promoted") == Some(&json!(true)) && (!same_ours || !same_theirs);
    if regression {
        for name in PROMOTION {
            report_field(
                report,
                o,
                id,
                name,
                "protected_promotion",
                b.fields.get(name),
                o.fields.get(name),
                t.fields.get(name),
            );
        }
        return Err(reject_protected(report));
    }
    if !valid(&ov) || !valid(&tv) || (!same_ours && !same_theirs && !same_sides) {
        let base = promotion_bytes(&b.fields);
        let ours = promotion_bytes(&o.fields);
        let theirs = promotion_bytes(&t.fields);
        conflict(
            report,
            &o.path,
            id,
            "$promotion",
            "promotion",
            Some(&base),
            Some(&ours),
            Some(&theirs),
            ConflictLocator::Yaml {
                keys: vec![id.into()],
                field: "$promotion".into(),
            },
        );
        return Ok(());
    }
    if same_ours && !same_theirs {
        for name in PROMOTION {
            field_patch(
                edits,
                doc,
                o,
                name,
                incoming.get(name).map(|f| f.raw.as_str()),
            )?;
        }
    }
    Ok(())
}

/// Extract a complete source row, excluding only its surrounding sequence marker.
fn row_value(field: &Field, row: &YamlNode, ordinal: usize) -> String {
    let raw = &field.raw;
    let list = &field.node;
    let start = row.start.saturating_sub(list.start);
    let end = if list.flow {
        row.end.saturating_sub(list.start)
    } else {
        list.sequence()
            .ok()
            .and_then(|items| items.get(ordinal + 1))
            .map_or(raw.len(), |next| {
                line_start(raw, next.start.saturating_sub(list.start))
            })
    };
    let piece = &raw[start.min(raw.len())..end.min(raw.len())];
    if list.flow {
        return piece.to_owned();
    }
    let local_line = line_start(raw, start);
    let column = start - local_line + if local_line == 0 { field.column } else { 0 };
    let mut lines = piece.split_inclusive('\n');
    let mut output = lines.next().unwrap_or("").to_owned();
    for line in lines {
        let indentation = line.len() - line.trim_start_matches(' ').len();
        output.push_str(&line[column.min(indentation)..]);
    }
    output.trim_end_matches(['\r', '\n']).to_owned()
}
fn append_values(
    edits: &mut Edits,
    doc: &Document,
    entry: &Record,
    name: &str,
    values: &[String],
) -> Result<(), MergeError> {
    if values.is_empty() {
        return Ok(());
    }
    let Some(field) = entry.fields.get(name) else {
        let value = format!(
            "\n{}",
            values
                .iter()
                .map(|s| render_row(s, entry.indent + 2))
                .collect::<String>()
        );
        return field_patch(edits, doc, entry, name, Some(&value));
    };
    let list = &field.node;
    let items = list.sequence()?;
    if list.flow {
        if values.iter().all(|s| {
            YamlDocument::parse(s)
                .is_ok_and(|d| matches!(d.root.kind, YamlKind::Scalar { .. }) || d.root.flow)
        }) {
            let at = list.end - 1;
            let prefix = if items.is_empty() { "" } else { ", " };
            let value = edits
                .appends
                .entry((entry.path.clone(), at))
                .or_insert((at..at, prefix.into()));
            if value.1 != prefix {
                value.1.push_str(", ");
            }
            value.1.push_str(&values.join(", "));
            return Ok(());
        }
        if !items.is_empty() {
            return Err(MergeError::content(
                "merge.unsupported_source",
                "lossless block row cannot be appended into a nonempty flow list",
            ));
        }
    }
    let indent = items
        .first()
        .map(|n| {
            let p = &doc.text[line_start(&doc.text, n.start)..n.start];
            p.find('-').unwrap_or(p.len())
        })
        .unwrap_or(entry.indent + 2);
    for value in values {
        let fragment = render_row(value, indent);
        edits.append(&entry.path, list, &doc.text, &fragment, indent)?;
    }
    Ok(())
}
fn render_row(value: &str, indent: usize) -> String {
    let mut lines = value.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let mut fragment = format!("{}- {}", " ".repeat(indent), first);
    if !fragment.ends_with('\n') {
        fragment.push('\n');
    }
    for line in lines {
        if !line.trim().is_empty() {
            fragment.push_str(&" ".repeat(indent + 2));
        }
        fragment.push_str(line);
    }
    if !fragment.ends_with('\n') {
        fragment.push('\n');
    }
    fragment
}
fn root_record(doc: &Document, path: &str) -> Result<Record, MergeError> {
    record(&doc.text, path, &doc.parsed.root, None, 0..doc.text.len()).map(|mut r| {
        r.indent = 0;
        r
    })
}
fn row_equal(
    a: &YamlNode,
    atext: &str,
    b: &YamlNode,
    btext: &str,
    map: &IdentityMap,
) -> Result<bool, MergeError> {
    if !matches!(a.kind, YamlKind::Mapping(_)) || !matches!(b.kind, YamlKind::Mapping(_)) {
        return Ok(a.to_json()? == b.to_json()?);
    }
    fn indexed_fields<'a>(
        text: &'a str,
        node: &'a YamlNode,
    ) -> Result<BTreeMap<&'a str, (&'a YamlNode, &'a str)>, MergeError> {
        let mut fields = BTreeMap::new();
        for (key, value) in node.mapping()? {
            let name = key
                .scalar()
                .ok_or_else(|| MergeError::content("merge.yaml", "complex YAML entry key"))?;
            if name == "children" {
                continue;
            }
            let end = if node.flow {
                value.end
            } else {
                field_range(text, node, name)?
                    .ok_or_else(|| MergeError::content("merge.yaml", "missing indexed field"))?
                    .end
                    .max(value.end)
            };
            if fields
                .insert(name, (value, &text[value.start..end]))
                .is_some()
            {
                return Err(MergeError::content(
                    "merge.yaml",
                    format!("duplicate entry field {name}"),
                ));
            }
        }
        Ok(fields)
    }
    let af = indexed_fields(atext, a)?;
    let bf = indexed_fields(btext, b)?;
    if af.len() != bf.len() {
        return Ok(false);
    }
    Ok(af.into_iter().all(|(name, (a, araw))| {
        let Some(&(b, braw)) = bf.get(name) else {
            return false;
        };
        if araw == braw && a.same_source_value(b) {
            return true;
        }
        let key = if name == "id" {
            "target"
        } else if known_row(name) {
            name
        } else {
            "opaque"
        };
        if !known(key) {
            return a.same_source_value(b);
        }
        match (a.to_json(), b.to_json()) {
            (Ok(a), Ok(b)) if structured_field("", key) && key != "concepts" => {
                rewrite::references_equal(&a, &b, map)
            }
            (Ok(a), Ok(b)) => a == b,
            _ => araw == braw,
        }
    }))
}
fn occurrence_ordinal(
    map: &IdentityMap,
    source_owner: &str,
    destination_owner: &str,
    name: &str,
    ordinal: usize,
) -> Result<usize, MergeError> {
    let original = format!("{source_owner}#{name}/{ordinal}");
    let Some(target) = map.get(&original) else {
        return Ok(ordinal);
    };
    let prefix = format!("{destination_owner}#{name}/");
    target
        .strip_prefix(&prefix)
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| {
            MergeError::content(
                "merge.occurrence",
                "established append occurrence belongs to a different destination",
            )
        })
}
/// Frozen rows compare exactly at their recorded destination occurrences. The
/// source allocation is applied once; prose and before/after stay opaque.
fn closed_destination_value(
    source: &Document,
    source_path: &str,
    destination_path: &str,
    id: &str,
    map: &IdentityMap,
    report: &MergeReport,
) -> Result<Value, MergeError> {
    let mut quiet = quiet_report(report);
    let relocated = rewrite_source(
        &source.text,
        &source.parsed.root,
        0..source.text.len(),
        destination_path,
        id,
        map,
        &mut quiet,
    )?;
    let mut expected = YamlDocument::parse(&relocated)?.root.to_json()?;
    for name in HISTORY {
        let Some(Value::Array(rows)) = expected.get_mut(name) else {
            continue;
        };
        let source_rows = std::mem::take(rows);
        let mut occurrences = vec![None; source_rows.len()];
        for (ordinal, row) in source_rows.into_iter().enumerate() {
            let destination =
                occurrence_ordinal(map, source_path, destination_path, name, ordinal)?;
            let slot = occurrences.get_mut(destination).ok_or_else(|| {
                MergeError::content(
                    "merge.occurrence",
                    "frozen session occurrence is outside its recorded history",
                )
            })?;
            if slot.replace(row).is_some() {
                return Err(MergeError::content(
                    "merge.occurrence",
                    "frozen session occurrences overlap",
                ));
            }
        }
        *rows = occurrences
            .into_iter()
            .map(|row| {
                row.ok_or_else(|| {
                    MergeError::content("merge.occurrence", "frozen session occurrence is missing")
                })
            })
            .collect::<Result<_, _>>()?;
    }
    Ok(expected)
}
fn session_history(
    edits: &mut Edits,
    bd: &Document,
    od: &Document,
    td: &Document,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let br = root_record(bd, &format!("trace/sessions/{id}.yaml"))?;
    let or = root_record(od, &format!("trace/sessions/{id}.yaml"))?;
    let tr = root_record(td, &format!("trace/sessions/{id}.yaml"))?;
    for name in HISTORY {
        let b = history_rows(&br, name)?;
        let o = history_rows(&or, name)?;
        let t = history_rows(&tr, name)?;
        let mut invalid = o.len() < b.len() || t.len() < b.len();
        for (ordinal, row) in b.iter().enumerate() {
            if let Some(peer) = t.get(ordinal) {
                invalid |= !row_equal(row, &bd.text, peer, &td.text, map)?;
            }
            let source_id = bd
                .parsed
                .root
                .get("session")?
                .and_then(|node| node.get("id").ok().flatten())
                .and_then(YamlNode::scalar)
                .ok_or_else(|| {
                    MergeError::content("merge.session", "missing historical source session")
                })?;
            let destination_ordinal = occurrence_ordinal(
                map,
                &format!("trace/sessions/{source_id}.yaml"),
                &or.path,
                name,
                ordinal,
            )?;
            if let Some(peer) = o.get(destination_ordinal) {
                let raw = row_value(&br.fields[name], row, ordinal);
                let mut quiet = quiet_report(report);
                let original = YamlDocument::parse(&raw)?;
                let mapped = rewrite_source(
                    &raw,
                    &original.root,
                    0..raw.len(),
                    &or.path,
                    id,
                    map,
                    &mut quiet,
                )?;
                let parsed = YamlDocument::parse(&mapped)?;
                invalid |= !row_equal(&parsed.root, &mapped, peer, &od.text, map)?;
            } else {
                invalid = true;
            }
        }
        if invalid {
            report_field(
                report,
                &or,
                id,
                name,
                "protected_session_rows",
                br.fields.get(name),
                or.fields.get(name),
                tr.fields.get(name),
            );
            return Err(reject_protected(report));
        }
        if let Some(field) = tr.fields.get(name) {
            let mut incoming = Vec::new();
            for (ordinal, row) in t.iter().enumerate().skip(b.len()) {
                check_extensions(row, &td.text, &or.path, id, map, report)?;
                let raw = row_value(field, row, ordinal);
                let parsed = YamlDocument::parse(&raw)?;
                incoming.push(rewrite_source(
                    &raw,
                    &parsed.root,
                    0..raw.len(),
                    &or.path,
                    &format!("{id}/{name}/{ordinal}"),
                    map,
                    report,
                )?);
            }
            // Append ordinal, never content equality or turn number, is occurrence identity.
            append_values(edits, od, &or, name, &incoming)?;
        }
    }
    let names = field_names(&br.fields, &or.fields, &tr.fields);
    for name in names {
        if name == "session" || HISTORY.contains(&name.as_str()) {
            continue;
        }
        if matches!(name.as_str(), "open_threads" | "ai_suggestions_pending") {
            merge_mutable(edits, od, &br, &or, &tr, id, name, map, report)?;
        } else if !equal(br.fields.get(name), or.fields.get(name), "opaque")
            || !equal(br.fields.get(name), tr.fields.get(name), "opaque")
        {
            report_field(
                report,
                &or,
                id,
                name,
                "protected_session_extension",
                br.fields.get(name),
                or.fields.get(name),
                tr.fields.get(name),
            );
            return Err(reject_protected(report));
        }
    }
    Ok(())
}
fn history_rows<'a>(record: &'a Record, name: &str) -> Result<&'a [YamlNode], MergeError> {
    record
        .fields
        .get(name)
        .map(|f| f.node.sequence())
        .transpose()
        .map(|v| v.unwrap_or(&[]))
        .map_err(Into::into)
}
fn opaque_roots(
    base: &Inventory,
    ours: &Inventory,
    theirs: &Inventory,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    for path in base.paths.union(&theirs.paths) {
        let Some(td) = theirs.docs.get(path) else {
            continue;
        };
        if td.kind == Kind::Session {
            continue;
        }
        let root_fields = |doc: &Document| {
            if doc.kind == Kind::Tree {
                fields_except(&doc.text, &doc.parsed.root, &[doc.kind.root(), "root"])
            } else {
                fields_except(&doc.text, &doc.parsed.root, &[doc.kind.root()])
            }
        };
        let br = base.docs.get(path).map(root_fields).transpose()?;
        let or = ours.docs.get(path).map(root_fields).transpose()?;
        let tr = root_fields(td)?;
        let names: BTreeSet<_> = tr
            .keys()
            .chain(br.iter().flat_map(|fields| fields.keys()))
            .chain(or.iter().flat_map(|fields| fields.keys()))
            .collect();
        for name in names {
            let b = br.as_ref().and_then(|fields| fields.get(name));
            let o = or.as_ref().and_then(|fields| fields.get(name));
            let t = tr.get(name);
            if !equal(b, t, "opaque") && !equal(o, t, "opaque") {
                let item = conflict(
                    report,
                    path,
                    "root",
                    name,
                    "opaque_yaml",
                    bytes(b),
                    bytes(o),
                    bytes(t),
                    ConflictLocator::Yaml {
                        keys: vec!["root".into()],
                        field: name.clone(),
                    },
                );
                ours_only(report, item);
            }
        }
    }
    Ok(())
}
pub(crate) fn apply(
    base: &Inventory,
    ours: &Inventory,
    theirs: &Inventory,
    map: &IdentityMap,
    working: &mut WorkingArtifact,
    report: &mut MergeReport,
    pending: &[MergeConflict],
    inherited: &super::origin::Inherited,
) -> Result<(), MergeError> {
    protected(base, ours, theirs, map, report)?;
    opaque_roots(base, ours, theirs, report)?;
    opaque_comments(base, ours, theirs, map, report);
    check_incoming_extensions(base, theirs, map, report)?;
    let mut edits = Edits::default();
    for (native, b) in &base.records {
        if native.starts_with("index:") {
            let source = native.trim_start_matches("index:");
            let destination = format!("index:{}", map.get(source).map_or(source, String::as_str));
            if let (Some(o), Some(t)) = (ours.records.get(&destination), theirs.records.get(native))
            {
                let names = field_names(&b.fields, &o.fields, &t.fields);
                for name in names {
                    if matches!(
                        name.as_str(),
                        "id" | "date"
                            | "timestamp"
                            | "path"
                            | "summary"
                            | "turn_count"
                            | "events_count"
                            | "claims_touched"
                            | "open_threads"
                    ) {
                        continue;
                    }
                    let bf = b.fields.get(name);
                    let of = o.fields.get(name);
                    let tf = t.fields.get(name);
                    if !equal(bf, tf, "opaque") && !equal(of, tf, "opaque") {
                        report_field(report, o, &destination, name, "opaque_yaml", bf, of, tf);
                    }
                }
            }
            continue;
        }
        let id = map.source_target(native);
        let Some(o) = ours.records.get(id) else {
            continue;
        };
        let Some(t) = theirs.records.get(native) else {
            continue;
        };
        let doc = &ours.docs[&o.path];
        let kind = doc.kind;
        if kind == Kind::Index {
            continue;
        }
        if kind == Kind::Tree {
            for name in ["also_depends_on", "same_as"] {
                if b.fields.contains_key(name)
                    || o.fields.contains_key(name)
                    || t.fields.contains_key(name)
                {
                    append_set(&mut edits, doc, b, o, t, id, name, map, report)?;
                }
            }
        }
        if kind == Kind::Observations {
            append_annotations(
                &mut edits,
                doc,
                &theirs.docs[&t.path],
                b,
                o,
                t,
                id,
                map,
                report,
            )?;
        }
        if kind == Kind::Observations
            && (b.fields.contains_key("stale")
                || o.fields.contains_key("stale")
                || t.fields.contains_key("stale"))
        {
            let state = |r: &Record| {
                r.fields
                    .get("stale")
                    .and_then(|f| f.node.to_json().ok())
                    .and_then(|v| v.as_bool())
            };
            if [b, o, t]
                .iter()
                .any(|r| r.fields.contains_key("stale") && state(r).is_none())
                || b.fields.contains_key("stale")
                    && (!o.fields.contains_key("stale") || !t.fields.contains_key("stale"))
                || state(b) == Some(true) && (state(o) != Some(true) || state(t) != Some(true))
            {
                report_field(
                    report,
                    o,
                    id,
                    "stale",
                    "protected_stale",
                    b.fields.get("stale"),
                    o.fields.get("stale"),
                    t.fields.get("stale"),
                );
                return Err(reject_protected(report));
            }
            if state(o) != Some(true) && state(t) == Some(true)
                || !o.fields.contains_key("stale") && t.fields.contains_key("stale")
            {
                field_patch(
                    &mut edits,
                    doc,
                    o,
                    "stale",
                    t.fields.get("stale").map(|f| f.raw.as_str()),
                )?;
            }
        }
        if kind == Kind::Observations {
            promotion(&mut edits, doc, b, o, t, id, map, report, pending)?;
        }
        if kind == Kind::Session {
            let closed = closed_session(&b.fields);
            let frozen = &base.docs[&b.path];
            let mapped_frozen = if closed {
                Some(closed_destination_value(
                    frozen, &b.path, &o.path, id, map, report,
                )?)
            } else {
                None
            };
            if closed
                && (mapped_frozen.as_ref() != Some(&doc.parsed.root.to_json()?)
                    || frozen.parsed.root.to_json()?
                        != theirs.docs[&t.path].parsed.root.to_json()?)
            {
                conflict(
                    report,
                    &o.path,
                    id,
                    "entry",
                    "protected_closed_session",
                    Some(base.docs[&b.path].text.as_bytes()),
                    Some(doc.text.as_bytes()),
                    Some(theirs.docs[&t.path].text.as_bytes()),
                    ConflictLocator::Document,
                );
                return Err(reject_protected(report));
            }
            merge_mutable(&mut edits, doc, b, o, t, id, "summary", map, report)?;
            if let (Some(bc), Some(oc), Some(tc)) = (
                b.fields.get("turn_count"),
                o.fields.get("turn_count"),
                t.fields.get("turn_count"),
            ) {
                let count = |f: &Field| {
                    f.node
                        .to_json()
                        .ok()
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| MergeError::content("merge.session", "invalid turn count"))
                };
                let (bc, oc, tc) = (count(bc)?, count(oc)?, count(tc)?);
                if oc < bc || tc < bc {
                    report_field(
                        report,
                        o,
                        id,
                        "turn_count",
                        "protected_turn_count",
                        b.fields.get("turn_count"),
                        o.fields.get("turn_count"),
                        t.fields.get("turn_count"),
                    );
                    return Err(reject_protected(report));
                }
                let merged = oc
                    .checked_add(tc - bc)
                    .ok_or_else(|| MergeError::content("merge.session", "turn count overflow"))?;
                if merged != oc {
                    field_patch(&mut edits, doc, o, "turn_count", Some(&merged.to_string()))?;
                }
            }
            if let (Some(bf), Some(of), Some(tf)) = (
                b.fields.get("last_turn"),
                o.fields.get("last_turn"),
                t.fields.get("last_turn"),
            ) {
                let key = |f: &Field| {
                    f.node
                        .scalar()
                        .ok_or_else(|| {
                            MergeError::content("merge.session", "timestamp must be a string")
                        })
                        .and_then(|s| crate::write::sessions::timestamp_key(s).map_err(Into::into))
                };
                let (bk, ok, tk) = (key(bf)?, key(of)?, key(tf)?);
                if ok < bk || tk < bk {
                    report_field(
                        report,
                        o,
                        id,
                        "last_turn",
                        "protected_session_chronology",
                        Some(bf),
                        Some(of),
                        Some(tf),
                    );
                    return Err(reject_protected(report));
                }
                if tk > ok {
                    field_patch(&mut edits, doc, o, "last_turn", Some(&tf.raw))?;
                }
            }
            session_history(
                &mut edits,
                &base.docs[&b.path],
                doc,
                &theirs.docs[&t.path],
                id,
                map,
                report,
            )?;
        }
    }
    // Source-only subtree roots are copied once; descendants are already in that span.
    let mut added_paths = BTreeMap::new();
    for (path, td) in &theirs.docs {
        if td.kind == Kind::Session {
            let native = &td.order[0];
            if !base.records.contains_key(native) {
                let id = map.get(native).unwrap_or(native);
                let destination = map
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| format!("trace/sessions/{id}.yaml"));
                if inherited.contains(native) && ours.docs.contains_key(&destination) {
                    inherited_session(
                        td,
                        &ours.docs[&destination],
                        &destination,
                        id,
                        inherited.yaml(native),
                        map,
                        report,
                    )?;
                    continue;
                }
                if working.exists(&destination) {
                    return Err(MergeError::content(
                        "merge.identity",
                        format!("session import collision {id}"),
                    ));
                }
                let incoming = rewrite_source(
                    &td.text,
                    &td.parsed.root,
                    0..td.text.len(),
                    &destination,
                    id,
                    map,
                    report,
                )?;
                working.stage_create(&destination, incoming.as_bytes())?;
            }
            continue;
        }
        let destination = map.get(path).unwrap_or(path);
        if !ours.docs.contains_key(destination) && !added_paths.contains_key(destination) {
            let text = format!("{}: []\n", td.kind.root());
            working.stage_create(destination, text.as_bytes())?;
            added_paths.insert(
                destination.clone(),
                Document {
                    text: text.clone(),
                    parsed: Arc::new(YamlDocument::parse(&text)?),
                    kind: td.kind,
                    order: vec![],
                },
            );
        }
        let od = ours
            .docs
            .get(destination)
            .or_else(|| added_paths.get(destination))
            .ok_or_else(|| MergeError::content("merge.yaml", "missing destination document"))?;
        // Inherited records already present in ours stay in place; their new
        // incoming descendants are still imported under the destination parent.
        let mut kept = BTreeSet::new();
        for native in &td.order {
            if base.records.contains_key(native) {
                continue;
            }
            let entry = &theirs.records[native];
            if entry
                .parent
                .as_ref()
                .is_some_and(|p| !base.records.contains_key(p) && !kept.contains(p))
            {
                continue;
            }
            let id = if td.kind == Kind::Index {
                let session = native.trim_start_matches("index:");
                format!("index:{}", map.get(session).map_or(session, String::as_str))
            } else {
                map.get(native).unwrap_or(native).clone()
            };
            if ours.records.contains_key(&id) {
                if td.kind == Kind::Index && inherited.contains(native.trim_start_matches("index:"))
                {
                    // Derived from the inherited session itself.
                    continue;
                }
                if inherited.contains(native) {
                    inherited_record(
                        theirs,
                        native,
                        ours,
                        &id,
                        inherited.yaml(native),
                        map,
                        report,
                    )?;
                    kept.insert(native.clone());
                    continue;
                }
                return Err(MergeError::content(
                    "merge.identity",
                    format!("unmapped YAML identity collision {id}"),
                ));
            }
            let mut incoming = rewrite_source(
                &td.text,
                &td.parsed.root,
                entry.span.clone(),
                destination,
                &id,
                map,
                report,
            )?;
            if td.text.as_bytes().get(entry.span.start) == Some(&b'{') {
                incoming = format!("- {incoming}\n");
            }
            if let Some(parent) = &entry.parent {
                let parent = map.get(parent).unwrap_or(parent);
                let target = ours.records.get(parent).ok_or_else(|| {
                    MergeError::content("merge.parent", format!("missing mapped parent {parent}"))
                })?;
                if let Some(children) = &target.children {
                    let child_indent = children
                        .sequence()?
                        .first()
                        .map(|n| {
                            let p = &od.text[line_start(&od.text, n.start)..n.start];
                            p.find('-').unwrap_or(p.len())
                        })
                        .unwrap_or(target.indent + 4);
                    edits.append(destination, children, &od.text, &incoming, child_indent)?;
                } else if target.flow {
                    let raw = incoming
                        .trim()
                        .strip_prefix("- ")
                        .unwrap_or(incoming.trim());
                    let parsed = YamlDocument::parse(raw)?;
                    if !parsed.root.flow || !matches!(parsed.root.kind, YamlKind::Mapping(_)) {
                        return Err(MergeError::content(
                            "merge.unsupported_source",
                            "block subtree cannot be copied losslessly into a flow parent without children",
                        ));
                    }
                    let at = target
                        .mapping_end
                        .checked_sub(1)
                        .filter(|at| od.text.as_bytes().get(*at) == Some(&b'}'))
                        .ok_or_else(|| {
                            MergeError::content(
                                "merge.yaml",
                                "flow parent closing delimiter is ambiguous",
                            )
                        })?;
                    let slot = edits
                        .appends
                        .entry((destination.clone(), at))
                        .or_insert((at..at, ", \"children\": []".into()));
                    let separator = if slot.1.ends_with("[]") { "" } else { ", " };
                    slot.1.pop();
                    slot.1.push_str(separator);
                    slot.1.push_str(raw);
                    slot.1.push(']');
                } else {
                    let at = target.span.end;
                    let value = edits.appends.entry((destination.clone(), at)).or_insert((
                        at..at,
                        format!("{}children:\n", " ".repeat(target.indent + 2)),
                    ));
                    value.1.push_str(&reindent(&incoming, target.indent + 4));
                }
            } else {
                if let Some(list) = od.parsed.root.get(od.kind.root())? {
                    let indent = list
                        .sequence()?
                        .first()
                        .map(|n| {
                            let p = &od.text[line_start(&od.text, n.start)..n.start];
                            p.find('-').unwrap_or(p.len())
                        })
                        .unwrap_or(2);
                    if !incoming.trim_start().starts_with("- ") {
                        incoming = render_row(&reindent(&incoming, 0), indent);
                    }
                    edits.append(destination, list, &od.text, &incoming, indent)?;
                } else if od.kind == Kind::Tree
                    && od
                        .parsed
                        .root
                        .get("root")?
                        .is_some_and(|r| r.to_json().is_ok_and(|v| v.is_null()))
                {
                    let root = root_record(od, destination)?;
                    field_patch(&mut edits, od, &root, "root", Some(&reindent(&incoming, 0)))?;
                } else {
                    return Err(MergeError::content(
                        "merge.root_collision",
                        "single-root dialect cannot invent a structural parent for an independent root",
                    ));
                }
            }
        }
    }
    edits.finish(working)?;
    derive_index(working)?;
    Ok(())
}

fn candidate_snapshot(working: &WorkingArtifact) -> ArtifactSnapshot {
    let mut snapshot = working.base.clone();
    for (path, bytes) in &working.files {
        snapshot.files.insert(
            path.clone(),
            crate::write::source::FileSnapshot {
                bytes: bytes.clone(),
                existed: true,
                permissions: None,
                digest: String::new(),
            },
        );
    }
    for path in &working.deleted_paths {
        snapshot.files.remove(path);
    }
    snapshot
}
fn derive_index(working: &mut WorkingArtifact) -> Result<(), MergeError> {
    if !working.exists(INDEX) {
        return Ok(());
    }
    // Index derivation only needs sessions and their index. Do not parse and
    // clone the entire exploration tree and every other YAML layer again.
    let snapshot = ArtifactSnapshot {
        root: working.base.root.clone(),
        identity_paths: BTreeSet::new(),
        files: working
            .base
            .files
            .iter()
            .filter(|(path, file)| {
                file.existed
                    && !working.deleted_paths.contains(*path)
                    && kind(path).is_some_and(|kind| matches!(kind, Kind::Session | Kind::Index))
            })
            .map(|(path, file)| (path.clone(), file.clone()))
            .chain(
                working
                    .files
                    .iter()
                    .filter(|(path, _)| {
                        kind(path).is_some_and(|kind| matches!(kind, Kind::Session | Kind::Index))
                    })
                    .map(|(path, bytes)| {
                        (
                            path.clone(),
                            crate::write::source::FileSnapshot {
                                bytes: bytes.clone(),
                                existed: true,
                                permissions: None,
                                digest: String::new(),
                            },
                        )
                    }),
            )
            .collect(),
    };
    let inv = inventory_cached(&snapshot, working)?;
    let doc = &inv.docs[INDEX];
    let mut edits = Edits::default();
    for address in &doc.order {
        let row = &inv.records[address];
        let id = address.trim_start_matches("index:");
        let session = &inv.records[id];
        if !working.files.contains_key(&session.path) {
            continue;
        }
        let sd = &inv.docs[&session.path];
        let metadata = sd
            .parsed
            .root
            .get("session")?
            .ok_or_else(|| MergeError::content("merge.session", "missing session metadata"))?;
        let mut derived = BTreeMap::new();
        for name in ["date", "summary", "turn_count"] {
            if let Some(value) = metadata.get(name)? {
                derived.insert(name, value.to_json()?);
            }
        }
        if let Some(events) = sd.parsed.root.get("events_logged")? {
            derived.insert("events_count", json!(events.sequence()?.len()));
        }
        let mut claims = BTreeSet::new();
        if let Some(touched) = sd.parsed.root.get("claims_touched")? {
            for item in touched.sequence()? {
                if let Some(id) = item.get("id")?.and_then(YamlNode::scalar) {
                    claims.insert(id.to_owned());
                }
            }
        }
        derived.insert("claims_touched", json!(claims));
        derived.insert(
            "open_threads",
            json!(
                sd.parsed
                    .root
                    .get("open_threads")?
                    .map(|n| n.sequence().map(|v| v.len()))
                    .transpose()?
                    .unwrap_or(0)
            ),
        );
        for (name, value) in derived {
            // Existing dialects remain existing dialects, including timestamp-only indexes.
            if row
                .fields
                .get(name)
                .is_some_and(|f| f.node.to_json().is_ok_and(|v| v != value))
            {
                field_patch(
                    &mut edits,
                    doc,
                    row,
                    name,
                    Some(&render_yaml(&value, row.indent + 4, "\n")),
                )?;
            }
        }
    }
    edits.finish(working)
}
fn located_record<'a>(
    inv: &'a Inventory,
    conflict: &MergeConflict,
) -> Result<(&'a Document, Record), MergeError> {
    let doc = inv.docs.get(&conflict.path).ok_or_else(|| {
        MergeError::content("merge.stale_conflict", "conflict document is absent")
    })?;
    let ConflictLocator::Yaml { keys, field } = &conflict.locator else {
        return Err(MergeError::content(
            "merge.resolution",
            "not a YAML field conflict",
        ));
    };
    if field != &conflict.field || keys.len() != 1 {
        return Err(MergeError::content(
            "merge.resolution",
            "inconsistent YAML field locator",
        ));
    }
    let is_root = keys.first().is_some_and(|s| s == "root");
    if !is_root && keys.first() != Some(&conflict.selector) {
        return Err(MergeError::content(
            "merge.resolution",
            "inconsistent YAML entry selector",
        ));
    }
    if is_root
        || doc.kind == Kind::Session
            && (HISTORY.contains(&conflict.field.as_str())
                || matches!(
                    conflict.field.as_str(),
                    "open_threads" | "ai_suggestions_pending"
                )
                || conflict.kind == "protected_session_extension")
    {
        return Ok((doc, root_record(doc, &conflict.path)?));
    }
    let id = keys
        .first()
        .ok_or_else(|| MergeError::content("merge.resolution", "missing entry locator"))?;
    let original = inv
        .records
        .get(id)
        .ok_or_else(|| MergeError::content("merge.stale_conflict", "conflict entry is absent"))?;
    if original.path != conflict.path {
        return Err(MergeError::content(
            "merge.resolution",
            "YAML entry belongs to another document",
        ));
    }
    let root = if doc.kind == Kind::Session {
        doc.parsed
            .root
            .get("session")?
            .ok_or_else(|| MergeError::content("merge.session", "missing metadata"))?
    } else {
        find_node(&doc.parsed.root, original.span.start, &doc.text).ok_or_else(|| {
            MergeError::content(
                "merge.stale_conflict",
                "conflict entry no longer has a structural location",
            )
        })?
    };
    Ok((
        doc,
        record(
            &doc.text,
            &conflict.path,
            root,
            original.parent.clone(),
            original.span.clone(),
        )?,
    ))
}
fn find_node<'a>(node: &'a YamlNode, start: usize, text: &str) -> Option<&'a YamlNode> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if matches!(node.kind, YamlKind::Mapping(_))
            && (node.start == start || line_start(text, node.start) == start)
        {
            return Some(node);
        }
        match &node.kind {
            YamlKind::Mapping(fields) => stack.extend(fields.iter().rev().map(|(_, node)| node)),
            YamlKind::Sequence(items) => stack.extend(items.iter().rev()),
            _ => {}
        }
    }
    None
}
fn replace_conflict(
    working: &mut WorkingArtifact,
    conflict: &MergeConflict,
    value: &MergeValue,
    protected_restore: bool,
) -> Result<(), MergeError> {
    if value.fingerprint
        != super::identity::value_fingerprint(if value.present {
            Some(value.bytes.as_slice())
        } else {
            None
        })
    {
        return Err(MergeError::content(
            "merge.resolution",
            "selected value fingerprint is invalid",
        ));
    }
    if !protected_restore {
        if conflict.kind.starts_with("protected") || conflict.allowed.is_empty() {
            return Err(MergeError::content(
                "merge.protected_content",
                "generic resolution cannot mutate protected history",
            ));
        }
        if !matches!(
            conflict.kind.as_str(),
            "mutable_field" | "promotion" | "opaque_yaml"
        ) {
            return Err(MergeError::content(
                "merge.resolution",
                "unsupported mutable YAML conflict kind",
            ));
        }
    }
    let inv = inventory_cached(&candidate_snapshot(working), working)?;
    let (doc, entry) = located_record(&inv, conflict)?;
    let promotion = (doc.kind == Kind::Observations && conflict.field == "$promotion")
        .then(|| promotion_bytes(&entry.fields));
    let current = MergeValue::new(
        promotion
            .as_deref()
            .or_else(|| bytes(entry.fields.get(&conflict.field))),
    );
    if current.fingerprint != conflict.ours.fingerprint || current.present != conflict.ours.present
    {
        return Err(MergeError::content(
            "merge.stale_conflict",
            "current exact YAML value does not match captured ours",
        ));
    }
    if !protected_restore {
        if value == &conflict.ours {
            return Ok(());
        }
        let ConflictLocator::Yaml { keys, .. } = &conflict.locator else {
            unreachable!()
        };
        let root = keys.first().is_some_and(|key| key == "root");
        let closed = doc.kind == Kind::Session
            && conflict
                .path
                .strip_prefix("trace/sessions/")
                .and_then(|path| path.strip_suffix(".yaml"))
                .and_then(|id| inv.records.get(id))
                .is_some_and(|record| closed_session(&record.fields));
        let mutable = match doc.kind {
            Kind::Session => match conflict.field.as_str() {
                "summary" => !root && entry.fields.contains_key("id"),
                "open_threads" | "ai_suggestions_pending" => entry.fields.contains_key("session"),
                _ => false,
            },
            Kind::Observations => !root && conflict.field == "$promotion",
            _ => false,
        };
        if closed || conflict.kind == "opaque_yaml" || !mutable {
            return Err(MergeError::content(
                "merge.protected_content",
                "generic resolution cannot replace this YAML origin; opaque values permit only captured ours",
            ));
        }
    }
    let replacement =
        if value.present {
            Some(std::str::from_utf8(&value.bytes).map_err(|_| {
                MergeError::content("merge.encoding", "conflict value is not UTF-8")
            })?)
        } else {
            None
        };
    let mut edits = Edits::default();
    if conflict.field == "$promotion" {
        let tuple: BTreeMap<String, String> =
            serde_json::from_str(replacement.ok_or_else(|| {
                MergeError::content("merge.promotion", "promotion tuple cannot be absent")
            })?)
            .map_err(|error| MergeError::content("merge.promotion", error.to_string()))?;
        if tuple.keys().any(|name| !PROMOTION.contains(&name.as_str())) {
            return Err(MergeError::content(
                "merge.promotion",
                "unexpected promotion tuple field",
            ));
        }
        for name in PROMOTION {
            field_patch(
                &mut edits,
                doc,
                &entry,
                name,
                tuple.get(name).map(String::as_str),
            )?;
        }
    } else {
        field_patch(&mut edits, doc, &entry, &conflict.field, replacement)?;
    }
    edits.finish(working)?;
    if doc.kind == Kind::Session {
        derive_index(working)?;
    }
    Ok(())
}
pub(crate) fn resolve(
    working: &mut WorkingArtifact,
    conflict: &MergeConflict,
    value: &MergeValue,
) -> Result<(), MergeError> {
    if conflict.kind == "opaque_yaml_comments" {
        if value != &conflict.ours {
            return Err(MergeError::content(
                "merge.resolution",
                "opaque YAML comments permit only the captured ours decision",
            ));
        }
        return verify_current(working, conflict);
    }
    replace_conflict(working, conflict, value, false)
}
/// Restoration is not a generic setter: only the conflict's captured base is eligible.
pub(crate) fn restore_base(
    working: &mut WorkingArtifact,
    conflict: &MergeConflict,
) -> Result<(), MergeError> {
    if !conflict.kind.starts_with("protected") {
        return Err(MergeError::content(
            "merge.restoration",
            "restoration requires protected conflict evidence",
        ));
    }
    verify_current(working, conflict)?;
    let mut map = IdentityMap::new();
    let ledger = super::identity::load(&working.base)?;
    for fact in ledger
        .records
        .iter()
        .filter_map(super::identity::Record::fact)
    {
        if fact.source_key == conflict.source_key {
            for mapping in fact.mappings {
                map.insert(mapping.original.clone(), mapping.target.clone());
            }
        }
    }
    if conflict.base.present {
        let text = std::str::from_utf8(&conflict.base.bytes)
            .map_err(|_| MergeError::content("merge.encoding", "restoration base is not UTF-8"))?;
        let options = super::types::MergeOptions {
            source_key: conflict.source_key.clone(),
            label: String::new(),
            time: String::new(),
            git: None,
            predecessor: None,
            self_key: None,
        };
        let mut quiet = MergeReport::new(&options, conflict.source_revision.clone());
        if rewrite::yaml_value(
            text,
            &conflict.path,
            &conflict.selector,
            &map,
            &mut quiet,
            structured_field(&conflict.path, &conflict.field),
        )? != text
        {
            return Err(MergeError::content(
                "merge.unsafe_restoration",
                "captured base requires historical identity relocation; exact restoration is unsafe",
            ));
        }
    }
    if matches!(conflict.locator, ConflictLocator::Document) {
        if conflict.field == "parent" || !conflict.ours.present {
            return Err(MergeError::content(
                "merge.unsafe_restoration",
                "original structural parent cannot be established from captured evidence",
            ));
        }
        let inv = inventory_cached(&candidate_snapshot(working), working)?;
        let entry = inv.records.get(&conflict.selector).ok_or_else(|| {
            MergeError::content(
                "merge.unsafe_restoration",
                "missing entry requires original parent evidence",
            )
        })?;
        let doc = &inv.docs[&entry.path];
        let current = MergeValue::new(Some(doc.text[entry.span.clone()].as_bytes()));
        if current.fingerprint != conflict.ours.fingerprint {
            return Err(MergeError::content(
                "merge.stale_conflict",
                "restoration preimage has changed",
            ));
        }
        if !conflict.base.present {
            return Err(MergeError::content(
                "merge.unsafe_restoration",
                "restoration lacks a base entry",
            ));
        }
        let bytes = std::str::from_utf8(&conflict.base.bytes)
            .map_err(|_| MergeError::content("merge.encoding", "base evidence is not UTF-8"))?;
        let mut edits = Edits::default();
        edits.patch(&entry.path, entry.span.clone(), bytes.into());
        edits.finish(working)?;
        return Ok(());
    }
    replace_conflict(working, conflict, &conflict.base, true)
}

pub(crate) fn verify_current(
    working: &WorkingArtifact,
    conflict: &MergeConflict,
) -> Result<(), MergeError> {
    let inv = inventory_cached(&candidate_snapshot(working), working)?;
    let current = match &conflict.locator {
        ConflictLocator::Yaml { keys, .. } => {
            if keys.first().is_some_and(|s| s == "comments") {
                let known = keys.iter().skip(1).cloned().collect();
                let raw = comment_value(&inv, &conflict.path, &known);
                MergeValue::new((!raw.is_empty()).then_some(raw.as_bytes()))
            } else {
                let (_, entry) = located_record(&inv, conflict)?;
                MergeValue::new(bytes(entry.fields.get(&conflict.field)))
            }
        }
        ConflictLocator::Document => {
            if let Some(entry) = inv.records.get(&conflict.selector) {
                MergeValue::new(Some(
                    inv.docs[&entry.path].text[entry.span.clone()].as_bytes(),
                ))
            } else if conflict.selector == conflict.path {
                MergeValue::new(working.bytes(&conflict.path).ok())
            } else {
                MergeValue::new(None)
            }
        }
        _ => return Err(MergeError::content("merge.resolution", "not YAML evidence")),
    };
    if current.fingerprint != conflict.ours.fingerprint || current.present != conflict.ours.present
    {
        return Err(MergeError::content(
            "merge.stale_conflict",
            "current exact protected value changed",
        ));
    }
    Ok(())
}

fn validate_session_source(
    doc: &Document,
    metadata: &YamlNode,
    id: &str,
) -> Result<(), MergeError> {
    if metadata
        .get("date")?
        .and_then(YamlNode::scalar)
        .is_some_and(|date| date != &id[..10])
    {
        return Err(MergeError::content(
            "merge.session",
            "session date differs from identity",
        ));
    }
    let time = |name: &str| -> Result<Option<i128>, MergeError> {
        metadata
            .get(name)?
            .map(|n| {
                n.scalar()
                    .ok_or_else(|| {
                        MergeError::content("merge.session", "timestamp is not a scalar")
                    })
                    .and_then(|s| crate::write::sessions::timestamp_key(s).map_err(Into::into))
            })
            .transpose()
    };
    if let (Some(start), Some(last)) = (time("started")?, time("last_turn")?)
        && last < start
    {
        return Err(MergeError::content(
            "merge.session",
            "session chronology regressed",
        ));
    }
    let count = metadata
        .get("turn_count")?
        .map(|n| {
            n.to_json().map_err(MergeError::from).and_then(|v| {
                v.as_u64().ok_or_else(|| {
                    MergeError::content("merge.session", "turn count must be a nonnegative integer")
                })
            })
        })
        .transpose()?;
    for name in HISTORY {
        if let Some(list) = doc.parsed.root.get(name)? {
            for row in list.sequence()? {
                if !matches!(row.kind, YamlKind::Mapping(_)) {
                    // Existing session histories can contain verbatim scalar/list
                    // records. They remain opaque, protected complete occurrences.
                    row.to_json()?;
                    continue;
                }
                if let Some(turn) = row.get("turn")? {
                    let turn = turn.to_json()?.as_u64().ok_or_else(|| {
                        MergeError::content("merge.session", "turn must be a positive integer")
                    })?;
                    if turn == 0 || count.is_some_and(|n| turn > n) {
                        return Err(MergeError::content(
                            "merge.session",
                            "row is outside complete session history",
                        ));
                    }
                }
            }
        }
    }
    for name in ["open_threads", "ai_suggestions_pending"] {
        if let Some(list) = doc.parsed.root.get(name)? {
            list.sequence()?;
        }
    }
    Ok(())
}
fn check_extensions(
    node: &YamlNode,
    text: &str,
    path: &str,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let mut quiet = quiet_report(report);
    let root = node
        .get("session")?
        .is_some_and(|n| matches!(n.kind, YamlKind::Mapping(_)));
    for (name, field) in fields(text, node)? {
        if root
            && matches!(
                name.as_str(),
                "session"
                    | "events_logged"
                    | "ai_actions"
                    | "claims_touched"
                    | "logic_revisions"
                    | "key_context"
                    | "open_threads"
                    | "ai_suggestions_pending"
            )
            || !root && known_row(&name)
        {
            continue;
        }
        if unknown_relocation(&field.node, path, id, map, &mut quiet)? {
            let mut item = conflict(
                report,
                path,
                id,
                &name,
                "unsupported_structured_reference",
                None,
                None,
                Some(field.raw.as_bytes()),
                ConflictLocator::Yaml {
                    keys: vec![id.into()],
                    field: name.clone(),
                },
            );
            item.allowed.clear();
            let mut error = MergeError::content(
                "merge.unsupported_structured_reference",
                "unknown YAML extension requires identifier relocation without an approved target kind",
            );
            error.evidence = vec![item];
            return Err(error);
        }
    }
    Ok(())
}
fn check_incoming_extensions(
    base: &Inventory,
    theirs: &Inventory,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    for (id, entry) in &theirs.records {
        if base.records.contains_key(id) {
            continue;
        }
        let doc = &theirs.docs[&entry.path];
        let mut quiet = quiet_report(report);
        for (name, field) in &entry.fields {
            if known_entry(doc.kind, name) {
                continue;
            }
            if unknown_relocation(&field.node, &entry.path, id, map, &mut quiet)? {
                let item = conflict(
                    report,
                    &entry.path,
                    id,
                    name,
                    "unsupported_structured_reference",
                    None,
                    None,
                    Some(field.raw.as_bytes()),
                    ConflictLocator::Yaml {
                        keys: vec![id.clone()],
                        field: name.clone(),
                    },
                );
                let mut error = MergeError::content(
                    "merge.unsupported_structured_reference",
                    "unknown incoming YAML value needs unapproved relocation",
                );
                error.evidence = vec![item];
                return Err(error);
            }
        }
        if doc.kind == Kind::Mutations {
            let pointer = |name: &str| -> Result<Option<String>, MergeError> {
                let Some(raw) = entry
                    .fields
                    .get(name)
                    .and_then(|f| f.node.scalar())
                    .filter(|s| *s != "null" && *s != "~")
                else {
                    return Ok(None);
                };
                let (path, target) = raw.split_once(':').ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "mutation requires qualified native pointer",
                    )
                })?;
                let scoped = format!("{path}#{target}");
                let exact = entry
                    .fields
                    .get(&format!("{name}_selector"))
                    .map(|field| field.node.to_json())
                    .transpose()?
                    .map(serde_json::from_value::<crate::write::EntrySelector>)
                    .transpose()
                    .map_err(|error| MergeError::content("merge.mutation", error.to_string()))?
                    .as_ref()
                    .and_then(|selector| {
                        let exact = super::markdown::exact_selector_key(selector)?;
                        if map.contains_key(&exact) {
                            Some(exact)
                        } else {
                            super::markdown::selector_key(selector)
                        }
                    });
                let mut mapped = exact.as_ref().and_then(|key| map.get(key)).or_else(|| {
                    map.get(raw)
                        .or_else(|| map.get(target))
                        .or_else(|| map.get(&scoped))
                });
                if mapped.is_none()
                    && name == "to"
                    && let Some(selector) = entry.fields.get("to_selector")
                {
                    let selected = selector
                        .node
                        .get("entry")?
                        .and_then(YamlNode::scalar)
                        .or_else(|| {
                            selector
                                .node
                                .get("heading")
                                .ok()
                                .flatten()
                                .and_then(|headings| headings.sequence().ok())
                                .and_then(|headings| headings.last())
                                .and_then(YamlNode::scalar)
                                .map(|heading| {
                                    heading
                                        .split_once(':')
                                        .map_or(heading, |(prefix, _)| prefix)
                                })
                        });
                    if let Some(selected) = selected {
                        mapped = map.get(selected);
                    }
                }
                mapped.cloned().map(Some).ok_or_else(|| {
                    MergeError::content(
                        "merge.unsafe_mutation_import",
                        format!("historical identity {raw} has no proven source mapping"),
                    )
                })
            };
            let endpoints = (pointer("from"), pointer("to"));
            let unsafe_row = match &endpoints {
                (Ok(Some(from)), Ok(Some(to))) => from == to,
                _ => endpoints.0.is_err() || endpoints.1.is_err(),
            };
            if unsafe_row {
                let item = conflict(
                    report,
                    &entry.path,
                    id,
                    "entry",
                    "unsafe_mutation_import",
                    None,
                    None,
                    Some(doc.text[entry.span.clone()].as_bytes()),
                    ConflictLocator::Document,
                );
                let mut error = MergeError::content(
                    "merge.unsafe_mutation_import",
                    "mutation origin is unproven or relocated endpoints collapse",
                );
                error.evidence = vec![item];
                return Err(error);
            }
        }
        if doc.kind == Kind::Session {
            check_extensions(&doc.parsed.root, &doc.text, &entry.path, id, map, report)?;
            for name in HISTORY {
                if let Some(list) = doc.parsed.root.get(name)? {
                    for row in list.sequence()? {
                        check_extensions(row, &doc.text, &entry.path, id, map, report)?;
                    }
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn validate_references(
    view: &Inventory,
    identities: &BTreeSet<String>,
    redirects: &BTreeMap<String, String>,
    markdown: &super::markdown::Inventory,
) -> Result<(), MergeError> {
    fn native(value: &str, identities: &BTreeSet<String>) -> String {
        if identities.contains(value) {
            value.into()
        } else {
            super::identity::normalize_local(value)
        }
    }
    fn require(
        value: &str,
        identities: &BTreeSet<String>,
        redirects: &BTreeMap<String, String>,
    ) -> Result<(), MergeError> {
        let mut target = value.to_owned();
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(target.clone()) {
                return Err(MergeError::content(
                    "merge.reference_cycle",
                    "local redirects form a cycle",
                ));
            }
            let key = native(&target, identities);
            if let Some(next) = redirects.get(&target).or_else(|| redirects.get(&key)) {
                target = next.clone();
                continue;
            }
            if identities.contains(&key) {
                return Ok(());
            }
            return Err(MergeError::content(
                "merge.dangling_reference",
                format!("unknown candidate YAML target {value}"),
            ));
        }
    }
    fn typed(value: &str) -> bool {
        value.len() > 1
            && matches!(value.as_bytes()[0], b'N' | b'O' | b'C' | b'H' | b'E' | b'T')
            && value.as_bytes()[1..].iter().all(u8::is_ascii_digit)
    }
    fn turn(view: &Inventory, value: &str) -> Result<(), MergeError> {
        let Some((id, number)) = value.split_once('#') else {
            return Err(MergeError::content(
                "merge.session_reference",
                "turn pointer requires session#turn",
            ));
        };
        let record = view.records.get(id).ok_or_else(|| {
            MergeError::content("merge.session_reference", format!("unknown session {id}"))
        })?;
        let turn = number
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| {
                MergeError::content("merge.session_reference", "invalid turn pointer")
            })?;
        if record
            .fields
            .get("turn_count")
            .and_then(|f| f.node.to_json().ok())
            .and_then(|v| v.as_u64())
            .is_some_and(|count| turn > count)
        {
            return Err(MergeError::content(
                "merge.session_reference",
                "pointer exceeds complete session history",
            ));
        }
        Ok(())
    }
    fn selector(
        node: &YamlNode,
        historical: bool,
        markdown: &super::markdown::Inventory,
        identities: &BTreeSet<String>,
        redirects: &BTreeMap<String, String>,
    ) -> Result<(), MergeError> {
        let value: crate::write::EntrySelector =
            serde_json::from_value(node.to_json()?).map_err(|error| {
                MergeError::content(
                    "merge.session_reference",
                    format!("invalid native selector: {error}"),
                )
            })?;
        let literal = match &value {
            crate::write::EntrySelector::Id { id } => {
                if id.is_empty() {
                    return Err(MergeError::content(
                        "merge.session_reference",
                        "selector ID is empty",
                    ));
                }
                if historical {
                    return Ok(());
                }
                return require(id, identities, redirects);
            }
            crate::write::EntrySelector::Document {
                document,
                heading,
                entry,
            } => {
                if document == "PAPER.md"
                    || !document.ends_with(".md")
                    || document.starts_with("src/")
                    || document.starts_with("evidence/")
                    || !historical && !identities.contains(document)
                {
                    return Err(MergeError::content(
                        "merge.session_reference",
                        "selector document is not native mutable knowledge",
                    ));
                }
                if (heading.is_empty() && entry.is_none())
                    || (!heading.is_empty() && entry.is_some())
                    || heading.iter().any(String::is_empty)
                    || entry.as_ref().is_some_and(String::is_empty)
                {
                    return Err(MergeError::content(
                        "merge.session_reference",
                        "selector requires exactly one nonempty heading path or entry",
                    ));
                }
                if historical {
                    return Ok(());
                }
                super::markdown::selector_key(&value).ok_or_else(|| {
                    MergeError::content("merge.session_reference", "empty native selector")
                })?
            }
        };
        match super::markdown::selector_address(markdown, &value) {
            Ok(Some(address)) => require(&address, identities, redirects),
            Ok(None) => {
                let exact = super::markdown::exact_selector_key(&value);
                if let Some(target) = exact
                    .as_ref()
                    .and_then(|key| redirects.get(key))
                    .or_else(|| redirects.get(&literal))
                {
                    require(target, identities, redirects)
                } else {
                    Err(MergeError::content(
                        "merge.session_reference",
                        "native selector has no exact current or historical alias target",
                    ))
                }
            }
            Err(error) => Err(MergeError::content(
                "merge.session_reference",
                error.message,
            )),
        }
    }
    let references = super::markdown::reference_targets(markdown, redirects);
    let redirects = &references;
    for (id, entry) in &view.records {
        (|| -> Result<(), MergeError> {
            let doc = &view.docs[&entry.path];
            if doc.kind == Kind::Mutations
                && (entry.fields.contains_key("from_selector")
                    || entry.fields.contains_key("to_selector"))
            {
                let from = entry.fields.get("from_selector").ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "authoritative mutation selectors require a from selector",
                    )
                })?;
                let to = entry.fields.get("to_selector").ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "authoritative mutation selectors require a to selector",
                    )
                })?;
                selector(&from.node, true, markdown, identities, redirects)?;
                let removed = entry
                    .fields
                    .get("to")
                    .map(|field| field.node.to_json())
                    .transpose()?
                    .is_some_and(|value| value.is_null());
                let null_selector = to.node.to_json()?.is_null();
                if removed != null_selector {
                    return Err(MergeError::content(
                        "merge.mutation",
                        "mutation target and selector null presence disagree",
                    ));
                }
                if !removed {
                    selector(&to.node, false, markdown, identities, redirects)?;
                }
            }
            for name in ["parent", "target", "promoted_to", "from", "to"] {
                if matches!(name, "from" | "to") && doc.kind != Kind::Mutations
                    || name == "from" && doc.kind == Kind::Mutations
                    || name == "to"
                        && doc.kind == Kind::Mutations
                        && entry.fields.contains_key("to_selector")
                {
                    continue;
                }
                if let Some(value) = entry.fields.get(name).and_then(|f| f.node.scalar()) {
                    if value == "null" || value == "~" || value.is_empty() {
                        continue;
                    }
                    if typed(value)
                        || value.contains(':') && matches!(name, "promoted_to" | "from" | "to")
                    {
                        require(value, identities, redirects)?;
                    }
                }
            }
            if doc.kind == Kind::Taste
                && let Some(target) = entry.fields.get("target").and_then(|f| f.node.scalar())
                && view
                    .records
                    .get(target)
                    .and_then(|r| r.fields.get("type"))
                    .is_some_and(|f| f.node.scalar() == Some("question"))
            {
                return Err(MergeError::content(
                    "merge.taste_target",
                    "taste target must be a non-question trace node",
                ));
            }
            for name in ["also_depends_on", "same_as", "bound_to"] {
                if let Some(field) = entry.fields.get(name) {
                    for node in field.node.sequence()? {
                        if let Some(value) = node.scalar() {
                            require(value, identities, redirects)?;
                        }
                    }
                }
            }
            if matches!(doc.kind, Kind::Reasoning | Kind::Mutations) {
                if let Some(value) = entry.fields.get("turn").and_then(|f| f.node.scalar())
                    && value.contains('#')
                {
                    turn(view, value)?;
                }
                if let Some(metadata) = entry.fields.get("session_metadata")
                    && let Some(session) = metadata.node.get("session")?.and_then(YamlNode::scalar)
                {
                    require(session, identities, redirects)?;
                }
                if let Some(session) = entry.fields.get("session").and_then(|f| f.node.scalar()) {
                    require(session, identities, redirects)?;
                }
            }
            if let Some(annotations) = entry.fields.get("conflict_annotations") {
                for row in annotations.node.sequence()? {
                    if let Some(references) = row.get("references")? {
                        for reference in references.sequence()? {
                            if let Some(value) = reference.scalar() {
                                require(value, identities, redirects)?;
                            }
                        }
                    }
                }
            }
            if doc.kind == Kind::Session {
                for name in ["events_logged", "claims_touched", "logic_revisions"] {
                    if let Some(list) = doc.parsed.root.get(name)? {
                        for row in list.sequence()? {
                            let field = if name == "logic_revisions" {
                                "entry"
                            } else {
                                "id"
                            };
                            let Some(value) = row.get(field)? else {
                                continue;
                            };
                            if let Some(value) = value.scalar() {
                                if typed(value) || value.contains(':') {
                                    require(value, identities, redirects)?;
                                }
                            } else if name == "logic_revisions" {
                                let archived = view.records.values().any(|mutation| {
                                    mutation.path == "trace/logic_mutations.yaml"
                                        && mutation
                                            .fields
                                            .get("from_selector")
                                            .and_then(|f| f.node.to_json().ok())
                                            == value.to_json().ok()
                                        && mutation
                                            .fields
                                            .get("session")
                                            .and_then(|f| f.node.scalar())
                                            == Some(id.as_str())
                                        && mutation
                                            .fields
                                            .get("turn")
                                            .and_then(|f| f.node.to_json().ok())
                                            == row
                                                .get("turn")
                                                .ok()
                                                .flatten()
                                                .and_then(|n| n.to_json().ok())
                                        && ["before", "after"].iter().all(|key| {
                                            mutation
                                                .fields
                                                .get(*key)
                                                .and_then(|f| f.node.to_json().ok())
                                                == row
                                                    .get(key)
                                                    .ok()
                                                    .flatten()
                                                    .and_then(|n| n.to_json().ok())
                                        })
                                });
                                selector(value, archived, markdown, identities, redirects)?;
                            } else {
                                return Err(MergeError::content(
                                    "merge.session_reference",
                                    "event or claim ID must be scalar",
                                )
                                .at(&entry.path));
                            }
                        }
                    }
                }
            }
            if doc.kind == Kind::Observations {
                let flag = entry
                    .fields
                    .get("promoted")
                    .map(|f| f.node.to_json())
                    .transpose()?;
                let target = entry
                    .fields
                    .get("promoted_to")
                    .map(|f| f.node.to_json())
                    .transpose()?;
                let signal = entry
                    .fields
                    .get("crystallized_via")
                    .map(|f| f.node.to_json())
                    .transpose()?;
                if flag == Some(json!(true)) {
                    let target = target
                        .as_ref()
                        .and_then(Value::as_str)
                        .filter(|s| promotion_target(s));
                    let signal = signal
                        .as_ref()
                        .and_then(Value::as_str)
                        .filter(|s| promotion_signal(s));
                    if target.is_none() || signal.is_none() {
                        return Err(MergeError::content(
                            "merge.promotion",
                            format!("incomplete promotion tuple {id}"),
                        )
                        .at(&entry.path));
                    }
                    let target = target.unwrap();
                    require(target, identities, redirects)?;
                    if let Some(node) = target.strip_prefix("trace:")
                        && view
                            .records
                            .get(node)
                            .and_then(|r| r.fields.get("type"))
                            .is_none_or(|f| f.node.scalar() != Some("dead_end"))
                    {
                        return Err(MergeError::content(
                            "merge.promotion",
                            "trace promotion requires a dead-end target",
                        )
                        .at(&entry.path));
                    }
                } else if flag.is_some_and(|v| v != json!(false))
                    || target.is_some_and(|v| !v.is_null())
                    || signal.is_some_and(|v| !v.is_null())
                {
                    return Err(MergeError::content(
                        "merge.promotion",
                        format!("invalid unpromoted tuple {id}"),
                    )
                    .at(&entry.path));
                }
            }
            Ok(())
        })()
        .map_err(|error| {
            if error.field.is_some() {
                error
            } else {
                error.at(&entry.path)
            }
        })?;
    }
    Ok(())
}

fn rewrite_source(
    text: &str,
    root: &YamlNode,
    range: Range<usize>,
    path: &str,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<String, MergeError> {
    fn gather(
        node: &YamlNode,
        text: &str,
        range: &Range<usize>,
        path: &str,
        id: &str,
        map: &IdentityMap,
        report: &mut MergeReport,
        patches: &mut Vec<(Range<usize>, String)>,
    ) -> Result<(), MergeError> {
        enum Frame<'a> {
            Node(&'a YamlNode),
            Field(&'a YamlNode, &'a YamlNode, &'a YamlNode),
        }
        let mut stack = vec![Frame::Node(node)];
        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Node(node) => {
                    if node.end <= range.start || node.start >= range.end {
                        continue;
                    }
                    match &node.kind {
                        YamlKind::Sequence(items) => {
                            let first = items.partition_point(|item| item.end <= range.start);
                            let last = items.partition_point(|item| item.start < range.end);
                            stack.extend(items[first..last].iter().rev().map(Frame::Node));
                        }
                        YamlKind::Mapping(fields) => stack.extend(
                            fields
                                .iter()
                                .rev()
                                .map(|(key, value)| Frame::Field(node, key, value)),
                        ),
                        _ => {}
                    }
                }
                Frame::Field(node, key, value) => {
                    let Some(name) = key.scalar() else { continue };
                    if matches!(
                        name,
                        "children"
                            | "tree"
                            | "root"
                            | "observations"
                            | "entries"
                            | "mutations"
                            | "sessions"
                            | "session_metadata"
                            | "conflict_annotations"
                    ) || name == "session" && matches!(value.kind, YamlKind::Mapping(_))
                        || HISTORY.contains(&name)
                            && value.sequence().is_ok_and(|items| {
                                items
                                    .iter()
                                    .all(|item| matches!(item.kind, YamlKind::Mapping(_)))
                            })
                    {
                        stack.push(Frame::Node(value));
                        continue;
                    }
                    if value.start < range.start
                        || value.start >= range.end
                        || matches!(name, "before" | "after")
                    {
                        continue;
                    }
                    if !known(name) && !matches!(name, "session" | "historical_references") {
                        continue;
                    }
                    let end = if node.flow {
                        value.end
                    } else {
                        field_range(text, node, name)?
                            .map_or(value.end, |range| range.end.max(value.end))
                    };
                    if end > range.end {
                        continue;
                    }
                    let raw = &text[value.start..end];
                    let historical = (path == "trace/logic_mutations.yaml"
                        && matches!(name, "from" | "from_selector"))
                        || (path.starts_with("trace/sessions/") && name == "entry");
                    let selector = if path == "trace/logic_mutations.yaml" {
                        match name {
                            "from" => node.get("from_selector")?,
                            "to" => node.get("to_selector")?,
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let rewritten = if path == "trace/logic_mutations.yaml"
                        && matches!(name, "from" | "to")
                    {
                        mutation_pointer(raw, value, selector, path, id, map, report, historical)?
                    } else if matches!(name, "from_selector" | "to_selector")
                        || name == "entry" && matches!(value.kind, YamlKind::Mapping(_))
                    {
                        selector_value(raw, value, path, id, map, report, historical)?
                    } else if historical {
                        rewrite::yaml_value_historical(
                            raw,
                            path,
                            id,
                            map,
                            report,
                            structured_field(path, name),
                        )?
                    } else {
                        rewrite_field(raw, value, path, id, name, map, report)?
                    };
                    if rewritten != raw {
                        patches.push((value.start..end, rewritten));
                    }
                }
            }
        }
        Ok(())
    }
    let mut patches = Vec::new();
    gather(root, text, &range, path, id, map, report, &mut patches)?;
    patches.sort_by_key(|(r, _)| r.start);
    let mut output = String::new();
    let mut cursor = range.start;
    for (span, value) in patches {
        if span.start < cursor {
            return Err(MergeError::content(
                "merge.intent",
                "overlapping incoming-origin rewrites",
            ));
        }
        output.push_str(&text[cursor..span.start]);
        output.push_str(&value);
        cursor = span.end;
    }
    output.push_str(&text[cursor..range.end]);
    Ok(output)
}

fn unknown_relocation(
    node: &YamlNode,
    path: &str,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<bool, MergeError> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        match &node.kind {
            YamlKind::Scalar { value, .. } => {
                if rewrite::incoming(value, path, id, map, report, false)? != *value {
                    return Ok(true);
                }
            }
            YamlKind::Sequence(items) => stack.extend(items.iter().rev()),
            YamlKind::Mapping(items) => {
                for (key, value) in items.iter().rev() {
                    stack.push(value);
                    stack.push(key);
                }
            }
            YamlKind::Alias(_) => return Ok(true),
        }
    }
    Ok(false)
}

fn promotion_signal(value: &str) -> bool {
    [
        "topic-abandonment",
        "verbal-affirmation",
        "empirical-resolution",
        "artifact-commitment",
    ]
    .contains(&value)
}
fn promotion_target(value: &str) -> bool {
    if let Some((path, heading)) = value.split_once('#') {
        return [
            "logic/concepts.md",
            "logic/solution/constraints.md",
            "logic/solution/architecture.md",
        ]
        .contains(&path)
            && !heading.is_empty();
    }
    let Some((path, target)) = value.split_once(':') else {
        return false;
    };
    let prefix = match path {
        "trace" => 'N',
        "logic/claims.md" => 'C',
        "logic/solution/heuristics.md" => 'H',
        _ => return false,
    };
    target.starts_with(prefix)
        && target.len() > 1
        && target.as_bytes()[1..].iter().all(u8::is_ascii_digit)
}
fn append_annotations(
    edits: &mut Edits,
    od: &Document,
    td: &Document,
    b: &Record,
    o: &Record,
    t: &Record,
    id: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let name = "conflict_annotations";
    let base = history_rows(b, name)?;
    let ours = history_rows(o, name)?;
    let theirs = history_rows(t, name)?;
    let mut valid = ours.len() >= base.len() && theirs.len() >= base.len();
    let mut quiet = quiet_report(report);
    for (ordinal, row) in base.iter().enumerate() {
        let raw = row_value(&b.fields[name], row, ordinal);
        let source = YamlDocument::parse(&raw)?;
        let mapped = rewrite_source(
            &raw,
            &source.root,
            0..raw.len(),
            &o.path,
            id,
            map,
            &mut quiet,
        )?;
        let expected = YamlDocument::parse(&mapped)?;
        let source_id = b
            .fields
            .get("id")
            .and_then(|field| field.node.scalar())
            .ok_or_else(|| {
                MergeError::content("merge.occurrence", "annotation owner has no identity")
            })?;
        let destination_ordinal = occurrence_ordinal(map, source_id, id, name, ordinal)?;
        if let Some(row) = ours.get(destination_ordinal) {
            valid &= row_equal(&expected.root, &mapped, row, &od.text, map)?;
        } else {
            valid = false;
        }
        if let Some(row) = theirs.get(ordinal) {
            valid &= row_equal(&source.root, &raw, row, &td.text, map)?;
        }
    }
    if !valid {
        report_field(
            report,
            o,
            id,
            name,
            "protected_annotations",
            b.fields.get(name),
            o.fields.get(name),
            t.fields.get(name),
        );
        return Err(reject_protected(report));
    }
    let mut rows = Vec::new();
    if let Some(field) = t.fields.get(name) {
        for (ordinal, row) in theirs.iter().enumerate().skip(base.len()) {
            check_extensions(row, &td.text, &o.path, id, map, report)?;
            let raw = row_value(field, row, ordinal);
            let doc = YamlDocument::parse(&raw)?;
            rows.push(rewrite_source(
                &raw,
                &doc.root,
                0..raw.len(),
                &o.path,
                id,
                map,
                report,
            )?);
        }
    }
    append_values(edits, od, o, name, &rows)
}

fn comment_value(view: &Inventory, path: &str, known: &BTreeSet<String>) -> String {
    fn quoted_or_literal(node: &YamlNode, text: &str, ranges: &mut Vec<Range<usize>>) {
        use yaml_rust2::scanner::TScalarStyle;
        let mut stack = vec![node];
        while let Some(node) = stack.pop() {
            if matches!(
                node.style,
                Some(TScalarStyle::SingleQuoted | TScalarStyle::DoubleQuoted)
            ) {
                ranges.push(node.start..node.end);
                continue;
            }
            if matches!(
                node.style,
                Some(TScalarStyle::Literal | TScalarStyle::Folded)
            ) {
                let header = line_start(text, node.start);
                let header_indent = text[header..node.start]
                    .bytes()
                    .take_while(u8::is_ascii_whitespace)
                    .count();
                let Some(newline) = text[node.start..node.end].find('\n') else {
                    continue;
                };
                let mut at = node.start + newline + 1;
                let mut indent = None;
                for line in text[at..node.end].split_inclusive('\n') {
                    if !line.trim().is_empty() {
                        let level = line.bytes().take_while(u8::is_ascii_whitespace).count();
                        let minimum = *indent.get_or_insert(level);
                        if minimum <= header_indent || level < minimum {
                            break;
                        }
                        ranges.push(at..at + line.len());
                    }
                    at += line.len();
                }
                continue;
            }
            match &node.kind {
                YamlKind::Sequence(items) => stack.extend(items.iter().rev()),
                YamlKind::Mapping(items) => {
                    for (key, node) in items.iter().rev() {
                        stack.push(node);
                        stack.push(key);
                    }
                }
                _ => {}
            }
        }
    }
    let Some(doc) = view.docs.get(path) else {
        return String::new();
    };
    let mut excluded = Vec::new();
    quoted_or_literal(&doc.parsed.root, &doc.text, &mut excluded);
    for (id, entry) in view.records.iter().filter(|(_, e)| e.path == path) {
        if !known.contains(id) {
            excluded.push(entry.span.clone());
        }
    }
    if doc.kind == Kind::Session {
        for name in HISTORY {
            let marker = format!("history:{name}:");
            let count = known
                .iter()
                .find_map(|key| {
                    key.strip_prefix(&marker)
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if let Ok(Some(field)) = doc.parsed.root.get(name)
                && let Ok(rows) = field.sequence()
            {
                excluded.extend(rows.iter().skip(count).map(|n| n.start..n.end));
            }
        }
    }
    excluded.sort_by_key(|r| r.start);
    let mut value = String::new();
    let mut offset = 0;
    let mut cursor = 0;
    for line in doc.text.split_inclusive('\n') {
        for (column, byte) in line.bytes().enumerate() {
            if byte != b'#' || column > 0 && !line.as_bytes()[column - 1].is_ascii_whitespace() {
                continue;
            }
            let at = offset + column;
            while cursor < excluded.len() && excluded[cursor].end <= at {
                cursor += 1;
            }
            if excluded
                .get(cursor)
                .is_some_and(|range| range.start <= at && at < range.end)
            {
                continue;
            }
            let start = if line[..column].trim().is_empty() {
                0
            } else {
                column
            };
            value.push_str(&line[start..]);
            break;
        }
        offset += line.len();
    }
    value
}
fn opaque_comments(
    base: &Inventory,
    ours: &Inventory,
    theirs: &Inventory,
    map: &IdentityMap,
    report: &mut MergeReport,
) {
    for (path, doc) in &theirs.docs {
        if doc.kind == Kind::Session && !base.docs.contains_key(path) {
            continue;
        }
        let mut native: BTreeSet<_> = base
            .records
            .iter()
            .filter(|(_, r)| r.path == *path)
            .map(|(id, _)| id.clone())
            .collect();
        if doc.kind == Kind::Session
            && let Some(bd) = base.docs.get(path)
        {
            for name in HISTORY {
                let count = bd
                    .parsed
                    .root
                    .get(name)
                    .ok()
                    .flatten()
                    .and_then(|f| f.sequence().ok())
                    .map_or(0, <[YamlNode]>::len);
                native.insert(format!("history:{name}:{count}"));
            }
        }
        let destination = map.get(path).unwrap_or(path);
        let mapped: BTreeSet<_> = native
            .iter()
            .map(|id| map.source_target(id).to_owned())
            .collect();
        let b = comment_value(base, path, &native);
        let o = comment_value(ours, destination, &mapped);
        let t = comment_value(theirs, path, &native);
        if t == b || t == o {
            continue;
        }
        let mut keys = vec!["comments".into()];
        keys.extend(mapped);
        let item = conflict(
            report,
            destination,
            "comments",
            "comments",
            "opaque_yaml_comments",
            (!b.is_empty()).then_some(b.as_bytes()),
            (!o.is_empty()).then_some(o.as_bytes()),
            (!t.is_empty()).then_some(t.as_bytes()),
            ConflictLocator::Yaml {
                keys,
                field: "comments".into(),
            },
        );
        ours_only(report, item);
    }
}

/// Destination-namespace field values of a record, for comparing one entry that
/// arrived through two routes.
fn relocated_fields<'a>(
    record: &'a Record,
    destination_path: &str,
    destination: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<BTreeMap<&'a str, Cow<'a, Field>>, MergeError> {
    let mut result = BTreeMap::new();
    for (name, field) in &record.fields {
        let selector = match name.as_str() {
            "from" => record.fields.get("from_selector"),
            "to" => record.fields.get("to_selector"),
            _ => None,
        }
        .filter(|_| destination_path == "trace/logic_mutations.yaml")
        .map(|field| &field.node);
        if let Some(value) = mapped_field(
            Some(field),
            destination_path,
            destination,
            name,
            map,
            report,
            selector,
        )? {
            result.insert(name.as_str(), value);
        }
    }
    Ok(result)
}
fn same_fields(
    left: &BTreeMap<&str, Cow<'_, Field>>,
    right: &BTreeMap<&str, Cow<'_, Field>>,
    kind: Kind,
    map: &IdentityMap,
) -> bool {
    left.keys().chain(right.keys()).all(|name| {
        let key = if known_entry(kind, name) {
            name
        } else {
            "opaque"
        };
        equal_mapped(
            left.get(name).map(AsRef::as_ref),
            right.get(name).map(AsRef::as_ref),
            key,
            map,
        )
    })
}
/// An incoming record whose proven origin already exists in ours. These layers
/// are immutable history: an equal record or a destination-only change keeps
/// ours; an incoming change relative to the shared source fact is rejected.
fn inherited_record(
    theirs: &Inventory,
    native: &str,
    ours: &Inventory,
    destination: &str,
    base: Option<(&Inventory, &str, &IdentityMap)>,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let t = &theirs.records[native];
    let o = &ours.records[destination];
    let kind = ours.docs[&o.path].kind;
    let mut quiet = quiet_report(report);
    let incoming = relocated_fields(t, &o.path, destination, map, &mut quiet)?;
    let current: BTreeMap<&str, Cow<'_, Field>> = o
        .fields
        .iter()
        .map(|(name, field)| (name.as_str(), Cow::Borrowed(field)))
        .collect();
    let parent = t.parent.as_ref().map(|p| map.get(p).unwrap_or(p));
    if parent == o.parent.as_ref() && same_fields(&incoming, &current, kind, map) {
        return Ok(());
    }
    let base = base.and_then(|(view, original, base_map)| {
        view.records
            .get(original)
            .map(|record| (view, record, base_map))
    });
    if let Some((_, b, base_map)) = base {
        let shared = relocated_fields(b, &o.path, destination, base_map, &mut quiet)?;
        if same_fields(&incoming, &shared, kind, map) {
            return Ok(());
        }
    }
    conflict(
        report,
        &o.path,
        destination,
        "entry",
        "protected_inherited_entry",
        base.map(|(view, b, _)| view.docs[&b.path].text[b.span.clone()].as_bytes()),
        Some(ours.docs[&o.path].text[o.span.clone()].as_bytes()),
        Some(theirs.docs[&t.path].text[t.span.clone()].as_bytes()),
        ConflictLocator::Document,
    );
    Err(reject_protected(report))
}
/// Whole-session comparison for a session that arrived through two routes.
fn inherited_session(
    incoming: &Document,
    current: &Document,
    destination: &str,
    id: &str,
    base: Option<(&Inventory, &str, &IdentityMap)>,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let mut quiet = quiet_report(report);
    let relocated = rewrite_source(
        &incoming.text,
        &incoming.parsed.root,
        0..incoming.text.len(),
        destination,
        id,
        map,
        &mut quiet,
    )?;
    if relocated == current.text {
        return Ok(());
    }
    let shared = base
        .and_then(|(view, original, _)| view.docs.get(&format!("trace/sessions/{original}.yaml")));
    if let (Some(shared), Some((_, _, base_map))) = (shared, base)
        && rewrite_source(
            &shared.text,
            &shared.parsed.root,
            0..shared.text.len(),
            destination,
            id,
            base_map,
            &mut quiet,
        )? == relocated
    {
        return Ok(());
    }
    conflict(
        report,
        destination,
        id,
        "entry",
        "protected_inherited_entry",
        shared.map(|doc| doc.text.as_bytes()),
        Some(current.text.as_bytes()),
        Some(incoming.text.as_bytes()),
        ConflictLocator::Document,
    );
    Err(reject_protected(report))
}
pub(crate) fn mutation_origins(view: &Inventory) -> Result<Vec<EntryIdentity>, MergeError> {
    let rows = view
        .docs
        .values()
        .filter(|doc| doc.kind == Kind::Mutations)
        .map(|doc| {
            doc.parsed
                .root
                .get("mutations")?
                .ok_or_else(|| {
                    crate::write::WriteError::semantic("merge.mutation", "missing mutation rows")
                })?
                .to_json()
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flat_map(|value| value.as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let mut retired = BTreeMap::new();
    for entry in view
        .records
        .values()
        .filter(|entry| view.docs[&entry.path].kind == Kind::Mutations)
    {
        let Some(origin) = entry
            .fields
            .get("from")
            .and_then(|field| field.node.scalar())
        else {
            continue;
        };
        let native = entry
            .fields
            .get("from_selector")
            .map(|field| field.node.to_json())
            .transpose()?
            .map(serde_json::from_value::<crate::write::EntrySelector>)
            .transpose()
            .map_err(|error| MergeError::content("merge.mutation", error.to_string()))?
            .as_ref()
            .and_then(|selector| super::identity::archived_selector_key(selector, &rows))
            .unwrap_or_else(|| super::identity::normalize_local(origin));
        let Some((path, _)) = origin.split_once(':').or_else(|| origin.rsplit_once('#')) else {
            continue;
        };
        let mut heading = Vec::new();
        if let Some(selector) = entry.fields.get("from_selector")
            && let Some(headings) = selector.node.get("heading")?
        {
            for (ordinal, node) in headings.sequence()?.iter().enumerate() {
                let literal = node.scalar().ok_or_else(|| {
                    MergeError::content("merge.mutation", "literal heading must be a scalar")
                })?;
                let typed = typed_heading(path, ordinal, literal).then(|| {
                    literal
                        .split_once(':')
                        .map_or(literal, |(id, _)| id)
                        .to_owned()
                });
                if let Some(id) = &typed {
                    retired.entry(id.clone()).or_insert_with(|| EntryIdentity {
                        address: id.clone(),
                        layer: "historical_identity".into(),
                        path: path.into(),
                        numeric: super::identity::numeric_prefix(id),
                        session: false,
                        heading: Vec::new(),
                    });
                }
                heading.push((literal.into(), typed));
            }
        }
        let numeric = super::identity::numeric_prefix(&native)
            .filter(|prefix| matches!(prefix, 'C' | 'H' | 'E'));
        if numeric.is_some() {
            heading.clear();
        }
        retired
            .entry(native.clone())
            .or_insert_with(|| EntryIdentity {
                address: native,
                layer: "historical_identity".into(),
                path: path.into(),
                numeric,
                session: false,
                heading,
            });
    }
    Ok(retired.into_values().collect())
}
