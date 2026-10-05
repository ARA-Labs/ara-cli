//! Source-document reads: whole documents, heading selections, canonical
//! addresses and legacy flattened locators.
use super::address::{self, Address};
use super::boundary::{invalid_document, knowledge_document};
use super::candidates::{self, Candidates, Miss};
use super::headings::{Lookup, Sections, source_row};
use super::{Artifact, Entry, diagnostics};
use crate::output::AgentError;
use ara_core::write::EntrySelector;
use serde_json::{Value, json};
use std::path::Path;

/// Native identity records that can redirect an archived selector.
const IDENTITY_RECORDS: [&str; 3] = [
    "trace/logic_mutations.yaml",
    "trace/merge_log.yaml",
    "trace/aliases.yaml",
];

pub(super) enum Selected<'a> {
    Entry(Entry<'a>),
    Source(Value),
}

fn valid_document_path(document: &str) -> bool {
    !document.contains('\\')&&Path::new(document).components().all(|component|matches!(component,std::path::Component::Normal(name) if name!=".git"&&name!=".ara"))
}
fn label(headings: &[String]) -> &str {
    headings.last().map_or("", String::as_str)
}

impl Artifact {
    /// The native load reads `trace/aliases.yaml` (and `merge_log.yaml`
    /// without a mutation ledger) only through the identity snapshot, so
    /// presence is checked on disk; any entry there routes through it.
    pub(super) fn has_identity_records(&self) -> bool {
        IDENTITY_RECORDS
            .iter()
            .any(|path| std::fs::symlink_metadata(self.root.join(path)).is_ok())
    }
    fn document_text(&self, document: &str) -> Result<&str, AgentError> {
        if !valid_document_path(document)
            || !self.is_knowledge(document)
            || ara_core::file_access_location(document)
        {
            return Err(invalid_document());
        }
        self.sources
            .get(document)
            .map(String::as_str)
            .ok_or_else(|| {
                candidates::unknown(
                    document,
                    Candidates::ranked(
                        document,
                        self.sources
                            .keys()
                            .filter(|path| self.is_knowledge(path))
                            .map(|path| (address::document(path), path.as_str())),
                    ),
                )
            })
    }
    /// The document `ls <path>` lists: a path, decoded once when it is
    /// percent-encoded, inside the knowledge boundary.
    pub(super) fn list_document(&self, input: &str) -> Result<(String, &str), AgentError> {
        let path = match address::parse(input) {
            Some(Err(error)) => return Err(error),
            Some(Ok(Address::Document { path })) if self.sources.contains_key(&path) => path,
            _ => input.to_owned(),
        };
        let text = self.document_text(&path)?;
        Ok((path, text))
    }
    /// Sections of a knowledge document that know its loaded entries, so
    /// candidates cite entry sections by `path#ID`.
    pub(super) fn sections<'a>(&'a self, path: &'a str, text: &'a str) -> Sections<'a> {
        Sections::new(path, text).with_entries(self.entry_keys(path))
    }
    /// Keys of the entries a document holds, whole-document entries excluded.
    pub(super) fn entry_keys<'a>(&'a self, path: &str) -> Vec<&'a str> {
        self.entries()
            .into_iter()
            .filter(|entry| {
                !matches!(entry, Entry::Document { .. } | Entry::Recipe(_))
                    && entry.source_matches(path)
            })
            .map(Entry::key)
            .collect()
    }
    fn markdown_sections<'a>(&'a self, path: &'a str) -> Option<Sections<'a>> {
        (path.ends_with(".md") && valid_document_path(path) && self.is_knowledge(path))
            .then(|| self.sources.get(path))
            .flatten()
            .map(|text| self.sections(path, text))
    }
    /// A read-facing miss for an entry selector, with canonical candidates
    /// from loaded knowledge: the named document's headings for `path#...`,
    /// entry addresses otherwise.
    pub(super) fn miss(&self, id: &str, miss: Miss) -> AgentError {
        if let Some((path, display)) = id.split_once('#')
            && let Some(sections) = self.markdown_sections(path)
        {
            let ranked = || sections.ranked(display.rsplit('/').next().unwrap_or(display));
            return match (miss, sections.display(display)) {
                (_, Lookup::Ambiguous(found)) => {
                    candidates::ambiguous(id, sections.matched(&found))
                }
                (Miss::Ambiguous, _) => candidates::ambiguous(id, ranked()),
                (Miss::Unknown, _) => candidates::unknown(id, ranked()),
            };
        }
        let entries = self.entries();
        let ranked = Candidates::ranked(
            id,
            entries.iter().map(|entry| {
                let address = match entry {
                    Entry::Document { path, .. } => address::document(path),
                    Entry::Recipe(recipe) => {
                        address::document(&format!("logic/solution/{}.md", recipe.name))
                    }
                    _ => entry.key().to_owned(),
                };
                (address, entry.key())
            }),
        );
        match miss {
            Miss::Unknown => candidates::unknown(id, ranked),
            Miss::Ambiguous => candidates::ambiguous(id, ranked),
        }
    }
    fn is_recipe_document(&self, path: &str) -> bool {
        self.manifest
            .recipes
            .iter()
            .any(|recipe| format!("logic/solution/{}.md", recipe.name) == path)
    }
}

/// Resolve one positional `show` selector: a canonical address, an entry ID,
/// or a legacy `path#A/B` locator that identifies exactly one section.
pub(super) fn select<'a>(
    artifact: &'a Artifact,
    id: &str,
    full: bool,
) -> Result<Selected<'a>, AgentError> {
    match address::parse(id) {
        Some(Err(error)) => return Err(error),
        Some(Ok(Address::Heading {
            path,
            heading,
            occurrence,
        })) => {
            let sections = artifact.sections(&path, artifact.document_text(&path)?);
            let lookup = sections.canonical(&heading, occurrence);
            return sections
                .row(lookup, id, label(&heading), &heading, full)
                .map(Selected::Source);
        }
        Some(Ok(Address::Document { path })) => {
            if !valid_document_path(&path) || ara_core::file_access_location(&path) {
                return Err(invalid_document());
            }
            // Decoded once; a raw path spelling stays readable when it names
            // no decoded document. A valid path naming no document may still
            // be an entry key, such as a concept spelled with `/`.
            for path in [path.as_str(), id] {
                if artifact.sources.contains_key(path) && !artifact.is_recipe_document(path) {
                    return document_row(artifact, path, &[], false, full).map(Selected::Source);
                }
            }
        }
        None => {}
    }
    let miss = match artifact.lookup_entry(id)? {
        Ok(entry) => return Ok(Selected::Entry(entry)),
        Err(miss) => miss,
    };
    // Only a plain miss falls back to a legacy locator; an ambiguity the
    // identity records reported is never overridden by a current match.
    if let Miss::Unknown = miss
        && let Some((path, display)) = id.split_once('#')
        && let Some(sections) = artifact.markdown_sections(path)
        && let Lookup::Found(index) = sections.display(display)
    {
        let heading = sections.path(index);
        return Ok(Selected::Source(
            sections.section_row(index, &heading, full),
        ));
    }
    Err(artifact.miss(id, miss))
}

pub(super) fn show_source(
    root: &Path,
    document: &str,
    headings: &[String],
    annotate: bool,
) -> Result<Value, AgentError> {
    if !valid_document_path(document) {
        return Err(invalid_document());
    }
    if !knowledge_document(document) {
        let paper_path = ara_core::write::transaction::checked_destination(root, "PAPER.md")
            .map_err(crate::write::convert_error)?;
        let paper = std::fs::read_to_string(paper_path)
            .map_err(|error| AgentError::io(error.to_string()))?;
        if !ara_core::knowledge_paths(&paper)
            .map_err(|message| AgentError::semantic("invalid_knowledge_registry", message))?
            .iter()
            .any(|path| path == document)
        {
            return Err(invalid_document());
        }
    }
    let path = ara_core::write::transaction::checked_destination(root, document)
        .map_err(crate::write::convert_error)?;
    let text = std::fs::read_to_string(path).map_err(|error| AgentError::io(error.to_string()))?;
    let mut row = if headings.is_empty() {
        source_row(document, &text, true)
    } else {
        let sections = Sections::new(document, &text);
        let lookup = match sections.exact(headings) {
            Lookup::Missing => sections.tolerant(headings),
            lookup => lookup,
        };
        sections.row(
            lookup,
            &headings.join(" / "),
            label(headings),
            headings,
            true,
        )?
    };
    if annotate {
        let paper = ara_core::write::transaction::checked_destination(root, "PAPER.md")
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok());
        let replaceable = super::display::replaceable(document, paper.as_deref());
        super::display::annotate(&mut row, &text, Vec::new(), replaceable);
    }
    Ok(json!({"format":"ara.show/v1","entries":[row],"artifact_validation":"not_run"}))
}

pub(super) fn show_document(
    artifact: &Artifact,
    document: &str,
    headings: &[String],
    source: bool,
    full: bool,
) -> Result<Value, AgentError> {
    let row = document_row(artifact, document, headings, source, full)?;
    Ok(json!({"format":"ara.show/v1","entries":[row],"diagnostics":diagnostics(&artifact.report)}))
}

/// Exact tiers, then an archived identity redirect, then tolerant tiers.
fn document_row(
    artifact: &Artifact,
    document: &str,
    headings: &[String],
    source: bool,
    full: bool,
) -> Result<Value, AgentError> {
    let text = artifact.document_text(document)?;
    let full = source || full;
    if headings.is_empty() {
        return Ok(source_row(document, text, full));
    }
    let sections = artifact.sections(document, text);
    let id = headings.join(" / ");
    match sections.exact(headings) {
        Lookup::Missing => {}
        lookup => return sections.row(lookup, &id, label(headings), headings, full),
    }
    if !source
        && artifact.has_identity_records()
        && let Some(row) = archived_row(artifact, &sections, document, headings, full)?
    {
        return Ok(row);
    }
    sections.row(
        sections.tolerant(headings),
        &id,
        label(headings),
        headings,
        full,
    )
}

/// Follow a recorded rename or merge redirect for an exact heading vector.
fn archived_row(
    artifact: &Artifact,
    sections: &Sections<'_>,
    document: &str,
    headings: &[String],
    full: bool,
) -> Result<Option<Value>, AgentError> {
    let selector = EntrySelector::Document {
        document: document.into(),
        heading: headings.to_vec(),
        entry: None,
    };
    let snapshot = artifact.snapshot()?;
    let resolved = match ara_core::merge::resolve_selector(snapshot, &selector) {
        Ok(resolved) if resolved != selector => resolved,
        Ok(_) => return Ok(None),
        Err(error) => {
            return match candidates::classify(error, snapshot)? {
                Miss::Unknown => Ok(None),
                Miss::Ambiguous => Err(candidates::ambiguous(
                    &headings.join(" / "),
                    sections.ranked(label(headings)),
                )),
            };
        }
    };
    let (current, heading) = match resolved {
        EntrySelector::Document {
            document,
            heading,
            entry,
        } => {
            let heading = if heading.is_empty() {
                entry.into_iter().collect()
            } else {
                heading
            };
            (document, heading)
        }
        EntrySelector::Id { id } if id.starts_with('C') => ("logic/claims.md".into(), vec![id]),
        EntrySelector::Id { id } if id.starts_with('H') => {
            ("logic/solution/heuristics.md".into(), vec![id])
        }
        EntrySelector::Id { .. } => return Ok(None),
    };
    let redirected = artifact.sections(&current, artifact.document_text(&current)?);
    let lookup = redirected.exact(&heading);
    redirected
        .row(
            lookup,
            &heading.join(" / "),
            label(&heading),
            &heading,
            full,
        )
        .map(Some)
}
