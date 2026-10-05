//! Data that only brief text needs, computed only when `--json` is absent:
//! the write selector a source digest guards, the native section of a
//! heading-backed entry and per-document summaries. JSON output never
//! carries them. Also the heading rows `ls <path>` lists in both modes.
use super::address;
use super::headings::{Sections, source_row};
use super::hits::LineIndex;
use super::{Artifact, Entry};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Whether `document.replace` can target `path`
/// ([`ara_core::write::source::replaceable`]); an invalid registry cannot.
pub fn replaceable(path: &str, paper: Option<&str>) -> bool {
    ara_core::write::source::replaceable(path, paper).unwrap_or(false)
}

/// Add `display` to a source row of `text`: `scope`, the `document.replace`
/// `selector` whose digest equals the row's (the whole document, or the
/// section's full heading vector), and `cited`, the short `path#ID` form of a
/// section that heads an entry (`keys` are the document's entry keys). When
/// no write can use the selection, `selector` is null and `no_selector`
/// says why.
pub fn annotate(row: &mut Value, text: &str, keys: Vec<&str>, replaceable: bool) {
    let document = row["document"].as_str().unwrap_or("").to_owned();
    let heading: Vec<String> = row["heading_path"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|segment| segment.as_str().map(str::to_owned))
        .collect();
    let mut blocked =
        (!replaceable).then_some("read-only document; document.replace cannot target it");
    let mut display = if heading.is_empty() {
        json!({"scope":"whole_document"})
    } else {
        let sections = Sections::new(&document, text).with_entries(keys);
        let index = sections.index_of(row["address"].as_str().unwrap_or(""));
        let mut display = json!({"scope": "heading_body"});
        if let Some(index) = index {
            display["cited"] = json!(sections.cited(index));
        }
        if ara_core::markdown::recovered_fence(&document, text) {
            blocked = blocked.or(Some(
                "an unclosed leading `---` hides this heading from writes (write.frontmatter); a whole-document replace that removes it is accepted",
            ));
        }
        if !index.is_some_and(|index| sections.writable(index)) {
            blocked = blocked.or(Some(
                "the heading vector repeats; no write selector names one occurrence",
            ));
        }
        display
    };
    match blocked {
        Some(reason) => {
            display["selector"] = Value::Null;
            display["no_selector"] = json!(reason);
        }
        None => display["selector"] = json!({"document": document, "heading": heading}),
    }
    row.as_object_mut()
        .expect("source row object")
        .insert("display".into(), display);
}

impl Artifact {
    /// Annotate every source row of a `show` value.
    pub(super) fn annotate_rows(&self, value: &mut Value) {
        let paper = self.sources.get("PAPER.md").map(String::as_str);
        for row in value["entries"].as_array_mut().into_iter().flatten() {
            if row["kind"] == "source_document"
                && let Some(document) = row["document"].as_str().map(str::to_owned)
                && let Some(text) = self.sources.get(&document)
            {
                let keys = self.entry_keys(&document);
                annotate(row, text, keys, replaceable(&document, paper));
            }
        }
    }
    /// The exact native source of an entry that has one: its section body,
    /// or its whole document for a recipe or document entry.
    pub(super) fn native_row(&self, entry: Entry<'_>) -> Option<Value> {
        let mut row = match entry {
            Entry::Recipe(_) | Entry::Document { .. } => {
                let path = entry.source_path();
                let text = self.sources.get(path.as_ref())?;
                source_row(&path, text, true)
            }
            _ => {
                let (sections, index) = self.entry_section(entry)?;
                sections.section_row(index, &sections.path(index), true)
            }
        };
        let object = row.as_object_mut().expect("source row object");
        object.insert(
            "entry".into(),
            json!({"id": entry.key(), "kind": entry.kind()}),
        );
        Some(row)
    }
    /// Add the one-based source `line` of each `refs` row.
    pub(super) fn annotate_lines(&self, value: &mut Value) {
        let mut indexes = BTreeMap::new();
        for key in ["structured", "prose"] {
            for row in value[key].as_array_mut().into_iter().flatten() {
                let (Some(text), Some(start)) = (
                    row["source"]
                        .as_str()
                        .and_then(|path| self.sources.get(path)),
                    row["range"]["start"].as_u64(),
                ) else {
                    continue;
                };
                let index = indexes
                    .entry(row["source"].as_str().unwrap_or("").to_owned())
                    .or_insert_with(|| LineIndex::new(text));
                row["line"] = json!(index.line(start as usize) + 1);
            }
        }
    }
    /// One summary per knowledge document: entry counts by kind, line count,
    /// and heading count for Markdown with no typed entries.
    pub(super) fn document_summaries(&self) -> Vec<Value> {
        let entries = super::entries(&self.manifest);
        self.sources
            .iter()
            .filter(|(path, _)| self.is_knowledge(path))
            .map(|(path, text)| {
                let mut counts = BTreeMap::<&str, usize>::new();
                for entry in entries.iter().filter(|entry| entry.source_matches(path)) {
                    *counts.entry(entry.kind()).or_default() += 1;
                }
                let mut summary = json!({
                    "path": path,
                    "address": address::document(path),
                    "counts": counts,
                    "lines": text.lines().count(),
                });
                if counts.is_empty() && path.ends_with(".md") {
                    summary["headings"] =
                        json!(ara_core::markdown::document_headings(path, text).len());
                }
                summary
            })
            .collect()
    }
}

/// Heading rows of one Markdown document, for `ls <path>` without entries.
pub fn heading_rows(document: &str, text: &str) -> Vec<Value> {
    let sections = Sections::new(document, text);
    (0..sections.len())
        .map(|index| {
            let path = sections.path(index);
            json!({
                "kind": "heading",
                "address": sections.address(index),
                "heading_path": path,
                "title": path.last(),
                "source": document,
            })
        })
        .collect()
}
