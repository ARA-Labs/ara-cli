//! Explicit-reference session history (plan 19, D1/D2).
//!
//! One extractor serves `ara open` (read path) and `observation.mark_stale`
//! (write path). It builds a timeline of logged turns from validated session
//! records, per-turn stamps (the session's `last_turn` and the archived
//! `session_metadata` in `trace/pm_reasoning_log.yaml`) and authenticated
//! merge aliases. It never uses numeric IDs, filesystem times, index totals
//! or lexical session order as chronology.
//!
//! A reference is an exact observation or bound-node ID in a typed turn field
//! (`structured`) or an exact token in caller-authored turn text (`literal`).
//! Generated counters, indexes, archived copies of rolling fields,
//! before/after revision values and stale-evidence records are not new
//! reference activity. Unknown chronology yields `None` with a diagnostic,
//! never zero or a guessed count. Semantic topic matching is the caller's.
//!
//! A session's turns are kept compactly (date, count and the stamps actually
//! recorded), never one value per claimed turn, so an absurd `turn_count`
//! costs nothing and all turn arithmetic is checked.
mod collect;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::positions::{YamlKind, YamlNode};
use super::records::REASONING;
use super::sessions::{INDEX, timestamp_key, validate_session_id};
use collect::{
    Aliases, Collector, METADATA_KEYS, Mention, UnitRef, get, scalar, sequence_strings,
    stale_record, turn_reference, written_date,
};

pub const ALIASES: &str = "trace/aliases.yaml";
const SESSIONS: &str = "trace/sessions/";
/// Typed per-turn arrays of a CLI session record.
const TURN_ARRAYS: [&str; 5] = [
    "events_logged",
    "ai_actions",
    "claims_touched",
    "logic_revisions",
    "key_context",
];
/// Rolling session fields; a turn authored them only where they changed.
const ROLLING_LISTS: [&str; 2] = ["open_threads", "ai_suggestions_pending"];
/// Examples kept per diagnostic code.
const EXAMPLES: usize = 3;

/// Whether `path` is a source the history reads.
pub fn is_history_path(path: &str) -> bool {
    path == REASONING || path == INDEX || session_path_id(path).is_some()
}
fn session_path_id(path: &str) -> Option<&str> {
    path.strip_prefix(SESSIONS)
        .and_then(|rest| rest.strip_suffix(".yaml"))
        .filter(|stem| !stem.contains('/') && *stem != "session_index")
}

/// One history source document: its path, exact text and parsed root (or
/// the parse failure).
pub struct Source<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub root: Result<&'a YamlNode, String>,
}

/// How an evidence occurrence names the observation or bound node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Basis {
    Structured,
    Literal,
}
impl Basis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Structured => "structured",
            Self::Literal => "literal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Gap {
    Missing,
    Ambiguous,
}
impl Gap {
    fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// One reason a count is unknown. `count` lets one example stand for many
/// occurrences (for instance a range of turns without stamps).
#[derive(Debug, Clone)]
struct Issue {
    code: &'static str,
    gap: Gap,
    example: String,
    count: u64,
}
impl Issue {
    fn new(code: &'static str, gap: Gap, example: impl Into<String>) -> Self {
        Self {
            code,
            gap,
            example: example.into(),
            count: 1,
        }
    }
    fn contradictory(example: impl Into<String>) -> Self {
        Self::new("history.contradictory", Gap::Ambiguous, example)
    }
}

#[derive(Debug, Default)]
struct SessionRecord {
    path: String,
    date: Option<String>,
    /// `None`: a legacy record without turn identities.
    turns: Option<u64>,
    /// `turn_count` is present but not a nonnegative integer.
    invalid_count: bool,
    started: Option<i128>,
    last_turn: Option<i128>,
    /// Stamps actually recorded, by turn; never one entry per claimed turn.
    stamps: BTreeMap<u64, i128>,
    archived: BTreeSet<u64>,
    contradictions: Vec<String>,
    import_keys: BTreeSet<String>,
    has_content: bool,
}
impl SessionRecord {
    /// Whether the record logged at least one turn on its date.
    fn logged(&self) -> bool {
        match self.turns {
            Some(n) => n > 0,
            None => self.has_content,
        }
    }
}

/// The parsed timeline of logged turns and indexed reference occurrences.
pub struct Timeline<'a> {
    docs: Vec<&'a Source<'a>>,
    sessions: BTreeMap<String, SessionRecord>,
    mentions: BTreeMap<String, Vec<Mention>>,
    /// Issues that make every turn count unknown.
    turn_issues: Vec<Issue>,
    /// Issues that make every day count unknown.
    day_issues: Vec<Issue>,
    /// Placeable session ID -> cluster index; overlapping sessions share one.
    clusters: BTreeMap<String, usize>,
    /// Sessions of each cluster, in placement order.
    cluster_list: Vec<Vec<String>>,
    cluster_bounds: Vec<(i128, i128)>,
    /// From cluster `k` on: the first overlapping cluster (if any) and the
    /// logged-turn total (`None` on overflow), so a count is O(1).
    suffix_overlap: Vec<Option<usize>>,
    suffix_turns: Vec<Option<u64>>,
}

/// The observation whose inactivity is measured.
pub struct Subject<'a> {
    pub id: &'a str,
    pub bound_to: Vec<&'a str>,
    /// The observation's staging `timestamp`, as written.
    pub timestamp: Option<&'a str>,
}

/// Restrictions for the write path: the stale operation's own turn proves
/// nothing, and eligible days end at the owning audit date.
#[derive(Default, Clone, Copy)]
pub struct Window<'a> {
    pub exclude: Option<(&'a str, u64)>,
    pub cutoff: Option<&'a str>,
}

/// One matched occurrence shown to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub source: String,
    pub line: usize,
    pub field: String,
    pub session: Option<String>,
    pub turn: Option<u64>,
    pub date: Option<String>,
    pub basis: Basis,
    pub literal: String,
    pub target: String,
    pub resolved_via: Option<String>,
    pub unresolved: bool,
    /// Stamped strictly before the staging instant: shown, not counted.
    pub before_staging: bool,
}
impl Evidence {
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "source": self.source,
            "line": self.line,
            "field": self.field,
            "session": self.session,
            "turn": self.turn,
            "date": self.date,
            "basis": self.basis.as_str(),
            "literal": self.literal,
            "target": self.target,
            "status": if self.unresolved {
                "unresolved"
            } else if self.before_staging {
                "before_staging"
            } else {
                "attributed"
            },
        });
        if let Some(via) = &self.resolved_via {
            value["resolved_via"] = json!(via);
        }
        value
    }
    fn field_label(&self) -> String {
        format!("{}:{} {}", self.source, self.line, self.field)
    }
}

/// An aggregated reason a count is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub status: &'static str,
    pub message: &'static str,
    pub count: u64,
    pub examples: Vec<String>,
}
impl Diagnostic {
    pub fn to_json(&self) -> Value {
        json!({"code":self.code,"status":self.status,"message":self.message,"count":self.count,"examples":self.examples})
    }
}

/// Measured inactivity of one observation, with its limits.
#[derive(Debug, Clone)]
pub struct Inactivity {
    pub turns_since_reference: Option<u64>,
    pub session_days_since_reference: Option<u64>,
    pub last_reference_turn: Option<String>,
    pub last_reference_date: Option<String>,
    /// `structured`, `literal`, `staging_timestamp`, or unknown.
    pub reference_basis: Option<&'static str>,
    pub evidence: Vec<Evidence>,
    /// `complete`, `missing` or `ambiguous`.
    pub history_status: &'static str,
    pub diagnostics: Vec<Diagnostic>,
    /// Eligible logged dates after the last reference, with the session
    /// documents that logged a turn on each; empty when the day count is unknown.
    pub eligible_days: Vec<(String, Vec<String>)>,
}
impl Inactivity {
    /// The additive `open` row fields.
    pub fn to_json(&self) -> Value {
        json!({
            "turns_since_reference": self.turns_since_reference,
            "session_days_since_reference": self.session_days_since_reference,
            "last_reference_turn": self.last_reference_turn,
            "last_reference_date": self.last_reference_date,
            "reference_basis": self.reference_basis,
            "evidence_sources": self.evidence.iter().map(Evidence::to_json).collect::<Vec<_>>(),
            "history_status": self.history_status,
            "history_diagnostics": self.diagnostics.iter().map(Diagnostic::to_json).collect::<Vec<_>>(),
        })
    }
}

fn message(code: &str) -> &'static str {
    match code {
        "history.invalid_source" => {
            "A history source cannot be parsed, so its turns and dates are unknown"
        }
        "history.alias_invalid" => {
            "Merge aliases cannot be decoded, so imported literals cannot be attributed"
        }
        "history.legacy_session" => {
            "A session record has no turn identities, so turn order cannot be proven"
        }
        "history.turn_stamp_missing" => {
            "A logged turn has no timestamp, so turn order cannot be proven"
        }
        "history.contradictory" => "Session metadata, archived stamps or turn rows disagree",
        "history.session_missing" => "History names a session that has no session record",
        "history.overlap" => {
            "Session turn intervals overlap or share a timestamp, so cross-session turn order is unknown"
        }
        "history.undated_turn" => "A logged turn has no written date",
        "history.reference_undated" => "A reference has no attributable date",
        "history.reference_turn_unknown" => "A reference has no attributable turn",
        "history.unresolved_reference" => "An imported literal has no authenticated redirect",
        "history.creation_evidence_missing" => {
            "The observation has no staging turn or staging timestamp"
        }
        "history.staging_position" => {
            "The staging timestamp does not place the observation among logged turns"
        }
        _ => "History is incomplete",
    }
}

/// The references of one subject, split by how they may be used.
struct References<'t> {
    relevant: Vec<&'t Mention>,
    attributable: Vec<&'t Mention>,
    unresolved: Vec<&'t Mention>,
    /// Full staging instant, when the timestamp has a time of day.
    staged_at: Option<i128>,
    /// Staging date when the timestamp is a date only.
    staged_day: Option<String>,
    /// Written date of the staging timestamp, either form.
    staged: Option<String>,
}

struct DayCount {
    days: Option<u64>,
    start_date: Option<String>,
    eligible: BTreeMap<String, BTreeSet<String>>,
    issues: Vec<Issue>,
}

struct TurnCount {
    turns: Option<u64>,
    last_reference_turn: Option<String>,
    basis: Option<&'static str>,
    issues: Vec<Issue>,
}

impl<'a> Timeline<'a> {
    /// Parse the history sources once. `aliases` is the raw
    /// `trace/aliases.yaml` bytes, when present.
    pub fn build(sources: &'a [Source<'a>], aliases: Option<&[u8]>) -> Self {
        let mut timeline = Self {
            docs: sources.iter().collect(),
            sessions: BTreeMap::new(),
            mentions: BTreeMap::new(),
            turn_issues: Vec::new(),
            day_issues: Vec::new(),
            clusters: BTreeMap::new(),
            cluster_list: Vec::new(),
            cluster_bounds: Vec::new(),
            suffix_overlap: Vec::new(),
            suffix_turns: Vec::new(),
        };
        let aliases = timeline.read_aliases(aliases);
        let mut mentions = Vec::new();
        let mut reasoning = None;
        let mut index = None;
        for (doc, source) in sources.iter().enumerate() {
            if source.path == REASONING {
                reasoning = Some(doc);
            } else if source.path == INDEX {
                index = Some(doc);
            } else if let Some(stem) = session_path_id(source.path) {
                timeline.read_session(doc, stem, &aliases, &mut mentions);
            }
        }
        if let Some(doc) = reasoning {
            timeline.read_reasoning(doc, &aliases, &mut mentions);
        }
        timeline.attribute_rolling_fallback(&aliases, &mut mentions);
        if let Some(doc) = index {
            timeline.check_index(doc);
        }
        timeline.place_and_cluster();
        for mention in mentions {
            timeline
                .mentions
                .entry(mention.target.clone())
                .or_default()
                .push(mention);
        }
        timeline
    }

    /// Record an issue that makes both counts unknown.
    fn push_both(&mut self, issue: Issue) {
        self.turn_issues.push(issue.clone());
        self.day_issues.push(issue);
    }

    fn read_aliases(&mut self, raw: Option<&[u8]>) -> Aliases {
        let mut aliases = Aliases::default();
        let Some(raw) = raw else {
            return aliases;
        };
        match crate::merge::alias_rows(raw) {
            Ok(rows) => {
                for (key, original, target) in rows {
                    if validate_session_id(&target).is_ok() {
                        aliases
                            .imported
                            .entry(target.clone())
                            .or_default()
                            .insert(key.clone());
                    }
                    aliases
                        .redirects
                        .entry((key, original))
                        .or_default()
                        .insert(target);
                }
            }
            Err(error) => self.push_both(Issue::new(
                "history.alias_invalid",
                Gap::Ambiguous,
                format!("{ALIASES}: {}", error.message),
            )),
        }
        aliases
    }

    /// One session record: metadata, recorded stamps and its turn rows.
    fn read_session(
        &mut self,
        doc: usize,
        stem: &str,
        aliases: &Aliases,
        mentions: &mut Vec<Mention>,
    ) {
        let source = self.docs[doc];
        let root = match &source.root {
            Ok(root) => *root,
            Err(error) => {
                self.push_both(Issue::new(
                    "history.invalid_source",
                    Gap::Missing,
                    format!("{}: {error}", source.path),
                ));
                return;
            }
        };
        let Some(metadata) = get(root, "session") else {
            self.push_both(Issue::new(
                "history.invalid_source",
                Gap::Missing,
                format!("{}: missing session metadata", source.path),
            ));
            return;
        };
        let mut record = SessionRecord {
            path: source.path.to_owned(),
            ..SessionRecord::default()
        };
        let id = scalar(metadata, "id").unwrap_or(stem);
        if id != stem {
            record.contradictions.push(format!(
                "{}: metadata id `{id}` differs from file name",
                source.path
            ));
        }
        if let Some(keys) = aliases.imported.get(stem) {
            record.import_keys = keys.clone();
        }
        record.date = scalar(metadata, "date")
            .or_else(|| scalar(metadata, "timestamp"))
            .and_then(written_date);
        record.started = scalar(metadata, "started").and_then(|s| timestamp_key(s).ok());
        record.last_turn = scalar(metadata, "last_turn").and_then(|s| timestamp_key(s).ok());
        record.turns = match get(metadata, "turn_count") {
            None => None,
            Some(node) => match node.scalar().and_then(|s| s.parse::<u64>().ok()) {
                Some(n) => Some(n),
                // A present but unreadable count: no turn identities can be
                // attributed (0), and the record contradicts itself. The day
                // count fails closed too: whether it logged a turn is unknown.
                None => {
                    record.invalid_count = true;
                    record.contradictions.push(format!(
                        "{}: turn_count is not a nonnegative integer",
                        source.path
                    ));
                    Some(0)
                }
            },
        };
        match record.turns {
            Some(n) => {
                self.read_turn_rows(doc, stem, root, metadata, &mut record, n, aliases, mentions)
            }
            None => Self::read_legacy(doc, stem, root, &mut record, aliases, mentions),
        }
        self.sessions.insert(stem.to_owned(), record);
    }

    #[allow(clippy::too_many_arguments)]
    fn read_turn_rows(
        &self,
        doc: usize,
        stem: &str,
        root: &YamlNode,
        metadata: &YamlNode,
        record: &mut SessionRecord,
        n: u64,
        aliases: &Aliases,
        mentions: &mut Vec<Mention>,
    ) {
        let path = self.docs[doc].path;
        if n > 0 {
            if let (Some(started), Some(last)) = (record.started, record.last_turn)
                && last < started
            {
                record
                    .contradictions
                    .push(format!("{path}: last_turn precedes started"));
            }
            if let (Some(last), Some(text)) = (record.last_turn, scalar(metadata, "last_turn")) {
                record.stamps.insert(n, last);
                if record.date.as_deref() != Some(&text[..10]) {
                    record.contradictions.push(format!(
                        "{path}: last_turn is not dated on the session date"
                    ));
                }
            }
        }
        for key in TURN_ARRAYS {
            let Some(YamlKind::Sequence(rows)) = get(root, key).map(|n| &n.kind) else {
                continue;
            };
            for (row_index, row) in rows.iter().enumerate() {
                let turn = scalar(row, "turn").and_then(|s| s.parse::<u64>().ok());
                let unit = match turn {
                    Some(t) if t >= 1 && t <= n => UnitRef::Turn(stem.to_owned(), t),
                    _ => {
                        record.contradictions.push(format!(
                            "{path}: {key}[{row_index}] turn is outside session history"
                        ));
                        UnitRef::Session(stem.to_owned())
                    }
                };
                let mut collector = Collector::new(doc, unit, &record.import_keys, aliases);
                collector.walk(row, &format!("{key}[{row_index}]"), None);
                mentions.extend(collector.out);
            }
        }
    }

    /// Legacy record: one dated unit whose whole caller text counts.
    fn read_legacy(
        doc: usize,
        stem: &str,
        root: &YamlNode,
        record: &mut SessionRecord,
        aliases: &Aliases,
        mentions: &mut Vec<Mention>,
    ) {
        let mut collector = Collector::new(
            doc,
            UnitRef::Session(stem.to_owned()),
            &record.import_keys,
            aliases,
        );
        if let YamlKind::Mapping(entries) = &root.kind {
            for (name, value) in entries {
                let Some(name) = name.scalar() else { continue };
                if name == "session" {
                    if let YamlKind::Mapping(meta) = &value.kind {
                        for (key, value) in meta {
                            let Some(key) = key.scalar() else { continue };
                            if METADATA_KEYS.contains(&key) {
                                continue;
                            }
                            record.has_content = true;
                            collector.walk(value, &format!("session.{key}"), Some(key));
                        }
                    }
                    continue;
                }
                if collect::EXCLUDED_KEYS.contains(&name) {
                    continue;
                }
                if !matches!(&value.kind, YamlKind::Sequence(items) if items.is_empty()) {
                    record.has_content = true;
                }
                collector.walk(value, name, Some(name));
            }
        }
        mentions.extend(collector.out);
    }

    /// The local session a reasoning entry names, directly or via an alias.
    fn resolve_session(&self, aliases: &Aliases, session: &str) -> Option<String> {
        if self.sessions.contains_key(session) {
            Some(session.to_owned())
        } else {
            aliases
                .session_redirect(session)
                .filter(|target| self.sessions.contains_key(target))
        }
    }

    /// Reasoning log: per-turn stamps, rolling-field changes and notes.
    fn read_reasoning(&mut self, doc: usize, aliases: &Aliases, mentions: &mut Vec<Mention>) {
        let root = match &self.docs[doc].root {
            Ok(root) => *root,
            Err(error) => {
                self.push_both(Issue::new(
                    "history.invalid_source",
                    Gap::Missing,
                    format!("{REASONING}: {error}"),
                ));
                return;
            }
        };
        let entries = match get(root, "entries").map(|n| &n.kind) {
            Some(YamlKind::Sequence(entries)) => entries.as_slice(),
            _ => &[],
        };
        for (index, entry) in entries.iter().enumerate() {
            let field = format!("entries[{index}]");
            if let Some(metadata) = get(entry, "session_metadata") {
                self.read_archive(doc, entry, metadata, &field, aliases, mentions);
            } else if !stale_record(entry) {
                self.read_note(doc, entry, &field, aliases, mentions);
            }
        }
    }

    /// One archived `session_metadata` transition: the turn's stamp and the
    /// rolling fields it changed.
    fn read_archive(
        &mut self,
        doc: usize,
        entry: &YamlNode,
        metadata: &YamlNode,
        field: &str,
        aliases: &Aliases,
        mentions: &mut Vec<Mention>,
    ) {
        let Some((session, t)) = scalar(entry, "turn").and_then(turn_reference) else {
            return;
        };
        let Some(resolved) = self.resolve_session(aliases, &session) else {
            self.turn_issues.push(Issue::new(
                "history.session_missing",
                Gap::Missing,
                format!("{session}#{t} ({REASONING} {field})"),
            ));
            return;
        };
        let after = get(metadata, "after");
        let before = get(metadata, "before");
        let stamp = after
            .and_then(|a| scalar(a, "last_turn"))
            .and_then(|s| timestamp_key(s).ok().map(|k| (k, s)));
        let count = after
            .and_then(|a| scalar(a, "turn_count"))
            .and_then(|s| s.parse::<u64>().ok());
        let record = self.sessions.get_mut(&resolved).expect("resolved");
        if scalar(metadata, "session").is_some_and(|s| s != session) {
            record.contradictions.push(format!(
                "{REASONING} {field}: archived session differs from its turn"
            ));
        }
        if record.turns.is_some_and(|n| t > n) {
            record.contradictions.push(format!(
                "{REASONING} {field}: archived turn {t} is beyond the session's turn_count"
            ));
        }
        if count != Some(t) {
            record.contradictions.push(format!(
                "{REASONING} {field}: archived turn_count differs from turn {t}"
            ));
        }
        match stamp {
            Some((key, text)) => {
                if record.date.as_deref() != Some(&text[..10]) {
                    record.contradictions.push(format!(
                        "{REASONING} {field}: turn {t} stamp is not dated on the session date"
                    ));
                }
                match record.stamps.get(&t) {
                    Some(existing) if *existing != key => record.contradictions.push(format!(
                        "{REASONING} {field}: turn {t} has two different stamps"
                    )),
                    _ => {
                        record.stamps.insert(t, key);
                    }
                }
            }
            None => record.contradictions.push(format!(
                "{REASONING} {field}: archived turn {t} has no valid last_turn"
            )),
        }
        record.archived.insert(t);
        let mut collector = Collector::new(
            doc,
            UnitRef::Turn(resolved, t),
            &record.import_keys,
            aliases,
        );
        if let Some(after) = after {
            // The first turn owns the summary its session was started with;
            // later turns own changes.
            let previous = before
                .filter(|b| scalar(b, "turn_count") != Some("0"))
                .and_then(|b| scalar(b, "summary"));
            if let Some(summary) = get(after, "summary")
                && let Some(text) = summary.scalar()
                && previous != Some(text)
            {
                collector.literal_text(
                    text,
                    summary.start,
                    &format!("{field}.session_metadata.after.summary"),
                );
            }
            for key in ROLLING_LISTS {
                let old: BTreeSet<&str> = sequence_strings(before.and_then(|b| get(b, key)))
                    .into_iter()
                    .map(|(s, _)| s)
                    .collect();
                for (i, (item, offset)) in sequence_strings(get(after, key)).into_iter().enumerate()
                {
                    if !old.contains(item) {
                        collector.literal_text(
                            item,
                            offset,
                            &format!("{field}.session_metadata.after.{key}[{i}]"),
                        );
                    }
                }
            }
        }
        mentions.extend(collector.out);
    }

    /// One caller reasoning record attributed to its turn or session.
    fn read_note(
        &mut self,
        doc: usize,
        entry: &YamlNode,
        field: &str,
        aliases: &Aliases,
        mentions: &mut Vec<Mention>,
    ) {
        let turn = scalar(entry, "turn").and_then(turn_reference);
        let named = scalar(entry, "session").filter(|s| validate_session_id(s).is_ok());
        let (unit, keys) = match (turn, named) {
            (Some((session, t)), _) => match self.resolve_session(aliases, &session) {
                Some(resolved) => {
                    let record = self.sessions.get_mut(&resolved).expect("resolved");
                    let keys = record.import_keys.clone();
                    match record.turns {
                        // A turn the session never logged is a contradiction,
                        // not a position.
                        Some(n) if t > n => {
                            record.contradictions.push(format!(
                                "{REASONING} {field}: turn {t} is beyond the session's turn_count {n}"
                            ));
                            (UnitRef::Session(resolved), keys)
                        }
                        _ => (UnitRef::Turn(resolved, t), keys),
                    }
                }
                None => {
                    self.turn_issues.push(Issue::new(
                        "history.session_missing",
                        Gap::Missing,
                        format!("{session}#{t} ({REASONING} {field})"),
                    ));
                    (UnitRef::Turn(session, t), BTreeSet::new())
                }
            },
            (None, Some(session)) => match self.resolve_session(aliases, session) {
                Some(resolved) => {
                    let keys = self.sessions[&resolved].import_keys.clone();
                    (UnitRef::Session(resolved), keys)
                }
                None => {
                    self.turn_issues.push(Issue::new(
                        "history.session_missing",
                        Gap::Missing,
                        format!("{session} ({REASONING} {field})"),
                    ));
                    (UnitRef::Session(session.to_owned()), BTreeSet::new())
                }
            },
            (None, None) => (UnitRef::Unattributed, BTreeSet::new()),
        };
        let mut collector = Collector::new(doc, unit, &keys, aliases);
        collector.walk_mapping(entry, field, &["session"]);
        mentions.extend(collector.out);
    }

    /// Rolling fields written by a CLI session's last turn when that turn
    /// has no archive record.
    fn attribute_rolling_fallback(&self, aliases: &Aliases, mentions: &mut Vec<Mention>) {
        for (id, record) in &self.sessions {
            let Some(n) = record.turns.filter(|n| *n > 0) else {
                continue;
            };
            if record.archived.contains(&n) {
                continue;
            }
            let Some(doc) = self.docs.iter().position(|s| s.path == record.path) else {
                continue;
            };
            let Ok(root) = &self.docs[doc].root else {
                continue;
            };
            let mut collector = Collector::new(
                doc,
                UnitRef::Turn(id.clone(), n),
                &record.import_keys,
                aliases,
            );
            if let Some(metadata) = get(root, "session")
                && let Some(summary) = get(metadata, "summary")
                && let Some(text) = summary.scalar()
            {
                collector.literal_text(text, summary.start, "session.summary");
            }
            for key in ROLLING_LISTS {
                for (i, (item, offset)) in sequence_strings(get(root, key)).into_iter().enumerate()
                {
                    collector.literal_text(item, offset, &format!("{key}[{i}]"));
                }
            }
            mentions.extend(collector.out);
        }
    }

    /// Index rows naming a session without a record.
    fn check_index(&mut self, doc: usize) {
        let Ok(root) = &self.docs[doc].root else {
            return;
        };
        let Some(YamlKind::Sequence(rows)) = get(root, "sessions").map(|n| &n.kind) else {
            return;
        };
        for row in rows {
            if let Some(id) = scalar(row, "id")
                && !self.sessions.contains_key(id)
            {
                self.turn_issues.push(Issue::new(
                    "history.session_missing",
                    Gap::Missing,
                    format!("{id} ({INDEX})"),
                ));
            }
        }
    }

    /// Check each session's turn evidence and cluster the placeable ones.
    fn place_and_cluster(&mut self) {
        let mut placed = Vec::new();
        let mut turn_issues = Vec::new();
        let mut day_issues = Vec::new();
        for (id, record) in &self.sessions {
            match record.turns {
                None if record.has_content => {
                    turn_issues.push(Issue::new("history.legacy_session", Gap::Missing, id));
                }
                None => {}
                Some(n) => {
                    let placeable = Self::check_turns(id, record, n, &mut turn_issues);
                    if placeable && n > 0 {
                        let first = record.stamps.range(1..=n).next().map(|(_, s)| *s);
                        let last = record.stamps.get(&n).copied();
                        if let (Some(first), Some(last)) = (first, last) {
                            placed.push((first, last, id.clone()));
                        }
                    }
                }
            }
            if record.logged() && record.date.is_none() {
                day_issues.push(Issue::new("history.undated_turn", Gap::Missing, id));
            }
            if record.invalid_count {
                day_issues.push(Issue::contradictory(format!(
                    "{}: turn_count is not a nonnegative integer",
                    record.path
                )));
            }
        }
        self.turn_issues.extend(turn_issues);
        self.day_issues.extend(day_issues);
        // Clusters of sessions whose turn intervals overlap or touch.
        placed.sort();
        for (first, last, id) in placed {
            match self.cluster_bounds.last_mut() {
                Some((_, end)) if first <= *end => {
                    *end = (*end).max(last);
                    self.cluster_list
                        .last_mut()
                        .expect("cluster")
                        .push(id.clone());
                }
                _ => {
                    self.cluster_bounds.push((first, last));
                    self.cluster_list.push(vec![id.clone()]);
                }
            }
            self.clusters.insert(id, self.cluster_bounds.len() - 1);
        }
        let clusters = self.cluster_list.len();
        self.suffix_overlap = vec![None; clusters + 1];
        self.suffix_turns = vec![Some(0); clusters + 1];
        for cluster in (0..clusters).rev() {
            let members = &self.cluster_list[cluster];
            self.suffix_overlap[cluster] = if members.len() > 1 {
                Some(cluster)
            } else {
                self.suffix_overlap[cluster + 1]
            };
            // Overflow leaves the total unknown (`None`), never saturated.
            self.suffix_turns[cluster] = self.suffix_turns[cluster + 1].and_then(|later| {
                members.iter().try_fold(later, |total, id| {
                    total.checked_add(self.sessions[id].turns.unwrap_or(0))
                })
            });
        }
    }

    /// Whether a session's `n` turns are fully and consistently stamped.
    /// Missing stamps are counted by range, with a few example turns, never
    /// enumerated per claimed turn.
    fn check_turns(id: &str, record: &SessionRecord, n: u64, issues: &mut Vec<Issue>) -> bool {
        let mut placeable = true;
        for contradiction in &record.contradictions {
            issues.push(Issue::contradictory(contradiction.clone()));
            placeable = false;
        }
        if n == 0 {
            return placeable;
        }
        let recorded = record.stamps.range(1..=n).count() as u64;
        let missing = n - recorded; // recorded <= n: stamps are keyed within 1..=n
        if missing > 0 {
            let mut examples = Vec::new();
            let mut t = 1u64;
            while examples.len() < EXAMPLES && t <= n {
                if !record.stamps.contains_key(&t) {
                    examples.push(t);
                }
                t += 1;
            }
            for (i, t) in examples.iter().enumerate() {
                let mut issue = Issue::new(
                    "history.turn_stamp_missing",
                    Gap::Missing,
                    format!("{id}#{t}"),
                );
                // The first example stands for every turn the others do not.
                if i == 0 {
                    issue.count = missing - (examples.len() as u64 - 1);
                }
                issues.push(issue);
            }
            placeable = false;
        }
        // A session that archives its turns but claims more turns than its
        // archive and `last_turn` evidence reach is contradictory.
        if let Some(max) = record.archived.last()
            && n > max.saturating_add(1)
        {
            issues.push(Issue::contradictory(format!(
                "{id}: turn_count {n} is beyond its archived turns (last archived turn {max})"
            )));
            placeable = false;
        }
        if record.archived.iter().any(|t| *t > n) {
            issues.push(Issue::contradictory(format!(
                "{id}: archived turn beyond turn_count"
            )));
            placeable = false;
        }
        let stamps: Vec<i128> = record.stamps.range(1..=n).map(|(_, s)| *s).collect();
        if stamps.windows(2).any(|w| w[1] < w[0])
            || record
                .started
                .is_some_and(|s| stamps.first().is_some_and(|f| *f < s))
        {
            issues.push(Issue::contradictory(format!(
                "{id}: turn stamps disagree with turn order"
            )));
            placeable = false;
        }
        placeable
    }

    fn unit_date(&self, unit: &UnitRef) -> Option<String> {
        match unit {
            // A session without a record has no attributable written date.
            UnitRef::Turn(session, _) | UnitRef::Session(session) => {
                self.sessions.get(session).and_then(|r| r.date.clone())
            }
            UnitRef::Unattributed => None,
        }
    }
    fn excluded(unit: &UnitRef, window: &Window<'_>) -> bool {
        matches!((unit, window.exclude), (UnitRef::Turn(s, t), Some((es, et))) if s == es && *t == et)
    }
    fn line(&self, doc: usize, offset: usize) -> usize {
        let text = self.docs[doc].text;
        text.get(..offset.min(text.len()))
            .map_or(0, |prefix| prefix.bytes().filter(|b| *b == b'\n').count())
            + 1
    }
    fn evidence(&self, mention: &Mention) -> Evidence {
        let (session, turn) = match &mention.unit {
            UnitRef::Turn(s, t) => (Some(s.clone()), Some(*t)),
            UnitRef::Session(s) => (Some(s.clone()), None),
            UnitRef::Unattributed => (None, None),
        };
        Evidence {
            source: self.docs[mention.doc].path.to_owned(),
            line: self.line(mention.doc, mention.offset),
            field: mention.field.clone(),
            session,
            turn,
            date: self.unit_date(&mention.unit),
            basis: mention.basis,
            literal: mention.literal.clone(),
            target: mention.target.clone(),
            resolved_via: mention.via.clone(),
            unresolved: mention.unresolved,
            before_staging: false,
        }
    }
    fn label(&self, mention: &Mention) -> String {
        self.evidence(mention).field_label()
    }
    /// The cluster position of a turn in a placeable session.
    fn position(&self, unit: &UnitRef) -> Option<(usize, String, u64)> {
        match unit {
            UnitRef::Turn(session, t) => self
                .clusters
                .get(session)
                .map(|cluster| (*cluster, session.clone(), *t)),
            _ => None,
        }
    }
    fn unit_stamp(&self, unit: &UnitRef) -> Option<i128> {
        match unit {
            UnitRef::Turn(session, t) => self.sessions.get(session)?.stamps.get(t).copied(),
            _ => None,
        }
    }
    fn turn_total(&self, session: &str) -> u64 {
        self.sessions[session].turns.unwrap_or(0)
    }
    fn cluster_members(&self, cluster: usize) -> String {
        self.cluster_list[cluster].join(", ")
    }
    /// Whether a reference predates the staging timestamp: stamped strictly
    /// before its instant, or (for a date-only stamp) on an earlier date.
    fn before_staging(&self, references: &References<'_>, mention: &Mention) -> bool {
        references.staged_at.is_some_and(|at| {
            self.unit_stamp(&mention.unit)
                .is_some_and(|stamp| stamp < at)
        }) || references.staged_day.as_ref().is_some_and(|day| {
            self.unit_date(&mention.unit)
                .is_some_and(|date| &date < day)
        })
    }

    /// Measure the subject's explicit-reference inactivity.
    pub fn measure(&self, subject: &Subject<'_>, window: Window<'_>) -> Inactivity {
        let references = self.references(subject, &window);
        let days = self.day_count(subject, &references, &window);
        let turns = self.turn_count(subject, &references);
        self.summarize(&references, days, turns)
    }

    /// The subject's occurrences outside the excluded turn, ordered by
    /// date and position, split into counted and unresolved references.
    fn references(&self, subject: &Subject<'_>, window: &Window<'_>) -> References<'_> {
        let mut keys: BTreeSet<&str> = subject.bound_to.iter().copied().collect();
        keys.insert(subject.id);
        let mut relevant: Vec<&Mention> = keys
            .iter()
            .filter_map(|key| self.mentions.get(*key))
            .flatten()
            .filter(|m| !Self::excluded(&m.unit, window))
            .collect();
        relevant.sort_by(|a, b| {
            (self.unit_date(&a.unit), &a.unit, a.doc, a.offset).cmp(&(
                self.unit_date(&b.unit),
                &b.unit,
                b.doc,
                b.offset,
            ))
        });
        // A reference stamped strictly before the staging instant predates
        // the observation: shown, never the latest attributable reference.
        // One at the same instant is the staging turn (one clock per batch).
        // A date-only staging stamp places the observation somewhere on that
        // date: earlier dates predate it, the same date is ambiguous.
        let mut references = References {
            relevant: Vec::new(),
            attributable: Vec::new(),
            unresolved: Vec::new(),
            staged_at: subject
                .timestamp
                .filter(|text| text.len() > 10)
                .and_then(|text| timestamp_key(text).ok()),
            staged_day: subject
                .timestamp
                .filter(|text| text.len() == 10)
                .and_then(written_date),
            staged: subject.timestamp.and_then(written_date),
        };
        references.attributable = relevant
            .iter()
            .copied()
            .filter(|m| !m.unresolved && !self.before_staging(&references, m))
            .collect();
        references.unresolved = relevant.iter().copied().filter(|m| m.unresolved).collect();
        references.relevant = relevant;
        references
    }

    /// Day count: distinct later dates with logged turns.
    fn day_count(
        &self,
        subject: &Subject<'_>,
        references: &References<'_>,
        window: &Window<'_>,
    ) -> DayCount {
        let mut issues = Vec::new();
        let mut start_date = references.staged.clone();
        for mention in &references.attributable {
            match self.unit_date(&mention.unit) {
                Some(date) => {
                    if start_date.as_ref().is_none_or(|start| &date > start) {
                        start_date = Some(date);
                    }
                }
                None => issues.push(Issue::new(
                    "history.reference_undated",
                    Gap::Missing,
                    self.label(mention),
                )),
            }
        }
        if start_date.is_none() && references.attributable.is_empty() {
            issues.push(Issue::new(
                "history.creation_evidence_missing",
                Gap::Missing,
                subject.id,
            ));
        }
        if let Some(start) = &start_date {
            for mention in &references.unresolved {
                if self
                    .unit_date(&mention.unit)
                    .is_none_or(|date| &date >= start)
                {
                    issues.push(Issue::new(
                        "history.unresolved_reference",
                        Gap::Ambiguous,
                        self.label(mention),
                    ));
                }
            }
        }
        let mut eligible: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        if let Some(start) = &start_date {
            for (id, record) in &self.sessions {
                let Some(date) = &record.date else {
                    continue; // reported once as a timeline day issue
                };
                if !record.logged()
                    || date <= start
                    || window.cutoff.is_some_and(|cutoff| date.as_str() > cutoff)
                {
                    continue;
                }
                // The excluded turn proves nothing; the session still counts
                // when it logged another turn.
                let only_excluded = matches!(
                    (record.turns, window.exclude),
                    (Some(1), Some((session, 1))) if session == id
                );
                if !only_excluded {
                    eligible
                        .entry(date.clone())
                        .or_default()
                        .insert(record.path.clone());
                }
            }
        }
        let days = (issues.is_empty() && self.day_issues.is_empty() && start_date.is_some())
            .then_some(eligible.len() as u64);
        DayCount {
            days,
            start_date,
            eligible,
            issues,
        }
    }

    /// Turn count: logged turns strictly after the latest attributable
    /// reference, or after the staging instant without one.
    fn turn_count(&self, subject: &Subject<'_>, references: &References<'_>) -> TurnCount {
        let mut count = TurnCount {
            turns: None,
            last_reference_turn: None,
            basis: None,
            issues: Vec::new(),
        };
        let attributable = &references.attributable;
        if attributable.is_empty() && references.staged.is_none() {
            count.issues.push(Issue::new(
                "history.creation_evidence_missing",
                Gap::Missing,
                subject.id,
            ));
        }
        if attributable.is_empty() {
            count.basis = references.staged.as_ref().map(|_| "staging_timestamp");
        }
        if count.issues.is_empty() && self.turn_issues.is_empty() {
            count.turns = if attributable.is_empty() {
                self.turns_after_staging(
                    subject.timestamp,
                    &references.unresolved,
                    &mut count.issues,
                )
            } else {
                self.turns_after_reference(references, &mut count)
            };
        }
        // With a date-only staging stamp, a latest reference on the staging
        // date may precede creation: the turn count is not provable.
        if let Some(day) = &references.staged_day
            && count.turns.is_some()
            && !attributable.is_empty()
            && !attributable
                .iter()
                .any(|m| self.unit_date(&m.unit).is_some_and(|date| &date > day))
        {
            count.issues.push(Issue::new(
                "history.staging_position",
                Gap::Ambiguous,
                format!(
                    "staging date {day} has no time of day; a reference on that date may precede it"
                ),
            ));
            count.last_reference_turn = None;
            count.basis = None;
            count.turns = None;
        }
        if count.basis.is_none() && !attributable.is_empty() {
            // Unordered references: report a basis only when the latest-dated
            // references agree on it.
            let latest = attributable
                .iter()
                .filter_map(|m| self.unit_date(&m.unit))
                .max();
            let bases: BTreeSet<Basis> = attributable
                .iter()
                .filter(|m| self.unit_date(&m.unit) == latest)
                .map(|m| m.basis)
                .collect();
            if bases.len() == 1 {
                count.basis = bases.into_iter().next().map(Basis::as_str);
            }
        }
        count
    }

    fn turns_after_reference(
        &self,
        references: &References<'_>,
        count: &mut TurnCount,
    ) -> Option<u64> {
        let attributable = &references.attributable;
        let mut latest: Option<(usize, String, u64)> = None;
        for mention in attributable {
            let Some(position) = self.position(&mention.unit) else {
                count.issues.push(Issue::new(
                    "history.reference_turn_unknown",
                    Gap::Missing,
                    self.label(mention),
                ));
                return None;
            };
            // Within a cluster only one session can be placed; a larger
            // cluster is rejected below, so (cluster, turn) orders candidates.
            if latest
                .as_ref()
                .is_none_or(|best| (position.0, position.2) > (best.0, best.2))
            {
                latest = Some(position);
            }
        }
        let (cluster, session, turn) = latest?;
        if self.cluster_list[cluster].len() > 1 {
            count.issues.push(Issue::new(
                "history.overlap",
                Gap::Ambiguous,
                self.cluster_members(cluster),
            ));
            return None;
        }
        let bases: BTreeSet<Basis> = attributable
            .iter()
            .filter(|m| self.position(&m.unit) == Some((cluster, session.clone(), turn)))
            .map(|m| m.basis)
            .collect();
        // Structured evidence outranks a literal in the same turn.
        count.basis = bases.into_iter().next().map(Basis::as_str);
        count.last_reference_turn = Some(format!("{session}#{turn}"));
        for mention in &references.unresolved {
            match self.position(&mention.unit) {
                Some((c, s, t)) if c < cluster || (s == session && t <= turn) => {}
                _ => {
                    count.issues.push(Issue::new(
                        "history.unresolved_reference",
                        Gap::Ambiguous,
                        self.label(mention),
                    ));
                    return None;
                }
            }
        }
        if let Some(later) = self.suffix_overlap.get(cluster + 1).copied().flatten() {
            count.issues.push(Issue::new(
                "history.overlap",
                Gap::Ambiguous,
                self.cluster_members(later),
            ));
            return None;
        }
        let later = self
            .suffix_turns
            .get(cluster + 1)
            .copied()
            .unwrap_or(Some(0));
        let total = self
            .turn_total(&session)
            .checked_sub(turn)
            .zip(later)
            .and_then(|(after, later)| after.checked_add(later));
        if total.is_none() {
            count.issues.push(Issue::contradictory(format!(
                "{session}#{turn}: turn total beyond turn_count or overflowing"
            )));
        }
        total
    }

    fn turns_after_staging(
        &self,
        timestamp: Option<&str>,
        unresolved: &[&Mention],
        issues: &mut Vec<Issue>,
    ) -> Option<u64> {
        let text = timestamp?; // creation evidence issue already recorded
        let Some(at) = (text.len() > 10)
            .then(|| timestamp_key(text).ok())
            .flatten()
        else {
            issues.push(Issue::new(
                "history.staging_position",
                Gap::Ambiguous,
                format!("staging timestamp `{text}` has no time of day"),
            ));
            return None;
        };
        if let Some(mention) = unresolved.first() {
            issues.push(Issue::new(
                "history.unresolved_reference",
                Gap::Ambiguous,
                self.label(mention),
            ));
            return None;
        }
        let mut count: u64 = 0;
        for (cluster, (first, last)) in self.cluster_bounds.iter().enumerate() {
            if *last < at {
                continue;
            }
            if self.cluster_list[cluster].len() > 1 {
                issues.push(Issue::new(
                    "history.overlap",
                    Gap::Ambiguous,
                    self.cluster_members(cluster),
                ));
                return None;
            }
            let session = &self.cluster_list[cluster][0];
            let after = if *first > at {
                Some(self.turn_total(session))
            } else {
                let record = &self.sessions[session];
                if record.stamps.values().any(|stamp| *stamp == at) {
                    issues.push(Issue::new(
                        "history.staging_position",
                        Gap::Ambiguous,
                        format!("{session}: a turn shares the staging timestamp `{text}`"),
                    ));
                    return None;
                }
                // Placeable sessions have one recorded stamp per turn.
                Some(record.stamps.values().filter(|stamp| **stamp > at).count() as u64)
            };
            match after.and_then(|after| count.checked_add(after)) {
                Some(total) => count = total,
                None => {
                    issues.push(Issue::contradictory(format!(
                        "{session}: turn total overflows"
                    )));
                    return None;
                }
            }
        }
        Some(count)
    }

    /// Combine both counts into the row result and its diagnostics.
    fn summarize(
        &self,
        references: &References<'_>,
        days: DayCount,
        turns: TurnCount,
    ) -> Inactivity {
        let mut issues: Vec<&Issue> = Vec::new();
        if days.days.is_none() {
            issues.extend(days.issues.iter().chain(&self.day_issues));
        }
        if turns.turns.is_none() {
            issues.extend(turns.issues.iter().chain(&self.turn_issues));
        }
        let status = if turns.turns.is_some() && days.days.is_some() {
            "complete"
        } else if issues.iter().any(|i| i.gap == Gap::Ambiguous) {
            "ambiguous"
        } else {
            "missing"
        };
        let mut grouped: BTreeMap<&'static str, (Gap, BTreeMap<&str, u64>)> = BTreeMap::new();
        for issue in issues {
            let entry = grouped
                .entry(issue.code)
                .or_insert((issue.gap, BTreeMap::new()));
            entry.1.entry(&issue.example).or_insert(issue.count);
        }
        let diagnostics = grouped
            .into_iter()
            .map(|(code, (gap, examples))| Diagnostic {
                code,
                status: gap.as_str(),
                message: message(code),
                count: examples
                    .values()
                    .fold(0u64, |total, count| total.saturating_add(*count)),
                examples: examples
                    .into_keys()
                    .take(EXAMPLES)
                    .map(str::to_owned)
                    .collect(),
            })
            .collect();
        Inactivity {
            turns_since_reference: turns.turns,
            session_days_since_reference: days.days,
            last_reference_turn: turns.last_reference_turn,
            last_reference_date: days.start_date,
            reference_basis: turns.basis,
            evidence: references
                .relevant
                .iter()
                .map(|m| {
                    let mut evidence = self.evidence(m);
                    evidence.before_staging = !m.unresolved && self.before_staging(references, m);
                    evidence
                })
                .collect(),
            history_status: status,
            diagnostics,
            eligible_days: if days.days.is_some() {
                days.eligible
                    .into_iter()
                    .map(|(date, paths)| (date, paths.into_iter().collect()))
                    .collect()
            } else {
                Vec::new()
            },
        }
    }
}
