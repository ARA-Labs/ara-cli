//! Append-only full-turn sessions and source-derived session indexes.
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::positions::PathPart;
use super::{OperationResult, WorkingArtifact, WriteError, WriteOperation};

pub const INDEX: &str = "trace/sessions/session_index.yaml";
const ARRAYS: [&str; 5] = [
    "events_logged",
    "ai_actions",
    "claims_touched",
    "logic_revisions",
    "key_context",
];
const ROLLING: [&str; 3] = ["summary", "last_turn", "turn_count"];

fn invalid(field: &str, message: impl Into<String>) -> WriteError {
    WriteError::semantic("write.session", message).at(field)
}

/// How a `session.log` found its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerSelection {
    /// The caller named the session.
    Explicit,
    /// The one open session on the log's written date.
    Selected,
    /// No open session existed on that date; the log created one.
    Created,
}

/// The owner of a batch's audit context: one planned `session.log` and the
/// next turn it reserved. Operations that omit `session`/`turn` attach to it,
/// and later bookkeeping can attach operation facts to the same anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAnchor {
    pub session: String,
    pub turn: u64,
    /// The log's effective timestamp: supplied, or the captured batch time.
    pub timestamp: String,
    /// Zero-based operation index of the owning `session.log`.
    pub operation: usize,
    pub selection: OwnerSelection,
    /// Other open sessions left untouched by selection or creation.
    pub open_sessions: Vec<String>,
}
impl OwnerAnchor {
    /// The anchor for the `session.log` planned at operation index `operation`.
    pub fn from_logged(logged: &LoggedTurn, operation: usize) -> Self {
        Self {
            session: logged.session.clone(),
            turn: logged.turn,
            timestamp: logged.timestamp.clone(),
            operation,
            selection: logged.selection,
            open_sessions: logged.open_sessions.clone(),
        }
    }
    pub fn turn_reference(&self) -> String {
        format!("{}#{}", self.session, self.turn)
    }
}

/// The outcome of planning one `session.log`.
#[derive(Debug, Clone)]
pub struct LoggedTurn {
    pub session: String,
    pub turn: u64,
    pub timestamp: String,
    pub selection: OwnerSelection,
    pub open_sessions: Vec<String>,
    /// Caller `events` and `claims_touched` rows as written (identical
    /// repeats collapsed), keyed by input index. `plan_batch` hands them to
    /// the bookkeeping ledger; other callers (merge audits) ignore them.
    pub events: Vec<(usize, Value)>,
    pub claims: Vec<(usize, Value)>,
}

impl LoggedTurn {
    /// The `session.log` operation result. `session` is reported when the
    /// writer selected or created the session rather than the caller naming it.
    pub fn result(&self) -> OperationResult {
        let mut result = OperationResult::new("session.log", Some(self.session.clone()));
        result.turn = Some(self.turn);
        result.session_created = self.selection == OwnerSelection::Created;
        if self.selection != OwnerSelection::Explicit {
            result.session = Some(self.session.clone());
        }
        result
    }
}

fn closed(metadata: &Value) -> bool {
    metadata.get("closed").and_then(Value::as_bool) == Some(true)
        || metadata.get("status").and_then(Value::as_str) == Some("closed")
        || metadata.get("ended").is_some_and(|v| !v.is_null())
        || metadata.get("closed_at").is_some_and(|v| !v.is_null())
}

pub fn validate_date(date: &str) -> Result<(), WriteError> {
    let b = date.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(invalid("date", "expected a calendar date YYYY-MM-DD"));
    }
    let year: u32 = date[..4]
        .parse()
        .map_err(|_| invalid("date", "invalid year"))?;
    let month: usize = date[5..7]
        .parse()
        .map_err(|_| invalid("date", "invalid month"))?;
    let day: u32 = date[8..]
        .parse()
        .map_err(|_| invalid("date", "invalid day"))?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days[month - 1] {
        return Err(invalid("date", "invalid calendar date"));
    }
    Ok(())
}

/// Timestamp validation is independent of the host clock and filesystem times.
pub fn validate_timestamp(timestamp: &str) -> Result<(), WriteError> {
    timestamp_key(timestamp).map(|_| ())
}

pub(crate) fn timestamp_key(timestamp: &str) -> Result<i128, WriteError> {
    let bad = || {
        invalid(
            "timestamp",
            "expected YYYY-MM-DDTHH:MM[:SS[.fraction]][Z|+HH:MM|-HH:MM]",
        )
    };
    let b = timestamp.as_bytes();
    if b.len() < 16 || !timestamp.is_ascii() || b[10] != b'T' || b[13] != b':' {
        return Err(bad());
    }
    validate_date(&timestamp[..10]).map_err(|e| e.at("timestamp"))?;
    let number = |a: usize, z: usize| -> Result<i128, WriteError> {
        let s = timestamp.get(a..z).ok_or_else(bad)?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        s.parse().map_err(|_| bad())
    };
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    if hour > 23 || minute > 59 {
        return Err(bad());
    }
    let mut p = 16;
    let mut second = 0;
    let mut nanos = 0;
    if b.get(p) == Some(&b':') {
        second = number(p + 1, p + 3)?;
        p += 3;
        if second > 59 {
            return Err(bad());
        }
        if b.get(p) == Some(&b'.') {
            p += 1;
            let start = p;
            while b.get(p).is_some_and(u8::is_ascii_digit) {
                p += 1;
            }
            if p == start || p - start > 9 {
                return Err(bad());
            }
            nanos = number(start, p)? * 10i128.pow((9 - (p - start)) as u32);
        }
    }
    let mut offset = 0;
    if b.get(p) == Some(&b'Z') {
        p += 1;
    } else if matches!(b.get(p), Some(b'+') | Some(b'-')) {
        let sign = if b[p] == b'+' { 1 } else { -1 };
        if b.get(p + 3) != Some(&b':') {
            return Err(bad());
        }
        let h = number(p + 1, p + 3)?;
        let m = number(p + 4, p + 6)?;
        if h > 23 || m > 59 {
            return Err(bad());
        }
        offset = sign * (h * 60 + m) * 60;
        p += 6;
    }
    if p != b.len() {
        return Err(bad());
    }
    let y = number(0, 4)? - 1;
    let month = number(5, 7)? as usize;
    let year = y + 1;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let days = y * 365 + y / 4 - y / 100
        + y / 400
        + lengths[..month - 1].iter().sum::<i128>()
        + number(8, 10)?
        - 1;
    Ok(((days * 24 + hour) * 3600 + minute * 60 + second - offset) * 1_000_000_000 + nanos)
}

pub fn validate_session_id(id: &str) -> Result<(), WriteError> {
    if id.len() != 14
        || id.as_bytes().get(10) != Some(&b'_')
        || !id.as_bytes()[11..].iter().all(u8::is_ascii_digit)
        || &id[11..] == "000"
    {
        return Err(invalid(
            "session",
            "expected YYYY-MM-DD_NNN with a nonzero sequence",
        ));
    }
    validate_date(&id[..10]).map_err(|e| e.at("session"))
}

pub fn validate_provenance(value: &str) -> Result<(), WriteError> {
    if !["user", "ai-suggested", "ai-executed", "user-revised"].contains(&value) {
        return Err(invalid("provenance", "unsupported provenance"));
    }
    Ok(())
}

pub fn validate_id(id: &str, prefixes: &str) -> Result<(), WriteError> {
    let b = id.as_bytes();
    if b.len() < 2
        || !prefixes.bytes().any(|p| p == b[0])
        || !b[1..].iter().all(u8::is_ascii_digit)
        || b[1..].iter().all(|c| *c == b'0')
    {
        return Err(invalid("id", format!("invalid typed reference {id}")));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    routing: String,
    provenance: String,
    summary: String,
    /// Additive: the exact destination of a promotion to a named section
    /// (concept, constraint, architecture), whose event `id` is the source O.
    #[serde(default)]
    target: Option<super::EntrySelector>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    action: String,
    provenance: String,
    files_changed: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimTouch {
    id: String,
    action: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    excerpt: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    entry: Value,
    field: String,
    before: Value,
    after: Value,
    signal: String,
    provenance: String,
    #[serde(default)]
    note: Option<String>,
}

fn typed<T: serde::de::DeserializeOwned>(value: &Value, field: &str) -> Result<T, WriteError> {
    T::deserialize(value).map_err(|e| invalid(field, e.to_string()))
}

pub fn validate_revision(value: &Value) -> Result<(), WriteError> {
    let r: Revision = typed(value, "logic_revisions")?;
    if !(r.entry.is_string() || r.entry.is_object())
        || r.entry.as_str().is_some_and(str::is_empty)
        || r.field.is_empty()
    {
        return Err(invalid(
            "logic_revisions",
            "entry locator and field are required",
        ));
    }
    if r.entry.is_object() {
        let _: super::EntrySelector = typed(&r.entry, "logic_revisions.entry")?;
    }
    if ![
        "empirical-resolution",
        "verbal-declaration",
        "dependency-change",
        "artifact-commitment",
        "terminology-drift",
        "user-directive",
        "verbal-affirmation",
    ]
    .contains(&r.signal.as_str())
    {
        return Err(invalid(
            "logic_revisions.signal",
            "unsupported revision signal",
        ));
    }
    validate_provenance(&r.provenance)?;
    // The caller's complete typed values are retained, never inferred or normalized.
    let _ = (r.before, r.after, r.note);
    Ok(())
}

fn stamp(values: &[Value], kind: &str, turn: u64) -> Result<Vec<Value>, WriteError> {
    let mut result = Vec::with_capacity(values.len());
    for (i, value) in values.iter().enumerate() {
        let validation = match kind {
            "events_logged" => {
                let r: Event = typed(value, kind)?;
                validate_id(&r.id, "NOCH")?;
                validate_provenance(&r.provenance)?;
                if ![
                    "question",
                    "decision",
                    "experiment",
                    "dead_end",
                    "pivot",
                    "observation",
                    "claim",
                    "heuristic",
                    "concept",
                    "constraint",
                    "architecture",
                ]
                .contains(&r.kind.as_str())
                    || !["direct", "staged", "crystallized"].contains(&r.routing.as_str())
                {
                    return Err(invalid(kind, "invalid event type or routing"));
                }
                // `target` is resolved later against the candidate (`validate_event_target`).
                let _ = (r.summary, r.target);
                Ok(())
            }
            "ai_actions" => {
                let r: Action = typed(value, kind)?;
                if r.provenance != "ai-executed" {
                    return Err(invalid(kind, "AI actions require ai-executed provenance"));
                }
                let _ = (r.action, r.files_changed);
                Ok(())
            }
            "claims_touched" => {
                let r: ClaimTouch = typed(value, kind)?;
                validate_id(&r.id, "C")?;
                if ![
                    "created",
                    "crystallized",
                    "advanced",
                    "weakened",
                    "confirmed",
                    "refuted",
                    "withdrawn",
                    "revised",
                    "split",
                    "merged",
                ]
                .contains(&r.action.as_str())
                {
                    return Err(invalid(kind, "unsupported claim action"));
                }
                Ok(())
            }
            "logic_revisions" => validate_revision(value),
            "key_context" => {
                let r: Context = typed(value, kind)?;
                let _ = r.excerpt;
                Ok(())
            }
            _ => Err(invalid(kind, "unsupported turn array")),
        };
        validation.map_err(|e| e.at(format!("{kind}[{i}]")))?;
        let mut stamped = value.clone();
        stamped
            .as_object_mut()
            .ok_or_else(|| invalid(kind, "record must be an object"))?
            .insert("turn".into(), json!(turn));
        result.push(stamped);
    }
    Ok(result)
}

fn session_path(id: &str) -> String {
    format!("trace/sessions/{id}.yaml")
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, WriteError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(key, "expected string"))
}
fn count(value: &Value, key: &str) -> Result<u64, WriteError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(key, "expected nonnegative integer"))
}

fn projected(node: &super::positions::YamlNode, keys: &[&str]) -> Result<Value, WriteError> {
    let mut value = serde_json::Map::new();
    for key in keys {
        if let Some(field) = node.get(key)? {
            value.insert((*key).into(), field.to_json()?);
        }
    }
    Ok(Value::Object(value))
}

fn session_values(root: &super::positions::YamlNode) -> Result<Value, WriteError> {
    let metadata = root
        .get("session")?
        .ok_or_else(|| invalid("session", "missing session mapping"))?;
    let mut value = serde_json::Map::new();
    value.insert(
        "session".into(),
        projected(
            metadata,
            &[
                "id",
                "date",
                "started",
                "last_turn",
                "turn_count",
                "summary",
                "closed",
                "status",
                "ended",
                "closed_at",
            ],
        )?,
    );
    for key in ARRAYS {
        if let Some(array) = root.get(key)? {
            let rows = array
                .sequence()?
                .iter()
                .map(|row| projected(row, &["turn", "id"]))
                .collect::<Result<Vec<_>, _>>()?;
            value.insert(key.into(), Value::Array(rows));
        }
    }
    for key in ["open_threads", "ai_suggestions_pending"] {
        if let Some(field) = root.get(key)? {
            value.insert(key.into(), field.to_json()?);
        }
    }
    Ok(Value::Object(value))
}

fn read_session(working: &WorkingArtifact, id: &str) -> Result<Value, WriteError> {
    validate_session_id(id)?;
    let path = session_path(id);
    let doc = working.yaml(&path)?;
    let value = session_values(&doc.root)?;
    let metadata = value
        .get("session")
        .ok_or_else(|| invalid("session", "missing session mapping"))?;
    if string(metadata, "id")? != id || string(metadata, "date")? != &id[..10] {
        return Err(invalid("session", "session filename, ID and date disagree"));
    }
    let started = string(metadata, "started")?;
    validate_timestamp(started)?;
    if started[..10] != id[..10] {
        return Err(invalid("started", "started date differs from session date"));
    }
    let last = string(metadata, "last_turn")?;
    if timestamp_key(last)? < timestamp_key(started)? || last[..10] != id[..10] {
        return Err(invalid("last_turn", "invalid session chronology"));
    }
    let turns = count(metadata, "turn_count")?;
    string(metadata, "summary")?;
    for key in ARRAYS {
        if let Some(array) = value.get(key) {
            let array = array
                .as_array()
                .ok_or_else(|| invalid(key, "expected sequence"))?;
            let mut previous = 0;
            for record in array {
                let turn = count(record, "turn")?;
                if turn == 0 || turn > turns || turn < previous {
                    return Err(invalid(
                        key,
                        "record turn is outside session history or out of order",
                    ));
                }
                previous = turn;
            }
        }
    }
    for key in ["open_threads", "ai_suggestions_pending"] {
        if let Some(v) = value.get(key)
            && !v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err(invalid(key, "expected string sequence"));
        }
    }
    Ok(value)
}

fn derived(value: &Value) -> Result<Value, WriteError> {
    let metadata = &value["session"];
    let mut claims = BTreeSet::new();
    if let Some(records) = value.get("claims_touched") {
        for record in records
            .as_array()
            .ok_or_else(|| invalid("claims_touched", "expected sequence"))?
        {
            let id = string(record, "id")?;
            validate_id(id, "C")?;
            claims.insert(id.to_owned());
        }
    }
    Ok(
        json!({"id":string(metadata,"id")?,"date":string(metadata,"date")?,"summary":string(metadata,"summary")?,"turn_count":count(metadata,"turn_count")?,"events_count":value.get("events_logged").and_then(Value::as_array).map_or(0,Vec::len),"claims_touched":claims.into_iter().collect::<Vec<_>>(),"open_threads":value.get("open_threads").and_then(Value::as_array).map_or(0,Vec::len)}),
    )
}

/// Validate complete index/record correspondence, retaining unknown values verbatim.
pub fn validate_index(working: &WorkingArtifact) -> Result<BTreeMap<String, usize>, WriteError> {
    let mut rows = BTreeMap::new();
    if working.exists(INDEX) {
        let doc = working.yaml(INDEX)?;
        let entries = doc
            .root
            .get("sessions")?
            .ok_or_else(|| invalid("sessions", "missing index sessions sequence"))?
            .sequence()?;
        for (i, node) in entries.iter().enumerate() {
            let row = projected(
                node,
                &[
                    "id",
                    "date",
                    "summary",
                    "turn_count",
                    "events_count",
                    "claims_touched",
                    "open_threads",
                    "path",
                ],
            )?;
            let id = string(&row, "id")?;
            validate_session_id(id)?;
            if rows.insert(id.to_owned(), i).is_some() {
                return Err(invalid("sessions", "duplicate index ID"));
            }
            let path = session_path(id);
            if let Some(path_value) = row.get("path")
                && path_value.as_str() != Some(path.as_str())
                && path_value.as_str() != Some(format!("{id}.yaml").as_str())
            {
                return Err(invalid("path", "session index path does not match ID"));
            }
            if !working.exists(&path) {
                return Err(invalid("sessions", format!("dangling index entry {id}")));
            }
            let expected = derived(&read_session(working, id)?)?;
            for key in [
                "id",
                "date",
                "summary",
                "turn_count",
                "events_count",
                "open_threads",
            ] {
                if row.get(key) != expected.get(key) {
                    return Err(invalid(
                        "sessions",
                        format!("index field {key} differs from session {id}"),
                    ));
                }
            }
            let actual = row
                .get("claims_touched")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("claims_touched", "index requires claim list"))?;
            let actual: BTreeSet<&str> = actual
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| invalid("claims_touched", "expected claim IDs"))
                })
                .collect::<Result<_, _>>()?;
            let expected: BTreeSet<&str> = expected["claims_touched"]
                .as_array()
                .ok_or_else(|| invalid("claims_touched", "expected claim IDs"))?
                .iter()
                .filter_map(Value::as_str)
                .collect();
            if actual != expected {
                return Err(invalid(
                    "claims_touched",
                    "index touched claims differ from complete session history",
                ));
            }
        }
    }
    for path in working.paths() {
        if let Some(name) = path
            .strip_prefix("trace/sessions/")
            .and_then(|p| p.strip_suffix(".yaml"))
            && name != "session_index"
            && !name.contains('/')
        {
            validate_session_id(name)?;
            if !rows.contains_key(name) {
                return Err(invalid(
                    "sessions",
                    format!("session {name} is absent from index"),
                ));
            }
        }
    }
    Ok(rows)
}

fn archive_state(value: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in ROLLING {
        if let Some(v) = value["session"].get(key) {
            result.insert(key.into(), v.clone());
        }
    }
    for key in ["open_threads", "ai_suggestions_pending"] {
        if let Some(v) = value.get(key) {
            result.insert(key.into(), v.clone());
        }
    }
    Value::Object(result)
}

fn update_index(working: &mut WorkingArtifact, id: &str, row: usize) -> Result<(), WriteError> {
    let values = derived(&read_session(working, id)?)?;
    let selector = [PathPart::from("sessions"), PathPart::Index(row)];
    for key in [
        "summary",
        "turn_count",
        "events_count",
        "claims_touched",
        "open_threads",
    ] {
        working.replace_yaml_field(INDEX, &selector, key, &values[key])?;
    }
    Ok(())
}

pub fn plan(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    match operation {
        WriteOperation::SessionStart {
            id,
            date,
            started,
            summary,
        } => {
            let started = working.explicit_or_clock(started.as_ref())?;
            validate_timestamp(&started)?;
            let date = date.clone().unwrap_or_else(|| started[..10].to_owned());
            let date = &date;
            validate_date(date)?;
            if &started[..10] != date {
                return Err(invalid(
                    "started",
                    format!(
                        "session start timestamp `{started}` is dated {} but the session date is {date}",
                        &started[..10]
                    ),
                ));
            }
            let rows = validate_index(working)?;
            let mut highest = 0u16;
            for existing in rows.keys().filter(|id| &id[..10] == date) {
                highest = highest.max(
                    existing[11..]
                        .parse::<u16>()
                        .map_err(|_| invalid("id", "invalid sequence"))?,
                );
            }
            let assigned = if let Some(id) = id {
                validate_session_id(id)?;
                if &id[..10] != date || rows.contains_key(id) || working.exists(&session_path(id)) {
                    return Err(invalid(
                        "id",
                        "requested session ID is occupied or differs from date",
                    ));
                }
                id.clone()
            } else {
                if highest >= 999 {
                    return Err(invalid("id", "session sequence exhausted"));
                }
                format!("{date}_{:03}", highest + 1)
            };
            let value = json!({"session":{"id":assigned,"date":date,"started":started,"last_turn":started,"turn_count":0,"summary":summary},"events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]});
            working.create(
                &session_path(&assigned),
                &super::source::render_yaml(&value, 0, "\n"),
            )?;
            if let Some(intent) = working.intents.last_mut() {
                intent.reason = "session.start".into();
            }
            working.ensure_yaml(INDEX, "sessions: []\n")?;
            working.append_yaml(INDEX, &[PathPart::from("sessions")], &derived(&value)?)?;
            Ok(OperationResult::new("session.start", Some(assigned)))
        }
        WriteOperation::SessionLog { .. } => Ok(plan_log(working, operation)?.result()),
        _ => Err(invalid("op", "not a session operation")),
    }
}

/// The shared owner allocator: validate that `session` is an indexed, open,
/// well-formed session and return its next turn (`turn_count + 1`).
pub fn next_turn(working: &WorkingArtifact, session: &str) -> Result<u64, WriteError> {
    validate_session_id(session)?;
    let rows = validate_index(working)?;
    if !rows.contains_key(session) {
        return Err(invalid("session", "session is absent from index"));
    }
    let value = read_session(working, session)?;
    if closed(&value["session"]) {
        return Err(invalid("session", "closed sessions are immutable"));
    }
    count(&value["session"], "turn_count")?
        .checked_add(1)
        .ok_or_else(|| invalid("turn", "turn count overflow"))
}

/// A3: choose the owner for a `session.log` that omits `session`. Select the
/// single open session dated `date`; create one with `summary` when none is
/// open on that date; refuse when several are. Closed sessions are never
/// selected or reopened. Other open sessions are returned, untouched.
fn select_or_create(
    working: &mut WorkingArtifact,
    timestamp: &str,
    summary: Option<&str>,
) -> Result<(String, OwnerSelection, Vec<String>), WriteError> {
    let summary = summary
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            WriteError::semantic(
                "write.owner_summary",
                "a session.log that omits `session` needs a nonempty caller-written summary",
            )
            .at("summary")
        })?;
    let date = &timestamp[..10];
    let rows = validate_index(working)?;
    let mut open = Vec::new();
    let mut candidates = Vec::new();
    for id in rows.keys() {
        let value = read_session(working, id)?;
        if closed(&value["session"]) {
            continue;
        }
        if &id[..10] == date {
            candidates.push(id.clone());
        }
        open.push(id.clone());
    }
    let (session, selection) = match candidates.as_slice() {
        [] => {
            let created = plan(
                working,
                &WriteOperation::SessionStart {
                    id: None,
                    date: Some(date.to_owned()),
                    started: Some(timestamp.to_owned()),
                    summary: summary.to_owned(),
                },
            )?
            .id
            .ok_or_else(|| invalid("session", "session creation returned no ID"))?;
            (created, OwnerSelection::Created)
        }
        [only] => (only.clone(), OwnerSelection::Selected),
        many => {
            return Err(WriteError::semantic(
                "write.session_ambiguous",
                format!(
                    "{} open sessions are dated {date}: {}; name one with `session`",
                    many.len(),
                    many.join(", ")
                ),
            )
            .at("session"));
        }
    };
    open.retain(|id| id != &session);
    Ok((session, selection, open))
}

/// Collapse repeated identical rows, keeping the first input index of each.
fn collapse_identical(values: &[Value]) -> (Vec<Value>, Vec<usize>) {
    let mut kept: Vec<Value> = Vec::new();
    let mut inputs = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if !kept.contains(value) {
            kept.push(value.clone());
            inputs.push(index);
        }
    }
    (kept, inputs)
}

/// Explicit event identity before typed resolution: `(id, routing, target)`.
fn raw_event_identity(value: &Value) -> (Option<&Value>, Option<&Value>, Option<&Value>) {
    (value.get("id"), value.get("routing"), value.get("target"))
}

/// Repeated explicit events collapse only when the complete row is identical;
/// two different rows with one identity are a conflict naming both rows.
fn collapse_events(values: &[Value]) -> Result<(Vec<Value>, Vec<usize>), WriteError> {
    for (later, value) in values.iter().enumerate() {
        if !value.is_object() {
            continue;
        }
        for (earlier, previous) in values[..later].iter().enumerate() {
            if previous != value && raw_event_identity(previous) == raw_event_identity(value) {
                return Err(WriteError::semantic(
                    "write.event_conflict",
                    format!(
                        "events[{earlier}] and events[{later}] describe the same event identity (id, routing, target) with different rows; keep one"
                    ),
                )
                .at(format!("events[{later}]"))
                .related_here(format!("events[{earlier}]")));
            }
        }
    }
    Ok(collapse_identical(values))
}

/// Insert `values` (already turn-stamped rows of `turn`) into the session's
/// `key` array after every row of an earlier or equal turn, so rows stay in
/// turn order when one batch owns several turns of one session.
pub(crate) fn insert_turn_rows(
    working: &mut WorkingArtifact,
    session: &str,
    key: &str,
    turn: u64,
    values: &[Value],
) -> Result<(), WriteError> {
    let path = session_path(session);
    for value in values {
        if read_session(working, session)?.get(key).is_none() {
            working.replace_yaml_field(&path, &[], key, &json!([]))?;
        }
        let document = working.yaml(&path)?;
        let rows = document
            .root
            .get(key)?
            .ok_or_else(|| invalid(key, "missing session sequence"))?;
        let mut later = None;
        for node in rows.sequence()? {
            if count(&projected(node, &["turn"])?, "turn")? > turn {
                later = Some((node.start, rows.flow));
                break;
            }
        }
        let Some((start, flow)) = later else {
            working.append_yaml(&path, &[PathPart::from(key)], value)?;
            continue;
        };
        let text = working.text(&path)?;
        let newline = super::positions::eol(text);
        let (at, fragment) = if flow {
            (
                start,
                format!(
                    "{}, ",
                    serde_json::to_string(value).map_err(|e| invalid(key, e.to_string()))?
                ),
            )
        } else {
            let at = super::positions::line_start(text, start);
            let indent = text[at..start]
                .find('-')
                .ok_or_else(|| invalid(key, "ambiguous session sequence indentation"))?;
            (
                at,
                format!(
                    "{}- {}{newline}",
                    " ".repeat(indent),
                    super::source::render_yaml(value, indent + 2, newline)
                ),
            )
        };
        working.edit(
            &path,
            at..at,
            &fragment,
            "append row to its batch-owned turn",
        )?;
    }
    Ok(())
}

/// Write operation-derived `events_logged` and `claims_touched` rows into an
/// owned turn and refresh the session index row they feed.
pub(crate) fn append_derived(
    working: &mut WorkingArtifact,
    session: &str,
    turn: u64,
    events: &[Value],
    claims: &[Value],
) -> Result<(), WriteError> {
    if events.is_empty() && claims.is_empty() {
        return Ok(());
    }
    if !working
        .owned_turns
        .contains_key(&(session.to_owned(), turn))
    {
        return Err(invalid("turn", "derived rows need a new batch-owned turn"));
    }
    let rows = validate_index(working)?;
    let row = *rows
        .get(session)
        .ok_or_else(|| invalid("session", "session is absent from index"))?;
    insert_turn_rows(
        working,
        session,
        "events_logged",
        turn,
        &stamp(events, "events_logged", turn)?,
    )?;
    insert_turn_rows(
        working,
        session,
        "claims_touched",
        turn,
        &stamp(claims, "claims_touched", turn)?,
    )?;
    update_index(working, session, row)
}

/// Plan one `session.log`, resolving an omitted timestamp from the captured
/// batch clock and an omitted session through [`select_or_create`].
pub fn plan_log(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<LoggedTurn, WriteError> {
    let WriteOperation::SessionLog {
        session,
        timestamp,
        summary,
        events,
        ai_actions,
        claims_touched,
        logic_revisions,
        key_context,
        open_threads,
        ai_suggestions_pending,
    } = operation
    else {
        return Err(invalid("op", "not a session.log operation"));
    };
    let timestamp = working.explicit_or_clock(timestamp.as_ref())?;
    validate_timestamp(&timestamp)?;
    let (session, selection, open_sessions) = match session {
        Some(session) => (session.clone(), OwnerSelection::Explicit, Vec::new()),
        None => select_or_create(working, &timestamp, summary.as_deref())?,
    };
    let session = &session;
    validate_session_id(session)?;
    let rows = validate_index(working)?;
    let row = *rows
        .get(session)
        .ok_or_else(|| invalid("session", "session is absent from index"))?;
    let before = read_session(working, session)?;
    let metadata = &before["session"];
    let session_date = string(metadata, "date")?;
    if &timestamp[..10] != session_date {
        return Err(invalid(
            "timestamp",
            format!(
                "turn timestamp `{timestamp}` is dated {} but session {session} is dated {session_date}; \
                 supply a timestamp on {session_date}, or omit `session` to select or create a session for {}",
                &timestamp[..10],
                &timestamp[..10]
            ),
        ));
    }
    if closed(metadata) {
        return Err(invalid("session", "closed sessions are immutable"));
    }
    let last_turn = string(metadata, "last_turn")?;
    if timestamp_key(&timestamp)? < timestamp_key(last_turn)? {
        return Err(invalid(
            "timestamp",
            format!("turn timestamp `{timestamp}` precedes the previous turn `{last_turn}`"),
        ));
    }
    let turn = count(metadata, "turn_count")?
        .checked_add(1)
        .ok_or_else(|| invalid("turn", "turn count overflow"))?;
    if working.owned_turns.contains_key(&(session.clone(), turn)) {
        return Err(invalid("turn", "duplicate batch turn ownership"));
    }
    let (events, event_inputs) = collapse_events(events)?;
    let (claims_touched, claim_inputs) = collapse_identical(claims_touched);
    let arrays = [
        &events,
        ai_actions,
        &claims_touched,
        logic_revisions,
        key_context,
    ];
    let stamped: Vec<Vec<Value>> = ARRAYS
        .iter()
        .zip(arrays)
        .map(|(key, values)| stamp(values, key, turn))
        .collect::<Result<_, _>>()?;
    let path = session_path(session);
    for (key, values) in ARRAYS.iter().zip(stamped) {
        if !values.is_empty() && before.get(*key).is_none() {
            working.replace_yaml_field(&path, &[], key, &json!([]))?;
        }
        for value in values {
            working.append_yaml(&path, &[PathPart::from(*key)], &value)?;
        }
    }
    let selector = [PathPart::from("session")];
    working.replace_yaml_field(&path, &selector, "last_turn", &json!(timestamp))?;
    working.replace_yaml_field(&path, &selector, "turn_count", &json!(turn))?;
    if let Some(summary) = summary {
        working.replace_yaml_field(&path, &selector, "summary", &json!(summary))?;
    }
    if let Some(values) = open_threads {
        working.replace_yaml_field(&path, &[], "open_threads", &json!(values))?;
    }
    if let Some(values) = ai_suggestions_pending {
        working.replace_yaml_field(&path, &[], "ai_suggestions_pending", &json!(values))?;
    }
    let after = read_session(working, session)?;
    super::records::append_archive(
        working,
        &format!("{session}#{turn}"),
        session,
        archive_state(&before),
        archive_state(&after),
    )?;
    update_index(working, session, row)?;
    working
        .owned_turns
        .insert((session.clone(), turn), timestamp.clone());
    Ok(LoggedTurn {
        session: session.clone(),
        turn,
        timestamp,
        selection,
        open_sessions,
        events: event_inputs.into_iter().zip(events).collect(),
        claims: claim_inputs.into_iter().zip(claims_touched).collect(),
    })
}

/// Coupled revisions may attach only to turns allocated by this working batch.
pub fn append_revision(
    working: &mut WorkingArtifact,
    session: &str,
    turn: u64,
    record: &Value,
) -> Result<(), WriteError> {
    if !working
        .owned_turns
        .contains_key(&(session.to_owned(), turn))
    {
        return Err(invalid(
            "turn",
            "revision requires a new batch-owned session turn",
        ));
    }
    validate_session_id(session)?;
    let path = session_path(session);
    if let Some(base) = working.base.files.get(&path).filter(|f| f.existed) {
        let document = super::positions::YamlDocument::parse(
            std::str::from_utf8(&base.bytes).map_err(|_| invalid("session", "invalid UTF-8"))?,
        )?;
        let previous = session_values(&document.root)?;
        if turn <= count(&previous["session"], "turn_count")? {
            return Err(invalid("turn", "historical turns are immutable"));
        }
    }
    let mut value = record.clone();
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("logic_revisions", "expected revision object"))?;
    if let Some(supplied) = object.remove("turn")
        && supplied.as_u64() != Some(turn)
    {
        return Err(invalid("turn", "revision turn differs from owned turn"));
    }
    validate_revision(&value)?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("logic_revisions", "expected object"))?
        .insert("turn".into(), json!(turn));
    let session_value = read_session(working, session)?;
    if turn > count(&session_value["session"], "turn_count")? {
        return Err(invalid(
            "turn",
            "owned turn does not exist in candidate session",
        ));
    }
    insert_turn_rows(working, session, "logic_revisions", turn, &[value])
}

pub fn validate_turn_reference(turn: &str) -> Result<(), WriteError> {
    let (session, number) = turn
        .split_once('#')
        .ok_or_else(|| invalid("turn", "expected session#turn"))?;
    validate_session_id(session)?;
    if number.is_empty()
        || !number.bytes().all(|c| c.is_ascii_digit())
        || number.parse::<u64>().ok().filter(|n| *n > 0).is_none()
    {
        return Err(invalid("turn", "expected positive turn number"));
    }
    Ok(())
}

pub fn require_turn(working: &WorkingArtifact, turn: &str) -> Result<(), WriteError> {
    validate_turn_reference(turn)?;
    let (session, number) = turn
        .split_once('#')
        .ok_or_else(|| invalid("turn", "expected session#turn"))?;
    let number = number
        .parse::<u64>()
        .map_err(|_| invalid("turn", "invalid turn"))?;
    let value = read_session(working, session)?;
    if number > count(&value["session"], "turn_count")? {
        return Err(invalid("turn", "turn does not exist in session"));
    }
    Ok(())
}

/// Resolve authored concrete record pointers after the complete batch exists.
pub fn require_entry_reference(
    working: &WorkingArtifact,
    reference: &str,
) -> Result<(), WriteError> {
    if !reference.contains([':', '#']) && working.is_allowed_document(reference)? {
        return if working.exists(reference) {
            Ok(())
        } else {
            Err(invalid(
                "reference",
                format!("missing reference document {reference}"),
            ))
        };
    }
    if reference.starts_with('N') && !reference.contains([':', '#', '/']) {
        validate_id(reference, "N")?;
        if super::node::node_kind(working, reference)?.is_none() {
            return Err(invalid("reference", format!("unknown node {reference}")));
        }
        return Ok(());
    }
    if reference.starts_with('O') && !reference.contains([':', '#', '/']) {
        validate_id(reference, "O")?;
        if working.exists(super::staging::OBSERVATIONS) {
            let doc = working.yaml(super::staging::OBSERVATIONS)?;
            let rows = doc
                .root
                .get("observations")?
                .ok_or_else(|| invalid("reference", "missing observations"))?
                .sequence()?;
            let mut matches = 0;
            for row in rows {
                if row.get("id")?.and_then(|n| n.scalar()) == Some(reference) {
                    matches += 1;
                }
            }
            if matches == 1 {
                return Ok(());
            }
        }
        return Err(invalid(
            "reference",
            format!("unknown or ambiguous observation {reference}"),
        ));
    }
    let (document, identity, numbered) =
        if reference.starts_with('C') && !reference.contains([':', '#', '/']) {
            validate_id(reference, "C")?;
            ("logic/claims.md", reference, true)
        } else if reference.starts_with('H') && !reference.contains([':', '#', '/']) {
            validate_id(reference, "H")?;
            ("logic/solution/heuristics.md", reference, true)
        } else if let Some((document, identity)) = reference.split_once('#') {
            if !working.is_allowed_document(document)? || identity.is_empty() {
                return Err(invalid("reference", "invalid document section reference"));
            }
            (document, identity, false)
        } else if let Some((document, identity)) = reference.split_once(':') {
            if document == "trace" {
                validate_id(identity, "N")?;
                return require_entry_reference(working, identity);
            }
            if !working.is_allowed_document(document)? || identity.is_empty() {
                return Err(invalid("reference", "invalid document entry reference"));
            }
            (document, identity, true)
        } else {
            return Err(invalid(
                "reference",
                format!("unsupported typed reference {reference}"),
            ));
        };
    if !working.exists(document) {
        return Err(invalid(
            "reference",
            format!("missing reference document {document}"),
        ));
    }
    let matches = crate::markdown::sections(working.text(document)?)
        .iter()
        .filter(|section| {
            if numbered {
                section
                    .heading
                    .split_once(':')
                    .map_or(section.heading, |(id, _)| id)
                    .trim()
                    == identity
            } else {
                section.heading == identity
            }
        })
        .count();
    if matches != 1 {
        return Err(invalid(
            "reference",
            format!("unknown or ambiguous entry {reference}"),
        ));
    }
    Ok(())
}

fn event_target_error(message: impl Into<String>) -> WriteError {
    WriteError::semantic("write.event_target", message).at("events_logged.target")
}

/// A new event `target` must resolve to the exact destination its row names.
/// For a named section (concept, constraint, architecture) the row's `id` is
/// the source observation, whose promotion tuple must point at that section.
pub(crate) fn validate_event_target(
    working: &WorkingArtifact,
    record: &Value,
) -> Result<(), WriteError> {
    let id = string(record, "id")?;
    let kind = string(record, "type")?;
    // The one routing check for `target`: only a promotion has a destination.
    if string(record, "routing")? != "crystallized" {
        return Err(event_target_error(
            "an event `target` needs `crystallized` routing",
        ));
    }
    let target: super::EntrySelector = typed(&record["target"], "events_logged.target")
        .map_err(|e| e.at("events_logged.target"))?;
    if id.starts_with('O') {
        let document = super::bookkeeping::named_document(kind).ok_or_else(|| {
            event_target_error(format!(
                "an observation event with a target must be a concept, constraint or architecture promotion, not `{kind}`"
            ))
        })?;
        let super::EntrySelector::Document {
            document: selected, ..
        } = &target
        else {
            return Err(event_target_error(format!(
                "a {kind} target selects a heading in {document}"
            )));
        };
        if selected != document {
            return Err(event_target_error(format!(
                "a {kind} target must select a heading in {document}, not {selected}"
            )));
        }
        let entry = super::logic::resolve(working, &target).map_err(|e| {
            event_target_error(format!("event target does not resolve: {}", e.message))
        })?;
        let promoted = super::staging::promoted_to(working, id)?;
        if promoted.as_deref() != Some(format!("{document}#{}", entry.heading).as_str()) {
            return Err(event_target_error(format!(
                "event target {document}#{} does not match {id}'s promotion tuple ({})",
                entry.heading,
                promoted.as_deref().unwrap_or("not promoted")
            )));
        }
        return Ok(());
    }
    let own = super::EntrySelector::Id { id: id.to_owned() };
    if !super::bookkeeping::same_entry(working, &own, &target) {
        return Err(event_target_error(format!(
            "event target does not select {id}, the entry the row names"
        )));
    }
    Ok(())
}

pub fn validate_references(working: &WorkingArtifact) -> Result<(), WriteError> {
    for path in working.changed_paths() {
        if !path.starts_with("trace/sessions/") || path == INDEX || !path.ends_with(".yaml") {
            continue;
        }
        let previous_turn = if let Some(base) = working.base.files.get(&path).filter(|f| f.existed)
        {
            let doc = super::positions::YamlDocument::parse(
                std::str::from_utf8(&base.bytes)
                    .map_err(|_| invalid("document", "invalid UTF-8"))?,
            )?;
            let value = session_values(&doc.root)?;
            count(&value["session"], "turn_count")?
        } else {
            0
        };
        let doc = working.yaml(&path)?;
        let value = session_values(&doc.root)?;
        for key in ["events_logged", "claims_touched"] {
            for record in value
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if count(record, "turn")? > previous_turn {
                    require_entry_reference(working, string(record, "id")?)?;
                }
            }
        }
        if let Some(events) = doc.root.get("events_logged")? {
            for node in events.sequence()? {
                let record = node.to_json()?;
                if count(&record, "turn")? > previous_turn && record.get("target").is_some() {
                    validate_event_target(working, &record)?;
                }
            }
        }
        if let Some(revisions) = doc.root.get("logic_revisions")? {
            let session = string(&value["session"], "id")?;
            for node in revisions.sequence()? {
                let turn = node
                    .get("turn")?
                    .ok_or_else(|| invalid("logic_revisions.turn", "revision turn is required"))?
                    .to_json()?
                    .as_u64()
                    .ok_or_else(|| invalid("logic_revisions.turn", "expected nonnegative turn"))?;
                if turn <= previous_turn {
                    continue;
                }
                let mut record = node.to_json()?;
                record
                    .as_object_mut()
                    .ok_or_else(|| invalid("logic_revisions", "expected revision object"))?
                    .remove("turn");
                validate_revision(&record)?;
                super::logic::validate_revision_entry(working, session, turn, &record)?;
            }
        }
    }
    Ok(())
}

/// The modern authoring contract does not retroactively normalize imported or
/// untouched legacy sessions. Merge has its own protected-history validator.
pub fn validate_authored(working: &WorkingArtifact) -> Result<(), WriteError> {
    if working.owned_turns.is_empty()
        && !working
            .intents
            .iter()
            .any(|intent| intent.reason == "session.start")
    {
        return Ok(());
    }
    validate_index(working)?;
    validate_references(working)
}
