//! Native source spans of loaded entries: the Markdown section that holds a
//! heading-backed entry, a whole document, or the YAML mapping of an entry
//! with its nested entries excluded.
use super::headings::{Lookup, Sections};
use super::{Artifact, Entry};
use ara_core::write::positions::{YamlDocument, YamlKind, YamlNode};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Range;

/// Byte ranges of one native document that hold an entry's source.
pub struct Span<'a> {
    pub path: Cow<'a, str>,
    pub text: &'a str,
    pub ranges: Vec<Range<usize>>,
}

/// One range covering `0..len`.
pub fn whole(len: usize) -> Vec<Range<usize>> {
    std::iter::once(0..len).collect()
}

impl<'a> Entry<'a> {
    /// The native document an entry is read from.
    pub(super) fn source_path(self) -> Cow<'a, str> {
        match self {
            Self::Node(_) => "trace/exploration_tree.yaml".into(),
            Self::Claim(_) => "logic/claims.md".into(),
            Self::Observation(v) => v.source_file.as_str().into(),
            Self::Session(v) => v.source_file.as_str().into(),
            Self::Heuristic(v) => v.source_file.as_str().into(),
            Self::Experiment(v) => v.source_file.as_str().into(),
            Self::Taste(v) => v.source_file.as_str().into(),
            Self::Concept(_) => "logic/concepts.md".into(),
            Self::RelatedWork(_) => "logic/related_work.md".into(),
            Self::Recipe(v) => format!("logic/solution/{}.md", v.name).into(),
            Self::Exhibit(v) => v.file.as_str().into(),
            Self::Document { path, .. } => path.into(),
        }
    }
}

impl Artifact {
    /// The one Markdown section headed by a typed entry, if exactly one is.
    /// The native Markdown document that holds a heading-backed entry.
    fn entry_document<'a>(&'a self, entry: Entry<'a>) -> Option<(&'a str, &'a str)> {
        if matches!(entry, Entry::Document { .. } | Entry::Recipe(_)) {
            return None;
        }
        let (path, text) = match entry.source_path() {
            Cow::Borrowed(path) => self.sources.get_key_value(path)?,
            Cow::Owned(path) => self.sources.get_key_value(&path)?,
        };
        (path.ends_with(".md") && self.is_knowledge(path)).then_some((path.as_str(), text.as_str()))
    }
    /// The one Markdown section headed by a typed entry, if exactly one is.
    pub(super) fn entry_section<'a>(&'a self, entry: Entry<'a>) -> Option<(Sections<'a>, usize)> {
        let (path, text) = self.entry_document(entry)?;
        let sections = Sections::new(path, text);
        match sections.entry(entry.key()) {
            Lookup::Found(index) => Some((sections, index)),
            _ => None,
        }
    }
    /// The source a search result was indexed from, when it can be located.
    /// `cache` keeps parsed documents for one command.
    pub(super) fn search_span<'a>(
        &'a self,
        entry: Entry<'a>,
        cache: &mut SpanCache<'a>,
    ) -> Option<Span<'a>> {
        let path = entry.source_path();
        let text = self.sources.get(path.as_ref())?.as_str();
        if !self.is_knowledge(&path) {
            return None;
        }
        let ranges = match entry {
            Entry::Document { path, content } => whole(self.searchable_text(path, content)?.len()),
            Entry::Recipe(_) | Entry::Session(_) => whole(text.len()),
            Entry::Node(_) | Entry::Observation(_) | Entry::Taste(_) => {
                let document = cache
                    .yaml
                    .entry(path.to_string())
                    .or_insert_with(|| YamlDocument::parse(text).ok())
                    .as_ref()?;
                yaml_ranges(&document.root, entry.key())?
            }
            _ => {
                let (document, text) = self.entry_document(entry)?;
                let sections = cache
                    .sections
                    .entry(document)
                    .or_insert_with(|| Sections::new(document, text));
                match sections.entry(entry.key()) {
                    Lookup::Found(index) => vec![sections.range(index)],
                    _ => return None,
                }
            }
        };
        Some(Span { path, text, ranges })
    }
}

/// Parsed sources reused across the results of one `find`.
#[derive(Default)]
pub struct SpanCache<'a> {
    yaml: BTreeMap<String, Option<YamlDocument>>,
    sections: BTreeMap<&'a str, Sections<'a>>,
}

fn identity(node: &YamlNode) -> Option<&str> {
    let YamlKind::Mapping(entries) = &node.kind else {
        return None;
    };
    entries
        .iter()
        .find(|(key, _)| key.scalar() == Some("id"))
        .and_then(|(_, value)| value.scalar())
        .map(str::trim)
}
fn children(node: &YamlNode) -> Box<dyn Iterator<Item = &YamlNode> + '_> {
    match &node.kind {
        YamlKind::Mapping(entries) => Box::new(entries.iter().map(|(_, value)| value)),
        YamlKind::Sequence(items) => Box::new(items.iter()),
        _ => Box::new(std::iter::empty()),
    }
}
/// The one mapping whose `id` is `key`, minus nested mappings with their own
/// `id` (child nodes), as ascending byte ranges.
fn yaml_ranges(root: &YamlNode, key: &str) -> Option<Vec<Range<usize>>> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if identity(node) == Some(key) {
            found.push(node);
        }
        pending.extend(children(node));
    }
    let [node] = found.as_slice() else {
        return None;
    };
    let mut nested = Vec::new();
    let mut pending: Vec<&YamlNode> = children(node).collect();
    while let Some(child) = pending.pop() {
        if identity(child).is_some() {
            nested.push(child.start..child.end);
        } else {
            pending.extend(children(child));
        }
    }
    nested.sort_by_key(|range| range.start);
    let mut ranges = Vec::new();
    let mut start = node.start;
    for range in nested {
        if range.start > start {
            ranges.push(start..range.start);
        }
        start = start.max(range.end);
    }
    if start < node.end {
        ranges.push(start..node.end);
    }
    Some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_ranges_exclude_nested_entries() {
        let text = "tree:\n  - id: N01\n    title: alpha\n    children:\n      - id: N02\n        title: beta\n    after: gamma\n";
        let document = YamlDocument::parse(text).unwrap();
        let ranges = yaml_ranges(&document.root, "N01").unwrap();
        let covered: String = ranges.iter().map(|range| &text[range.clone()]).collect();
        assert!(covered.contains("alpha") && covered.contains("gamma"));
        assert!(!covered.contains("beta"));
        let child = yaml_ranges(&document.root, "N02").unwrap();
        assert_eq!(child.len(), 1);
        assert!(text[child[0].clone()].contains("beta"));
        assert!(yaml_ranges(&document.root, "N03").is_none());
    }
}
