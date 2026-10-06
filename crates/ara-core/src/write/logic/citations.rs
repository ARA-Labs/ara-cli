//! Typed inventory of the current citations of one logic entry (plan 19 C1).
//!
//! A restructure (rename, merge, redirecting removal, split) needs to know
//! which current mutable fields cite the restructured entry. Token-shaped
//! text alone is not enough: Proof and Sources mix prose with references,
//! headings may contain `/`, `#` or delimiters, and the same leaf heading can
//! exist under different parents. This module tokenizes only the accepted
//! reference fields of native Markdown entries, resolves every token through
//! the current headings and authenticated claim redirects, and classifies it
//! against the subject. Everything else that mentions the subject is reported
//! as a located possible mention and is never edited.
use super::tokens::{
    LineIndex, contains, id_tokens, item_range, occurrences, path_byte, source_offset, terminated,
};
use super::{heading_id, native_document_prefix, native_id_document};
use crate::markdown::{self, MarkdownField};
use crate::write::{
    EntrySelector, WorkingArtifact, WriteError,
    citation_rules::{self, is_protected, protected_ranges},
    fields,
    source::OwnedMarkdownHeading,
};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

/// How a citation spells its target. A rewrite keeps the spelling style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Spelling {
    /// A bare native ID (`C01`).
    Id,
    /// `document:ID` or `document#ID`.
    QualifiedId { document: String, separator: char },
    /// `document#path` or `document:path` with the last `segments` headings.
    QualifiedPath {
        document: String,
        separator: char,
        segments: usize,
    },
    /// A bare concept name in a `Related` list (`segments` joined headings).
    Name { segments: usize },
}

/// What one token resolves to in the current source.
#[derive(Debug, Clone)]
enum Target {
    Entry {
        document: String,
        index: usize,
    },
    Ambiguous {
        document: String,
        indexes: Vec<usize>,
    },
}

#[derive(Debug, Clone)]
struct Token {
    range: Range<usize>,
    spelling: Spelling,
    target: Target,
}

/// The restructured entry at its pre-operation location.
#[derive(Debug, Clone)]
pub(super) struct Subject {
    pub document: String,
    pub path: Vec<String>,
    pub range: Range<usize>,
    /// The subject's own canonical ID still resolves afterwards (title-only
    /// rename), so ID spellings of it are not rewritten.
    pub root_id_retained: bool,
    /// Numbered descendants keep their IDs (rename, not removal).
    pub descendant_ids_retained: bool,
}

/// One typed citation of the subject inside a citing field.
#[derive(Debug, Clone)]
pub(super) struct Span {
    /// Byte range in the decoded field value.
    pub range: Range<usize>,
    pub spelling: Spelling,
    /// Heading path below the subject root (empty for the root itself).
    pub suffix: Vec<String>,
    /// Absolute source byte range of the token.
    pub source: Range<usize>,
}

/// One current mutable field that cites the subject through typed tokens.
#[derive(Debug, Clone)]
pub(super) struct Citation {
    pub document: String,
    /// Full literal heading vector of the citing entry.
    pub path: Vec<String>,
    /// Source start of the citing heading in this inventory's snapshot.
    pub start: usize,
    pub heading: String,
    /// The field's source label spelling.
    pub field: String,
    /// The exact decoded field value.
    pub before: String,
    /// First source line of the field.
    pub line: usize,
    pub spans: Vec<Span>,
    /// C1 may rewrite this field; read-only reference fields are listed for
    /// `refs` and reported to C1 as `read_only_field` mentions.
    pub rewritable: bool,
}

impl Citation {
    /// The exact selector of the citing entry: native claim/heuristic IDs,
    /// otherwise the complete literal heading vector.
    pub fn selector(&self) -> EntrySelector {
        let id = heading_id(&self.heading);
        if self.document == "logic/claims.md" && fields::typed_id(id, "C")
            || self.document == "logic/solution/heuristics.md" && fields::typed_id(id, "H")
        {
            EntrySelector::Id { id: id.into() }
        } else {
            EntrySelector::Document {
                document: self.document.clone(),
                heading: self.path.clone(),
                entry: None,
            }
        }
    }
    pub fn location(&self) -> Value {
        json!({"document":self.document,"heading":self.path,"field":self.field,"line":self.line,"before":self.before})
    }
    /// Replace every span with `spelling(span)`, keeping all other bytes.
    /// Returns the new value and the ranges of the new spellings in it.
    pub fn rewrite(
        &self,
        mut spelling: impl FnMut(&Span) -> Result<String, WriteError>,
    ) -> Result<(String, Vec<Range<usize>>), WriteError> {
        let mut spans: Vec<&Span> = self.spans.iter().collect();
        spans.sort_by_key(|span| span.range.start);
        let mut after = String::with_capacity(self.before.len());
        let mut produced = Vec::with_capacity(spans.len());
        let mut cursor = 0;
        for span in spans {
            after.push_str(&self.before[cursor..span.range.start]);
            let replacement = spelling(span)?;
            produced.push(after.len()..after.len() + replacement.len());
            after.push_str(&replacement);
            cursor = span.range.end;
        }
        after.push_str(&self.before[cursor..]);
        Ok((after, produced))
    }
}

/// New spellings written into one field, which later scans of the same
/// operation must not report as mentions of the old identity.
#[derive(Debug, Clone)]
pub(super) struct Produced {
    pub document: String,
    pub path: Vec<String>,
    pub field: String,
    pub ranges: Vec<Range<usize>>,
}

/// A possible mention of the subject that C1 never edits.
#[derive(Debug, Clone)]
pub(super) struct Mention {
    pub document: String,
    pub heading: Vec<String>,
    pub field: Option<String>,
    pub line: usize,
    pub literal: String,
    /// `prose`, `heading`, `unknown_field`, `untyped_field`, `read_only_field`,
    /// `protected` (quoted, backticked or commented), `unparsed` or `ambiguous`.
    pub reason: &'static str,
}
impl Mention {
    /// Heading text is an identity definition, not an inbound reference; the
    /// existing dangling-reference guard never counted it either.
    pub fn blocking(&self) -> bool {
        self.reason != "heading"
    }
    pub fn to_json(&self) -> Value {
        json!({"document":self.document,"heading":self.heading,"field":self.field,"line":self.line,"literal":self.literal,"reason":self.reason})
    }
}

#[derive(Debug, Default)]
pub(super) struct Inventory {
    pub citations: Vec<Citation>,
    pub mentions: Vec<Mention>,
}
impl Inventory {
    /// Only the citations C1 may rewrite.
    pub fn rewritable_only(mut self) -> Self {
        self.citations.retain(|citation| citation.rewritable);
        self
    }
}

/// Heading indexes of one snapshot, shared by every token of an inventory.
pub(super) struct Resolver<'w> {
    pub working: &'w WorkingArtifact,
    headings: RefCell<BTreeMap<String, Option<Arc<Vec<OwnedMarkdownHeading>>>>>,
    redirects: BTreeMap<String, String>,
}

impl<'w> Resolver<'w> {
    pub fn new(working: &'w WorkingArtifact) -> Result<Self, WriteError> {
        Ok(Self {
            working,
            headings: RefCell::new(BTreeMap::new()),
            redirects: super::claim_redirects_from_source(working)?,
        })
    }
    /// Live headings only, without claim redirects. A planner uses it while
    /// its own new mapping rows still await their owning revision rows.
    pub fn live(working: &'w WorkingArtifact) -> Self {
        Self {
            working,
            headings: RefCell::new(BTreeMap::new()),
            redirects: BTreeMap::new(),
        }
    }
    /// Current headings of a registered native document, if it exists.
    pub fn headings(
        &self,
        document: &str,
    ) -> Result<Option<Arc<Vec<OwnedMarkdownHeading>>>, WriteError> {
        if let Some(cached) = self.headings.borrow().get(document) {
            return Ok(cached.clone());
        }
        let value =
            if self.working.exists(document) && self.working.is_allowed_document(document)? {
                Some(self.working.headings(document)?)
            } else {
                None
            };
        self.headings
            .borrow_mut()
            .insert(document.to_owned(), value.clone());
        Ok(value)
    }
    /// Retired claim IDs (authenticated renames) and their live terminals.
    pub fn redirects(&self) -> &BTreeMap<String, String> {
        &self.redirects
    }
    fn resolve_id(&self, id: &str) -> Result<Option<Target>, WriteError> {
        let Some(document) = native_id_document(id) else {
            return Ok(None);
        };
        let Some(headings) = self.headings(document)? else {
            return Ok(None);
        };
        let find = |id: &str| -> Vec<usize> {
            headings
                .iter()
                .enumerate()
                .filter(|(_, h)| heading_id(&h.heading) == id)
                .map(|(i, _)| i)
                .collect()
        };
        let mut indexes = find(id);
        if indexes.is_empty()
            && let Some(live) = self.redirects.get(id)
        {
            indexes = find(live);
        }
        Ok(target(document, indexes))
    }
    /// A field value split into literal text and unprotected typed claim
    /// citations (resolved through redirects).
    pub fn claim_parts(
        &self,
        canonical_field: &str,
        value: &str,
    ) -> Result<Vec<super::row_mapping::Part>, WriteError> {
        use super::row_mapping::Part;
        let protected = protected_ranges(value);
        let mut claims = Vec::new();
        for token in self.tokens(canonical_field, value)? {
            if is_protected(&protected, &token.range) {
                continue;
            }
            if let Target::Entry { document, index } = &token.target
                && document == "logic/claims.md"
                && let Some(headings) = self.headings(document)?
                && let Some(heading) = headings.get(*index)
            {
                let id = heading_id(&heading.heading);
                if fields::typed_id(id, "C") {
                    claims.push((token.range, id.to_owned()));
                }
            }
        }
        claims.sort_by_key(|(range, _)| range.start);
        let mut parts = Vec::with_capacity(claims.len() * 2 + 1);
        let mut cursor = 0;
        for (range, id) in claims {
            if cursor < range.start {
                parts.push(Part::Lit(value[cursor..range.start].to_owned()));
            }
            parts.push(Part::Claim {
                text: value[range.clone()].to_owned(),
                id,
            });
            cursor = range.end;
        }
        if cursor < value.len() {
            parts.push(Part::Lit(value[cursor..].to_owned()));
        }
        Ok(parts)
    }
    fn tokens(&self, canonical_field: &str, value: &str) -> Result<Vec<Token>, WriteError> {
        let mut tokens = self.qualified_tokens(value)?;
        if canonical_field == "related" {
            for token in self.name_tokens(value)? {
                if !overlaps(&tokens, &token.range) {
                    tokens.push(token);
                }
            }
        }
        for range in id_tokens(value) {
            if overlaps(&tokens, &range) {
                continue;
            }
            if let Some(target) = self.resolve_id(&value[range.clone()])? {
                tokens.push(Token {
                    range,
                    spelling: Spelling::Id,
                    target,
                });
            }
        }
        Ok(tokens)
    }
    /// `document:identity` / `document#identity` locators of registered
    /// documents. The longest spelling that ends at a delimiter wins; equal
    /// spellings of several headings are ambiguous, never guessed.
    fn qualified_tokens(&self, value: &str) -> Result<Vec<Token>, WriteError> {
        let bytes = value.as_bytes();
        let mut tokens = Vec::new();
        let mut search = 0;
        while let Some(found) = value.get(search..).and_then(|rest| rest.find(".md")) {
            let dot = search + found;
            search = dot + 3;
            let separator = match bytes.get(dot + 3) {
                Some(b':') => ':',
                Some(b'#') => '#',
                _ => continue,
            };
            let mut start = dot;
            while start > 0 && path_byte(bytes[start - 1]) {
                start -= 1;
            }
            if start == dot
                || start > 0
                    && (crate::query::local_boundary_block(bytes[start - 1])
                        || value[..start]
                            .chars()
                            .next_back()
                            .is_some_and(char::is_alphanumeric))
            {
                continue;
            }
            let document = &value[start..dot + 3];
            let Some(headings) = self.headings(document)? else {
                continue;
            };
            let rest = &value[dot + 4..];
            // (length, heading index, spelling)
            let mut candidates: Vec<(usize, usize, Spelling)> = Vec::new();
            let numbered = native_document_prefix(document);
            for (index, heading) in headings.iter().enumerate() {
                let id = heading_id(&heading.heading);
                if numbered.is_some_and(|prefix| fields::typed_id(id, prefix))
                    && terminated(rest, id)
                {
                    candidates.push((
                        id.len(),
                        index,
                        Spelling::QualifiedId {
                            document: document.into(),
                            separator,
                        },
                    ));
                }
                for segments in 1..=heading.path.len() {
                    let spelled = heading.path[heading.path.len() - segments..].join("/");
                    if terminated(rest, &spelled) {
                        candidates.push((
                            spelled.len(),
                            index,
                            Spelling::QualifiedPath {
                                document: document.into(),
                                separator,
                                segments,
                            },
                        ));
                    }
                }
            }
            if document == "logic/claims.md" {
                for (alias, live) in &self.redirects {
                    if terminated(rest, alias) {
                        for (index, heading) in headings.iter().enumerate() {
                            if heading_id(&heading.heading) == live {
                                candidates.push((
                                    alias.len(),
                                    index,
                                    Spelling::QualifiedId {
                                        document: document.into(),
                                        separator,
                                    },
                                ));
                            }
                        }
                    }
                }
            }
            let Some(longest) = candidates.iter().map(|(length, ..)| *length).max() else {
                continue;
            };
            candidates.retain(|(length, ..)| *length == longest);
            let mut indexes: Vec<usize> = candidates.iter().map(|(_, index, _)| *index).collect();
            indexes.sort_unstable();
            indexes.dedup();
            // Equal-length candidates naming one heading differ only in
            // spelling kind (an ID equal to a one-segment path is the same
            // text); any of them respells the same bytes. Several headings
            // make the token ambiguous below, and its spelling is unused.
            let spelling = candidates.swap_remove(0).2;
            let end = dot + 4 + longest;
            if let Some(target) = target(document, indexes) {
                tokens.push(Token {
                    range: start..end,
                    spelling,
                    target,
                });
            }
            search = end;
        }
        Ok(tokens)
    }
    /// Comma-separated `Related` items naming concepts by heading.
    fn name_tokens(&self, value: &str) -> Result<Vec<Token>, WriteError> {
        let Some(headings) = self.headings("logic/concepts.md")? else {
            return Ok(Vec::new());
        };
        let mut tokens = Vec::new();
        let mut offset = 0;
        for piece in value.split(',') {
            let range = item_range(piece, offset);
            offset += piece.len() + 1;
            let Some(range) = range else {
                continue;
            };
            let item = &value[range.clone()];
            let candidates: Vec<(usize, usize)> = headings
                .iter()
                .enumerate()
                .filter_map(|(index, heading)| {
                    citation_rules::concept_name_match(&heading.heading, &heading.path, item)
                        .map(|segments| (index, segments))
                })
                .collect();
            let Some(&(_, segments)) = candidates.first() else {
                continue;
            };
            let indexes = candidates.iter().map(|(index, _)| *index).collect();
            if let Some(target) = target("logic/concepts.md", indexes) {
                tokens.push(Token {
                    range,
                    spelling: Spelling::Name { segments },
                    target,
                });
            }
        }
        Ok(tokens)
    }
    fn heading(&self, document: &str, index: usize) -> Result<OwnedMarkdownHeading, WriteError> {
        self.headings(document)?
            .and_then(|headings| headings.get(index).cloned())
            .ok_or_else(|| WriteError::semantic("write.reference", "citation heading vanished"))
    }
    /// Classify one field value against the subject.
    fn classify(
        &self,
        subject: &Subject,
        canonical_field: &str,
        value: &str,
    ) -> Result<Classified, WriteError> {
        let mut result = Classified::default();
        let protected = protected_ranges(value);
        for token in self.tokens(canonical_field, value)? {
            if is_protected(&protected, &token.range) {
                result.protected.push(token.range);
                continue;
            }
            match &token.target {
                Target::Entry { document, index } => {
                    let heading = self.heading(document, *index)?;
                    let inside = *document == subject.document
                        && subject.range.contains(&heading.range.start);
                    let Some(suffix) = heading
                        .path
                        .strip_prefix(subject.path.as_slice())
                        .filter(|_| inside)
                    else {
                        result.covered.push(token.range);
                        continue;
                    };
                    let by_id =
                        matches!(token.spelling, Spelling::Id | Spelling::QualifiedId { .. });
                    let retained = if suffix.is_empty() {
                        subject.root_id_retained
                    } else {
                        subject.descendant_ids_retained
                    };
                    if by_id && retained {
                        result.covered.push(token.range);
                    } else {
                        result.spans.push(Span {
                            range: token.range,
                            spelling: token.spelling,
                            suffix: suffix.to_vec(),
                            source: 0..0,
                        });
                    }
                }
                Target::Ambiguous { document, indexes } => {
                    let mut inside = false;
                    if *document == subject.document {
                        for index in indexes {
                            inside |= subject
                                .range
                                .contains(&self.heading(document, *index)?.range.start);
                        }
                    }
                    if inside {
                        result.ambiguous.push(token.range);
                    } else {
                        result.covered.push(token.range);
                    }
                }
            }
        }
        Ok(result)
    }

    /// Every typed citation of `subject` in accepted reference fields of the
    /// current mutable Markdown, plus located possible mentions of `names`
    /// anywhere else in registered knowledge documents. `exclude` skips a
    /// source range of the subject's document (a removed or retained entry).
    pub fn inventory(
        &self,
        subject: &Subject,
        names: &[String],
        exclude: Option<Range<usize>>,
        produced: &[Produced],
    ) -> Result<Inventory, WriteError> {
        let mut inventory = Inventory::default();
        for document in self.working.paths() {
            if !document.ends_with(".md") || !self.working.is_allowed_document(&document)? {
                continue;
            }
            let Ok(text) = self.working.text(&document) else {
                continue;
            };
            let excluded = |start: usize| {
                document == subject.document
                    && exclude.as_ref().is_some_and(|range| range.contains(&start))
            };
            let headings = self.working.headings(&document)?;
            let lines = LineIndex::new(text);
            let mut scan = Scan {
                inventory: &mut inventory,
                names,
                document: &document,
                text,
                lines: &lines,
            };
            let first = headings.first().map_or(text.len(), |h| h.range.start);
            scan.prose(0..first, &[], "prose", &excluded);
            for (index, heading) in headings.iter().enumerate() {
                let own =
                    document == subject.document && heading.range.start == subject.range.start;
                if !own {
                    scan.prose(
                        heading.range.start..heading.body_range.start,
                        &heading.path,
                        "heading",
                        &excluded,
                    );
                }
                let end = headings
                    .get(index + 1)
                    .map_or(heading.body_range.end, |next| {
                        next.range.start.min(heading.body_range.end)
                    });
                let body = heading.body_range.start..end;
                // Every registered Markdown document carries fields; only native
                // entry schemas make them rewritable (`citation_rules::rewritable`).
                let found = markdown::fields(text, body.clone());
                let mut cursor = body.start;
                for field in &found {
                    scan.prose(cursor..field.range.start, &heading.path, "prose", &excluded);
                    cursor = field.range.end;
                    if excluded(field.range.start) {
                        continue;
                    }
                    let written = produced.iter().find(|p| {
                        p.document == document
                            && p.path == heading.path
                            && fields::canonical(&p.field) == fields::canonical(field.name)
                    });
                    self.field(&mut scan, subject, heading, field, written)?;
                }
                scan.prose(cursor..body.end, &heading.path, "prose", &excluded);
            }
        }
        Ok(inventory)
    }

    fn field(
        &self,
        scan: &mut Scan<'_, '_>,
        subject: &Subject,
        heading: &OwnedMarkdownHeading,
        field: &MarkdownField<'_>,
        written: Option<&Produced>,
    ) -> Result<(), WriteError> {
        let value = markdown::decode_field(field);
        let accepted = fields::kind(scan.document)
            .ok()
            .and_then(|kind| fields::spelling(kind, field.name));
        // An inline value followed by unindented prose lines mixes the field
        // with free text (`entry.edit` refuses to replace it); never edit it.
        let prose_tail = !field.raw_value.starts_with('\n')
            && field
                .raw_value
                .split_inclusive('\n')
                .skip(1)
                .any(|line| !line.trim().is_empty() && !line.starts_with(char::is_whitespace));
        let reference = citation_rules::reference_field(field.name).is_some();
        let rewritable = citation_rules::rewritable(scan.document, field.name);
        let reason = if prose_tail {
            "ambiguous"
        } else if rewritable {
            "unparsed"
        } else if reference {
            "read_only_field"
        } else if accepted.is_some() {
            "untyped_field"
        } else {
            "unknown_field"
        };
        let mut classified = if reference && !prose_tail {
            self.classify(subject, &fields::canonical(field.name), &value)?
        } else {
            Classified::default()
        };
        for span in &mut classified.spans {
            let start = source_offset(field, span.range.start);
            span.source = start..start + span.range.len();
        }
        let covering: Vec<&Range<usize>> = if rewritable {
            classified.spans.iter().map(|span| &span.range).collect()
        } else {
            Vec::new()
        };
        for (range, literal) in occurrences(&value, scan.names) {
            if covering
                .iter()
                .copied()
                .chain(&classified.covered)
                .chain(written.iter().flat_map(|written| &written.ranges))
                .any(|covered| contains(covered, &range))
            {
                continue;
            }
            let reason = if classified.ambiguous.iter().any(|r| contains(r, &range)) {
                "ambiguous"
            } else if classified.protected.iter().any(|r| contains(r, &range)) {
                "protected"
            } else {
                reason
            };
            let line = scan.lines.line(source_offset(field, range.start));
            scan.inventory.mentions.push(Mention {
                document: scan.document.to_owned(),
                heading: heading.path.clone(),
                field: Some(field.name.to_owned()),
                line,
                literal: literal.to_owned(),
                reason,
            });
        }
        if !classified.spans.is_empty() {
            scan.inventory.citations.push(Citation {
                document: scan.document.to_owned(),
                path: heading.path.clone(),
                start: heading.range.start,
                heading: heading.heading.clone(),
                field: field.name.to_owned(),
                before: value.into_owned(),
                line: scan.lines.line(field.range.start),
                spans: classified.spans,
                rewritable,
            });
        }
        Ok(())
    }

    /// Whether `spelled` (written with `spelling`) resolves to exactly the
    /// entry at `path` of `document` in the current source.
    pub fn resolves_exactly(
        &self,
        spelled: &str,
        spelling: &Spelling,
        document: &str,
        path: &[String],
    ) -> Result<bool, WriteError> {
        let field = if matches!(spelling, Spelling::Name { .. }) {
            "related"
        } else {
            "sources"
        };
        let tokens = self.tokens(field, spelled)?;
        let [token] = tokens.as_slice() else {
            return Ok(false);
        };
        if token.range != (0..spelled.len()) {
            return Ok(false);
        }
        Ok(match &token.target {
            Target::Entry {
                document: found,
                index,
            } => found == document && self.heading(found, *index)?.path == path,
            Target::Ambiguous { .. } => false,
        })
    }
}

#[derive(Debug, Default)]
struct Classified {
    spans: Vec<Span>,
    protected: Vec<Range<usize>>,
    covered: Vec<Range<usize>>,
    ambiguous: Vec<Range<usize>>,
}

struct Scan<'a, 'i> {
    inventory: &'i mut Inventory,
    names: &'a [String],
    document: &'a str,
    text: &'a str,
    lines: &'a LineIndex,
}
impl Scan<'_, '_> {
    fn prose(
        &mut self,
        range: Range<usize>,
        heading: &[String],
        reason: &'static str,
        excluded: &dyn Fn(usize) -> bool,
    ) {
        if range.start >= range.end || excluded(range.start) {
            return;
        }
        let Some(text) = self.text.get(range.clone()) else {
            return;
        };
        for (found, literal) in occurrences(text, self.names) {
            self.inventory.mentions.push(Mention {
                document: self.document.to_owned(),
                heading: heading.to_vec(),
                field: None,
                line: self.lines.line(range.start + found.start),
                literal: literal.to_owned(),
                reason,
            });
        }
    }
}

fn target(document: &str, indexes: Vec<usize>) -> Option<Target> {
    match indexes.len() {
        0 => None,
        1 => Some(Target::Entry {
            document: document.into(),
            index: indexes[0],
        }),
        _ => Some(Target::Ambiguous {
            document: document.into(),
            indexes,
        }),
    }
}

fn overlaps(tokens: &[Token], range: &Range<usize>) -> bool {
    tokens
        .iter()
        .any(|token| token.range.start < range.end && range.start < token.range.end)
}

/// One typed citation of an entry in a current Markdown reference field, as
/// `ara refs` lists it.
#[derive(Debug, Clone)]
pub struct MarkdownCitation {
    pub document: String,
    /// The citing heading's ID (or full heading text).
    pub owner: String,
    pub field: String,
    pub literal: String,
    /// Absolute source byte range of the token.
    pub range: Range<usize>,
    /// C1 may rewrite it ([`citation_rules::rewritable`]).
    pub rewritable: bool,
}

/// Every typed citation of exactly the entry `target` selects, classified by
/// the same rules C1 uses to repair citations.
pub fn markdown_citations(
    working: &WorkingArtifact,
    target: &EntrySelector,
) -> Result<Vec<MarkdownCitation>, WriteError> {
    let entry = super::resolve(working, target)?;
    let subject = Subject {
        document: entry.document,
        path: entry.path,
        range: entry.range,
        root_id_retained: false,
        descendant_ids_retained: false,
    };
    let inventory = Resolver::new(working)?.inventory(&subject, &[], None, &[])?;
    let mut result = Vec::new();
    for citation in inventory.citations {
        let text = working.text(&citation.document)?;
        for span in citation.spans.iter().filter(|span| span.suffix.is_empty()) {
            result.push(MarkdownCitation {
                document: citation.document.clone(),
                owner: heading_id(&citation.heading).to_owned(),
                field: citation.field.clone(),
                literal: text.get(span.source.clone()).unwrap_or_default().to_owned(),
                range: span.source.clone(),
                rewritable: citation.rewritable,
            });
        }
    }
    Ok(result)
}
