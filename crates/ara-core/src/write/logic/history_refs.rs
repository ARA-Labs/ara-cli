//! Typed historical citations of a restructured identity (plan 19 C1).
//!
//! History is never rewritten. A rename or redirecting removal records every
//! immutable typed citation of the old identity (tree evidence, concepts,
//! source refs, artifact pointers, annotations, observation pointers, session
//! rows, taste and alias targets); final validation resolves each one through
//! the retained entry or the authenticated mutation ledger. A citation that is
//! ambiguous between the subject and another entry refuses the restructure.
use super::citations::{Resolver, Subject};
use super::{
    heading_id, locator_parts, registry_anchors, registry_node, resolve_audited_selector,
    selector_from_yaml,
};
use crate::write::{
    EntrySelector, WorkingArtifact, WriteError,
    citation_rules::{self, HistoryRole},
    fields,
    source::{YamlKind, YamlNode},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn refusal(code: &str, message: String, field: &str, locations: Vec<Value>) -> WriteError {
    WriteError::semantic(code, message)
        .at(field)
        .with_locations(locations)
}

/// A historical typed citation of a restructured identity. Final validation
/// resolves it through the retained entry or the authenticated mutation
/// ledger; unresolvable history refuses the whole batch.
#[derive(Debug, Clone)]
pub struct HistoricalCitation {
    pub selector: EntrySelector,
    pub source: String,
    pub field: String,
    pub literal: String,
    /// Zero-based index of the restructuring operation.
    pub operation: usize,
}

/// Typed citations of the subject in immutable history. An ambiguous one
/// refuses immediately: the restructure could silently re-point it.
pub(super) fn historical(
    resolver: &Resolver<'_>,
    subject: &Subject,
) -> Result<Vec<HistoricalCitation>, WriteError> {
    let working = resolver.working;
    let mut found = Vec::new();
    let mut ambiguous = Vec::new();
    for source in working.paths() {
        if !citation_rules::is_history_source(&source) {
            continue;
        }
        // Already malformed legacy YAML was not a readable native reference.
        let Ok(document) = working.yaml(&source) else {
            continue;
        };
        let mut anchors = BTreeMap::new();
        registry_anchors(&document.root, &mut anchors);
        let mut visited = Vec::new();
        citation_rules::walk_history(&source, &document.root, &mut |value| {
            if matches!(value.role, HistoryRole::Citation | HistoryRole::ConceptName) {
                visited.push(value);
            }
        });
        for value in visited {
            values(
                resolver,
                subject,
                value.node,
                &anchors,
                &source,
                value.field,
                &mut found,
                &mut ambiguous,
            )?;
        }
    }
    if !ambiguous.is_empty() {
        return Err(refusal(
            "write.history_unresolved",
            "historical citations are ambiguous between the restructured entry and another; the restructure would silently re-point them".into(),
            "target",
            ambiguous,
        ));
    }
    Ok(found)
}

#[allow(clippy::too_many_arguments)]
fn values(
    resolver: &Resolver<'_>,
    subject: &Subject,
    node: &YamlNode,
    anchors: &BTreeMap<usize, &YamlNode>,
    source: &str,
    field: &str,
    found: &mut Vec<HistoricalCitation>,
    ambiguous: &mut Vec<Value>,
) -> Result<(), WriteError> {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        let node = registry_node(node, anchors);
        match &node.kind {
            YamlKind::Sequence(items) => pending.extend(items.iter()),
            YamlKind::Mapping(_) => {
                if let Some(selector) = selector_from_yaml(node, anchors) {
                    let literal = serde_json::to_string(&selector).unwrap_or_default();
                    classify_selector(
                        resolver, subject, selector, source, field, literal, found, ambiguous,
                    )?;
                }
            }
            YamlKind::Scalar { .. } => {
                let Some(literal) = node.scalar() else {
                    continue;
                };
                let selector = if field == "concepts" && locator_parts(literal).is_none() {
                    EntrySelector::Document {
                        document: "logic/concepts.md".into(),
                        heading: vec![literal.into()],
                        entry: None,
                    }
                } else if !literal.contains([':', '#', '/'])
                    && super::native_id_document(literal).is_some()
                {
                    EntrySelector::Id { id: literal.into() }
                } else if let Some((document, Some(identity), numbered)) = locator_parts(literal) {
                    let numbered = numbered
                        && super::native_document_prefix(document)
                            .is_some_and(|prefix| fields::typed_id(identity, prefix));
                    if numbered {
                        EntrySelector::Document {
                            document: document.into(),
                            heading: Vec::new(),
                            entry: Some(identity.into()),
                        }
                    } else {
                        EntrySelector::Document {
                            document: document.into(),
                            heading: vec![identity.into()],
                            entry: None,
                        }
                    }
                } else {
                    continue;
                };
                classify_selector(
                    resolver,
                    subject,
                    selector,
                    source,
                    field,
                    literal.to_owned(),
                    found,
                    ambiguous,
                )?;
            }
            YamlKind::Alias(_) => {}
        }
    }
    Ok(())
}

/// Record a historical selector that resolves (or may resolve) into the
/// subject. Heading-vector selectors match by literal suffix or joined path.
#[allow(clippy::too_many_arguments)]
fn classify_selector(
    resolver: &Resolver<'_>,
    subject: &Subject,
    selector: EntrySelector,
    source: &str,
    field: &str,
    literal: String,
    found: &mut Vec<HistoricalCitation>,
    ambiguous: &mut Vec<Value>,
) -> Result<(), WriteError> {
    let (document, by_id) = match &selector {
        EntrySelector::Id { id } => {
            let Some(document) = super::native_id_document(id) else {
                return Ok(());
            };
            let live = resolver.redirects().get(id).cloned().unwrap_or(id.clone());
            (document.to_owned(), Some(live))
        }
        EntrySelector::Document {
            document, entry, ..
        } => (
            document.clone(),
            entry
                .as_ref()
                .map(|id| resolver.redirects().get(id).cloned().unwrap_or(id.clone())),
        ),
    };
    if document != subject.document {
        return Ok(());
    }
    let Some(headings) = resolver.headings(&document)? else {
        return Ok(());
    };
    let mut matches = Vec::new();
    for heading in headings.iter() {
        let hit = match (&selector, &by_id) {
            (_, Some(id)) => heading_id(&heading.heading) == id,
            // A one-segment vector is a bare name: the shared concept-name
            // rule. A longer vector keeps selector suffix semantics.
            (EntrySelector::Document { heading: path, .. }, None) => match path.as_slice() {
                [] => false,
                [name] => citation_rules::concept_name_match(&heading.heading, &heading.path, name)
                    .is_some(),
                _ => heading.path.ends_with(path),
            },
            _ => false,
        };
        if hit {
            matches.push(heading);
        }
    }
    let inside: Vec<_> = matches
        .iter()
        .filter(|heading| subject.range.contains(&heading.range.start))
        .collect();
    let Some(first) = inside.first() else {
        return Ok(());
    };
    let location = json!({"source":source,"field":field,"literal":literal});
    if matches.len() > 1 {
        ambiguous.push(location);
        return Ok(());
    }
    let root = first.range.start == subject.range.start;
    if by_id.is_some()
        && if root {
            subject.root_id_retained
        } else {
            subject.descendant_ids_retained
        }
    {
        return Ok(());
    }
    let selector = match selector {
        EntrySelector::Document {
            document,
            entry: None,
            ..
        } => EntrySelector::Document {
            document,
            heading: first.path.clone(),
            entry: None,
        },
        selector => selector,
    };
    found.push(HistoricalCitation {
        selector,
        source: source.into(),
        field: field.into(),
        literal,
        operation: 0,
    });
    Ok(())
}

/// Final validation: every recorded historical citation of a restructured
/// identity resolves through a live entry or an authenticated redirect.
pub(super) fn validate_history(working: &WorkingArtifact) -> Result<(), WriteError> {
    if working.citation_checks.is_empty() {
        return Ok(());
    }
    // The write-side redirect chain and the read-side identity index (what
    // `show`/`refs` consult) must both resolve every recorded citation.
    let snapshot = crate::merge::candidate_snapshot(working);
    // Each literal spelling is checked too, exactly as `show` resolves it:
    // a scalar locator or ID (tree concept names are concepts-document
    // locators); structured selectors are checked as selectors.
    let items: Vec<(EntrySelector, Option<String>)> = working
        .citation_checks
        .iter()
        .map(|check| {
            let literal = if check.literal.starts_with('{') {
                None
            } else if check.field == "concepts" && locator_parts(&check.literal).is_none() {
                Some(format!("logic/concepts.md#{}", check.literal))
            } else {
                Some(check.literal.clone())
            };
            (check.selector.clone(), literal)
        })
        .collect();
    let read_side = crate::merge::check_citations(&snapshot, &items);
    for (index, check) in working.citation_checks.iter().enumerate() {
        let failure = if let Err(error) = resolve_audited_selector(working, &check.selector) {
            Some(error.message)
        } else {
            match &read_side {
                Err(error) => Some(format!(
                    "recorded identities cannot be consulted: {}",
                    error.message
                )),
                Ok(results) => results.get(index).cloned().flatten(),
            }
        };
        if let Some(reason) = failure {
            let mut error = refusal(
                "write.history_unresolved",
                format!(
                    "historical citation `{}` in {} would no longer resolve through a retained identity or authenticated redirect ({reason})",
                    check.literal, check.source
                ),
                "target",
                vec![json!({"source":check.source,"field":check.field,"literal":check.literal})],
            );
            error.line = Some(check.operation + 1);
            return Err(error);
        }
    }
    Ok(())
}
