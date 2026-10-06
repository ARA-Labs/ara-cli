use super::{Artifact, Entry};
use crate::output::AgentError;
use ara_core::write::positions::{YamlDocument, YamlKind, YamlNode};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

pub struct References {
    pub rows: Vec<Value>,
    pub ranges: BTreeMap<String, BTreeSet<(usize, usize)>>,
}
impl References {
    fn add(
        &mut self,
        source: &str,
        owner: Option<&str>,
        field: &str,
        literal: &str,
        range: Range<usize>,
    ) {
        let ranges = self.ranges.entry(source.into()).or_default();
        if !ranges.insert((range.start, range.end)) {
            return;
        }
        self.rows.push(json!({"id":owner,"source":source,"field":field,"literal":literal,"range":range,"certainty":"certain"}));
    }
}
fn get<'a>(node: &'a YamlNode, name: &str) -> Option<&'a YamlNode> {
    match &node.kind {
        YamlKind::Mapping(values) => values
            .iter()
            .find(|(key, _)| key.scalar() == Some(name))
            .map(|(_, value)| value),
        _ => None,
    }
}
fn rows(node: &YamlNode) -> impl DoubleEndedIterator<Item = &YamlNode> {
    let values = match &node.kind {
        YamlKind::Sequence(values) => values.as_slice(),
        _ => std::slice::from_ref(node),
    };
    values.iter()
}
#[derive(Clone, Copy)]
struct Target<'a> {
    entry: Entry<'a>,
    artifact: &'a Artifact,
    bare_unique: bool,
}
impl<'a> Target<'a> {
    fn key(self) -> &'a str {
        self.entry.key()
    }
    fn source_matches(self, path: &str) -> bool {
        self.entry.source_matches(path)
    }
    fn key_matches(self, literal: &str) -> bool {
        let current = if matches!(self.entry, Entry::Claim(_)) {
            self.artifact
                .claim_redirects
                .get(literal)
                .map_or(literal, String::as_str)
        } else {
            literal
        };
        current == self.key()
    }
}
fn matches(target: Target<'_>, literal: &str) -> bool {
    if let Some((scope, key)) = literal.split_once(':').or_else(|| literal.split_once('#')) {
        if matches!(target.entry,Entry::Document{path,..}if path==scope) {
            return true;
        }
        if (target.source_matches(scope)
            || scope == "trace" && matches!(target.entry, Entry::Node(_)))
            && target.key_matches(key)
        {
            return true;
        }
        if matches!(target.entry, Entry::Session(_))
            && scope == target.key()
            && key.bytes().all(|byte| byte.is_ascii_digit())
            && !key.is_empty()
        {
            return true;
        }
        return false;
    }
    target.bare_unique && target.key_matches(literal)
}
fn values(
    result: &mut References,
    source: &str,
    owner: Option<&str>,
    field: &str,
    node: &YamlNode,
    target: Target<'_>,
) {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        match &node.kind {
            YamlKind::Scalar { value, .. } => {
                if matches(target, value) {
                    result.add(source, owner, field, value, node.start..node.end);
                }
            }
            YamlKind::Sequence(items) => pending.extend(items.iter().rev()),
            YamlKind::Mapping(_) => {
                // Revision selectors are structured native addresses, not arbitrary
                // mappings whose unknown contents happen to look like identifiers.
                if let Some(id) = get(node, "id") {
                    pending.push(id);
                } else if let (Some(document), Some(heading)) = (
                    get(node, "document").and_then(YamlNode::scalar),
                    get(node, "heading"),
                ) {
                    let names: Vec<_> = rows(heading).filter_map(YamlNode::scalar).collect();
                    if target.source_matches(document)
                        && names.last().is_some_and(|name| {
                            super::headings::heading_matches(name, target.key())
                                || name
                                    .split_once(':')
                                    .is_some_and(|(id, _)| target.key_matches(id.trim()))
                        })
                    {
                        result.add(
                            source,
                            owner,
                            field,
                            &format!("{document}#{}", names.join(" / ")),
                            node.start..node.end,
                        );
                    }
                    if let Some(entry) = get(node, "entry") {
                        values(result, source, owner, field, entry, target);
                    }
                }
            }
            YamlKind::Alias(_) => {}
        }
    }
}
fn fields(
    result: &mut References,
    source: &str,
    owner: Option<&str>,
    node: &YamlNode,
    names: &[&str],
    target: Target<'_>,
) {
    for name in names {
        if let Some(value) = get(node, name) {
            values(result, source, owner, name, value, target);
        }
    }
}
fn annotations(
    result: &mut References,
    source: &str,
    owner: Option<&str>,
    node: &YamlNode,
    target: Target<'_>,
) {
    for name in ["annotations", "conflict_annotations"] {
        if let Some(list) = get(node, name) {
            for row in rows(list) {
                fields(result, source, owner, row, &["references"], target);
            }
        }
    }
}
fn yaml(
    result: &mut References,
    source: &str,
    text: &str,
    target: Target<'_>,
) -> Result<(), AgentError> {
    let parsed = YamlDocument::parse(text).map_err(crate::write::convert_error)?;
    let root = &parsed.root;
    match source {
        "trace/exploration_tree.yaml" => {
            if let Some(tree) = get(root, "tree").or_else(|| get(root, "root")) {
                let mut pending: Vec<_> = rows(tree).rev().map(|node| (node, None)).collect();
                while let Some((node, parent)) = pending.pop() {
                    let owner = get(node, "id").and_then(YamlNode::scalar);
                    if let (Some(parent), Some(id)) = (parent, get(node, "id")) {
                        values(result, source, Some(parent), "children", id, target);
                    }
                    fields(
                        result,
                        source,
                        owner,
                        node,
                        &["parent", "evidence", "also_depends_on", "same_as"],
                        target,
                    );
                    if matches!(target.entry, Entry::Concept(_)) {
                        fields(
                            result,
                            source,
                            owner,
                            node,
                            &["concepts"],
                            Target {
                                bare_unique: true,
                                ..target
                            },
                        );
                    }
                    fields(result, source, owner, node, &["source_refs"], target);
                    if let Some(artifacts) = get(node, "artifacts") {
                        for artifact in rows(artifacts) {
                            fields(result, source, owner, artifact, &["pointer"], target);
                        }
                    }
                    annotations(result, source, owner, node, target);
                    if let Some(children) = get(node, "children") {
                        pending.extend(rows(children).rev().map(|node| (node, owner)));
                    }
                }
            }
        }
        "staging/observations.yaml"
        | "trace/taste_log.yaml"
        | "trace/taste.yaml"
        | "trace/pm_reasoning_log.yaml"
        | "trace/reasoning.yaml"
        | "trace/logic_mutations.yaml" => {
            let (list, names): (&str, &[&str]) = match source {
                "staging/observations.yaml" => (
                    "observations",
                    &["bound_to", "promoted_to", "crystallized_via"],
                ),
                "trace/taste_log.yaml" | "trace/taste.yaml" => ("entries", &["target"]),
                "trace/logic_mutations.yaml" => (
                    "mutations",
                    &[
                        "from",
                        "to",
                        "from_selector",
                        "to_selector",
                        "session",
                        "turn",
                        "historical_references",
                    ],
                ),
                _ => ("entries", &["turn"]),
            };
            if let Some(list) = get(root, list) {
                for row in rows(list) {
                    let owner = get(row, "id").and_then(YamlNode::scalar);
                    fields(result, source, owner, row, names, target);
                    annotations(result, source, owner, row, target);
                }
            }
        }
        "trace/sessions/session_index.yaml" => {
            if let Some(list) = get(root, "sessions") {
                for row in rows(list) {
                    fields(result, source, None, row, &["id", "file", "path"], target);
                }
            }
        }
        _ if source.starts_with("trace/sessions/") => {
            let owner = get(root, "session")
                .and_then(|metadata| get(metadata, "id"))
                .and_then(YamlNode::scalar);
            for (list, names) in [
                ("events_logged", &["id", "target"] as &[&str]),
                ("claims_touched", &["id"]),
                ("logic_revisions", &["entry"]),
                ("ai_actions", &["files_changed"]),
            ] {
                if let Some(list) = get(root, list) {
                    for row in rows(list) {
                        fields(result, source, owner, row, names, target);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn structured(artifact: &Artifact, target: Entry<'_>) -> Result<References, AgentError> {
    let mut result = References {
        rows: Vec::new(),
        ranges: BTreeMap::new(),
    };
    let target = Target {
        entry: target,
        artifact,
        bare_unique: artifact
            .entries()
            .iter()
            .filter(|entry| entry.key() == target.key())
            .count()
            == 1,
    };
    for (source, text) in &artifact.sources {
        if !artifact.is_knowledge(source) {
            continue;
        }
        if source.ends_with(".yaml") || source.ends_with(".yml") {
            yaml(&mut result, source, text, target)?;
            continue;
        }
        if !source.ends_with(".md") {
            continue;
        }
        for section in ara_core::markdown::document_sections(source, text) {
            let owner = super::headings::heading_id(section.heading);
            for field in ara_core::markdown::fields(text, section.body_range) {
                if !matches!(
                    field.name,
                    "Proof"
                        | "Dependencies"
                        | "Depends on"
                        | "Deps"
                        | "Sources"
                        | "Claims affected"
                        | "Related"
                        | "Promoted from"
                        | "Last revised"
                        | "Merged into"
                        | "Evidence output"
                        | "Code ref"
                ) {
                    continue;
                }
                if matches(target, field.value) {
                    result.add(
                        source,
                        owner,
                        field.name,
                        field.value,
                        field.value_range.clone(),
                    );
                    continue;
                }
                for token in ara_core::query::scan_tokens(field.value) {
                    if matches(target, token.literal) {
                        result.add(
                            source,
                            owner,
                            field.name,
                            token.literal,
                            field.value_range.start + token.range.start
                                ..field.value_range.start + token.range.end,
                        );
                    }
                }
                // Scoped locators are deliberately excluded by the bare-ID prose
                // scanner. A qualified spelling is certain only in a declared field.
                let mut offset = 0;
                for part in field.value.split_inclusive(|character: char| {
                    character.is_whitespace()
                        || matches!(character, ',' | '[' | ']' | '(' | ')' | '`' | '\"' | '\'')
                }) {
                    let literal = part.trim_matches(|character: char| {
                        character.is_whitespace()
                            || matches!(character, ',' | '[' | ']' | '(' | ')' | '`' | '\"' | '\'')
                    });
                    if !literal.is_empty() && matches(target, literal) {
                        let start = part.find(literal).expect("trimmed slice");
                        result.add(
                            source,
                            owner,
                            field.name,
                            literal,
                            field.value_range.start + offset + start
                                ..field.value_range.start + offset + start + literal.len(),
                        );
                    }
                    offset += part.len();
                }
            }
        }
    }
    Ok(result)
}
