//! Operation-derived session rows (plan 19, A4/A5).
//!
//! A batch that logs a turn gets the mechanical part of that turn's
//! `events_logged` and `claims_touched` rows from the operations it ran:
//! creations, stagings and promotions become events, and claim creations,
//! promotions and revisions become claim touches. Caller rows stay the source
//! of summaries and scientific judgments. A caller row that names an entry this
//! batch created must agree with the operation's facts; the matching derived
//! row is then suppressed so the turn never holds a duplicate.
//!
//! Rows are derived only when the batch contains a `session.log`. With one
//! log, every eligible operation belongs to its turn. With several, each
//! creation must be attributed through an explicit row in exactly one log, and
//! revisions belong to the turn they name. Only the new owned turns are
//! touched; earlier history is never searched or rewritten.
use super::{EntrySelector, Fields, OperationResult, WorkingArtifact, WriteError, WriteOperation};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// One planned `session.log` and the caller rows it wrote, keyed by their
/// original input index (identical repeats are already collapsed).
#[derive(Debug, Clone)]
pub struct LogRows {
    pub operation: usize,
    pub session: String,
    pub turn: u64,
    pub events: Vec<(usize, Value)>,
    pub claims: Vec<(usize, Value)>,
}

/// The facts of one successful creation, staging or promotion.
#[derive(Debug, Clone)]
pub struct Creation {
    pub operation: usize,
    /// Event ID: the allocated N/O/C/H ID, or the source O ID of a promotion
    /// to a named section.
    pub id: String,
    pub kind: String,
    pub routing: &'static str,
    /// Supplied or inherited provenance; `None` when the operation had none.
    pub provenance: Option<String>,
    /// Where the caller supplies provenance on this operation.
    pub provenance_field: &'static str,
    /// The operation's explicit title, or an observation's complete content.
    pub summary: String,
    /// Exact destination of a promotion to a named section.
    pub target: Option<EntrySelector>,
    /// Source observation of a promotion.
    pub source: Option<String>,
    /// A claim this operation created (`created`) or promoted (`crystallized`).
    pub claim: Option<(String, &'static str)>,
}

/// One audited change to a claim's source, owned by an explicit turn.
#[derive(Debug, Clone)]
pub struct ClaimChange {
    pub operation: usize,
    pub session: String,
    pub turn: u64,
    pub claim: String,
    /// The Status value written by this change, when it changed Status.
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Ledger {
    /// Zero-based index of the operation being planned, set by `plan_batch`.
    /// Only `plan_batch` records logs and creations and calls [`finalize`];
    /// planners used outside it (merge audits, direct `plan_operation`) may
    /// record claim changes that are never finalized and are dropped with the
    /// working artifact.
    pub current: usize,
    pub logs: Vec<LogRows>,
    pub creations: Vec<Creation>,
    pub changes: Vec<ClaimChange>,
}

impl Ledger {
    /// Record an audited, non-no-op change to claim `claim` from the fields it
    /// wrote (`(field, after)`).
    pub fn claim_changed<'a>(
        &mut self,
        session: &str,
        turn: u64,
        claim: &str,
        fields: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    ) {
        if !super::fields::typed_id(claim, "C") {
            return;
        }
        let mut status = None;
        for (field, after) in fields {
            if super::fields::canonical(field) == "status" {
                status = after.map(str::to_owned);
            }
        }
        self.changes.push(ClaimChange {
            operation: self.current,
            session: session.to_owned(),
            turn,
            claim: claim.to_owned(),
            status,
        });
    }
}

/// The canonical document of a promotion to a named section.
pub fn named_document(kind: &str) -> Option<&'static str> {
    match kind {
        "concept" => Some("logic/concepts.md"),
        "constraint" => Some("logic/solution/constraints.md"),
        "architecture" => Some("logic/solution/architecture.md"),
        _ => None,
    }
}

fn supplied_provenance(fields: &Fields) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| super::fields::canonical(key) == "provenance")
        .and_then(|(_, value)| value.as_str())
        .map(str::to_owned)
}

fn allocated(result: &OperationResult) -> Result<String, WriteError> {
    result.id.clone().ok_or_else(|| {
        WriteError::semantic(
            "write.binding_result",
            "creation did not return its native identity",
        )
    })
}

/// Collect the facts of one successful, non-no-op operation.
pub fn record_operation(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
    result: &OperationResult,
) -> Result<(), WriteError> {
    if result.no_op {
        return Ok(());
    }
    let index = working.bookkeeping.current;
    let creation = match operation {
        WriteOperation::NodeAdd {
            kind,
            title,
            fields,
            ..
        } => Creation {
            operation: index,
            id: allocated(result)?,
            kind: kind.clone(),
            routing: "direct",
            // Node fields are native lowercase YAML keys; `node.add` accepts
            // only the exact `provenance` spelling, unlike Markdown labels.
            provenance: fields
                .get("provenance")
                .and_then(Value::as_str)
                .map(str::to_owned),
            provenance_field: "fields.provenance",
            summary: title.clone(),
            target: None,
            source: None,
            claim: None,
        },
        WriteOperation::ObservationStage {
            content,
            provenance,
            ..
        } => Creation {
            operation: index,
            id: allocated(result)?,
            kind: "observation".into(),
            routing: "staged",
            provenance: Some(provenance.clone()),
            provenance_field: "provenance",
            summary: content.clone(),
            target: None,
            source: None,
            claim: None,
        },
        WriteOperation::ClaimAdd { title, fields, .. }
        | WriteOperation::HeuristicAdd { title, fields, .. } => {
            let id = allocated(result)?;
            let claim = matches!(operation, WriteOperation::ClaimAdd { .. });
            Creation {
                operation: index,
                kind: if claim { "claim" } else { "heuristic" }.into(),
                routing: "direct",
                provenance: supplied_provenance(fields),
                provenance_field: "fields.Provenance",
                summary: title.clone(),
                target: None,
                source: None,
                claim: claim.then(|| (id.clone(), "created")),
                id,
            }
        }
        WriteOperation::ObservationPromote {
            observation,
            to,
            title,
            fields,
            ..
        } => {
            let provenance = match supplied_provenance(fields) {
                Some(value) => Some(value),
                None => super::staging::observation_provenance(working, observation)?,
            };
            let (id, target) = if let Some(document) = named_document(to) {
                let section = result
                    .target
                    .as_deref()
                    .and_then(|target| target.strip_prefix(document))
                    .and_then(|rest| rest.strip_prefix('#'))
                    .ok_or_else(|| {
                        WriteError::semantic(
                            "write.observation",
                            "promotion did not report its named destination",
                        )
                    })?;
                (
                    observation.clone(),
                    Some(EntrySelector::Document {
                        document: document.into(),
                        heading: vec![section.into()],
                        entry: None,
                    }),
                )
            } else {
                (allocated(result)?, None)
            };
            Creation {
                operation: index,
                kind: to.clone(),
                routing: "crystallized",
                provenance,
                provenance_field: "fields.Provenance",
                summary: title.clone(),
                target,
                source: Some(observation.clone()),
                claim: (to == "claim").then(|| (id.clone(), "crystallized")),
                id,
            }
        }
        _ => return Ok(()),
    };
    working.bookkeeping.creations.push(creation);
    Ok(())
}

/// An error at operation `operation` (zero-based), optionally naming a second
/// input location.
fn located(
    code: &str,
    message: String,
    operation: usize,
    field: impl Into<String>,
    related: Option<(usize, String)>,
) -> WriteError {
    let mut error = WriteError::semantic(code, message).at(field);
    error.line = Some(operation + 1);
    if let Some((operation, field)) = related {
        error = error.related(operation + 1, field);
    }
    error
}

/// Whether two selectors resolve to the same existing entry (node or logic
/// entry). Unresolvable selectors never match.
pub(crate) fn same_entry(working: &WorkingArtifact, a: &EntrySelector, b: &EntrySelector) -> bool {
    let a = destination(working, a);
    a.is_some() && a == destination(working, b)
}

/// A typed identity for comparing destinations: a node, or the resolved
/// logic entry. `None` when the selector does not resolve to one entry.
fn destination(working: &WorkingArtifact, selector: &EntrySelector) -> Option<String> {
    if let EntrySelector::Id { id } = selector
        && super::fields::typed_id(id, "N")
    {
        return Some(format!("trace:{id}"));
    }
    super::logic::resolve(working, selector)
        .ok()
        .map(|entry| format!("{}@{}", entry.document, entry.range.start))
}

/// One caller event row as written by its `session.log`.
struct EventRow {
    input: usize,
    id: String,
    kind: String,
    routing: String,
    provenance: String,
    target: Option<EntrySelector>,
}
impl EventRow {
    /// `stamp` already validated the row's shape and vocabulary when its
    /// `session.log` was planned; this only reads the typed fields back.
    fn parse(input: usize, value: &Value) -> Result<Self, WriteError> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default();
        Ok(Self {
            input,
            id: text("id").into(),
            kind: text("type").into(),
            routing: text("routing").into(),
            provenance: text("provenance").into(),
            target: value
                .get("target")
                .map(|target| serde_json::from_value(target.clone()))
                .transpose()
                .map_err(|e| {
                    WriteError::semantic("write.event_target", e.to_string())
                        .at(format!("events[{input}].target"))
                })?,
        })
    }
    fn field(&self) -> String {
        format!("events[{}]", self.input)
    }
    /// Identity after typed resolution: `(id, routing, target)`.
    fn identity(&self, working: &WorkingArtifact) -> (String, String, Option<String>) {
        let target = self.target.as_ref().map(|target| {
            destination(working, target).unwrap_or_else(|| {
                format!("raw:{}", serde_json::to_string(target).unwrap_or_default())
            })
        });
        (self.id.clone(), self.routing.clone(), target)
    }
}

/// Whether a caller row describes the same destination as a creation fact.
/// An absent row `target` matches a numeric-ID fact, and a supplied one must
/// resolve to the fact's exact entry.
fn same_destination(working: &WorkingArtifact, fact: &Creation, row: &EventRow) -> bool {
    match (&fact.target, &row.target) {
        (Some(expected), Some(supplied)) => same_entry(working, expected, supplied),
        (Some(_), None) => false,
        (None, None) => true,
        (None, Some(supplied)) => same_entry(
            working,
            &EntrySelector::Id {
                id: fact.id.clone(),
            },
            supplied,
        ),
    }
}

/// Whether a caller row names an entry a creation fact produced, under any
/// routing or target. Such a row must match that fact exactly.
fn names_fact(fact: &Creation, row: &EventRow) -> bool {
    row.id == fact.id || fact.source.as_deref() == Some(&row.id) && row.routing == "crystallized"
}

/// Choose the log that owns a creation's derived rows.
fn attribute(logs: &[LogRows], fact: &Creation) -> Result<usize, WriteError> {
    if logs.len() == 1 {
        return Ok(0);
    }
    let names = |log: &LogRows, exact: bool| {
        log.events.iter().any(|(_, row)| {
            row.get("id").and_then(Value::as_str) == Some(&fact.id)
                && (!exact || row.get("routing").and_then(Value::as_str) == Some(fact.routing))
        }) || fact.claim.as_ref().is_some_and(|(claim, action)| {
            log.claims.iter().any(|(_, row)| {
                row.get("id").and_then(Value::as_str) == Some(claim)
                    && (!exact || row.get("action").and_then(Value::as_str) == Some(action))
            })
        })
    };
    let mut owners: Vec<usize> = (0..logs.len()).filter(|i| names(&logs[*i], true)).collect();
    if owners.is_empty() {
        owners = (0..logs.len())
            .filter(|i| names(&logs[*i], false))
            .collect();
    }
    match owners.as_slice() {
        [only] => Ok(*only),
        [] => Err(located(
            "write.owner_ambiguous",
            format!(
                "{} {} is not attributed to a turn: with {} session.log operations, name it in an explicit events (or claims_touched) row of exactly one log",
                fact.kind,
                fact.id,
                logs.len()
            ),
            fact.operation,
            "op",
            None,
        )),
        [first, second, ..] => Err(located(
            "write.owner_ambiguous",
            format!(
                "{} {} is named by rows in more than one session.log; name it in exactly one",
                fact.kind, fact.id
            ),
            logs[*second].operation,
            "events",
            Some((logs[*first].operation, "events".into())),
        )),
    }
}

const JUDGMENTS: [&str; 8] = [
    "revised",
    "advanced",
    "weakened",
    "confirmed",
    "refuted",
    "withdrawn",
    "merged",
    "split",
];

/// The Status a terminal judgment names; other judgments name none and are
/// never compared with Status values.
fn named_status(judgment: &str) -> Option<&'static str> {
    match judgment {
        "confirmed" => Some("supported"),
        "refuted" => Some("refuted"),
        "withdrawn" | "merged" => Some("withdrawn"),
        _ => None,
    }
}

/// Check the caller's judgments for a claim changed in this turn against the
/// explicit Status changes of that turn. Without a Status change only the
/// vocabulary applies; the stored Status is never consulted.
fn check_judgments(
    log: &LogRows,
    claim: &str,
    rows: &[(usize, &str)],
    changes: &[&ClaimChange],
) -> Result<(), WriteError> {
    let transitions: Vec<(usize, &str)> = changes
        .iter()
        .filter_map(|change| {
            change
                .status
                .as_deref()
                .map(|status| (change.operation, status))
        })
        .collect();
    let conflict = |input: usize, message: String, related: Option<(usize, String)>| {
        located(
            "write.claim_touch_conflict",
            message,
            log.operation,
            format!("claims_touched[{input}]"),
            related,
        )
    };
    for (input, judgment) in rows {
        if let Some(required) = named_status(judgment)
            && !transitions.iter().any(|(_, status)| *status == required)
            && let Some((operation, status)) = transitions.last()
        {
            return Err(conflict(
                *input,
                format!(
                    "{claim} `{judgment}` contradicts this turn's explicit Status change to `{status}`; it needs a change to `{required}`"
                ),
                Some((*operation, "set.Status".into())),
            ));
        }
    }
    let confirmed = rows.iter().find(|(_, j)| *j == "confirmed");
    let refuted = rows.iter().find(|(_, j)| *j == "refuted");
    if let (Some((first, _)), Some((second, _))) = (confirmed, refuted) {
        // One change writes one value, so a change to each is two distinct changes.
        let distinct = transitions.iter().any(|(_, s)| *s == "supported")
            && transitions.iter().any(|(_, s)| *s == "refuted");
        if !distinct {
            return Err(conflict(
                (*first).max(*second),
                format!(
                    "{claim} is both `confirmed` and `refuted` in one turn without distinct explicit Status changes to `supported` and `refuted`"
                ),
                Some((
                    log.operation,
                    format!("claims_touched[{}]", (*first).min(*second)),
                )),
            ));
        }
    }
    Ok(())
}

/// Attach every log's operation-derived rows after the ordered operations
/// succeed. Explicit rows keep their order; derived rows follow in operation
/// order. Any conflict rejects the whole batch.
pub fn finalize(working: &mut WorkingArtifact) -> Result<(), WriteError> {
    let ledger = std::mem::take(&mut working.bookkeeping);
    if ledger.logs.is_empty() {
        return Ok(());
    }
    let owners = ledger
        .creations
        .iter()
        .map(|fact| attribute(&ledger.logs, fact))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, log) in ledger.logs.iter().enumerate() {
        let facts: Vec<&Creation> = ledger
            .creations
            .iter()
            .zip(&owners)
            .filter(|(_, owner)| **owner == index)
            .map(|(fact, _)| fact)
            .collect();
        let events = derive_events(working, log, &facts, &ledger.creations)?;
        let changes: Vec<&ClaimChange> = ledger
            .changes
            .iter()
            .filter(|change| change.session == log.session && change.turn == log.turn)
            .collect();
        let claims = derive_touches(log, &facts, &changes, &ledger)?;
        super::sessions::append_derived(working, &log.session, log.turn, &events, &claims)
            .map_err(|mut error| {
                error.line.get_or_insert(log.operation + 1);
                error
            })?;
    }
    Ok(())
}

fn derive_events(
    working: &WorkingArtifact,
    log: &LogRows,
    facts: &[&Creation],
    all: &[Creation],
) -> Result<Vec<Value>, WriteError> {
    let rows = log
        .events
        .iter()
        .map(|(input, value)| EventRow::parse(*input, value))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|mut error| {
            error.line = Some(log.operation + 1);
            error
        })?;
    // Validated here for a precise `events[i].target` location on the log's
    // line; final validation (`validate_references`) checks every new row again.
    for (row, (_, value)) in rows.iter().zip(&log.events) {
        if value.get("target").is_some() {
            let mut record = value.clone();
            record["turn"] = json!(log.turn);
            super::sessions::validate_event_target(working, &record).map_err(|mut error| {
                error.line = Some(log.operation + 1);
                error.field = Some(format!("{}.target", row.field()));
                error
            })?;
        }
    }
    for (later, row) in rows.iter().enumerate() {
        if let Some(earlier) = rows[..later]
            .iter()
            .find(|earlier| earlier.identity(working) == row.identity(working))
        {
            return Err(located(
                "write.event_conflict",
                format!(
                    "{} and {} resolve to the same event identity (id, routing, target) with different rows; keep one",
                    earlier.field(),
                    row.field()
                ),
                log.operation,
                row.field(),
                Some((log.operation, earlier.field())),
            ));
        }
    }
    let mut consumed = vec![false; rows.len()];
    let mut derived = Vec::new();
    for fact in facts {
        let matching: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.id == fact.id
                    && row.routing == fact.routing
                    && same_destination(working, fact, row)
            })
            .map(|(i, _)| i)
            .collect();
        if let [first, second, ..] = matching.as_slice() {
            return Err(located(
                "write.event_conflict",
                format!(
                    "{} and {} both describe the {} of {}; keep one row",
                    rows[*first].field(),
                    rows[*second].field(),
                    fact.routing,
                    fact.id
                ),
                log.operation,
                rows[*second].field(),
                Some((log.operation, rows[*first].field())),
            ));
        }
        if let Some(i) = matching.first() {
            let row = &rows[*i];
            let disagreement = if row.kind != fact.kind {
                Some(format!(
                    "type `{}` but the operation made a {}",
                    row.kind, fact.kind
                ))
            } else {
                fact.provenance
                    .as_deref()
                    .filter(|provenance| *provenance != row.provenance)
                    .map(|provenance| {
                        format!(
                            "provenance `{}` but the operation recorded `{provenance}`",
                            row.provenance
                        )
                    })
            };
            if let Some(disagreement) = disagreement {
                return Err(located(
                    "write.event_conflict",
                    format!("{} names {} with {disagreement}", row.field(), fact.id),
                    log.operation,
                    row.field(),
                    Some((fact.operation, "op".into())),
                ));
            }
            consumed[*i] = true;
            continue;
        }
        let provenance = fact
            .provenance
            .clone()
            .filter(|value| super::sessions::validate_provenance(value).is_ok())
            .ok_or_else(|| {
                located(
                    "write.event_provenance",
                    format!(
                        "the logged turn needs an event for {} {}, but its provenance is {}; supply a valid `{}` or an explicit events row",
                        fact.kind,
                        fact.id,
                        if fact.provenance.is_some() { "invalid" } else { "missing" },
                        fact.provenance_field
                    ),
                    fact.operation,
                    fact.provenance_field,
                    None,
                )
            })?;
        let mut row = json!({
            "type": fact.kind,
            "id": fact.id,
            "routing": fact.routing,
            "provenance": provenance,
            "summary": fact.summary,
        });
        if let Some(EntrySelector::Document {
            document, heading, ..
        }) = &fact.target
        {
            row["target"] = json!({"document": document, "heading": heading});
        }
        derived.push(row);
    }
    for (i, row) in rows.iter().enumerate() {
        if consumed[i] {
            continue;
        }
        // Several facts can share an ID (staged, then promoted): report the
        // one with the row's routing, else the latest (the promotion).
        let named: Vec<&Creation> = all.iter().filter(|fact| names_fact(fact, row)).collect();
        if let Some(fact) = named
            .iter()
            .find(|fact| fact.routing == row.routing)
            .or_else(|| named.last())
        {
            return Err(located(
                "write.event_conflict",
                format!(
                    "{} names {}, which this batch's operation produced as {} `{}`{}; an explicit row must match that operation's routing and destination",
                    row.field(),
                    row.id,
                    fact.kind,
                    fact.routing,
                    match &fact.target {
                        Some(target) => format!(
                            " with target {}",
                            serde_json::to_string(target).unwrap_or_default()
                        ),
                        None => String::new(),
                    }
                ),
                log.operation,
                row.field(),
                Some((fact.operation, "op".into())),
            ));
        }
    }
    Ok(derived)
}

fn derive_touches(
    log: &LogRows,
    facts: &[&Creation],
    changes: &[&ClaimChange],
    ledger: &Ledger,
) -> Result<Vec<Value>, WriteError> {
    let rows: Vec<(usize, &str, &str)> = log
        .claims
        .iter()
        .map(|(input, value)| {
            (
                *input,
                value.get("id").and_then(Value::as_str).unwrap_or_default(),
                value
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        })
        .collect();
    let explicit =
        |claim: &str, action: &str| rows.iter().any(|(_, c, a)| *c == claim && *a == action);
    for (input, claim, action) in &rows {
        if !matches!(*action, "created" | "crystallized") {
            continue;
        }
        let creation = ledger
            .creations
            .iter()
            .find(|fact| fact.claim.as_ref().is_some_and(|(id, _)| id == claim));
        let wrong = match creation {
            Some(fact) => fact
                .claim
                .as_ref()
                .filter(|(_, made)| made != action)
                .map(|(_, made)| (fact.operation, format!("the operation {made} it"))),
            None => ledger
                .changes
                .iter()
                .find(|change| change.claim == *claim)
                .map(|change| {
                    (
                        change.operation,
                        "this batch revises an existing claim".to_owned(),
                    )
                }),
        };
        if let Some((operation, reason)) = wrong {
            return Err(located(
                "write.claim_touch_conflict",
                format!("claims_touched[{input}] labels {claim} `{action}`, but {reason}"),
                log.operation,
                format!("claims_touched[{input}]"),
                Some((operation, "op".into())),
            ));
        }
    }
    let mut derived: Vec<(usize, String, &str)> = Vec::new();
    for fact in facts {
        if let Some((claim, action)) = &fact.claim
            && !explicit(claim, action)
        {
            derived.push((fact.operation, claim.clone(), action));
        }
    }
    let mut seen = BTreeSet::new();
    for change in changes {
        if !seen.insert(change.claim.as_str()) {
            continue;
        }
        let own: Vec<&ClaimChange> = changes
            .iter()
            .copied()
            .filter(|other| other.claim == change.claim)
            .collect();
        let judgments: Vec<(usize, &str)> = rows
            .iter()
            .filter(|(_, claim, action)| *claim == change.claim && JUDGMENTS.contains(action))
            .map(|(input, _, action)| (*input, *action))
            .collect();
        check_judgments(log, &change.claim, &judgments, &own)?;
        // A caller judgment replaces the generic row; otherwise derive it.
        if judgments.is_empty() {
            derived.push((change.operation, change.claim.clone(), "revised"));
        }
    }
    derived.sort_by_key(|(operation, _, _)| *operation);
    Ok(derived
        .into_iter()
        .map(|(_, claim, action)| json!({"id": claim, "action": action}))
        .collect())
}
