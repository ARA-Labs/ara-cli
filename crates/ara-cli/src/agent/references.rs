use super::{Artifact, Entry};
use crate::output::AgentError;
use ara_core::write::citation_rules::{self, HistoryRole};
use ara_core::write::positions::{YamlDocument, YamlKind, YamlNode};
use ara_core::write::{EntrySelector, WorkingArtifact};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Parsed sources and the writer snapshot shared by all refs targets of one show.
pub(super) struct Inventory<'a> {
    yaml: BTreeMap<String, YamlDocument>,
    markdown: Vec<(
        Entry<'a>,
        Option<Vec<ara_core::write::logic::MarkdownCitation>>,
    )>,
    pub(super) tokens: BTreeMap<&'a str, Vec<ara_core::query::TokenMatch<'a>>>,
}
impl<'a> Inventory<'a> {
    pub(super) fn new(artifact: &'a Artifact, targets: &[Entry<'a>]) -> Result<Self, AgentError> {
        let needs_writer = targets
            .iter()
            .any(|target| logic_selector(*target).is_some())
            || artifact.root.join("trace/aliases.yaml").is_file();
        let working = needs_writer
            .then(|| {
                artifact
                    .snapshot()
                    .map(|snapshot| WorkingArtifact::new(snapshot.clone()))
            })
            .transpose()?;
        let mut yaml = BTreeMap::new();
        for (source, text) in &artifact.sources {
            if artifact.is_knowledge(source)
                && citation_rules::is_history_source(source)
                && (source.ends_with(".yaml") || source.ends_with(".yml"))
            {
                yaml.insert(
                    source.clone(),
                    YamlDocument::parse(text).map_err(crate::write::convert_error)?,
                );
            }
        }
        if !yaml.contains_key("trace/aliases.yaml")
            && let Some(working) = &working
            && let Some(file) = working
                .base
                .files
                .get("trace/aliases.yaml")
                .filter(|file| file.existed)
            && let Ok(text) = std::str::from_utf8(&file.bytes)
        {
            yaml.insert(
                "trace/aliases.yaml".into(),
                YamlDocument::parse(text).map_err(crate::write::convert_error)?,
            );
        }
        let mut selected = Vec::new();
        let mut selectors = Vec::new();
        for target in targets {
            if let Some(selector) = logic_selector(*target) {
                selected.push(*target);
                selectors.push(selector);
            }
        }
        let markdown = if selectors.is_empty() {
            Vec::new()
        } else {
            let working = working.as_ref().expect("logic target snapshot");
            // Per-selector failures preserve the read-only fallback for recovered
            // headings; a shared classification failure falls back for all targets.
            match ara_core::write::logic::markdown_citations_many(working, &selectors) {
                Ok(results) => selected
                    .into_iter()
                    .zip(results.into_iter().map(Result::ok))
                    .collect(),
                Err(_) => selected.into_iter().map(|entry| (entry, None)).collect(),
            }
        };
        let tokens = artifact
            .sources
            .iter()
            .filter(|(source, _)| artifact.is_knowledge(source))
            .map(|(source, text)| (source.as_str(), ara_core::query::scan_tokens(text)))
            .collect();
        Ok(Self {
            yaml,
            markdown,
            tokens,
        })
    }
}
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
fn yaml(
    result: &mut References,
    source: &str,
    parsed: &YamlDocument,
    target: Target<'_>,
) -> Result<(), AgentError> {
    let concepts = concept_paths(target);
    citation_rules::walk_history(source, &parsed.root, &mut |value| {
        if value.role == HistoryRole::ConceptName {
            if let Entry::Concept(concept) = target.entry {
                concept_values(result, source, value, &concept.term, concepts.as_deref());
            }
            return;
        }
        values(result, source, value.owner, value.field, value.node, target);
    });
    Ok(())
}
/// Concept headings of the current `logic/concepts.md` as (leaf, path).
fn concept_paths(target: Target<'_>) -> Option<Vec<(String, Vec<String>)>> {
    let text = target.artifact.sources.get("logic/concepts.md")?;
    Some(
        ara_core::markdown::headings(text)
            .into_iter()
            .map(|h| {
                (
                    h.heading.to_owned(),
                    h.path.iter().map(|part| (*part).to_owned()).collect(),
                )
            })
            .collect(),
    )
}
/// A bare tree concept name cites a concept only when it resolves to exactly
/// one concept heading, by the writer's rule ([`citation_rules::concept_name_targets`]).
fn concept_values(
    result: &mut References,
    source: &str,
    value: citation_rules::HistoryValue<'_>,
    term: &str,
    paths: Option<&[(String, Vec<String>)]>,
) {
    let Some(paths) = paths else {
        return;
    };
    let shaped: Vec<(&str, &[String])> = paths
        .iter()
        .map(|(leaf, path)| (leaf.as_str(), path.as_slice()))
        .collect();
    let mut pending = vec![value.node];
    while let Some(node) = pending.pop() {
        match &node.kind {
            YamlKind::Scalar { value: literal, .. } => {
                let found = citation_rules::concept_name_targets(&shaped, literal);
                if let [index] = found.as_slice()
                    && shaped[*index].0 == term
                {
                    result.add(
                        source,
                        value.owner,
                        value.field,
                        literal,
                        node.start..node.end,
                    );
                }
            }
            YamlKind::Sequence(items) => pending.extend(items.iter().rev()),
            _ => {}
        }
    }
}
/// The writer selector of a native logic entry, when it has one.
fn logic_selector(entry: Entry<'_>) -> Option<EntrySelector> {
    let document = |document: &str, id: &str| EntrySelector::Document {
        document: document.into(),
        heading: Vec::new(),
        entry: Some(id.into()),
    };
    match entry {
        Entry::Claim(claim) => Some(EntrySelector::Id {
            id: claim.id.as_str().into(),
        }),
        Entry::Heuristic(heuristic) => {
            Some(document(&heuristic.source_file, heuristic.id.as_str()))
        }
        Entry::Experiment(experiment) => {
            Some(document(&experiment.source_file, experiment.id.as_str()))
        }
        Entry::Concept(concept) => Some(EntrySelector::Document {
            document: "logic/concepts.md".into(),
            heading: vec![concept.term.clone()],
            entry: None,
        }),
        _ => None,
    }
}
pub(super) fn structured(
    artifact: &Artifact,
    target: Entry<'_>,
    inventory: &Inventory<'_>,
) -> Result<References, AgentError> {
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
    // Native logic entries share the writer's typed citation inventory, the
    // same classifier C1 repairs with.
    let shared = inventory
        .markdown
        .iter()
        .find(|(entry, _)| {
            entry.key() == target.key()
                && entry.kind() == target.entry.kind()
                && entry.source_matches(target.entry.source_path().as_ref())
        })
        .and_then(|(_, citations)| citations.as_ref());
    if let Some(citations) = shared {
        for citation in citations {
            result.add(
                &citation.document,
                Some(&citation.owner),
                &citation.field,
                &citation.literal,
                citation.range.clone(),
            );
        }
    }
    // History layers the read model does not load (the portable alias
    // ledger) still carry citations; walk them from the exact snapshot.
    if !artifact.sources.contains_key("trace/aliases.yaml")
        && let Some(parsed) = inventory.yaml.get("trace/aliases.yaml")
    {
        yaml(&mut result, "trace/aliases.yaml", parsed, target)?;
    }
    for (source, text) in &artifact.sources {
        if !artifact.is_knowledge(source) {
            continue;
        }
        if source.ends_with(".yaml") || source.ends_with(".yml") {
            if citation_rules::is_history_source(source) {
                yaml(
                    &mut result,
                    source,
                    inventory.yaml.get(source).expect("parsed history"),
                    target,
                )?;
            }
            continue;
        }
        if !source.ends_with(".md") || shared.is_some() {
            continue;
        }
        for section in ara_core::markdown::document_sections(source, text) {
            let owner = super::headings::heading_id(section.heading);
            for field in ara_core::markdown::fields(text, section.body_range) {
                if citation_rules::reference_field(field.name).is_none() {
                    continue;
                }
                // Quoted, backticked and commented tokens are not references.
                let protected = citation_rules::protected_ranges(field.value);
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
                    if matches(target, token.literal)
                        && !citation_rules::is_protected(&protected, &token.range)
                    {
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
                    let start = part.find(literal).unwrap_or(0);
                    let range = offset + start..offset + start + literal.len();
                    if !literal.is_empty()
                        && matches(target, literal)
                        && !citation_rules::is_protected(&protected, &range)
                    {
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
