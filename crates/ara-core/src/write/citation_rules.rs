//! Shared typed-citation rules: which Markdown fields carry references, which
//! of them C1 may rewrite, which spans are protected, and which historical
//! YAML values are native citations. `ara refs` (the read model) and the
//! writer's citation inventory (plan 19 C1) both use these rules, so a listed
//! reference and a repairable citation are classified the same way.
use super::fields;
use super::positions::{YamlKind, YamlNode};
use std::ops::Range;

/// One reference-bearing Markdown field label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceField {
    pub label: &'static str,
    /// C1 may rewrite parsed citations in it (where the entry's schema also
    /// accepts the field). Read-only aliases stay readable for `refs` only.
    pub rewritable: bool,
}

/// Every Markdown field whose parsed tokens are native references.
pub const REFERENCE_FIELDS: &[ReferenceField] = &[
    ReferenceField {
        label: "Proof",
        rewritable: true,
    },
    ReferenceField {
        label: "Dependencies",
        rewritable: true,
    },
    ReferenceField {
        label: "Depends on",
        rewritable: false,
    },
    ReferenceField {
        label: "Deps",
        rewritable: false,
    },
    ReferenceField {
        label: "Sources",
        rewritable: true,
    },
    ReferenceField {
        label: "Claims affected",
        rewritable: true,
    },
    ReferenceField {
        label: "Related",
        rewritable: true,
    },
    ReferenceField {
        label: "Related concepts",
        rewritable: true,
    },
    ReferenceField {
        label: "Promoted from",
        rewritable: false,
    },
    ReferenceField {
        label: "Last revised",
        rewritable: false,
    },
    ReferenceField {
        label: "Merged into",
        rewritable: true,
    },
    ReferenceField {
        label: "Evidence output",
        rewritable: true,
    },
    ReferenceField {
        label: "Code ref",
        rewritable: true,
    },
];

/// The reference-field rule for a source label (canonical spelling).
pub fn reference_field(label: &str) -> Option<&'static ReferenceField> {
    let canonical = fields::canonical(label);
    REFERENCE_FIELDS
        .iter()
        .find(|field| fields::canonical(field.label) == canonical)
}

/// Whether C1 may rewrite citations in `label` of an entry of `document`:
/// a rewritable reference field that the entry's schema accepts.
pub fn rewritable(document: &str, label: &str) -> bool {
    reference_field(label).is_some_and(|field| field.rewritable)
        && fields::kind(document)
            .ok()
            .and_then(|kind| fields::spelling(kind, label))
            .is_some()
}

/// Spans of a field value whose tokens are never references: quoted or
/// backticked text, fenced code and HTML comments (merge rewriting's rule).
/// A value that is exactly a JSON array of strings is a native list: only its
/// outer item quotes are delimiters. Inner quoted or backticked spans remain
/// protected, as do whole escaped items and HTML comments.
pub fn protected_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = if let Some(items) = json_string_items(text) {
        let mut ranges = Vec::new();
        for item in items {
            if text[item.clone()].contains('\\') {
                ranges.push(item);
            } else {
                let start = item.start + 1;
                let end = item.end - 1;
                ranges.extend(
                    crate::merge::quoted_ranges(&text[start..end])
                        .into_iter()
                        .map(|range| start + range.start..start + range.end),
                );
            }
        }
        ranges
    } else {
        crate::merge::quoted_ranges(text)
    };
    let mut search = 0;
    while let Some(found) = text.get(search..).and_then(|rest| rest.find("<!--")) {
        let start = search + found;
        let end = text[start + 4..]
            .find("-->")
            .map_or(text.len(), |end| start + 4 + end + 3);
        ranges.push(start..end);
        search = end;
    }
    ranges.sort_by_key(|range| range.start);
    ranges
}

/// Whether `range` lies inside any protected span.
pub fn is_protected(ranges: &[Range<usize>], range: &Range<usize>) -> bool {
    ranges
        .iter()
        .any(|protected| protected.start <= range.start && range.end <= protected.end)
}

/// Source ranges of the string items (quotes included) when `text` is exactly
/// a JSON array of strings.
fn json_string_items(text: &str) -> Option<Vec<Range<usize>>> {
    serde_json::from_str::<Vec<String>>(text).ok()?;
    let bytes = text.as_bytes();
    let mut items = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b'\\' {
                i += 1;
            }
            i += 1;
        }
        i = (i + 1).min(bytes.len());
        items.push(start..i);
    }
    Some(items)
}

/// How a historical YAML value takes part in identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRole {
    /// A native citation (ID, locator or selector) that must keep resolving.
    Citation,
    /// A tree `concepts` name, implicitly in `logic/concepts.md`.
    ConceptName,
    /// A tree child's own ID listed under its parent.
    Child,
    /// A mutation-ledger row describing an identity transition itself.
    Ledger,
    /// A session-index row or reasoning turn locator.
    Index,
}

/// One historical value, its owning record ID and its field.
#[derive(Debug, Clone, Copy)]
pub struct HistoryValue<'a> {
    pub owner: Option<&'a str>,
    pub field: &'static str,
    pub node: &'a YamlNode,
    pub role: HistoryRole,
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

/// Whether `source` is a historical YAML layer with native citations.
pub fn is_history_source(source: &str) -> bool {
    matches!(
        source,
        "trace/exploration_tree.yaml"
            | "staging/observations.yaml"
            | "trace/taste_log.yaml"
            | "trace/taste.yaml"
            | "trace/pm_reasoning_log.yaml"
            | "trace/reasoning.yaml"
            | "trace/logic_mutations.yaml"
            | "trace/aliases.yaml"
            | "trace/sessions/session_index.yaml"
    ) || source.starts_with("trace/sessions/") && source.ends_with(".yaml")
}

/// Visit every native citation-bearing value of one historical YAML source.
pub fn walk_history<'a>(source: &str, root: &'a YamlNode, visit: &mut dyn FnMut(HistoryValue<'a>)) {
    let fields = |owner: Option<&'a str>,
                  node: &'a YamlNode,
                  names: &[&'static str],
                  role: HistoryRole,
                  visit: &mut dyn FnMut(HistoryValue<'a>)| {
        for name in names {
            if let Some(value) = get(node, name) {
                visit(HistoryValue {
                    owner,
                    field: name,
                    node: value,
                    role,
                });
            }
        }
    };
    let annotations =
        |owner: Option<&'a str>, node: &'a YamlNode, visit: &mut dyn FnMut(HistoryValue<'a>)| {
            for name in ["annotations", "conflict_annotations"] {
                if let Some(list) = get(node, name) {
                    for row in rows(list) {
                        if let Some(value) = get(row, "references") {
                            visit(HistoryValue {
                                owner,
                                field: "references",
                                node: value,
                                role: HistoryRole::Citation,
                            });
                        }
                    }
                }
            }
        };
    match source {
        "trace/exploration_tree.yaml" => {
            let Some(tree) = get(root, "tree").or_else(|| get(root, "root")) else {
                return;
            };
            let mut pending: Vec<_> = rows(tree).rev().map(|node| (node, None)).collect();
            while let Some((node, parent)) = pending.pop() {
                let owner = get(node, "id").and_then(YamlNode::scalar);
                if let (Some(parent), Some(id)) = (parent, get(node, "id")) {
                    visit(HistoryValue {
                        owner: Some(parent),
                        field: "children",
                        node: id,
                        role: HistoryRole::Child,
                    });
                }
                fields(
                    owner,
                    node,
                    &["parent", "evidence", "also_depends_on", "same_as"],
                    HistoryRole::Citation,
                    visit,
                );
                fields(owner, node, &["concepts"], HistoryRole::ConceptName, visit);
                fields(owner, node, &["source_refs"], HistoryRole::Citation, visit);
                if let Some(artifacts) = get(node, "artifacts") {
                    for artifact in rows(artifacts) {
                        fields(owner, artifact, &["pointer"], HistoryRole::Citation, visit);
                    }
                }
                annotations(owner, node, visit);
                if let Some(children) = get(node, "children") {
                    pending.extend(rows(children).rev().map(|child| (child, owner)));
                }
            }
        }
        "staging/observations.yaml"
        | "trace/taste_log.yaml"
        | "trace/taste.yaml"
        | "trace/pm_reasoning_log.yaml"
        | "trace/reasoning.yaml"
        | "trace/logic_mutations.yaml"
        | "trace/aliases.yaml" => {
            let (list, names, role): (&str, &[&'static str], HistoryRole) = match source {
                "staging/observations.yaml" => (
                    "observations",
                    &["bound_to", "promoted_to", "crystallized_via"],
                    HistoryRole::Citation,
                ),
                "trace/taste_log.yaml" | "trace/taste.yaml" => {
                    ("entries", &["target"], HistoryRole::Citation)
                }
                "trace/aliases.yaml" => ("aliases", &["target"], HistoryRole::Citation),
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
                    HistoryRole::Ledger,
                ),
                _ => ("entries", &["turn"], HistoryRole::Index),
            };
            if let Some(list) = get(root, list) {
                for row in rows(list) {
                    let owner = get(row, "id").and_then(YamlNode::scalar);
                    fields(owner, row, names, role, visit);
                    annotations(owner, row, visit);
                }
            }
        }
        "trace/sessions/session_index.yaml" => {
            if let Some(list) = get(root, "sessions") {
                for row in rows(list) {
                    fields(
                        None,
                        row,
                        &["id", "file", "path"],
                        HistoryRole::Index,
                        visit,
                    );
                }
            }
        }
        _ if source.starts_with("trace/sessions/") => {
            let owner = get(root, "session")
                .and_then(|metadata| get(metadata, "id"))
                .and_then(YamlNode::scalar);
            for (list, names) in [
                ("events_logged", &["id", "target"] as &[&'static str]),
                ("claims_touched", &["id"]),
                ("logic_revisions", &["entry"]),
                ("ai_actions", &["files_changed"]),
            ] {
                if let Some(list) = get(root, list) {
                    for row in rows(list) {
                        fields(owner, row, names, HistoryRole::Citation, visit);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Whether `text` spells `path` joined by `/` (the complete vector).
pub fn joined_heading_path<S: AsRef<str>>(path: &[S], text: &str) -> bool {
    let mut rest = text;
    for (index, part) in path.iter().enumerate() {
        if index > 0 {
            let Some(next) = rest.strip_prefix('/') else {
                return false;
            };
            rest = next;
        }
        let Some(next) = rest.strip_prefix(part.as_ref()) else {
            return false;
        };
        rest = next;
    }
    rest.is_empty()
}

/// The one rule for a bare concept name: it names a concept heading by its
/// exact leaf text (one segment) or its complete joined heading path.
/// Returns the number of segments the spelling uses.
pub fn concept_name_match<S: AsRef<str>>(leaf: &str, path: &[S], name: &str) -> Option<usize> {
    if leaf == name {
        Some(1)
    } else if path.len() > 1 && joined_heading_path(path, name) {
        Some(path.len())
    } else {
        None
    }
}

/// Indexes of the concept headings a bare concept name resolves to
/// ([`concept_name_match`]). One index is a unique resolution; several are
/// ambiguous.
pub fn concept_name_targets<S: AsRef<str>>(paths: &[(&str, &[S])], name: &str) -> Vec<usize> {
    paths
        .iter()
        .enumerate()
        .filter(|(_, (leaf, path))| concept_name_match(leaf, path, name).is_some())
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_marks_read_only_aliases() {
        assert!(reference_field("Related concepts").unwrap().rewritable);
        assert!(!reference_field("Depends on").unwrap().rewritable);
        assert!(reference_field("Statement").is_none());
        assert!(rewritable("logic/claims.md", "Proof"));
        assert!(!rewritable("logic/claims.md", "Code ref"));
        assert!(!rewritable("logic/claims.md", "Last revised"));
    }

    #[test]
    fn quotes_and_comments_protect_but_json_lists_do_not() {
        let text = "see \"C01\" and C02 <!-- C03 --> `C04`";
        let ranges = protected_ranges(text);
        let at = |needle: &str| {
            let start = text.find(needle).unwrap();
            start..start + needle.len()
        };
        assert!(is_protected(&ranges, &at("C01")));
        assert!(!is_protected(&ranges, &at("C02")));
        assert!(is_protected(&ranges, &at("C03")));
        assert!(is_protected(&ranges, &at("C04")));
        let list = "[\"paper\", \"C02\"]";
        assert!(!is_protected(&protected_ranges(list), &(11..14)));
    }
}
