//! Precise mutable-entry operations and coupled revision history.
use super::{
    EntrySelector, Fields, OperationResult, ReferenceEdit, WorkingArtifact, WriteError,
    WriteOperation,
    fields::{self, EntryKind},
    source::{self, PendingRevision},
};
use crate::markdown;
use serde_json::{Value, json};
use std::{collections::BTreeSet, ops::Range};

#[derive(Debug, Clone)]
pub struct Entry {
    pub document: String,
    pub heading: String,
    pub path: Vec<String>,
    pub range: Range<usize>,
    pub body: Range<usize>,
    pub field_body: Range<usize>,
    pub level: usize,
}

fn heading_id(heading: &str) -> &str {
    heading.split([':', ' ', '\t']).next().unwrap_or(heading)
}

fn native_document_prefix(document: &str) -> Option<&'static str> {
    match document {
        "logic/claims.md" => Some("C"),
        "logic/solution/heuristics.md" => Some("H"),
        "logic/experiments.md" => Some("E"),
        "logic/related_work.md" => Some("RW"),
        _ => None,
    }
}

pub fn resolve(working: &WorkingArtifact, selector: &EntrySelector) -> Result<Entry, WriteError> {
    let mut matches = Vec::new();
    let paths: Vec<String> = match selector {
        EntrySelector::Id { id } => {
            if fields::typed_id(id, "E") || fields::typed_id(id, "RW") {
                return Err(WriteError::semantic(
                    "write.namespace",
                    "Experiment plans and related work require document-qualified selectors",
                ));
            }
            let document = match id.as_str() {
                _ if fields::typed_id(id, "C") => "logic/claims.md",
                _ if fields::typed_id(id, "H") => "logic/solution/heuristics.md",
                _ => {
                    return Err(WriteError::semantic(
                        "write.namespace",
                        "Native ID selectors require claim or heuristic IDs; named entries need document-qualified heading paths",
                    ));
                }
            };
            vec![document.into()]
        }
        EntrySelector::Document {
            document,
            heading,
            entry,
        } => {
            if !working.is_allowed_document(document)?
                || document == "PAPER.md"
                || (heading.is_empty() && entry.is_none())
                || (!heading.is_empty() && entry.is_some())
            {
                return Err(WriteError::semantic(
                    "write.selector",
                    "Use an allowed document and exactly one entry or heading path",
                ));
            }
            vec![document.clone()]
        }
    };
    let selects = |text: &str, path: &[&str]| match selector {
        EntrySelector::Id { id } => heading_id(text) == id,
        EntrySelector::Document { heading, entry, .. } => match entry {
            Some(id) => heading_id(text) == id,
            None => {
                path.len() >= heading.len()
                    && path[path.len() - heading.len()..]
                        .iter()
                        .zip(heading)
                        .all(|(actual, wanted)| actual == wanted)
            }
        },
    };
    for document in &paths {
        let headings = working.headings(document)?;
        for (index, h) in headings.iter().enumerate() {
            let path: Vec<&str> = h.path.iter().map(String::as_str).collect();
            if selects(&h.heading, &path) {
                let field_end = headings.get(index + 1).map_or(h.body_range.end, |next| {
                    next.range.start.min(h.body_range.end)
                });
                matches.push(Entry {
                    document: document.clone(),
                    heading: h.heading.clone(),
                    path: h.path.clone(),
                    range: h.range.clone(),
                    body: h.body_range.clone(),
                    field_body: h.body_range.start..field_end,
                    level: h.level,
                });
            }
        }
    }
    if matches.is_empty() {
        // Reads may recover entries behind a stray `---`; writes never do.
        // Name the fence only when it hides a heading this selector targets.
        for document in &paths {
            let text = working.text(document)?;
            if crate::stray_fence::hidden_headings(text)
                .iter()
                .any(|h| selects(h.heading, &h.path))
                && let Some(error) = super::source::unclosed_fence_error(document, text)
            {
                return Err(error);
            }
        }
    }
    if matches.len() != 1 {
        return Err(WriteError::semantic(
            "write.selector",
            format!("Expected one entry; found {}", matches.len()),
        ));
    }
    Ok(matches.remove(0))
}

/// Retiring canonical numeric identities must reserve them in the mutation
/// ledger. Complete body operations remain free to replace ordinary headings.
pub fn preserve_canonical_ids(
    working: &WorkingArtifact,
    document: &str,
    range: &Range<usize>,
    content: &str,
) -> Result<(), WriteError> {
    let Some(prefix) = native_document_prefix(document) else {
        return Ok(());
    };
    // Selectors stay strict, but retention counts every identity reads see,
    // including claims recovered behind a stray leading `---`.
    let before = markdown::document_headings(document, working.text(document)?);
    let after = markdown::document_headings(document, content);
    for entry in before
        .iter()
        .filter(|h| h.range.start >= range.start && h.range.start < range.end)
    {
        let id = heading_id(entry.heading);
        if fields::typed_id(id, prefix) && !after.iter().any(|h| heading_id(h.heading) == id) {
            if document == "logic/claims.md" {
                return Err(WriteError::semantic(
                    "write.claim_retention",
                    format!(
                        "Retiring canonical claim {id} requires a retained withdrawal/merge entry or an audited entry.rename"
                    ),
                ));
            }
            return Err(WriteError::semantic(
                "write.structural_operation",
                format!("Retiring canonical identity {id} requires entry.remove or entry.rename"),
            ));
        }
    }
    Ok(())
}

pub fn add(
    working: &mut WorkingArtifact,
    prefix: char,
    requested: Option<&str>,
    title: &str,
    input: &Fields,
) -> Result<OperationResult, WriteError> {
    if title.trim().is_empty() || title.contains(['\r', '\n']) {
        return Err(WriteError::semantic(
            "write.title",
            "Title must be one nonempty line",
        ));
    }
    let (document, kind, op) = match prefix {
        'C' => ("logic/claims.md", EntryKind::Claim, "claim.add"),
        'H' => (
            "logic/solution/heuristics.md",
            EntryKind::Heuristic,
            "heuristic.add",
        ),
        _ => {
            return Err(WriteError::semantic(
                "write.kind",
                "Expected claim or heuristic",
            ));
        }
    };
    let fields = fields::validate(kind, input, true, false)?;
    let prefix_text = if prefix == 'C' { "C" } else { "H" };
    let mut ids = Vec::new();
    for path in working.paths() {
        if native_document_prefix(&path) == Some(prefix_text) {
            let headings = working.headings(&path)?;
            ids.extend(
                headings
                    .iter()
                    .map(|h| heading_id(&h.heading))
                    .filter(|id| fields::typed_id(id, prefix_text))
                    .map(str::to_owned),
            );
        }
    }
    let id = working.allocate_id(prefix, &ids, requested)?;
    // Structural lines follow a CRLF target file; a new file uses LF.
    let eol = if working.exists(document) && working.text(document)?.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let block = format!(
        "## {id}: {title}{eol}{}",
        fields::render_created(kind, &fields, eol)
    );
    if !working.exists(document) {
        working.create(
            document,
            &format!(
                "# {}\n\n{block}",
                if prefix == 'C' {
                    "Claims"
                } else {
                    "Heuristics"
                }
            ),
        )?;
    } else {
        append_with(working, document, &block, op, eol)?;
    }
    let mut result = OperationResult::new(op, Some(id.clone()));
    result.target = Some(format!("{document}:{id}"));
    Ok(result)
}

pub(crate) fn append(
    working: &mut WorkingArtifact,
    document: &str,
    block: &str,
    reason: &str,
) -> Result<(), WriteError> {
    append_with(working, document, block, reason, "\n")
}

/// Append after one blank line, writing separators with `eol`.
fn append_with(
    working: &mut WorkingArtifact,
    document: &str,
    block: &str,
    reason: &str,
    eol: &str,
) -> Result<(), WriteError> {
    let text = working.text(document)?;
    let end = text.len();
    // A CRLF file may still end in an LF blank line (e.g. after a continuation
    // field), so either form already separates the new block.
    let blank = format!("{eol}{eol}");
    let separator = if text.is_empty() || text.ends_with("\n\n") || text.ends_with(&blank) {
        ""
    } else if text.ends_with('\n') {
        eol
    } else {
        &blank
    };
    working.edit(document, end..end, &format!("{separator}{block}"), reason)
}

pub fn field_value(
    working: &WorkingArtifact,
    target: &EntrySelector,
    name: &str,
) -> Result<String, WriteError> {
    let entry = resolve(working, target)?;
    let text = working.text(&entry.document)?;
    let matches: Vec<_> = markdown::fields(text, entry.field_body.clone())
        .into_iter()
        .filter(|f| fields::canonical(f.name) == fields::canonical(name))
        .collect();
    if matches.len() != 1 {
        return Err(WriteError::semantic(
            "write.field_occurrence",
            format!("Expected one source field {name}; found {}", matches.len()),
        )
        .at(name));
    }
    Ok(markdown::decode_field(&matches[0]).into_owned())
}

fn edit(
    working: &mut WorkingArtifact,
    target: &EntrySelector,
    input: &Fields,
    revision: bool,
) -> Result<OperationResult, WriteError> {
    if let EntrySelector::Id { id } = target {
        if fields::typed_id(id, "N") {
            return Err(WriteError::semantic(
                "write.immutable",
                "Trace records are immutable; use a typed additive operation",
            ));
        }
        if fields::typed_id(id, "O") {
            return super::staging::edit_pointer(working, id, input);
        }
    }
    let entry = resolve(working, target)?;
    let kind = fields::kind(&entry.document)?;
    let set = fields::validate(kind, input, false, revision)?;
    let text = working.text(&entry.document)?;
    let body = entry.field_body.clone();
    let source_fields = markdown::fields(text, body.clone());
    let mut seen = BTreeSet::new();
    for field in &source_fields {
        let key = fields::canonical(field.name);
        if fields::spelling(kind, field.name).is_some() && !seen.insert(key) {
            return Err(WriteError::semantic(
                "write.field_duplicate",
                format!("Duplicate source field {}", field.name),
            )
            .at(field.name));
        }
    }
    let mut edits = Vec::new();
    let mut added = String::new();
    for (name, value) in &set {
        if let Some(field) = source_fields
            .iter()
            .find(|f| fields::canonical(f.name) == fields::canonical(name))
        {
            if markdown::decode_field(field).as_ref() != fields::value_text(value) {
                if !field.raw_value.starts_with('\n')
                    && field.raw_value.split_inclusive('\n').skip(1).any(|line| {
                        !line.trim().is_empty() && !line.starts_with(char::is_whitespace)
                    })
                {
                    return Err(WriteError::semantic(
                        "write.field_ambiguous",
                        "Unindented continuation or prose cannot be safely replaced",
                    )
                    .at(name));
                }
                let mut range = field.range.clone();
                if !field.raw_value.starts_with('\n') {
                    let raw = &text[range.clone()];
                    let trimmed = raw.trim_end_matches(['\r', '\n']);
                    range.end = range.start
                        + trimmed.len()
                        + usize::from(raw[trimmed.len()..].starts_with('\n'))
                        + if raw[trimmed.len()..].starts_with("\r\n") {
                            2
                        } else {
                            0
                        };
                }
                edits.push((range, fields::render(field.name, value)));
            }
        } else {
            added.push_str(&fields::render(name, value));
        }
    }
    if !added.is_empty() {
        let prefix = if body.end > 0 && !text[..body.end].ends_with('\n') {
            "\n"
        } else {
            ""
        };
        edits.push((body.end..body.end, format!("{prefix}{added}")));
    }
    let no_op = edits.is_empty();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, content) in edits {
        working.edit(&entry.document, range, &content, "entry.edit")?;
    }
    let mut result = OperationResult::new("entry.edit", None);
    result.target = Some(format!("{}:{}", entry.document, entry.heading));
    result.no_op = no_op;
    Ok(result)
}

/// Planner-level guard: omitted ownership is filled only by a batch anchor.
pub(crate) fn missing_owner() -> WriteError {
    WriteError::semantic(
        "write.revision_required",
        "omitted session/turn needs the batch's sole summarized session.log anchor",
    )
    .at("session")
}

pub(crate) fn revision_context(
    session: &str,
    turn: u64,
    signal: &str,
    provenance: &str,
) -> Result<(), WriteError> {
    super::sessions::validate_session_id(session)?;
    if turn == 0 {
        return Err(WriteError::semantic(
            "write.revision_turn",
            "Revision requires a positive batch-owned turn",
        ));
    }
    if !matches!(
        signal,
        "user-directive"
            | "empirical-resolution"
            | "verbal-affirmation"
            | "verbal-declaration"
            | "dependency-change"
            | "artifact-commitment"
            | "terminology-drift"
    ) {
        return Err(WriteError::semantic(
            "write.signal",
            "Unknown revision signal",
        ));
    }
    super::sessions::validate_provenance(provenance)?;
    Ok(())
}

pub(crate) fn valid_date(date: &str) -> bool {
    let pieces: Vec<_> = date.split('-').collect();
    if pieces.len() != 3 || pieces[0].len() != 4 || pieces[1].len() != 2 || pieces[2].len() != 2 {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        pieces[0].parse::<u32>(),
        pieces[1].parse::<u32>(),
        pieces[2].parse::<u32>(),
    ) else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}

#[allow(clippy::too_many_arguments)]
fn revise(
    working: &mut WorkingArtifact,
    target: &EntrySelector,
    input: &Fields,
    session: &str,
    turn: u64,
    signal: &str,
    provenance: &str,
    note: Option<&str>,
    expected: Option<&str>,
) -> Result<OperationResult, WriteError> {
    revision_context(session, turn, signal, provenance)?;
    if input.keys().any(|field| fields::canonical(field) == "body") {
        return revise_body(
            working, target, input, session, turn, signal, provenance, note, expected,
        );
    }
    if expected.is_some() {
        return Err(WriteError::semantic(
            "write.revision_digest",
            "expected digest is defined only for Body revision",
        ));
    }
    if input
        .keys()
        .any(|field| fields::canonical(field) == "last revised")
    {
        return Err(WriteError::semantic(
            "write.revision_required",
            "Last revised is computed by the revision planner",
        ));
    }
    let entry = resolve(working, target)?;
    let set = fields::validate(fields::kind(&entry.document)?, input, false, true)?;
    let mut records = Vec::new();
    for (field, value) in &set {
        let source = working.text(&entry.document)?;
        let matches: Vec<_> = markdown::fields(source, entry.field_body.clone())
            .into_iter()
            .filter(|f| fields::canonical(f.name) == fields::canonical(field))
            .collect();
        if matches.len() > 1 {
            return Err(WriteError::semantic(
                "write.field_duplicate",
                "Duplicate source revision field",
            )
            .at(field));
        }
        let before = matches
            .first()
            .map(|f| Value::String(markdown::decode_field(f).into_owned()))
            .unwrap_or(Value::Null);
        let after = fields::value_text(value);
        if before.as_str() != Some(after.as_str()) {
            records.push(json!({"entry":target,"field":field,"before":before,"after":after,"signal":signal,"provenance":provenance,"note":note}));
        }
    }
    let mut result = edit(working, target, &set, true)?;
    if !result.no_op {
        let pointer = Fields::from([(
            "Last revised".into(),
            Value::String(format!("{} ({session}#{turn})", &session[..10])),
        )]);
        edit(working, target, &pointer, true)?;
        for record in records {
            working.revisions.push(PendingRevision {
                session: session.into(),
                turn,
                record,
            });
        }
    }
    result.operation = "logic.revise".into();
    result.turn = Some(turn);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn revise_body(
    working: &mut WorkingArtifact,
    target: &EntrySelector,
    input: &Fields,
    session: &str,
    turn: u64,
    signal: &str,
    provenance: &str,
    note: Option<&str>,
    expected: Option<&str>,
) -> Result<OperationResult, WriteError> {
    let EntrySelector::Document {
        document,
        heading,
        entry,
    } = target
    else {
        return Err(WriteError::semantic(
            "write.body_selector",
            "Body revision requires an explicit native document selector",
        ));
    };
    if document == "PAPER.md" || !working.is_allowed_document(document)? {
        return Err(WriteError::semantic(
            "write.body_selector",
            "Body revision is restricted to mutable knowledge documents",
        ));
    }
    if input.len() != 1 {
        return Err(WriteError::semantic(
            "write.body_fields",
            "Body cannot be mixed with field setters",
        ));
    }
    let content = input
        .values()
        .next()
        .and_then(Value::as_str)
        .ok_or_else(|| {
            WriteError::semantic("write.body_type", "Body must be a complete source string")
        })?;
    let whole = heading.is_empty() && entry.is_none();
    let selected = if whole {
        None
    } else {
        Some(resolve(working, target)?)
    };
    let text = working.text(document)?;
    let range = selected
        .as_ref()
        .map_or(0..text.len(), |entry| entry.body.clone());
    let expected = expected.ok_or_else(|| {
        WriteError::semantic(
            "write.digest_required",
            "Body revision requires the exact selected before-body digest",
        )
    })?;
    if source::digest(text[range.clone()].as_bytes()) != expected {
        return Err(WriteError::semantic(
            "write.digest_conflict",
            "Body revision source digest does not match",
        ));
    }
    preserve_canonical_ids(working, document, &range, content)?;
    if selected.as_ref().is_some_and(|entry| {
        markdown::headings(content)
            .iter()
            .any(|h| h.level <= entry.level)
    }) {
        return Err(WriteError::semantic(
            "write.heading_escape",
            "Body revision cannot escape its selected heading",
        ));
    }
    let before = text[range.clone()].to_owned();
    let no_op = before == content;
    if !no_op {
        working.edit(document, range, content, "logic.revise Body")?;
        if selected.is_some() && fields::kind(document).is_ok() {
            let pointer = Fields::from([(
                "Last revised".into(),
                Value::String(format!("{} ({session}#{turn})", &session[..10])),
            )]);
            edit(working, target, &pointer, true)?;
        }
        let after_range = if whole {
            0..working.text(document)?.len()
        } else {
            resolve(working, target)?.body
        };
        let after = working.text(document)?[after_range].to_owned();
        let record = json!({"entry":target,"field":"Body","before":before,"after":after,"signal":signal,"provenance":provenance,"note":note});
        working.revisions.push(PendingRevision {
            session: session.into(),
            turn,
            record,
        });
    }
    let mut result = OperationResult::new("logic.revise", None);
    result.target = Some(if let Some(selected) = selected {
        format!("{document}:{}", selected.heading)
    } else {
        document.clone()
    });
    result.turn = Some(turn);
    result.no_op = no_op;
    Ok(result)
}

fn contains_reference(text: &str, identity: &str) -> bool {
    text.match_indices(identity).any(|(i, _)| {
        let left = text[..i].chars().next_back();
        let right = text[i + identity.len()..].chars().next();
        !left.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !right.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

#[allow(clippy::too_many_arguments)]
fn structural(
    working: &mut WorkingArtifact,
    target: &EntrySelector,
    name: Option<&str>,
    expected: &str,
    references: &[ReferenceEdit],
    session: Option<&str>,
    turn: Option<u64>,
    signal: Option<&str>,
    provenance: Option<&str>,
    redirect: Option<&EntrySelector>,
) -> Result<OperationResult, WriteError> {
    let (Some(session), Some(turn), Some(signal), Some(provenance)) =
        (session, turn, signal, provenance)
    else {
        return Err(WriteError::semantic(
            "write.revision_required",
            "Structural changes require session, turn, signal and provenance",
        ));
    };
    revision_context(session, turn, signal, provenance)?;
    let entry = resolve(working, target)?;
    let before = working.text(&entry.document)?[entry.range.clone()].to_owned();
    if source::digest(before.as_bytes()) != expected {
        return Err(WriteError::semantic(
            "write.digest_conflict",
            "Entry digest does not match",
        ));
    }
    if name.is_none() && entry.document == "logic/claims.md" {
        return Err(WriteError::semantic(
            "write.claim_retention",
            "Claims are retained: use an audited withdrawal or merge revision",
        ));
    }
    let canonical = native_document_prefix(&entry.document)
        .filter(|prefix| fields::typed_id(heading_id(&entry.heading), prefix));
    let new_heading = if let Some(name) = name {
        if name.trim().is_empty() || name.contains(['\n', '\r']) {
            return Err(WriteError::semantic(
                "write.name",
                "Name must be a nonempty single line",
            ));
        }
        if let Some(prefix) = canonical {
            if fields::typed_id(name, prefix) {
                Some(format!(
                    "{name}{}",
                    entry
                        .heading
                        .strip_prefix(heading_id(&entry.heading))
                        .unwrap_or("")
                ))
            } else if fields::typed_id(heading_id(name), prefix) && name.contains(':') {
                Some(name.to_owned())
            } else {
                return Err(WriteError::semantic(
                    "write.name",
                    "ID rename must retain its namespace",
                ));
            }
        } else {
            Some(name.to_owned())
        }
    } else {
        None
    };
    let rendered_header = new_heading
        .as_ref()
        .map(|heading| format!("{} {heading}\n", "#".repeat(entry.level)));
    let new_heading = rendered_header
        .as_ref()
        .map(|header| {
            markdown::headings(header)
                .first()
                .map(|h| h.heading.to_owned())
                .ok_or_else(|| {
                    WriteError::semantic("write.name", "Name does not produce one native heading")
                })
        })
        .transpose()?;
    if new_heading.as_deref() == Some(entry.heading.as_str()) {
        let mut result = OperationResult::new("entry.rename", None);
        result.target = Some(format!("{}:{}", entry.document, entry.heading));
        result.turn = Some(turn);
        result.no_op = true;
        return Ok(result);
    }
    let title_only = canonical.is_some()
        && new_heading
            .as_ref()
            .is_some_and(|heading| heading_id(heading) == heading_id(&entry.heading));
    let old = if canonical.is_some() && !title_only {
        heading_id(&entry.heading).to_owned()
    } else {
        entry.path.join("/")
    };
    let mut affected = vec![(old.clone(), entry.path.clone())];
    let mut from_selectors = std::collections::BTreeMap::from([(
        entry.path.clone(),
        EntrySelector::Document {
            document: entry.document.clone(),
            heading: entry.path.clone(),
            entry: None,
        },
    )]);
    let mut archived = std::collections::BTreeMap::from([(entry.path.clone(), 0..before.len())]);
    let mut audit_names = vec![if canonical.is_some() && !title_only {
        heading_id(&entry.heading).to_owned()
    } else {
        entry.heading.clone()
    }];
    {
        let headings = working.headings(&entry.document)?;
        for heading in headings
            .iter()
            .filter(|h| h.range.start > entry.range.start && h.range.start < entry.range.end)
        {
            let identity = heading_id(&heading.heading);
            let numeric = native_document_prefix(&entry.document)
                .is_some_and(|prefix| fields::typed_id(identity, prefix));
            let identity = if name.is_none() && numeric {
                identity.to_owned()
            } else {
                heading.path.join("/")
            };
            if from_selectors
                .insert(
                    heading.path.clone(),
                    EntrySelector::Document {
                        document: entry.document.clone(),
                        heading: heading.path.clone(),
                        entry: None,
                    },
                )
                .is_some()
            {
                return Err(WriteError::semantic(
                    "write.ambiguous_locator",
                    "Distinct source headings have identical literal vectors",
                ));
            }
            archived.insert(
                heading.path.clone(),
                heading.range.start - entry.range.start..heading.range.end - entry.range.start,
            );
            affected.push((identity, heading.path.clone()));
            if name.is_none() {
                audit_names.push(if numeric {
                    heading_id(&heading.heading).into()
                } else {
                    heading.heading.clone()
                });
            }
        }
    }
    affected.sort();
    affected.dedup();
    audit_names.sort();
    audit_names.dedup();
    let mut destination_selector = None;
    let destination = if let Some(heading) = new_heading.as_ref() {
        let identity = if canonical.is_some() && !title_only {
            heading_id(heading).to_owned()
        } else {
            let parents = &entry.path[..entry.path.len() - 1];
            let mut identity = String::with_capacity(
                parents.iter().map(|part| part.len() + 1).sum::<usize>() + heading.len(),
            );
            for parent in parents {
                identity.push_str(parent);
                identity.push('/');
            }
            identity.push_str(heading);
            identity
        };
        let mut path = entry.path.clone();
        *path.last_mut().unwrap() = heading.clone();
        destination_selector = Some(EntrySelector::Document {
            document: entry.document.clone(),
            heading: path,
            entry: None,
        });
        Some(format!("{}:{identity}", entry.document))
    } else if let Some(redirect) = redirect {
        let e = resolve(working, redirect)?;
        if e.document == entry.document
            && e.range.start >= entry.range.start
            && e.range.end <= entry.range.end
        {
            return Err(WriteError::semantic(
                "write.redirect",
                "Removal cannot redirect into its own subtree",
            ));
        }
        let identity = if native_document_prefix(&e.document)
            .is_some_and(|prefix| fields::typed_id(heading_id(&e.heading), prefix))
        {
            heading_id(&e.heading).to_owned()
        } else {
            e.path.join("/")
        };
        destination_selector = Some(EntrySelector::Document {
            document: e.document.clone(),
            heading: e.path,
            entry: None,
        });
        Some(format!("{}:{identity}", e.document))
    } else {
        None
    };
    if let Some(heading) = new_heading.as_ref() {
        let destination_path = match destination_selector.as_ref().unwrap() {
            EntrySelector::Document { heading, .. } => heading,
            _ => unreachable!(),
        };
        let destination_identity = if canonical.is_some() && !title_only {
            EntrySelector::Id {
                id: heading_id(heading).into(),
            }
        } else {
            destination_selector.clone().unwrap()
        };
        let retired = mutation_sources(working)?;
        if retired
            .iter()
            .any(|source| exact_identity_matches(&destination_identity, source))
        {
            return Err(WriteError::semantic(
                "write.redirect_cycle",
                "Historical identities cannot be reused or create redirect cycles",
            ));
        }
        let headings = working.headings(&entry.document)?;
        for (_, source_path) in &affected {
            let mut renamed_path = source_path.clone();
            renamed_path[entry.path.len() - 1] = heading.clone();
            if retired.iter().any(|source| {
                exact_identity_matches(
                    &EntrySelector::Document {
                        document: entry.document.clone(),
                        heading: renamed_path.clone(),
                        entry: None,
                    },
                    source,
                )
            }) {
                return Err(WriteError::semantic(
                    "write.redirect_cycle",
                    "Historical heading identities cannot be reused",
                ));
            }
            if headings.iter().any(|h| {
                h.path == renamed_path
                    && !(h.range.start >= entry.range.start && h.range.start < entry.range.end)
            }) {
                return Err(WriteError::semantic(
                    "write.name_duplicate",
                    "Rename destination already exists",
                ));
            }
        }
        if canonical.is_some()
            && headings.iter().any(|h| {
                heading_id(&h.heading) == heading_id(heading)
                    && h.path != *destination_path
                    && h.range.start != entry.range.start
            })
        {
            return Err(WriteError::semantic(
                "write.name_duplicate",
                "Canonical rename destination already exists",
            ));
        }
    }
    let mut ref_seen = BTreeSet::new();
    for reference in references {
        let referred = resolve(working, &reference.target)?;
        let headings = working.headings(&referred.document)?;
        let hierarchy = headings
            .iter()
            .find(|h| h.range.start == referred.range.start)
            .ok_or_else(|| WriteError::semantic("write.reference", "Reference heading missing"))?
            .path
            .clone();
        if !ref_seen.insert((
            referred.document.clone(),
            hierarchy,
            fields::canonical(&reference.field),
        )) {
            return Err(WriteError::semantic(
                "write.reference_duplicate",
                "Duplicate source reference field",
            ));
        }
        if name.is_none()
            && referred.document == entry.document
            && referred.range.start >= entry.range.start
            && referred.range.end <= entry.range.end
        {
            return Err(WriteError::semantic(
                "write.reference",
                "Reference edit cannot target the removed subtree",
            ));
        }
        if field_value(working, &reference.target, &reference.field)? != reference.before {
            return Err(WriteError::semantic(
                "write.reference_before",
                "Reference before does not match exact source",
            ));
        }
        if !audit_names
            .iter()
            .any(|old| contains_reference(&reference.before, old))
            || audit_names
                .iter()
                .any(|old| contains_reference(&reference.after, old))
        {
            return Err(WriteError::semantic(
                "write.reference",
                "Reference edit must remove the old identity",
            ));
        }
        let value = if fields::canonical(&reference.field) == "dependencies" {
            serde_json::from_str::<Value>(&reference.after).map_err(|_| {
                WriteError::semantic(
                    "write.reference",
                    "Dependency after must be a typed JSON string array",
                )
            })?
        } else {
            Value::String(reference.after.clone())
        };
        revise(
            working,
            &reference.target,
            &Fields::from([(reference.field.clone(), value)]),
            session,
            turn,
            signal,
            provenance,
            None,
            None,
        )?;
    }
    // Re-resolve: earlier reference edits can shift this entry's byte offsets.
    let entry = resolve(working, target)?;
    let mut historical = Vec::new();
    for path in working.paths() {
        let Ok(text) = working.text(&path) else {
            continue;
        };
        if working.is_allowed_document(&path)? {
            let remaining = if path == entry.document {
                if name.is_some() {
                    format!(
                        "{}{}",
                        &text[..entry.range.start],
                        &text[entry.body.start..]
                    )
                } else {
                    format!("{}{}", &text[..entry.range.start], &text[entry.range.end..])
                }
            } else {
                text.to_owned()
            };
            // A separate heading is an identity definition, not an inbound
            // prose reference. Explicit paths may distinguish equal names.
            let headings = markdown::headings(&remaining);
            let mut prose = String::new();
            let mut cursor = 0;
            for heading in headings {
                prose.push_str(&remaining[cursor..heading.range.start]);
                cursor = heading.body_range.start;
            }
            prose.push_str(&remaining[cursor..]);
            if audit_names
                .iter()
                .any(|old| contains_reference(&prose, old))
            {
                return Err(WriteError::semantic(
                    "write.dangling_reference",
                    format!("Unmigrated prose or field reference in {path}"),
                ));
            }
        } else if audit_names.iter().any(|old| contains_reference(text, old)) {
            historical.push(path);
        }
    }
    if name.is_none() && destination.is_none() && (!historical.is_empty() || !references.is_empty())
    {
        return Err(WriteError::semantic(
            "write.redirect_required",
            "Referenced removals require an existing replacement; a null tombstone cannot resolve historical citations",
        ));
    }
    let after = if new_heading.is_some() {
        let text = working.text(&entry.document)?;
        let heading = rendered_header.as_ref().unwrap();
        let content = format!("{heading}{}", &text[entry.body.clone()]);
        working.edit(
            &entry.document,
            entry.range.clone(),
            &content,
            "entry.rename",
        )?;
        let renamed = destination_selector.as_ref().unwrap();
        if fields::kind(&entry.document).is_ok() {
            let pointer = Fields::from([(
                "Last revised".into(),
                Value::String(format!("{} ({session}#{turn})", &session[..10])),
            )]);
            edit(working, renamed, &pointer, true)?;
        }
        let renamed = resolve(working, renamed)?;
        working.text(&entry.document)?[renamed.range].to_owned()
    } else {
        working.edit(&entry.document, entry.range.clone(), "", "entry.remove")?;
        String::new()
    };
    let record = json!({"entry":target,"field":"entry","before":before,"after":after,"signal":signal,"provenance":provenance});
    working.revisions.push(PendingRevision {
        session: session.into(),
        turn,
        record,
    });
    // Authoritative mappings append; immutable historical references stay byte-identical.
    working.ensure_yaml("trace/logic_mutations.yaml", "mutations: []\n")?;
    let current_headings = working.headings(&entry.document)?;
    let mut current_index = std::collections::BTreeMap::<&[String], Option<Range<usize>>>::new();
    for heading in current_headings.iter() {
        current_index
            .entry(heading.path.as_slice())
            .and_modify(|value| *value = None)
            .or_insert(Some(heading.range.clone()));
    }
    let mut mappings = Vec::with_capacity(affected.len());
    for (identity, path) in &affected {
        let from_selector = &from_selectors[path];
        let to_selector = if name.is_some() && path != &entry.path {
            let EntrySelector::Document {
                document, heading, ..
            } = from_selector
            else {
                unreachable!()
            };
            let mut heading = heading.clone();
            heading[entry.path.len() - 1] = new_heading.as_ref().unwrap().clone();
            Some(EntrySelector::Document {
                document: document.clone(),
                heading,
                entry: None,
            })
        } else {
            destination_selector.clone()
        };
        let to = if name.is_some() && path != &entry.path {
            let EntrySelector::Document {
                document, heading, ..
            } = to_selector.as_ref().unwrap()
            else {
                unreachable!()
            };
            Some(format!("{document}:{}", heading.join("/")))
        } else {
            destination.clone()
        };
        let source_after = if name.is_some() {
            let EntrySelector::Document { heading, .. } = to_selector.as_ref().unwrap() else {
                unreachable!()
            };
            let range = current_index
                .get(heading.as_slice())
                .and_then(Clone::clone)
                .ok_or_else(|| {
                    WriteError::semantic(
                        "write.selector",
                        "Renamed heading path is ambiguous or missing",
                    )
                })?;
            &working.text(&entry.document)?[range]
        } else {
            ""
        };
        let mapping = json!({"from":format!("{}:{identity}",entry.document),"to":to,"from_selector":from_selector,"to_selector":to_selector,"before":&before[archived[path].clone()],"after":source_after,"action":if name.is_some(){"rename"}else{"remove"},"session":session,"turn":turn,"signal":signal,"provenance":provenance,"historical_references":historical});
        if path != &entry.path {
            let record = json!({"entry":from_selector,"field":"entry","before":mapping["before"],"after":mapping["after"],"signal":signal,"provenance":provenance});
            working.revisions.push(PendingRevision {
                session: session.into(),
                turn,
                record,
            });
        }
        mappings.push(mapping);
    }
    working.append_yaml_many(
        "trace/logic_mutations.yaml",
        &["mutations".into()],
        &mappings,
    )?;
    let mut result = OperationResult::new(
        if name.is_some() {
            "entry.rename"
        } else {
            "entry.remove"
        },
        None,
    );
    result.target = destination.or_else(|| Some(format!("{}:{old}", entry.document)));
    result.turn = Some(turn);
    Ok(result)
}

fn taste(
    working: &mut WorkingArtifact,
    target: &EntrySelector,
    record: &Value,
) -> Result<OperationResult, WriteError> {
    let entry = resolve(working, target)?;
    if !matches!(
        fields::kind(&entry.document)?,
        EntryKind::Claim | EntryKind::Heuristic
    ) {
        return Err(WriteError::semantic(
            "write.taste_target",
            "Inline taste applies only to claims and heuristics",
        ));
    }
    let mut record = record.clone();
    let object = record
        .as_object_mut()
        .ok_or_else(|| WriteError::semantic("write.taste_record", "Taste needs an object"))?;
    if !object.contains_key("date") {
        // An inline taste row carries a date, not a timestamp: the UTC date of
        // the captured batch time.
        let date = working.clock_time()?[..10].to_owned();
        object.insert("date".into(), Value::String(date));
    }
    let object = &*object;
    for key in object.keys() {
        if !matches!(key.as_str(), "date" | "tag" | "object" | "comment") {
            return Err(WriteError::semantic(
                "write.taste_field",
                format!("Unknown taste field {key}"),
            )
            .at(key));
        }
    }
    for key in ["date", "tag", "object", "comment"] {
        if !object.get(key).is_some_and(Value::is_string) {
            return Err(WriteError::semantic(
                "write.taste_field",
                format!("Required string {key}"),
            )
            .at(key));
        }
    }
    if !valid_date(object["date"].as_str().unwrap()) {
        return Err(WriteError::semantic(
            "write.taste_date",
            "Invalid calendar date",
        ));
    }
    if !matches!(
        object["tag"].as_str().unwrap(),
        "endorse" | "reject" | "uncertain"
    ) {
        return Err(WriteError::semantic(
            "write.taste_tag",
            "Unknown taste attitude",
        ));
    }
    if !matches!(
        object["object"].as_str().unwrap(),
        "claim" | "evidence" | "framing" | "priority"
    ) {
        return Err(WriteError::semantic(
            "write.taste_object",
            "Unknown taste object",
        ));
    }
    if object["comment"].as_str().unwrap().trim().is_empty() {
        return Err(WriteError::semantic(
            "write.taste_comment",
            "Caller-confirmed comment is required",
        ));
    }
    let source = working.text(&entry.document)?;
    let body = entry.field_body.clone();
    let existing: Vec<_> = markdown::fields(source, body.clone())
        .into_iter()
        .filter(|f| fields::canonical(f.name) == "taste")
        .collect();
    if existing.len() > 1 {
        return Err(WriteError::semantic(
            "write.taste_duplicate",
            "Inline taste has one source subsection",
        ));
    }
    let at = if let Some(field) = existing.first() {
        if field
            .raw_value
            .split_inclusive('\n')
            .skip(1)
            .any(|line| !line.trim().is_empty() && !line.starts_with(char::is_whitespace))
        {
            return Err(WriteError::semantic(
                "write.taste_source",
                "Taste source contains ambiguous unindented prose",
            ));
        }
        field.range.end
    } else {
        body.end
    };
    let mut text = String::new();
    if at > 0 && !source[..at].ends_with('\n') {
        text.push('\n');
    }
    if existing.is_empty() {
        text.push_str("- **Taste**:\n");
    }
    let comment = object["comment"].as_str().unwrap();
    let mut lines = comment.split('\n');
    text.push_str(&format!(
        "  - [{}] `{}` on `{}` — {}\n",
        object["date"].as_str().unwrap(),
        object["tag"].as_str().unwrap(),
        object["object"].as_str().unwrap(),
        lines.next().unwrap()
    ));
    for line in lines {
        text.push_str("    ");
        text.push_str(line);
        text.push('\n');
    }
    working.edit(&entry.document, at..at, &text, "entry.taste_append")?;
    let mut result = OperationResult::new("entry.taste_append", None);
    result.target = Some(format!("{}:{}", entry.document, entry.heading));
    Ok(result)
}

fn conflict_marker(reference: &str) -> String {
    let escape_hyphens = reference.contains("--");
    let mut marker = String::from("<!-- CONFLICT: see ");
    for character in reference.chars() {
        match character {
            '&' => marker.push_str("&amp;"),
            '<' => marker.push_str("&lt;"),
            '>' => marker.push_str("&gt;"),
            '-' if escape_hyphens => marker.push_str("&#45;"),
            other => marker.push(other),
        }
    }
    marker.push_str(" -->");
    marker
}

fn annotate(
    working: &mut WorkingArtifact,
    target: &EntrySelector,
    kind: &str,
    references: &[String],
    comment: &str,
) -> Result<OperationResult, WriteError> {
    if kind != "conflict" || references.is_empty() || comment.trim().is_empty() {
        return Err(WriteError::semantic(
            "write.annotation",
            "Conflict requires references and caller comment",
        ));
    }
    if references.iter().collect::<BTreeSet<_>>().len() != references.len() {
        return Err(WriteError::semantic(
            "write.annotation",
            "Duplicate conflict references",
        ));
    }
    for reference in references {
        let typed = ["C", "H", "N", "O", "T", "E", "RW"]
            .iter()
            .any(|prefix| fields::typed_id(reference, prefix));
        let document = reference.split(['#', ':']).next().unwrap_or(reference);
        let native = working.is_allowed_document(document)?
            && (document == reference
                || reference
                    .get(document.len() + 1..)
                    .is_some_and(|identity| !identity.is_empty()));
        if reference.contains(['\r', '\n']) || (!typed && !native) {
            return Err(WriteError::semantic(
                "write.annotation_reference",
                format!("Invalid native reference {reference}"),
            ));
        }
    }
    if let EntrySelector::Id { id } = target {
        if fields::typed_id(id, "N") {
            return super::node::annotate(working, id, kind, references, comment);
        }
        if fields::typed_id(id, "O") {
            return super::staging::annotate(working, id, kind, references, comment);
        }
    }
    let entry = resolve(working, target)?;
    let record = json!({"kind":kind,"references":references,"comment":comment});
    let encoded = serde_json::to_string(&record)
        .map_err(|e| WriteError::io(e.to_string()))?
        .replace("--", "\\u002d\\u002d");
    let source = working.text(&entry.document)?;
    let at = entry.field_body.end;
    let body = &source[entry.field_body.clone()];
    let context = format!("<!-- ARA annotation: {encoded} -->");
    let no_op = body.lines().any(|line| line == context)
        && references.iter().all(|reference| {
            let marker = conflict_marker(reference);
            body.lines().any(|line| line == marker)
        });
    let mut result = OperationResult::new("entry.annotate", None);
    result.target = Some(format!("{}:{}", entry.document, entry.heading));
    result.no_op = no_op;
    if no_op {
        return Ok(result);
    }
    let mut text = if at > 0 && !source[..at].ends_with('\n') {
        String::from("\n")
    } else {
        String::new()
    };
    for reference in references {
        let marker = conflict_marker(reference);
        if !body.lines().any(|line| line == marker) {
            text.push_str(&marker);
            text.push('\n');
        }
    }
    text.push_str(&context);
    text.push('\n');
    let reason = format!(
        "mutable.conflict.references:{}",
        serde_json::to_string(references).map_err(|e| WriteError::io(e.to_string()))?
    );
    working.edit(&entry.document, at..at, &text, &reason)?;
    Ok(result)
}

pub fn plan(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    match operation {
        WriteOperation::ClaimAdd { id, title, fields } => {
            add(working, 'C', id.as_deref(), title, fields)
        }
        WriteOperation::HeuristicAdd { id, title, fields } => {
            add(working, 'H', id.as_deref(), title, fields)
        }
        WriteOperation::EntryEdit { target, set } => edit(working, target, set, false),
        WriteOperation::LogicRevise {
            target,
            set,
            session,
            turn,
            signal,
            provenance,
            note,
            expected,
        } => revise(
            working,
            target,
            set,
            session.as_deref().ok_or_else(missing_owner)?,
            turn.ok_or_else(missing_owner)?,
            signal,
            provenance,
            note.as_deref(),
            expected.as_deref(),
        ),
        WriteOperation::EntryRename {
            target,
            name,
            expected,
            references,
            session,
            turn,
            signal,
            provenance,
        } => structural(
            working,
            target,
            Some(name),
            expected,
            references,
            session.as_deref(),
            *turn,
            signal.as_deref(),
            provenance.as_deref(),
            None,
        ),
        WriteOperation::EntryRemove {
            target,
            expected,
            references,
            session,
            turn,
            signal,
            provenance,
            redirect,
        } => structural(
            working,
            target,
            None,
            expected,
            references,
            session.as_deref(),
            *turn,
            signal.as_deref(),
            provenance.as_deref(),
            redirect.as_ref(),
        ),
        WriteOperation::EntryTasteAppend { target, record } => taste(working, target, record),
        WriteOperation::EntryAnnotate {
            target,
            kind,
            references,
            comment,
        } => annotate(working, target, kind, references, comment),
        _ => Err(WriteError::semantic(
            "write.operation",
            "Not a logic operation",
        )),
    }
}

/// Final candidate references permit concrete forward references while refusing
/// newly introduced dangling dependencies, plan-proof links and conflicts.
pub fn validate_references(working: &WorkingArtifact) -> Result<(), WriteError> {
    validate_claim_retention(working)?;
    validate_registry_removals(working)?;
    validate_retired_origins(working)?;
    validate_new_annotations(working)?;
    let paths = working.paths();
    let candidate: std::collections::BTreeMap<&str, &str> = paths
        .iter()
        .filter_map(|path| working.text(path).ok().map(|text| (path.as_str(), text)))
        .collect();
    let baseline: std::collections::BTreeMap<&str, &str> = working
        .base
        .files
        .iter()
        .filter(|(_, file)| file.existed)
        .filter_map(|(path, file)| {
            std::str::from_utf8(&file.bytes)
                .ok()
                .map(|text| (path.as_str(), text))
        })
        .collect();
    let before = reference_issues(&baseline)?;
    let after = reference_issues(&candidate)?;
    for (issue, count) in after {
        if count > before.get(&issue).copied().unwrap_or(0) {
            return Err(WriteError::semantic("write.reference", issue));
        }
    }
    Ok(())
}

/// Native concrete IDs and exact source-native document/heading locators.
pub fn known_reference_ids(working: &WorkingArtifact) -> Result<BTreeSet<String>, WriteError> {
    let mut ids: BTreeSet<String> = super::node::node_ids(working)?.into_iter().collect();
    for id in claim_redirects_from_source(working)?.keys() {
        ids.insert(id.clone());
    }
    for path in working.paths() {
        if working.is_allowed_document(&path)? {
            ids.insert(path.clone());
            let headings = working.headings(&path)?;
            for heading in headings.iter() {
                let id = heading_id(&heading.heading);
                if native_document_prefix(&path).is_some_and(|prefix| fields::typed_id(id, prefix))
                {
                    ids.insert(id.into());
                }
                ids.insert(format!("{path}:{}", heading.heading));
                ids.insert(format!("{path}#{}", heading.heading));
            }
        }
    }
    for (path, key) in [
        ("staging/observations.yaml", "observations"),
        ("trace/taste_log.yaml", "entries"),
    ] {
        if working.exists(path) {
            let document = working.yaml(path)?;
            let records = document
                .root
                .get(key)?
                .ok_or_else(|| {
                    WriteError::semantic("write.reference", "Missing native records list")
                })?
                .sequence()?;
            for record in records {
                if let Some(id) = record.get("id")?.and_then(super::source::YamlNode::scalar) {
                    ids.insert(id.into());
                }
            }
        }
    }
    Ok(ids)
}

fn dependency_list(text: &str) -> Result<Vec<String>, WriteError> {
    if let Ok(values) = serde_json::from_str::<Vec<String>>(text) {
        return Ok(values);
    }
    let Some(inner) = text
        .strip_prefix('[')
        .and_then(|text| text.strip_suffix(']'))
    else {
        return Err(WriteError::semantic(
            "write.dependencies",
            "Dependencies needs a string array",
        ));
    };
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    let values: Vec<_> = inner
        .split(',')
        .map(|text| text.trim().trim_matches(['\'', '"']).to_owned())
        .collect();
    if values
        .iter()
        .all(|id| ["C", "E"].iter().any(|prefix| fields::typed_id(id, prefix)))
    {
        Ok(values)
    } else {
        Err(WriteError::semantic(
            "write.dependencies",
            "Invalid typed dependency",
        ))
    }
}

fn reference_issues(
    documents: &std::collections::BTreeMap<&str, &str>,
) -> Result<std::collections::BTreeMap<String, usize>, WriteError> {
    let redirects = claim_redirects_with(
        |path| documents.get(path).copied(),
        |_, text| super::source::YamlDocument::parse(text).map(std::sync::Arc::new),
    )?;
    let mut ids = BTreeSet::new();
    let registered = documents
        .get("PAPER.md")
        .map(|paper| crate::knowledge_paths(paper))
        .transpose()
        .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?
        .unwrap_or_default();
    let mut heading_index = std::collections::BTreeMap::new();
    for (path, text) in documents {
        if super::documents::allowed(path) || registered.iter().any(|p| p == *path) {
            let headings = markdown::headings(text);
            for heading in &headings {
                let id = heading_id(heading.heading);
                if native_document_prefix(path).is_some_and(|prefix| fields::typed_id(id, prefix)) {
                    ids.insert(id.to_owned());
                }
            }
            heading_index.insert(*path, headings);
        }
    }
    let mut issues = std::collections::BTreeMap::new();
    let mut graph = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (path, headings) in heading_index {
        let Ok(kind) = fields::kind(path) else {
            continue;
        };
        let text = documents[path];
        for (index, heading) in headings.iter().enumerate() {
            let end = headings
                .get(index + 1)
                .map_or(heading.body_range.end, |next| {
                    next.range.start.min(heading.body_range.end)
                });
            let id = heading_id(heading.heading);
            for field in markdown::fields(text, heading.body_range.start..end) {
                let value = markdown::decode_field(&field);
                if fields::spelling(kind, field.name).is_none()
                    && !(matches!(kind, fields::EntryKind::Heuristic)
                        && fields::canonical(field.name) == "boundaries")
                {
                    continue;
                }
                let mut refs = match fields::canonical(field.name).as_str() {
                    "dependencies" => match dependency_list(&value) {
                        Ok(refs) => refs,
                        Err(_) => {
                            *issues
                                .entry(format!("{path}:{id}: invalid Dependencies"))
                                .or_insert(0) += 1;
                            continue;
                        }
                    },
                    "merged into" => {
                        if fields::typed_id(&value, "C") {
                            vec![value.into_owned()]
                        } else {
                            *issues
                                .entry(format!("{path}:{id}: invalid Merged into"))
                                .or_insert(0) += 1;
                            continue;
                        }
                    }
                    "proof" | "sources" | "bounds" | "boundaries" | "boundary" | "related"
                    | "claims affected" | "verifies" | "evidence" | "caused by"
                    | "derived from" | "enables" | "appears in" => {
                        crate::query::scan_tokens(&value)
                            .into_iter()
                            .filter(|token| native_id_document(token.literal).is_some())
                            .map(|token| token.literal.to_owned())
                            .collect()
                    }
                    _ => continue,
                };
                for reference in &mut refs {
                    if let Some(target) = redirects.get(reference) {
                        reference.clone_from(target);
                    }
                }
                if matches!(
                    fields::canonical(field.name).as_str(),
                    "dependencies" | "merged into"
                ) {
                    graph
                        .entry(id.to_owned())
                        .or_default()
                        .extend(refs.iter().cloned());
                }
                for reference in refs {
                    if !ids.contains(&reference) {
                        *issues
                            .entry(format!(
                                "{path}:{id}: {} -> {reference} is dangling",
                                field.name
                            ))
                            .or_insert(0) += 1;
                    }
                }
            }
        }
    }
    fn reaches(
        graph: &std::collections::BTreeMap<String, Vec<String>>,
        at: &str,
        target: &str,
        seen: &mut BTreeSet<String>,
    ) -> bool {
        let mut pending = vec![at];
        while let Some(at) = pending.pop() {
            if !seen.insert(at.into()) {
                continue;
            }
            if let Some(edges) = graph.get(at) {
                for edge in edges {
                    if edge == target {
                        return true;
                    }
                    pending.push(edge);
                }
            }
        }
        false
    }
    for id in graph.keys() {
        if reaches(&graph, id, id, &mut BTreeSet::new()) {
            issues.insert(format!("{id}: dependency cycle"), 1);
        }
    }
    Ok(issues)
}

fn exact_identity_matches(request: &EntrySelector, source: &EntrySelector) -> bool {
    match (request, source) {
        (
            EntrySelector::Document {
                document,
                heading,
                entry,
            },
            EntrySelector::Document {
                document: other,
                heading: other_heading,
                entry: other_entry,
            },
        ) => document == other && heading == other_heading && entry == other_entry,
        (EntrySelector::Id { .. }, _) => selector_corresponds(request, source),
        (_, EntrySelector::Id { .. }) => selector_corresponds(source, request),
    }
}

fn mutation_sources(working: &WorkingArtifact) -> Result<Vec<EntrySelector>, WriteError> {
    if !working.exists("trace/logic_mutations.yaml") {
        return Ok(Vec::new());
    }
    let document = working.yaml("trace/logic_mutations.yaml")?;
    let entries = document
        .root
        .get("mutations")?
        .ok_or_else(|| WriteError::semantic("write.redirect", "Missing mutation records"))?
        .sequence()?;
    let mut anchors = std::collections::BTreeMap::new();
    registry_anchors(&document.root, &mut anchors);
    entries
        .iter()
        .map(|entry| {
            let from = entry
                .get("from")?
                .and_then(super::source::YamlNode::scalar)
                .ok_or_else(|| {
                    WriteError::semantic("write.redirect", "Mutation source locator missing")
                })?;
            if let Some((document, Some(id), true)) = locator_parts(from)
                && native_document_prefix(document)
                    .is_some_and(|prefix| fields::typed_id(id, prefix))
            {
                return Ok(EntrySelector::Id { id: id.into() });
            }
            if let Some(selector) = entry
                .get("from_selector")?
                .and_then(|node| selector_from_yaml(node, &anchors))
            {
                return Ok(selector);
            }
            selector_for_locator(working, from)
        })
        .collect()
}

fn validate_new_annotations(working: &WorkingArtifact) -> Result<(), WriteError> {
    let mut known = None;
    for intent in &working.intents {
        let Some(references) = intent.reason.strip_prefix("mutable.conflict.references:") else {
            continue;
        };
        let references: Vec<String> = serde_json::from_str(references)
            .map_err(|e| WriteError::semantic("write.annotation", e.to_string()))?;
        let text = working.text(&intent.path)?;
        for reference in references {
            let marker = conflict_marker(&reference);
            if text.lines().any(|line| line == marker) {
                if known.is_none() {
                    known = Some(known_reference_ids(working)?);
                }
                if !known.as_ref().unwrap().contains(&reference) {
                    return Err(WriteError::semantic(
                        "write.annotation_reference",
                        format!("Dangling conflict reference {reference}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn registry_anchors<'a>(
    node: &'a super::source::YamlNode,
    anchors: &mut std::collections::BTreeMap<usize, &'a super::source::YamlNode>,
) {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if node.anchor != 0 {
            anchors.insert(node.anchor, node);
        }
        match &node.kind {
            super::source::YamlKind::Sequence(items) => pending.extend(items.iter()),
            super::source::YamlKind::Mapping(entries) => {
                for (key, value) in entries {
                    pending.push(key);
                    pending.push(value);
                }
            }
            _ => {}
        }
    }
}

fn registry_node<'a>(
    mut node: &'a super::source::YamlNode,
    anchors: &std::collections::BTreeMap<usize, &'a super::source::YamlNode>,
) -> &'a super::source::YamlNode {
    for _ in 0..=anchors.len() {
        let super::source::YamlKind::Alias(id) = &node.kind else {
            return node;
        };
        let Some(target) = anchors.get(id) else {
            return node;
        };
        node = target;
    }
    node
}

struct ReferenceAudit<'a> {
    working: &'a WorkingArtifact,
    removed: BTreeSet<String>,
    old_headings: std::collections::BTreeMap<String, Vec<Vec<String>>>,
}

impl ReferenceAudit<'_> {
    fn selector(
        &self,
        selector: &EntrySelector,
        source: &str,
        field: &str,
    ) -> Result<(), WriteError> {
        let document = match selector {
            EntrySelector::Id { id } => native_id_document(id),
            EntrySelector::Document { document, .. } => Some(document.as_str()),
        };
        let Some(document) = document else {
            return Ok(());
        };
        if self.removed.contains(document) {
            return Err(WriteError::semantic("write.registration_reference",format!("Removing knowledge registration for {document} would orphan an immutable native reference in {source}")).at(field));
        }
        let Some(old) = self.old_headings.get(document) else {
            return Ok(());
        };
        let existed = match selector {
            EntrySelector::Id { id } => old
                .iter()
                .any(|path| path.last().is_some_and(|heading| heading_id(heading) == id)),
            EntrySelector::Document { heading, entry, .. } => {
                if let Some(id) = entry {
                    old.iter()
                        .any(|path| path.last().is_some_and(|heading| heading_id(heading) == id))
                } else {
                    !heading.is_empty() && old.iter().any(|path| path.ends_with(heading))
                }
            }
        };
        if existed && resolve_audited_selector(self.working, selector).is_err() {
            return Err(WriteError::semantic("write.heading_reference",format!("Changing {document} would orphan an existing native heading reference in {source}")).at(field));
        }
        Ok(())
    }

    fn locator(&self, locator: &str, source: &str, field: &str) -> Result<(), WriteError> {
        let implicit = field == "concepts" && locator_parts(locator).is_none();
        let (document, identity, numbered) = if implicit {
            ("logic/concepts.md", Some(locator), false)
        } else {
            let Some(parts) = locator_parts(locator) else {
                return Ok(());
            };
            parts
        };
        if self.removed.contains(document) {
            return Err(WriteError::semantic("write.registration_reference",format!("Removing knowledge registration for {document} would orphan an immutable native reference in {source}")).at(field));
        }
        let Some(identity) = identity else {
            return Ok(());
        };
        let Some(old) = self.old_headings.get(document) else {
            return Ok(());
        };
        let selector = if numbered
            && native_document_prefix(document)
                .is_some_and(|prefix| fields::typed_id(identity, prefix))
        {
            EntrySelector::Document {
                document: document.into(),
                heading: Vec::new(),
                entry: Some(identity.into()),
            }
        } else {
            let mut matches = old.iter().filter(|path| {
                path.last().is_some_and(|heading| heading == identity)
                    || joined_heading_path(path, identity)
            });
            let Some(path) = matches.next() else {
                return Ok(());
            };
            if matches.next().is_some() {
                return Ok(());
            }
            EntrySelector::Document {
                document: document.into(),
                heading: path.clone(),
                entry: None,
            }
        };
        self.selector(&selector, source, field)
    }
}

fn audit_removed_locator<'a>(
    node: &'a super::source::YamlNode,
    audit: &ReferenceAudit<'_>,
    source: &str,
    field: &str,
    anchors: &std::collections::BTreeMap<usize, &'a super::source::YamlNode>,
    seen: &mut BTreeSet<usize>,
) -> Result<(), WriteError> {
    use super::source::YamlKind;
    let mut pending = Vec::new();
    let mut first = Some(node);
    while let Some(node) = first.take().or_else(|| pending.pop()) {
        let node = registry_node(node, anchors);
        if !seen.insert(node.start) {
            continue;
        }
        match &node.kind {
            YamlKind::Scalar { .. } => {
                if let Some(locator) = node.scalar() {
                    audit.locator(locator, source, field)?;
                }
            }
            YamlKind::Sequence(items) => pending.extend(items.iter()),
            YamlKind::Mapping(entries) => {
                if let Some(selector) = selector_from_yaml(node, anchors) {
                    audit.selector(&selector, source, field)?;
                    continue;
                }
                for (key, value) in entries {
                    if key.scalar().is_some_and(|key| {
                        matches!(key, "document" | "id" | "entry" | "target" | "pointer")
                    }) {
                        pending.push(value);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn registry_yaml_fields<'a, 'n>(
    node: &'a super::source::YamlNode,
    name: &'n str,
    anchors: &std::collections::BTreeMap<usize, &'a super::source::YamlNode>,
) -> impl Iterator<Item = &'a super::source::YamlNode> + 'n
where
    'a: 'n,
{
    let node = registry_node(node, anchors);
    let entries = match &node.kind {
        super::source::YamlKind::Mapping(entries) => entries.as_slice(),
        _ => &[],
    };
    entries
        .iter()
        .filter_map(move |(key, value)| (key.scalar() == Some(name)).then_some(value))
}

fn audit_registry_rows<'a>(
    root: &'a super::source::YamlNode,
    key: &str,
    fields: &[&str],
    audit: &ReferenceAudit<'_>,
    source: &str,
    anchors: &std::collections::BTreeMap<usize, &'a super::source::YamlNode>,
    seen: &mut BTreeSet<usize>,
) -> Result<(), WriteError> {
    for collection in registry_yaml_fields(root, key, anchors) {
        let collection = registry_node(collection, anchors);
        let super::source::YamlKind::Sequence(rows) = &collection.kind else {
            continue;
        };
        for (index, row) in rows.iter().enumerate() {
            for field in fields {
                if key == "logic_revisions"
                    && *field == "entry"
                    && registry_yaml_fields(row, "field", anchors)
                        .next()
                        .and_then(super::source::YamlNode::scalar)
                        == Some("entry")
                {
                    let get = |name| {
                        registry_yaml_fields(row, name, anchors)
                            .next()
                            .map(|node| registry_node(node, anchors))
                    };
                    if let (Some(entry), Some(turn), Some(session)) = (
                        get("entry"),
                        get("turn")
                            .and_then(super::source::YamlNode::scalar)
                            .and_then(|value| value.parse::<u64>().ok()),
                        source
                            .strip_prefix("trace/sessions/")
                            .and_then(|path| path.strip_suffix(".yaml")),
                    ) {
                        let entry = entry.scalar().map(|value| json!(value)).or_else(|| {
                            selector_from_yaml(entry, anchors)
                                .and_then(|selector| serde_json::to_value(selector).ok())
                        });
                        if let Some(entry) = entry {
                            let record = json!({"entry":entry,"field":"entry","before":get("before").and_then(super::source::YamlNode::scalar),"after":get("after").and_then(super::source::YamlNode::scalar),"signal":get("signal").and_then(super::source::YamlNode::scalar),"provenance":get("provenance").and_then(super::source::YamlNode::scalar)});
                            if validate_revision_entry(audit.working, session, turn, &record)
                                .is_ok()
                            {
                                continue;
                            }
                        }
                    }
                }
                for value in registry_yaml_fields(row, field, anchors) {
                    audit_removed_locator(
                        value, audit, source,
                        // Registry addresses never denote the implicit node concepts field.
                        "", anchors, seen,
                    )
                    .map_err(|error| error.at(format!("{key}[{index}].{field}")))?;
                }
            }
        }
    }
    Ok(())
}

fn audit_registry_tree<'a>(
    nodes: &'a super::source::YamlNode,
    audit: &ReferenceAudit<'_>,
    source: &str,
    anchors: &std::collections::BTreeMap<usize, &'a super::source::YamlNode>,
    seen: &mut BTreeSet<usize>,
    trees: &mut BTreeSet<usize>,
) -> Result<(), WriteError> {
    let mut pending = vec![nodes];
    while let Some(nodes) = pending.pop() {
        let nodes = registry_node(nodes, anchors);
        if !trees.insert(nodes.start) {
            continue;
        }
        let super::source::YamlKind::Sequence(nodes) = &nodes.kind else {
            continue;
        };
        for node in nodes {
            for field in ["concepts", "source_refs", "artifacts", "target", "entry"] {
                for value in registry_yaml_fields(node, field, anchors) {
                    audit_removed_locator(value, audit, source, field, anchors, seen)?;
                }
            }
            pending.extend(registry_yaml_fields(node, "children", anchors));
        }
    }
    Ok(())
}

/// Registration changes do not rewrite history. Audit only native pointer
/// fields, not prose, physical files_changed records, or redirect source names.
fn validate_registry_removals(working: &WorkingArtifact) -> Result<(), WriteError> {
    let before = working
        .base
        .files
        .get("PAPER.md")
        .filter(|file| file.existed)
        .map(|file| {
            std::str::from_utf8(&file.bytes)
                .map_err(|e| WriteError::semantic("write.encoding", e.to_string()))
        })
        .transpose()?
        .map(crate::knowledge_paths)
        .transpose()
        .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?
        .unwrap_or_default();
    let after = if working.exists("PAPER.md") {
        crate::knowledge_paths(working.text("PAPER.md")?)
            .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?
    } else {
        Vec::new()
    };
    let mut removed = BTreeSet::new();
    for document in before {
        if !after.contains(&document)
            && !super::documents::allowed(&document)
            && document != "PAPER.md"
        {
            removed.insert(document);
        }
    }
    let mut old_headings = std::collections::BTreeMap::new();
    for path in working.files.keys() {
        if path != "PAPER.md"
            && !super::documents::allowed(path)
            && !removed.contains(path)
            && !working.is_allowed_document(path)?
        {
            continue;
        }
        let Some(file) = working.base.files.get(path).filter(|file| file.existed) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        if working.text(path).ok() == Some(text) {
            continue;
        }
        old_headings.insert(
            path.clone(),
            markdown::headings(text)
                .into_iter()
                .map(|heading| heading.path.iter().map(|part| part.to_string()).collect())
                .collect(),
        );
    }
    if removed.is_empty() && old_headings.is_empty() {
        return Ok(());
    }
    let audit = ReferenceAudit {
        working,
        removed,
        old_headings,
    };
    for source in working.paths() {
        let session = source.starts_with("trace/sessions/")
            && source.ends_with(".yaml")
            && source != super::sessions::INDEX;
        if !session
            && !matches!(
                source.as_str(),
                "trace/exploration_tree.yaml"
                    | "staging/observations.yaml"
                    | "trace/taste_log.yaml"
                    | "trace/aliases.yaml"
                    | "trace/logic_mutations.yaml"
            )
        {
            continue;
        }
        // Already malformed legacy YAML was not a readable native reference
        // before this change; registration edits do not normalize that history.
        let Ok(document) = working.yaml(&source) else {
            continue;
        };
        let mut anchors = std::collections::BTreeMap::new();
        registry_anchors(&document.root, &mut anchors);
        let mut seen = BTreeSet::new();
        let mut trees = BTreeSet::new();
        if !matches!(&document.root.kind, super::source::YamlKind::Mapping(_)) {
            continue;
        }
        if session {
            for key in ["events_logged", "events", "logic_revisions"] {
                audit_registry_rows(
                    &document.root,
                    key,
                    &["id", "entry", "target"],
                    &audit,
                    &source,
                    &anchors,
                    &mut seen,
                )?;
            }
        } else {
            match source.as_str() {
                "trace/exploration_tree.yaml" => {
                    for tree in registry_yaml_fields(&document.root, "tree", &anchors) {
                        audit_registry_tree(
                            tree, &audit, &source, &anchors, &mut seen, &mut trees,
                        )?;
                    }
                }
                "staging/observations.yaml" => audit_registry_rows(
                    &document.root,
                    "observations",
                    &["promoted_to"],
                    &audit,
                    &source,
                    &anchors,
                    &mut seen,
                )?,
                "trace/taste_log.yaml" => audit_registry_rows(
                    &document.root,
                    "entries",
                    &["target"],
                    &audit,
                    &source,
                    &anchors,
                    &mut seen,
                )?,
                "trace/aliases.yaml" => audit_registry_rows(
                    &document.root,
                    "aliases",
                    &["target"],
                    &audit,
                    &source,
                    &anchors,
                    &mut seen,
                )?,
                "trace/logic_mutations.yaml" => audit_registry_rows(
                    &document.root,
                    "mutations",
                    &["to"],
                    &audit,
                    &source,
                    &anchors,
                    &mut seen,
                )?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn mutation_source_key(
    row: &super::source::YamlNode,
    anchors: &std::collections::BTreeMap<usize, &super::source::YamlNode>,
) -> Result<Option<String>, WriteError> {
    let Some(from) = row.get("from")?.and_then(super::source::YamlNode::scalar) else {
        return Ok(None);
    };
    let numbered = locator_parts(from).and_then(|(document, identity, numbered)| {
        identity.filter(|id| {
            numbered
                && native_document_prefix(document)
                    .is_some_and(|prefix| fields::typed_id(id, prefix))
        })
    });
    let selector = if let Some(id) = numbered {
        Some(EntrySelector::Id { id: id.into() })
    } else {
        row.get("from_selector")?
            .and_then(|node| selector_from_yaml(node, anchors))
    };
    Ok(Some(if let Some(selector) = selector {
        format!(
            "typed:{}",
            serde_json::to_string(&selector).map_err(|error| WriteError::io(error.to_string()))?
        )
    } else {
        format!("legacy:{from}")
    }))
}

fn validate_retired_origins(working: &WorkingArtifact) -> Result<(), WriteError> {
    if !working.exists("trace/logic_mutations.yaml") {
        return Ok(());
    }
    let mut prior = BTreeSet::new();
    if let Some(file) = working
        .base
        .files
        .get("trace/logic_mutations.yaml")
        .filter(|file| file.existed)
        && let Ok(text) = std::str::from_utf8(&file.bytes)
    {
        let ledger = working.indexed_yaml("trace/logic_mutations.yaml", text, true)?;
        let mut anchors = std::collections::BTreeMap::new();
        registry_anchors(&ledger.root, &mut anchors);
        if let Some(rows) = ledger.root.get("mutations")? {
            for row in rows.sequence()? {
                if let Some(key) = mutation_source_key(row, &anchors)? {
                    prior.insert(key);
                }
            }
        }
    }
    let ledger = working.yaml("trace/logic_mutations.yaml")?;
    let mut anchors = std::collections::BTreeMap::new();
    registry_anchors(&ledger.root, &mut anchors);
    for rows in registry_yaml_fields(&ledger.root, "mutations", &anchors) {
        let rows = registry_node(rows, &anchors);
        let super::source::YamlKind::Sequence(rows) = &rows.kind else {
            continue;
        };
        for row in rows {
            let get = |name| {
                registry_yaml_fields(row, name, &anchors)
                    .next()
                    .map(|node| registry_node(node, &anchors))
            };
            let Some(from) = get("from").and_then(super::source::YamlNode::scalar) else {
                continue;
            };
            let exact = get("from_selector").and_then(|node| selector_from_yaml(node, &anchors));
            let parsed = locator_parts(from);
            let (document, identity, numbered) = if let Some(EntrySelector::Document {
                document,
                heading,
                entry,
            }) = &exact
            {
                let numbered = parsed.is_some_and(|(path, identity, numbered)| {
                    numbered
                        && native_document_prefix(path).is_some_and(|prefix| {
                            identity.is_some_and(|id| fields::typed_id(id, prefix))
                        })
                });
                (
                    document.as_str(),
                    if numbered {
                        parsed.and_then(|(_, identity, _)| identity)
                    } else {
                        entry
                            .as_deref()
                            .or_else(|| heading.last().map(String::as_str))
                    },
                    numbered,
                )
            } else if let Some(parsed) = parsed {
                parsed
            } else {
                continue;
            };
            let Some(identity) = identity else {
                continue;
            };
            if !working.is_allowed_document(document)? || !working.exists(document) {
                continue;
            }
            let headings = working.headings(document)?;
            let live = if numbered
                && native_document_prefix(document)
                    .is_some_and(|prefix| fields::typed_id(identity, prefix))
            {
                headings
                    .iter()
                    .any(|heading| heading_id(&heading.heading) == identity)
            } else if let Some(selector) = &exact {
                headings.iter().any(|heading| match selector {
                    EntrySelector::Document {
                        document: source,
                        heading: path,
                        entry: None,
                    } => source == document && heading.path == *path,
                    _ => false,
                })
            } else {
                headings.iter().any(|heading| {
                    joined_heading_path(&heading.path, identity) || heading.heading == identity
                })
            };
            if !live {
                continue;
            }
            let was_live =
                if mutation_source_key(row, &anchors)?.is_some_and(|key| prior.contains(&key)) {
                    working
                        .base
                        .files
                        .get(document)
                        .filter(|file| file.existed)
                        .and_then(|file| std::str::from_utf8(&file.bytes).ok())
                        .is_some_and(|text| {
                            markdown::headings(text).iter().any(|heading| {
                                if numbered
                                    && native_document_prefix(document)
                                        .is_some_and(|prefix| fields::typed_id(identity, prefix))
                                {
                                    heading_id(heading.heading) == identity
                                } else if let Some(EntrySelector::Document {
                                    document: source,
                                    heading: path,
                                    entry: None,
                                }) = &exact
                                {
                                    source == document
                                        && heading
                                            .path
                                            .iter()
                                            .copied()
                                            .eq(path.iter().map(String::as_str))
                                } else {
                                    joined_heading_path(&heading.path, identity)
                                        || heading.heading == identity
                                }
                            })
                        })
                } else {
                    false
                };
            if !was_live {
                return Err(WriteError::semantic(
                    "write.retired_identity",
                    format!("Mutable content cannot resurrect retired identity {from}"),
                ));
            }
        }
    }
    Ok(())
}

fn native_id_document(id: &str) -> Option<&'static str> {
    [
        ("C", "logic/claims.md"),
        ("H", "logic/solution/heuristics.md"),
        ("E", "logic/experiments.md"),
        ("RW", "logic/related_work.md"),
    ]
    .into_iter()
    .find_map(|(prefix, document)| fields::typed_id(id, prefix).then_some(document))
}

fn joined_heading_path<S: AsRef<str>>(path: &[S], text: &str) -> bool {
    let mut offset = 0;
    for (index, part) in path.iter().enumerate() {
        if index > 0 {
            if text.as_bytes().get(offset) != Some(&b'/') {
                return false;
            }
            offset += 1;
        }
        let part = part.as_ref();
        if !text[offset..].starts_with(part) {
            return false;
        }
        offset += part.len();
    }
    offset == text.len()
}

fn locator_parts(reference: &str) -> Option<(&str, Option<&str>, bool)> {
    if let Some(document) = native_id_document(reference) {
        return Some((document, Some(reference), true));
    }
    if let Some((document, identity)) = reference.split_once('#') {
        return document
            .ends_with(".md")
            .then_some((document, Some(identity), false));
    }
    if let Some((document, identity)) = reference.split_once(':') {
        return document
            .ends_with(".md")
            .then_some((document, Some(identity), true));
    }
    reference
        .ends_with(".md")
        .then_some((reference, None, false))
}

fn selector_from_yaml(
    node: &super::source::YamlNode,
    anchors: &std::collections::BTreeMap<usize, &super::source::YamlNode>,
) -> Option<EntrySelector> {
    let node = registry_node(node, anchors);
    let super::source::YamlKind::Mapping(entries) = &node.kind else {
        return None;
    };
    if entries.iter().any(|(key, _)| {
        !key.scalar()
            .is_some_and(|key| matches!(key, "id" | "document" | "heading" | "entry"))
    }) {
        return None;
    }
    let get = |name| {
        registry_yaml_fields(node, name, anchors)
            .next()
            .map(|value| registry_node(value, anchors))
    };
    if let Some(id) = get("id") {
        return if entries.len() == 1 {
            Some(EntrySelector::Id {
                id: id.scalar()?.into(),
            })
        } else {
            None
        };
    }
    let document = get("document")?.scalar()?.to_owned();
    let heading = if let Some(value) = get("heading") {
        let super::source::YamlKind::Sequence(items) = &value.kind else {
            return None;
        };
        items
            .iter()
            .map(|item| registry_node(item, anchors).scalar().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let entry=get("entry").and_then(|value|if matches!(&value.kind,super::source::YamlKind::Scalar{plain:true,value,..} if matches!(value.as_str(),"null"|"Null"|"NULL"|"~"|"")){None}else{value.scalar().map(str::to_owned)});
    if !heading.is_empty() && entry.is_some() {
        return None;
    }
    Some(EntrySelector::Document {
        document,
        heading,
        entry,
    })
}

fn direct_selector(working: &WorkingArtifact, selector: &EntrySelector) -> Result<(), WriteError> {
    match selector {
        EntrySelector::Id { id } => {
            if fields::typed_id(id, "N") || fields::typed_id(id, "O") {
                return super::sessions::require_entry_reference(working, id);
            }
            let document = native_id_document(id).ok_or_else(|| {
                WriteError::semantic("write.reference", "Unknown native reference namespace")
            })?;
            if !working.exists(document) {
                return Err(WriteError::semantic(
                    "write.reference",
                    "Reference document is missing",
                ));
            }
            let headings = working.headings(document)?;
            if headings
                .iter()
                .filter(|heading| heading_id(&heading.heading) == id)
                .count()
                != 1
            {
                return Err(WriteError::semantic(
                    "write.reference",
                    "Reference ID is missing or ambiguous",
                ));
            }
            Ok(())
        }
        EntrySelector::Document {
            document,
            heading,
            entry,
        } => {
            if !working.is_allowed_document(document)?
                || !working.exists(document)
                || !heading.is_empty() && entry.is_some()
            {
                return Err(WriteError::semantic(
                    "write.reference",
                    "Invalid or unavailable native document selector",
                ));
            }
            if heading.is_empty() && entry.is_none() {
                return Ok(());
            }
            let headings = working.headings(document)?;
            let count = headings
                .iter()
                .filter(|h| match entry {
                    Some(id) => heading_id(&h.heading) == id,
                    None => h.path.ends_with(heading),
                })
                .count();
            if count != 1 {
                return Err(WriteError::semantic(
                    "write.reference",
                    "Native heading is missing or ambiguous",
                ));
            }
            Ok(())
        }
    }
}

fn selector_for_locator(
    working: &WorkingArtifact,
    reference: &str,
) -> Result<EntrySelector, WriteError> {
    if !reference.contains([':', '#', '/']) && native_id_document(reference).is_some() {
        return Ok(EntrySelector::Id {
            id: reference.into(),
        });
    }
    if fields::typed_id(reference, "N") || fields::typed_id(reference, "O") {
        return Ok(EntrySelector::Id {
            id: reference.into(),
        });
    }
    let (document, identity, numbered) = locator_parts(reference)
        .ok_or_else(|| WriteError::semantic("write.reference", "Unsupported native locator"))?;
    if !working.is_allowed_document(document)? {
        return Err(WriteError::semantic(
            "write.reference",
            "Native locator is outside registered knowledge",
        ));
    }
    let Some(identity) = identity else {
        return Ok(EntrySelector::Document {
            document: document.into(),
            heading: Vec::new(),
            entry: None,
        });
    };
    if identity.is_empty() {
        return Err(WriteError::semantic(
            "write.reference",
            "Native locator has an empty identity",
        ));
    }
    if numbered
        && native_document_prefix(document).is_some_and(|prefix| fields::typed_id(identity, prefix))
    {
        return Ok(EntrySelector::Document {
            document: document.into(),
            heading: Vec::new(),
            entry: Some(identity.into()),
        });
    }
    if working.exists(document) {
        let headings = working.headings(document)?;
        let mut found = headings
            .iter()
            .filter(|h| h.heading == identity || joined_heading_path(&h.path, identity));
        if let Some(heading) = found.next() {
            if found.next().is_some() {
                return Err(WriteError::semantic(
                    "write.reference",
                    "Native locator is ambiguous",
                ));
            }
            return Ok(EntrySelector::Document {
                document: document.into(),
                heading: heading.path.clone(),
                entry: None,
            });
        }
    }
    Ok(EntrySelector::Document {
        document: document.into(),
        heading: vec![identity.into()],
        entry: None,
    })
}

pub(crate) fn selector_corresponds(request: &EntrySelector, source: &EntrySelector) -> bool {
    match (request, source) {
        (
            EntrySelector::Id { id },
            EntrySelector::Document {
                document,
                heading,
                entry,
            },
        ) => {
            native_id_document(id) == Some(document.as_str())
                && (entry.as_deref() == Some(id)
                    || heading
                        .last()
                        .is_some_and(|heading| heading_id(heading) == id))
        }
        (
            EntrySelector::Document {
                document,
                heading,
                entry,
            },
            EntrySelector::Document {
                document: other,
                heading: full,
                entry: other_entry,
            },
        ) if document == other => {
            if let Some(id) = entry {
                return other_entry.as_ref() == Some(id)
                    || full.last().is_some_and(|heading| heading_id(heading) == id);
            }
            !heading.is_empty() && full.ends_with(heading)
        }
        _ => request == source,
    }
}

fn resolve_audited_selector(
    working: &WorkingArtifact,
    selector: &EntrySelector,
) -> Result<(), WriteError> {
    let mut current = selector.clone();
    let mut seen = BTreeSet::new();
    loop {
        if direct_selector(working, &current).is_ok() {
            return Ok(());
        }
        if let EntrySelector::Document { document, .. } = &current
            && !working.is_allowed_document(document)?
        {
            return Err(WriteError::semantic(
                "write.reference",
                "Retired locator cannot bypass the final knowledge registry",
            ));
        }
        if !seen.insert(serde_json::to_string(&current).map_err(|e| WriteError::io(e.to_string()))?)
        {
            return Err(WriteError::semantic(
                "write.redirect_cycle",
                "Native redirect cycle",
            ));
        }
        if !working.exists("trace/logic_mutations.yaml") {
            return Err(WriteError::semantic(
                "write.reference",
                "Native entry does not exist",
            ));
        }
        let ledger = working.yaml("trace/logic_mutations.yaml")?;
        let mut anchors = std::collections::BTreeMap::new();
        registry_anchors(&ledger.root, &mut anchors);
        let mut next = None;
        for collection in registry_yaml_fields(&ledger.root, "mutations", &anchors) {
            let collection = registry_node(collection, &anchors);
            let super::source::YamlKind::Sequence(rows) = &collection.kind else {
                continue;
            };
            for row in rows {
                let get = |name| {
                    registry_yaml_fields(row, name, &anchors)
                        .next()
                        .map(|node| registry_node(node, &anchors))
                };
                let from = get("from_selector").and_then(|node| selector_from_yaml(node, &anchors));
                let matched = from
                    .as_ref()
                    .is_some_and(|from| selector_corresponds(&current, from));
                let prefix = match (&current, &from) {
                    (
                        EntrySelector::Document {
                            document,
                            heading,
                            entry: None,
                        },
                        Some(EntrySelector::Document {
                            document: other,
                            heading: old,
                            entry: None,
                        }),
                    ) if document == other && !old.is_empty() && heading.starts_with(old) => {
                        Some(&heading[old.len()..])
                    }
                    _ => None,
                };
                if !matched && prefix.is_none() {
                    if from.is_some() {
                        continue;
                    }
                    let Some(old) = get("from").and_then(super::source::YamlNode::scalar) else {
                        continue;
                    };
                    let Ok(legacy) = selector_for_locator(working, old) else {
                        continue;
                    };
                    if current != legacy {
                        continue;
                    }
                }
                if !authenticated_mutation(working, row, &anchors)? {
                    return Err(WriteError::semantic(
                        "write.redirect",
                        "Native redirect lacks its exact owning source revision",
                    ));
                }
                let Some(target) = get("to")
                    .and_then(super::source::YamlNode::scalar)
                    .filter(|target| !matches!(*target, "null" | "~" | ""))
                else {
                    continue;
                };
                let mut target = get("to_selector")
                    .and_then(|node| selector_from_yaml(node, &anchors))
                    .map(Ok)
                    .unwrap_or_else(|| selector_for_locator(working, target))?;
                if let (
                    Some(suffix),
                    EntrySelector::Document {
                        heading,
                        entry: None,
                        ..
                    },
                ) = (prefix, &mut target)
                {
                    heading.extend_from_slice(suffix);
                }
                if next.as_ref().is_some_and(|old| old != &target) {
                    return Err(WriteError::semantic(
                        "write.redirect_ambiguous",
                        "Conflicting native redirect selectors",
                    ));
                }
                next = Some(target);
            }
        }
        current = next.ok_or_else(|| {
            WriteError::semantic(
                "write.reference",
                "Native entry is missing without a live audited redirect",
            )
        })?;
    }
}

pub fn validate_revision_entry(
    working: &WorkingArtifact,
    session: &str,
    turn: u64,
    record: &Value,
) -> Result<(), WriteError> {
    if crate::merge::authenticates_resolution_audit(working, session, turn, record)
        .map_err(|error| WriteError::semantic(&error.code, error.message))?
    {
        return Ok(());
    }
    let entry = record
        .get("entry")
        .ok_or_else(|| WriteError::semantic("write.reference", "Revision entry is required"))?;
    let selector = if let Some(reference) = entry.as_str() {
        selector_for_locator(working, reference)?
    } else {
        serde::Deserialize::deserialize(entry).map_err(|e| {
            WriteError::semantic("write.reference", format!("Invalid revision selector: {e}"))
        })?
    };
    if let EntrySelector::Document {
        document,
        heading,
        entry,
    } = &selector
        && (!working.is_allowed_document(document)? || !heading.is_empty() && entry.is_some())
    {
        return Err(WriteError::semantic(
            "write.reference",
            "Revision selector is outside the final registry or combines heading and entry",
        ));
    }
    if direct_selector(working, &selector).is_ok() {
        return Ok(());
    }
    if record.get("field").and_then(Value::as_str) != Some("entry") {
        return resolve_audited_selector(working, &selector);
    }
    if working.exists("trace/logic_mutations.yaml") {
        let ledger = working.yaml("trace/logic_mutations.yaml")?;
        let mut anchors = std::collections::BTreeMap::new();
        registry_anchors(&ledger.root, &mut anchors);
        for collection in registry_yaml_fields(&ledger.root, "mutations", &anchors) {
            let collection = registry_node(collection, &anchors);
            let super::source::YamlKind::Sequence(rows) = &collection.kind else {
                continue;
            };
            for row in rows {
                let get = |name| {
                    registry_yaml_fields(row, name, &anchors)
                        .next()
                        .map(|node| registry_node(node, &anchors))
                };
                if get("session").and_then(super::source::YamlNode::scalar) != Some(session)
                    || get("turn")
                        .and_then(super::source::YamlNode::scalar)
                        .and_then(|s| s.parse::<u64>().ok())
                        != Some(turn)
                {
                    continue;
                }
                if ["before", "after", "signal", "provenance"]
                    .iter()
                    .any(|field| {
                        get(field).and_then(super::source::YamlNode::scalar)
                            != record.get(*field).and_then(Value::as_str)
                    })
                {
                    continue;
                }
                let from = get("from_selector")
                    .and_then(|node| selector_from_yaml(node, &anchors))
                    .or_else(|| {
                        get("from")
                            .and_then(super::source::YamlNode::scalar)
                            .and_then(|value| selector_for_locator(working, value).ok())
                    });
                if !from
                    .as_ref()
                    .is_some_and(|source| selector_corresponds(&selector, source))
                {
                    continue;
                }
                if get("action").and_then(super::source::YamlNode::scalar)==Some("remove")&&record.get("after").and_then(Value::as_str)==Some("")&&get("to").is_some_and(|node|matches!(&node.kind,super::source::YamlKind::Scalar{plain:true,value,..} if matches!(value.as_str(),"null"|"~"|""))){return Ok(());}
                return resolve_audited_selector(working, &selector);
            }
        }
    }
    Err(WriteError::semantic(
        "write.reference",
        "Retired structural revision lacks its exact native session/turn/source audit",
    ))
}

fn canonical_claim_locator(value: &str) -> Option<&str> {
    let id = value.strip_prefix("logic/claims.md:")?;
    fields::typed_id(id, "C").then_some(id)
}

/// Present-state body edits cannot erase canonical claims. The only identity
/// replacement is a complete rename archive authenticated by its owning turn.
fn validate_claim_retention(working: &WorkingArtifact) -> Result<(), WriteError> {
    const CLAIMS: &str = "logic/claims.md";
    if !working.changed_paths().iter().any(|path| path == CLAIMS) {
        return Ok(());
    }
    let Some(base) = working.base.files.get(CLAIMS).filter(|file| file.existed) else {
        return Ok(());
    };
    let before = std::str::from_utf8(&base.bytes)
        .map_err(|error| WriteError::semantic("write.encoding", error.to_string()))?;
    let after = if working.exists(CLAIMS) {
        working.text(CLAIMS)?
    } else {
        ""
    };
    // Recovery-aware on both sides: any claim reads list must stay retained.
    let current: BTreeSet<_> = markdown::document_headings(CLAIMS, after)
        .into_iter()
        .map(|heading| heading_id(heading.heading))
        .filter(|id| fields::typed_id(id, "C"))
        .collect();
    let removed: BTreeSet<_> = markdown::document_headings(CLAIMS, before)
        .into_iter()
        .map(|heading| heading_id(heading.heading))
        .filter(|id| fields::typed_id(id, "C") && !current.contains(id))
        .collect();
    if removed.is_empty() {
        return Ok(());
    }
    let imported = super::intent::imports_history(working, CLAIMS)?;
    let redirects = claim_redirects_from_source(working)?;
    if !working.exists("trace/logic_mutations.yaml") {
        return Err(WriteError::semantic(
            "write.claim_retention",
            "Canonical claims require retained entries or full audited canonical renames",
        ));
    }
    let ledger = working.yaml("trace/logic_mutations.yaml")?;
    let rows = ledger
        .root
        .get("mutations")?
        .ok_or_else(|| {
            WriteError::semantic("write.claim_retention", "Missing canonical rename archive")
        })?
        .sequence()?;
    for id in removed {
        let mut proven = false;
        for row in rows {
            if row
                .get("from")?
                .and_then(super::source::YamlNode::scalar)
                .and_then(canonical_claim_locator)
                != Some(id)
                || row.get("action")?.and_then(super::source::YamlNode::scalar) != Some("rename")
            {
                continue;
            }
            let Some(to) = row
                .get("to")?
                .and_then(super::source::YamlNode::scalar)
                .and_then(canonical_claim_locator)
            else {
                continue;
            };
            let Some(session) = row
                .get("session")?
                .and_then(super::source::YamlNode::scalar)
            else {
                continue;
            };
            let Some(turn) = row
                .get("turn")?
                .and_then(super::source::YamlNode::scalar)
                .and_then(|value| value.parse::<u64>().ok())
            else {
                continue;
            };
            if (!imported && !working.owned_turns.contains_key(&(session.into(), turn)))
                || !redirects.contains_key(id)
            {
                continue;
            }
            let archived_before = row
                .get("before")?
                .and_then(super::source::YamlNode::scalar)
                .unwrap_or("");
            let archived_after = row
                .get("after")?
                .and_then(super::source::YamlNode::scalar)
                .unwrap_or("");
            if markdown::headings(archived_before)
                .first()
                .is_some_and(|heading| {
                    heading.range.start == 0 && heading_id(heading.heading) == id
                })
                && markdown::headings(archived_after)
                    .first()
                    .is_some_and(|heading| {
                        heading.range.start == 0 && heading_id(heading.heading) == to
                    })
            {
                proven = true;
                break;
            }
        }
        if !proven {
            return Err(WriteError::semantic(
                "write.claim_retention",
                format!(
                    "Canonical claim {id} cannot be physically removed; retain it or author its full audited rename"
                ),
            ));
        }
    }
    Ok(())
}

fn claim_redirects_with<'a>(
    mut source: impl FnMut(&str) -> Option<&'a str>,
    mut parse: impl FnMut(&str, &str) -> Result<std::sync::Arc<super::source::YamlDocument>, WriteError>,
) -> Result<std::collections::BTreeMap<String, String>, WriteError> {
    let Some(text) = source("trace/logic_mutations.yaml") else {
        return Ok(std::collections::BTreeMap::new());
    };
    let ledger = parse("trace/logic_mutations.yaml", text)?;
    let claims = source("logic/claims.md").unwrap_or("");
    let mut live = std::collections::BTreeMap::<&str, usize>::new();
    for section in markdown::sections(claims) {
        let id = heading_id(section.heading);
        if fields::typed_id(id, "C") {
            *live.entry(id).or_default() += 1;
        }
    }
    let mut edges = std::collections::BTreeMap::<String, String>::new();
    let rows = ledger
        .root
        .get("mutations")?
        .ok_or_else(|| {
            WriteError::semantic("write.redirect", "Mutation ledger requires mutations")
        })?
        .sequence()?;
    let mut sessions = std::collections::BTreeMap::new();
    let mut ledger_anchors = std::collections::BTreeMap::new();
    registry_anchors(&ledger.root, &mut ledger_anchors);
    for row in rows {
        let Some(from) = row
            .get("from")?
            .and_then(super::source::YamlNode::scalar)
            .and_then(canonical_claim_locator)
        else {
            continue;
        };
        let destination = row.get("to")?.ok_or_else(|| {
            WriteError::semantic("write.redirect", "Claim mutation destination is missing")
        })?;
        let to = if matches!(&destination.kind,super::source::YamlKind::Scalar{value,plain:true} if value=="null"||value=="~")
        {
            None
        } else {
            Some(
                destination
                    .scalar()
                    .and_then(canonical_claim_locator)
                    .ok_or_else(|| {
                        WriteError::semantic(
                            "write.redirect",
                            "Canonical claim redirects must stay in the claims namespace",
                        )
                    })?,
            )
        };
        let scalar = |field: &str| {
            row.get(field)?
                .and_then(super::source::YamlNode::scalar)
                .ok_or_else(|| {
                    WriteError::semantic(
                        "write.redirect",
                        format!("Claim redirect requires {field}"),
                    )
                })
        };
        let session = scalar("session")?;
        let turn = scalar("turn")?.parse::<u64>().map_err(|_| {
            WriteError::semantic(
                "write.redirect",
                "Claim redirect turn must be positive integer",
            )
        })?;
        revision_context(session, turn, scalar("signal")?, scalar("provenance")?)?;
        if scalar("action")? != "rename" && scalar("action")? != "remove" {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim redirect action must be rename/remove",
            ));
        }
        let before = scalar("before")?;
        let after = scalar("after")?;
        if before.is_empty() {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim redirect must archive exact before source",
            ));
        }
        if !sessions.contains_key(session) {
            let path = format!("trace/sessions/{session}.yaml");
            let text = source(&path).ok_or_else(|| {
                WriteError::semantic("write.redirect", "Claim redirect owning session is missing")
            })?;
            sessions.insert(session, parse(&path, text)?);
        }
        let owner = &sessions[session];
        if owner
            .root
            .get("session")?
            .and_then(|node| node.get("id").ok().flatten())
            .and_then(super::source::YamlNode::scalar)
            != Some(session)
        {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim redirect session identity mismatch",
            ));
        }
        if owner
            .root
            .get("session")?
            .and_then(|node| node.get("turn_count").ok().flatten())
            .and_then(super::source::YamlNode::scalar)
            .and_then(|value| value.parse::<u64>().ok())
            .is_none_or(|count| count < turn)
        {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim redirect turn is not owned by its session",
            ));
        }
        let mut owner_anchors = std::collections::BTreeMap::new();
        registry_anchors(&owner.root, &mut owner_anchors);
        let source_selector = row
            .get("from_selector")?
            .and_then(|node| selector_from_yaml(node, &ledger_anchors))
            .unwrap_or_else(|| EntrySelector::Id { id: from.into() });
        if !selector_corresponds(&EntrySelector::Id { id: from.into() }, &source_selector) {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim source selector disagrees with its canonical origin",
            ));
        }
        let revisions = owner
            .root
            .get("logic_revisions")?
            .ok_or_else(|| {
                WriteError::semantic(
                    "write.redirect",
                    "Claim redirect needs owning source revision",
                )
            })?
            .sequence()?;
        let mut proven = false;
        for revision in revisions {
            if revision
                .get("turn")?
                .and_then(super::source::YamlNode::scalar)
                .and_then(|value| value.parse::<u64>().ok())
                != Some(turn)
                || revision
                    .get("field")?
                    .and_then(super::source::YamlNode::scalar)
                    != Some("entry")
            {
                continue;
            }
            if ["before", "after", "signal", "provenance"]
                .iter()
                .any(|field| {
                    revision
                        .get(field)
                        .ok()
                        .flatten()
                        .and_then(super::source::YamlNode::scalar)
                        != row
                            .get(field)
                            .ok()
                            .flatten()
                            .and_then(super::source::YamlNode::scalar)
                })
            {
                continue;
            }
            let Some(entry) = revision.get("entry")? else {
                continue;
            };
            let entry_matches = if let Some(reference) = entry.scalar() {
                reference == from || canonical_claim_locator(reference) == Some(from)
            } else {
                selector_from_yaml(entry, &owner_anchors)
                    .is_some_and(|selector| selector_corresponds(&selector, &source_selector))
            };
            if entry_matches {
                proven = true;
                break;
            }
        }
        if !proven {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim redirect lacks exact owning revision before/after/signal/provenance",
            ));
        }
        if scalar("action")? == "rename" && after.is_empty() {
            return Err(WriteError::semantic(
                "write.redirect",
                "Rename redirect must archive after source",
            ));
        }
        let Some(to) = to else {
            if scalar("action")? != "remove" || !after.is_empty() {
                return Err(WriteError::semantic(
                    "write.redirect",
                    "Null claim retirement requires an exact empty-after removal archive",
                ));
            }
            if live.contains_key(from) {
                return Err(WriteError::semantic(
                    "write.redirect",
                    "Retired claim identity is still live",
                ));
            }
            continue;
        };
        if from == to || live.contains_key(from) {
            return Err(WriteError::semantic(
                "write.redirect",
                "Retired claim redirect origin is still live or self-targeted",
            ));
        }
        if edges
            .insert(from.into(), to.into())
            .is_some_and(|old| old != to)
        {
            return Err(WriteError::semantic(
                "write.redirect",
                "Conflicting claim redirect destinations",
            ));
        }
    }
    let mut resolved = std::collections::BTreeMap::<&str, &str>::new();
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    for from in edges.keys() {
        if resolved.contains_key(from.as_str()) {
            continue;
        }
        chain.clear();
        seen.clear();
        let mut target = from.as_str();
        while let Some(next) = edges.get(target) {
            if let Some(current) = resolved.get(target) {
                target = current;
                break;
            }
            if !seen.insert(target) {
                return Err(WriteError::semantic(
                    "write.redirect_cycle",
                    "Claim redirect cycle",
                ));
            }
            chain.push(target);
            target = next;
        }
        if live.get(target) != Some(&1) {
            return Err(WriteError::semantic(
                "write.redirect",
                "Claim redirect terminal is missing or ambiguous",
            ));
        }
        for origin in chain.drain(..) {
            resolved.insert(origin, target);
        }
    }
    Ok(resolved
        .into_iter()
        .map(|(from, to)| (from.to_owned(), to.to_owned()))
        .collect())
}

pub fn claim_redirects_from_source(
    working: &WorkingArtifact,
) -> Result<std::collections::BTreeMap<String, String>, WriteError> {
    claim_redirects_with(
        |path| working.text(path).ok(),
        |path, text| working.indexed_yaml(path, text, false),
    )
}
pub fn claim_redirects_from_snapshot(
    snapshot: &super::source::ArtifactSnapshot,
) -> Result<std::collections::BTreeMap<String, String>, WriteError> {
    claim_redirects_with(
        |path| {
            snapshot
                .files
                .get(path)
                .filter(|file| file.existed)
                .and_then(|file| std::str::from_utf8(&file.bytes).ok())
        },
        |_, text| super::source::YamlDocument::parse(text).map(std::sync::Arc::new),
    )
}
pub(super) fn claim_redirects_from_base(
    working: &WorkingArtifact,
) -> Result<std::collections::BTreeMap<String, String>, WriteError> {
    claim_redirects_with(
        |path| {
            working
                .base
                .files
                .get(path)
                .filter(|file| file.existed)
                .and_then(|file| std::str::from_utf8(&file.bytes).ok())
        },
        |path, text| working.indexed_yaml(path, text, true),
    )
}
pub fn claim_redirects_from_sources(
    sources: &std::collections::BTreeMap<String, String>,
) -> Result<std::collections::BTreeMap<String, String>, WriteError> {
    claim_redirects_with(
        |path| sources.get(path).map(String::as_str),
        |_, text| super::source::YamlDocument::parse(text).map(std::sync::Arc::new),
    )
}

fn authenticated_mutation(
    working: &WorkingArtifact,
    row: &super::source::YamlNode,
    anchors: &std::collections::BTreeMap<usize, &super::source::YamlNode>,
) -> Result<bool, WriteError> {
    let get = |name| {
        registry_yaml_fields(row, name, anchors)
            .next()
            .map(|node| registry_node(node, anchors))
    };
    let Some(session) = get("session").and_then(super::source::YamlNode::scalar) else {
        return Ok(false);
    };
    let Some(turn) = get("turn")
        .and_then(super::source::YamlNode::scalar)
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Ok(false);
    };
    let (Some(signal), Some(provenance)) = (
        get("signal").and_then(super::source::YamlNode::scalar),
        get("provenance").and_then(super::source::YamlNode::scalar),
    ) else {
        return Ok(false);
    };
    revision_context(session, turn, signal, provenance)?;
    let (Some(before), Some(after), Some(action)) = (
        get("before").and_then(super::source::YamlNode::scalar),
        get("after").and_then(super::source::YamlNode::scalar),
        get("action").and_then(super::source::YamlNode::scalar),
    ) else {
        return Ok(false);
    };
    if before.is_empty()
        || !matches!(action, "rename" | "remove")
        || action == "rename" && after.is_empty()
    {
        return Ok(false);
    }
    let Some(from_text) = get("from").and_then(super::source::YamlNode::scalar) else {
        return Ok(false);
    };
    let from = get("from_selector")
        .and_then(|node| selector_from_yaml(node, anchors))
        .map(Ok)
        .unwrap_or_else(|| selector_for_locator(working, from_text))?;
    let path = format!("trace/sessions/{session}.yaml");
    if !working.exists(&path) {
        return Ok(false);
    }
    let owner = working.yaml(&path)?;
    let mut owner_anchors = std::collections::BTreeMap::new();
    registry_anchors(&owner.root, &mut owner_anchors);
    let Some(metadata) = registry_yaml_fields(&owner.root, "session", &owner_anchors).next() else {
        return Ok(false);
    };
    if registry_yaml_fields(metadata, "id", &owner_anchors)
        .next()
        .and_then(super::source::YamlNode::scalar)
        != Some(session)
        || registry_yaml_fields(metadata, "turn_count", &owner_anchors)
            .next()
            .and_then(super::source::YamlNode::scalar)
            .and_then(|value| value.parse::<u64>().ok())
            .is_none_or(|count| count < turn)
    {
        return Ok(false);
    }
    for rows in registry_yaml_fields(&owner.root, "logic_revisions", &owner_anchors) {
        let rows = registry_node(rows, &owner_anchors);
        let super::source::YamlKind::Sequence(rows) = &rows.kind else {
            continue;
        };
        for revision in rows {
            let value = |name| {
                registry_yaml_fields(revision, name, &owner_anchors)
                    .next()
                    .map(|node| registry_node(node, &owner_anchors))
            };
            if value("turn")
                .and_then(super::source::YamlNode::scalar)
                .and_then(|value| value.parse::<u64>().ok())
                != Some(turn)
                || value("field").and_then(super::source::YamlNode::scalar) != Some("entry")
            {
                continue;
            }
            if ["before", "after", "signal", "provenance"]
                .iter()
                .any(|field| {
                    value(field).and_then(super::source::YamlNode::scalar)
                        != get(field).and_then(super::source::YamlNode::scalar)
                })
            {
                continue;
            }
            let Some(entry) = value("entry") else {
                continue;
            };
            if entry.scalar() == Some(from_text) {
                return Ok(true);
            }
            let entry = entry
                .scalar()
                .and_then(|reference| selector_for_locator(working, reference).ok())
                .or_else(|| selector_from_yaml(entry, &owner_anchors));
            if entry.is_some_and(|entry| {
                selector_corresponds(&entry, &from)
                    || get("from_selector").is_none() && selector_corresponds(&from, &entry)
            }) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
