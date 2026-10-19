//! Citation repair for renames, merges, redirecting removals and splits
//! (plan 19 C1).
//!
//! The planners here reuse the working snapshot, the typed citation inventory
//! ([`super::citations`]) and the ordinary revision path: each changed citing
//! field gets one exact before/after `logic_revisions` row, a `Last revised`
//! pointer and, for claims, a claim touch. Historical records are never
//! edited. Their typed citations of a renamed or removed identity are checked
//! through the authenticated mutation ledger before commit.
use super::citations::{Citation, Produced, Resolver, Span, Spelling, Subject};
use super::{Entry, edit, field_value, heading_id, resolve, revise, revision_context};
use crate::markdown;
use crate::write::{
    EntrySelector, Fields, OperationResult, ReferenceEdit, WorkingArtifact, WriteError, fields,
    source::{self, PendingRevision},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::ops::Range;

const CLAIMS: &str = "logic/claims.md";

/// The audit context shared by every row one operation appends.
#[derive(Clone, Copy)]
pub(super) struct Audit<'a> {
    pub session: &'a str,
    pub turn: u64,
    pub signal: &'a str,
    pub provenance: &'a str,
}

/// Where citations of the subject move.
#[derive(Debug, Clone)]
pub(super) struct Destination {
    pub document: String,
    /// The destination's (new) full heading vector.
    pub path: Vec<String>,
    /// Its canonical ID, when native-numbered.
    pub id: Option<String>,
    /// Descendant citations keep their suffix below the destination (rename);
    /// otherwise every affected identity maps to the destination (removal).
    pub keeps_suffix: bool,
    /// Source start of a different destination entry (merge survivor, removal
    /// redirect); a citer there would cite itself.
    pub start: Option<usize>,
}

/// One planned citation repair, verified after the restructure.
#[derive(Debug, Clone)]
struct Respelled {
    text: String,
    spelling: Spelling,
    path: Vec<String>,
    location: Value,
}

/// Generated rewrites applied before the structural change of a rename or
/// redirecting removal.
#[derive(Debug, Default)]
pub(super) struct Rewrite {
    pub rewritten: Vec<Value>,
    pub history: Vec<Value>,
    pub produced: Vec<Produced>,
    respelled: Vec<Respelled>,
    destination: Option<Destination>,
}

fn refusal(code: &str, message: String, field: &str, locations: Vec<Value>) -> WriteError {
    WriteError::semantic(code, message)
        .at(field)
        .with_locations(locations)
}

/// The new spelling of one span at `destination`, or why it cannot be
/// represented in the same style.
fn respell(span: &Span, destination: &Destination) -> Result<(String, Vec<String>), String> {
    let mut path = destination.path.clone();
    if destination.keeps_suffix {
        path.extend(span.suffix.iter().cloned());
    }
    let tail = |segments: usize| {
        let segments = segments.min(path.len()).max(1);
        path[path.len().saturating_sub(segments)..].join("/")
    };
    let text = match &span.spelling {
        Spelling::Id => destination
            .id
            .clone()
            .ok_or("a bare ID citation cannot name an unnumbered destination")?,
        Spelling::QualifiedId { separator, .. } => format!(
            "{}{separator}{}",
            destination.document,
            destination
                .id
                .as_deref()
                .ok_or("an ID-qualified citation cannot name an unnumbered destination")?
        ),
        Spelling::QualifiedPath {
            separator,
            segments,
            ..
        } => format!("{}{separator}{}", destination.document, tail(*segments)),
        Spelling::Name { segments } => {
            if destination.document != "logic/concepts.md" {
                return Err(
                    "a bare concept name cannot name a destination outside logic/concepts.md"
                        .into(),
                );
            }
            tail(*segments)
        }
    };
    Ok((text, path))
}

/// Rewrite one citing field to `after` through the ordinary audit path.
fn revise_field_text(
    working: &mut WorkingArtifact,
    citation: &Citation,
    after: &str,
    audit: Audit<'_>,
) -> Result<Value, WriteError> {
    let selector = citation.selector();
    let location = citation.location();
    let stale = || {
        refusal(
            "write.reference_before",
            format!(
                "{} {} field {} changed during this operation",
                citation.document,
                citation.path.join(" / "),
                citation.field
            ),
            "references",
            vec![location.clone()],
        )
    };
    let entry = resolve(working, &selector)?;
    if entry.document != citation.document || entry.path != citation.path {
        return Err(stale());
    }
    let (range, inline, value_range, name) = {
        let text = working.text(&entry.document)?;
        let found: Vec<_> = markdown::fields(text, entry.field_body.clone())
            .into_iter()
            .filter(|f| fields::canonical(f.name) == fields::canonical(&citation.field))
            .collect();
        let [field] = found.as_slice() else {
            return Err(stale());
        };
        if markdown::decode_field(field) != citation.before {
            return Err(stale());
        }
        let inline = !field.raw_value.starts_with('\n');
        let mut range = field.range.clone();
        if inline {
            // Keep exactly one line ending after a re-rendered inline field.
            let raw = &text[range.clone()];
            let trimmed = raw.trim_end_matches(['\r', '\n']);
            let ending = &raw[trimmed.len()..];
            range.end = range.start
                + trimmed.len()
                + if ending.starts_with("\r\n") {
                    2
                } else {
                    usize::from(ending.starts_with('\n'))
                };
        }
        (
            range,
            inline,
            field.value_range.clone(),
            field.name.to_owned(),
        )
    };
    if citation.before == after {
        return Ok(Value::Null);
    }
    let reason = "logic.reference_rewrite";
    if inline && !after.is_empty() && !after.contains(['\n', '\r']) && after.trim() == after {
        working.edit(&entry.document, value_range, after, reason)?;
    } else {
        let rendered = fields::render(&name, &Value::String(after.to_owned()));
        working.edit(&entry.document, range, &rendered, reason)?;
    }
    let written = resolve(working, &selector)?;
    let text = working.text(&written.document)?;
    let exact = markdown::fields(text, written.field_body.clone())
        .iter()
        .filter(|f| fields::canonical(f.name) == fields::canonical(&citation.field))
        .map(|f| markdown::decode_field(f) == after)
        .collect::<Vec<_>>()
        == [true];
    if !exact {
        return Err(refusal(
            "write.reference_rewrite",
            format!(
                "{} {} field {} cannot be written exactly",
                citation.document,
                citation.path.join(" / "),
                citation.field
            ),
            "references",
            vec![location],
        ));
    }
    let pointer = Fields::from([(
        "Last revised".into(),
        Value::String(format!(
            "{} ({}#{})",
            audit.session.get(..10).unwrap_or(audit.session),
            audit.session,
            audit.turn
        )),
    )]);
    edit(working, &selector, &pointer, true)?;
    let record = json!({"entry":selector,"field":name,"before":citation.before,"after":after,"signal":audit.signal,"provenance":audit.provenance,"note":Value::Null});
    if citation.document == CLAIMS {
        working.bookkeeping.claim_changed(
            audit.session,
            audit.turn,
            heading_id(&citation.heading),
            [(name.as_str(), Some(after))],
        );
    }
    working.revisions.push(PendingRevision {
        session: audit.session.into(),
        turn: audit.turn,
        record: record.clone(),
    });
    Ok(
        json!({"document":citation.document,"entry":record["entry"],"field":name,"before":citation.before,"after":after}),
    )
}

/// Refuse when a citer is the destination itself: it would cite itself.
fn self_citations(citations: &[Citation], destination: &Destination) -> Result<(), WriteError> {
    let locations: Vec<Value> = citations
        .iter()
        .filter(|c| c.document == destination.document && Some(c.start) == destination.start)
        .map(Citation::location)
        .collect();
    if locations.is_empty() {
        return Ok(());
    }
    Err(refusal(
        "write.reference_rewrite",
        "the destination entry cites the restructured identity; repairing it would make the entry cite itself, so revise its content explicitly first".into(),
        "rewrite_references",
        locations,
    ))
}

/// One citing field's repair: the field, its new value with the ranges of
/// the new spellings, and the spellings to verify afterwards.
type Respelling = (Citation, (String, Vec<Range<usize>>), Vec<Respelled>);

/// Respell every span of every citation at `destination`. Spellings that
/// cannot be represented are collected, and all of their locations refuse
/// together.
fn plan_respellings(
    citations: &[Citation],
    destination: &Destination,
) -> Result<Vec<Respelling>, WriteError> {
    let mut planned = Vec::with_capacity(citations.len());
    let mut unrepresentable = Vec::new();
    for citation in citations {
        let mut spelled = Vec::new();
        let rewritten = citation.rewrite(|span| match respell(span, destination) {
            Ok((text, path)) => {
                spelled.push(Respelled {
                    text: text.clone(),
                    spelling: span.spelling.clone(),
                    path,
                    location: citation.location(),
                });
                Ok(text)
            }
            Err(reason) => {
                let mut location = citation.location();
                location["reason"] = json!(reason);
                unrepresentable.push(location);
                Ok(String::new())
            }
        })?;
        planned.push((citation.clone(), rewritten, spelled));
    }
    if !unrepresentable.is_empty() {
        return Err(refusal(
            "write.reference_rewrite",
            "a citation cannot be represented at the destination in its existing spelling; edit it explicitly".into(),
            "rewrite_references",
            unrepresentable,
        ));
    }
    Ok(planned)
}

/// Plan and apply the typed citation repairs of a rename or redirecting
/// removal, before its structural change.
pub(super) fn rewrite_structural(
    working: &mut WorkingArtifact,
    subject: &Subject,
    names: &[String],
    destination: Destination,
    removal: bool,
    audit: Audit<'_>,
) -> Result<Rewrite, WriteError> {
    let resolver = Resolver::new(working)?;
    let checks = super::history_refs::historical(&resolver, subject)?;
    let exclude = removal.then(|| subject.range.clone());
    let inventory = resolver
        .inventory(subject, names, exclude, &[])?
        .rewritable_only();
    drop(resolver);
    self_citations(&inventory.citations, &destination)?;
    let planned = plan_respellings(&inventory.citations, &destination)?;
    let operation = working.bookkeeping.current;
    let history: Vec<Value> = checks
        .iter()
        .map(|check| json!({"source":check.source,"field":check.field,"literal":check.literal}))
        .collect();
    working
        .citation_checks
        .extend(checks.into_iter().map(|mut check| {
            check.operation = operation;
            check
        }));
    let mut result = Rewrite {
        history,
        destination: Some(destination),
        ..Rewrite::default()
    };
    for (citation, (after, ranges), spelled) in planned {
        let row = revise_field_text(working, &citation, &after, audit)?;
        if !row.is_null() {
            result.rewritten.push(row);
        }
        result.produced.push(Produced {
            document: citation.document.clone(),
            path: citation.path.clone(),
            field: citation.field.clone(),
            ranges,
        });
        result.respelled.extend(spelled);
    }
    Ok(result)
}

/// The existing dangling-reference guard, computed from the typed inventory:
/// any remaining non-heading mention of an old identity refuses the
/// restructure with its locations. Returns the non-blocking mentions.
pub(super) fn guard(
    working: &WorkingArtifact,
    subject: &Subject,
    names: &[String],
    exclude: Option<Range<usize>>,
    produced: &[Produced],
) -> Result<Vec<Value>, WriteError> {
    let inventory = Resolver::new(working)?
        .inventory(subject, names, exclude, produced)?
        .rewritable_only();
    let mut blocking: Vec<Value> = inventory.citations.iter().map(Citation::location).collect();
    blocking.extend(
        inventory
            .mentions
            .iter()
            .filter(|mention| mention.blocking())
            .map(|mention| mention.to_json()),
    );
    if !blocking.is_empty() {
        return Err(refusal(
            "write.dangling_reference",
            format!(
                "{} current mention(s) of the old identity are outside parsed reference fields; edit them with an explicit audited operation first",
                blocking.len()
            ),
            "rewrite_references",
            blocking,
        ));
    }
    Ok(inventory
        .mentions
        .iter()
        .map(|mention| mention.to_json())
        .collect())
}

/// After the structural change, every new spelling must resolve to exactly
/// its destination.
pub(super) fn verify(working: &WorkingArtifact, rewrite: &Rewrite) -> Result<(), WriteError> {
    let Some(destination) = &rewrite.destination else {
        return Ok(());
    };
    verify_spellings(working, &rewrite.respelled, &destination.document)
}

fn verify_spellings(
    working: &WorkingArtifact,
    respelled: &[Respelled],
    document: &str,
) -> Result<(), WriteError> {
    let resolver = Resolver::live(working);
    let mut failures = Vec::new();
    for spelled in respelled {
        if !resolver.resolves_exactly(&spelled.text, &spelled.spelling, document, &spelled.path)? {
            let mut location = spelled.location.clone();
            location["spelling"] = json!(spelled.text);
            failures.push(location);
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(refusal(
        "write.reference_rewrite",
        "a repaired citation would not resolve to exactly its destination; edit it explicitly"
            .into(),
        "rewrite_references",
        failures,
    ))
}

/// `logic.revise` with C1 restructure fields: an explicit claim merge or split.
pub(super) struct Restructure<'a> {
    pub target: &'a EntrySelector,
    pub set: &'a Fields,
    pub audit: Audit<'a>,
    pub note: Option<&'a str>,
    pub expected: Option<&'a str>,
    pub rewrite: bool,
    pub references: &'a [ReferenceEdit],
    pub action: Option<&'a str>,
    pub split_into: &'a [EntrySelector],
}

fn set_value<'v>(set: &'v Fields, name: &str) -> Option<&'v Value> {
    set.iter()
        .find(|(key, _)| fields::canonical(key) == name)
        .map(|(_, value)| value)
}

pub(super) fn revise_restructure(
    working: &mut WorkingArtifact,
    op: Restructure<'_>,
) -> Result<OperationResult, WriteError> {
    let audit = op.audit;
    revision_context(audit.session, audit.turn, audit.signal, audit.provenance)?;
    if op.set.keys().any(|key| fields::canonical(key) == "body") {
        return Err(WriteError::semantic(
            "write.restructure_body",
            "A merge or split revises claim fields, not Body",
        )
        .at("set"));
    }
    match (op.action, op.split_into.is_empty()) {
        (None, true) | (Some("split"), false) => {}
        (Some(_), _) => {
            return Err(WriteError::semantic(
                "write.split",
                "`action` must be `split`, together with a nonempty `split_into`",
            )
            .at("action")
            .related_here("split_into"));
        }
        (None, false) => {
            return Err(WriteError::semantic(
                "write.split",
                "`split_into` requires `action: \"split\"`",
            )
            .at("split_into")
            .related_here("action"));
        }
    }
    let split = op.action.is_some();
    if op.rewrite && !op.references.is_empty() {
        return Err(WriteError::semantic(
            "write.reference_mode",
            "`rewrite_references` and explicit `references` are mutually exclusive",
        )
        .at("rewrite_references")
        .related_here("references"));
    }
    if split && op.rewrite {
        return Err(WriteError::semantic(
            "write.reference_mode",
            "A split classifies its citers with explicit `references`; `rewrite_references` cannot choose a destination",
        )
        .at("rewrite_references")
        .related_here("action"));
    }
    let survivor = set_value(op.set, "merged into")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let status = set_value(op.set, "status").and_then(Value::as_str);
    if split && survivor.is_some() {
        return Err(WriteError::semantic(
            "write.split",
            "A split retains its primary claim; it cannot also set Merged into",
        )
        .at("set"));
    }
    if !split && (survivor.is_none() || status != Some("withdrawn")) {
        return Err(WriteError::semantic(
            "write.merge_shape",
            "rewrite_references/references on logic.revise need an explicit merge: set Status: withdrawn and Merged into: <existing claim ID>",
        )
        .at("set"));
    }
    let entry = resolve(working, op.target)?;
    let id = heading_id(&entry.heading).to_owned();
    if entry.document != CLAIMS || !fields::typed_id(&id, "C") {
        return Err(WriteError::semantic(
            "write.restructure_target",
            "A merge or split targets one canonical claim",
        )
        .at("target"));
    }
    if let Some(expected) = op.expected
        && source::digest(working.text(CLAIMS)?[entry.body.clone()].as_bytes()) != expected
    {
        return Err(
            WriteError::semantic("write.digest_conflict", "Entry digest does not match")
                .at("expected"),
        );
    }
    let subject = Subject {
        document: CLAIMS.into(),
        path: entry.path.clone(),
        range: entry.range.clone(),
        root_id_retained: false,
        descendant_ids_retained: false,
    };
    let resolver = Resolver::new(working)?;
    let inventory = resolver
        .inventory(
            &subject,
            std::slice::from_ref(&id),
            Some(entry.range.clone()),
            &[],
        )?
        .rewritable_only();
    let skipped: Vec<Value> = inventory
        .mentions
        .iter()
        .map(|mention| mention.to_json())
        .collect();
    let (destination, judgment, edits, split_into) = if let Some(survivor) = survivor {
        let destination = merge_destination(working, &resolver, &id, &survivor)?;
        let edits = if op.rewrite {
            self_citations(&inventory.citations, &destination)?;
            plan_respellings(&inventory.citations, &destination)?
                .into_iter()
                .map(|(citation, (after, _), spelled)| (citation, after, spelled))
                .collect()
        } else {
            let allowed = BTreeSet::from([survivor.clone()]);
            explicit_rows(
                working,
                &resolver,
                &inventory.citations,
                op.references,
                &id,
                &allowed,
                false,
            )?
        };
        (destination, "merged", edits, Vec::new())
    } else {
        let mut destinations = Vec::with_capacity(op.split_into.len());
        let mut ids = BTreeSet::new();
        for (index, selector) in op.split_into.iter().enumerate() {
            let at = format!("split_into[{index}]");
            let spin = resolve(working, selector).map_err(|error| {
                WriteError::semantic(
                    "write.split_destination",
                    format!("{at} does not name one existing claim: {}", error.message),
                )
                .at(at.clone())
            })?;
            let spin_id = heading_id(&spin.heading).to_owned();
            if spin.document != CLAIMS || !fields::typed_id(&spin_id, "C") {
                return Err(WriteError::semantic(
                    "write.split_destination",
                    "split_into names existing canonical claims",
                )
                .at(at));
            }
            if spin_id == id || !ids.insert(spin_id.clone()) {
                return Err(WriteError::semantic(
                    "write.split_destination",
                    format!("split_into needs distinct spin-offs other than the primary; {spin_id} repeats"),
                )
                .at(at));
            }
            destinations.push(EntrySelector::Id { id: spin_id });
        }
        let mut allowed = ids;
        allowed.insert(id.clone());
        let edits = explicit_rows(
            working,
            &resolver,
            &inventory.citations,
            op.references,
            &id,
            &allowed,
            true,
        )?;
        let destination = Destination {
            document: CLAIMS.into(),
            path: entry.path.clone(),
            id: Some(id.clone()),
            keeps_suffix: false,
            start: None,
        };
        (destination, "split", edits, destinations)
    };
    drop(resolver);
    let first_revision = working.revisions.len();
    let mut result = revise(
        working,
        op.target,
        op.set,
        audit.session,
        audit.turn,
        audit.signal,
        audit.provenance,
        op.note,
        None,
    )?;
    if split && result.no_op {
        return Err(WriteError::semantic(
            "write.split",
            "A split must revise the primary claim's own content",
        )
        .at("set"));
    }
    if !split_into.is_empty() {
        for pending in &mut working.revisions[first_revision..] {
            pending.record["action"] = json!("split");
            pending.record["split_into"] = json!(split_into);
        }
    }
    if !result.no_op {
        working.bookkeeping.claim_judgment(&id, judgment);
    }
    let mut respelled = Vec::new();
    for (citation, after, spelled) in edits {
        let row = revise_field_text(working, &citation, &after, audit)?;
        if !row.is_null() {
            result.rewritten_references.push(row);
        }
        respelled.extend(spelled);
    }
    verify_spellings(working, &respelled, &destination.document)?;
    result.no_op = result.no_op && result.rewritten_references.is_empty();
    result.skipped_references = skipped;
    Ok(result)
}

/// The live merge survivor; it cannot be the source or lead back to it.
fn merge_destination(
    working: &WorkingArtifact,
    resolver: &Resolver<'_>,
    source: &str,
    survivor: &str,
) -> Result<Destination, WriteError> {
    let field = "set.Merged into";
    let entry: Entry = resolve(
        working,
        &EntrySelector::Id {
            id: survivor.into(),
        },
    )
    .map_err(|error| {
        WriteError::semantic(
            "write.merge_survivor",
            format!(
                "Merged into must name an existing live claim; {survivor} is not one ({})",
                error.message
            ),
        )
        .at(field)
    })?;
    if survivor == source {
        return Err(WriteError::semantic(
            "write.merge_survivor",
            "A claim cannot be merged into itself",
        )
        .at(field));
    }
    let mut current = survivor.to_owned();
    let mut seen = BTreeSet::new();
    while let Ok(next) = field_value(working, &EntrySelector::Id { id: current }, "Merged into") {
        let next = resolver.redirects().get(&next).cloned().unwrap_or(next);
        if next == source {
            return Err(WriteError::semantic(
                "write.redirect_cycle",
                format!("{survivor} is already merged (transitively) into {source}; this merge would create a redirect cycle"),
            )
            .at(field));
        }
        if !seen.insert(next.clone()) || !fields::typed_id(&next, "C") {
            break;
        }
        current = next;
    }
    Ok(Destination {
        document: CLAIMS.into(),
        path: entry.path,
        id: Some(survivor.into()),
        keeps_suffix: false,
        start: Some(entry.range.start),
    })
}

type PlannedEdit = (Citation, String, Vec<Respelled>);

/// Validate caller-written citer rows of a merge or split. A split must
/// classify every current citing field, including unchanged rows that keep
/// the primary.
fn explicit_rows(
    working: &WorkingArtifact,
    resolver: &Resolver<'_>,
    citations: &[Citation],
    rows: &[ReferenceEdit],
    primary: &str,
    allowed: &BTreeSet<String>,
    split: bool,
) -> Result<Vec<PlannedEdit>, WriteError> {
    let mut seen = BTreeSet::new();
    let mut edits = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let at = format!("references[{index}]");
        let referred =
            resolve(working, &row.target).map_err(|error| error.at(format!("{at}.target")))?;
        let canonical = fields::canonical(&row.field);
        let Some(citation) = citations.iter().find(|citation| {
            citation.document == referred.document
                && citation.start == referred.range.start
                && fields::canonical(&citation.field) == canonical
        }) else {
            return Err(WriteError::semantic(
                "write.reference",
                format!("{at} does not name a current typed citing field of {primary}"),
            )
            .at(format!("{at}.field")));
        };
        if !seen.insert((citation.document.clone(), citation.start, canonical.clone())) {
            return Err(WriteError::semantic(
                "write.reference_duplicate",
                "Duplicate source reference field",
            )
            .at(at));
        }
        if citation.before != row.before {
            return Err(WriteError::semantic(
                "write.reference_before",
                "Reference before does not match exact source",
            )
            .at(format!("{at}.before")));
        }
        let before = resolver.claim_parts(&canonical, &row.before)?;
        let after = resolver.claim_parts(&canonical, &row.after)?;
        // One-to-many only in list-typed values: Dependencies, or a value
        // that is a JSON array of strings before and after.
        let json_list = |text: &str| serde_json::from_str::<Vec<String>>(text).is_ok();
        let list = canonical == "dependencies" || json_list(&row.before) && json_list(&row.after);
        match super::row_mapping::check(&before, &after, primary, allowed, list) {
            Ok(()) => {}
            Err(super::row_mapping::Mismatch::Destination(new)) => {
                return Err(WriteError::semantic(
                    if split {
                        "write.split_destination"
                    } else {
                        "write.merge_destination"
                    },
                    format!(
                        "{at} maps a citation to {new}, which is not {}",
                        if split {
                            "the retained primary or a declared spin-off"
                        } else {
                            "the merge survivor"
                        }
                    ),
                )
                .at(format!("{at}.after")));
            }
            Err(super::row_mapping::Mismatch::Content) => {
                return Err(WriteError::semantic(
                    if canonical == "merged into" {
                        "write.reference_scalar"
                    } else {
                        "write.reference_mapping"
                    },
                    format!(
                        "{at}.after must equal its before except that each citation of {primary} is replaced by {}; other content changes need their own audited revision",
                        if list {
                            "one or more allowed destinations"
                        } else {
                            "exactly one allowed destination (this field is not a list)"
                        }
                    ),
                )
                .at(format!("{at}.after")));
            }
        }
        if referred.document == CLAIMS {
            let citer_id = heading_id(&referred.heading);
            let self_citations = |parts: &[super::row_mapping::Part]| {
                parts
                    .iter()
                    .filter(|part| {
                        matches!(part, super::row_mapping::Part::Claim { id, .. } if id == citer_id)
                    })
                    .count()
            };
            if self_citations(&after) > self_citations(&before) {
                return Err(refusal(
                    "write.reference_rewrite",
                    "repairing this citation would make the entry cite itself, so revise its content explicitly first".into(),
                    &format!("{at}.after"),
                    vec![citation.location()],
                ));
            }
        }
        if !split && row.before == row.after {
            return Err(WriteError::semantic(
                "write.reference",
                format!("{at} must move the citations of {primary} to the survivor; omit the row to keep it"),
            )
            .at(format!("{at}.after")));
        }
        match canonical.as_str() {
            "merged into" if !fields::typed_id(&row.after, "C") => {
                return Err(WriteError::semantic(
                    "write.reference_scalar",
                    "Merged into is a scalar citation; mapping it to several destinations needs a caller-authored content revision",
                )
                .at(format!("{at}.after")));
            }
            "dependencies" if super::dependency_list(&row.after).is_err() => {
                return Err(WriteError::semantic(
                    "write.dependencies",
                    "Dependencies after must be a typed ID list",
                )
                .at(format!("{at}.after")));
            }
            _ => {}
        }
        edits.push((citation.clone(), row.after.clone(), Vec::new()));
    }
    if split {
        let missing: Vec<Value> = citations
            .iter()
            .filter(|citation| {
                !seen.contains(&(
                    citation.document.clone(),
                    citation.start,
                    fields::canonical(&citation.field),
                ))
            })
            .map(Citation::location)
            .collect();
        if !missing.is_empty() {
            return Err(refusal(
                "write.split_unclassified",
                format!(
                    "{} current citing field(s) of {primary} have no references row; classify each one, including rows that keep {primary}",
                    missing.len()
                ),
                "references",
                missing,
            ));
        }
    }
    Ok(edits)
}
