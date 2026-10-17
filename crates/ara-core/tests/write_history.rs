#![cfg(feature = "native")]
//! Plan 19 D1/D2: the shared explicit-reference session history.
use ara_core::write::history::{Inactivity, Source, Subject, Timeline, Window};
use ara_core::write::positions::YamlDocument;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A constructed history: CLI session records plus the reasoning log with
/// one archived `session_metadata` per logged turn.
#[derive(Default)]
struct History {
    sessions: BTreeMap<String, Value>,
    reasoning: Vec<Value>,
    raw: Vec<(String, String)>,
    aliases: Option<String>,
    alias_bytes: Option<Vec<u8>>,
    merge_log: Option<String>,
}
impl History {
    fn start(&mut self, id: &str, started: &str) -> &mut Self {
        self.sessions.insert(
            id.into(),
            json!({"session":{"id":id,"date":&id[..10],"started":started,"last_turn":started,"turn_count":0,"summary":"Started"},
                "events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]}),
        );
        self
    }
    /// Log a turn; `rows` holds turn arrays and optional `summary`/`open_threads`.
    fn log(&mut self, id: &str, stamp: &str, rows: Value) -> &mut Self {
        self.log_with(id, stamp, rows, true)
    }
    fn log_with(&mut self, id: &str, stamp: &str, rows: Value, archive: bool) -> &mut Self {
        let session = self.sessions.get_mut(id).unwrap();
        let state = |s: &Value| json!({"summary":s["session"]["summary"],"last_turn":s["session"]["last_turn"],"turn_count":s["session"]["turn_count"],"open_threads":s["open_threads"]});
        let before = state(session);
        let turn = session["session"]["turn_count"].as_u64().unwrap() + 1;
        session["session"]["turn_count"] = json!(turn);
        session["session"]["last_turn"] = json!(stamp);
        for (key, value) in rows.as_object().unwrap() {
            match key.as_str() {
                "summary" => session["session"]["summary"] = value.clone(),
                "open_threads" => session["open_threads"] = value.clone(),
                _ => {
                    for row in value.as_array().unwrap() {
                        let mut row = row.clone();
                        row["turn"] = json!(turn);
                        session[key].as_array_mut().unwrap().push(row);
                    }
                }
            }
        }
        let after = state(session);
        if archive {
            self.reasoning.push(json!({"turn":format!("{id}#{turn}"),"session_metadata":{"session":id,"before":before,"after":after}}));
        }
        self
    }
    fn note(&mut self, record: Value) -> &mut Self {
        self.reasoning.push(record);
        self
    }
    fn texts(&self) -> Vec<(String, String)> {
        let mut texts: Vec<(String, String)> = self
            .sessions
            .iter()
            .map(|(id, value)| (format!("trace/sessions/{id}.yaml"), value.to_string()))
            .collect();
        texts.push((
            "trace/pm_reasoning_log.yaml".into(),
            json!({"entries":self.reasoning}).to_string(),
        ));
        texts.extend(self.raw.iter().cloned());
        texts
    }
    fn measure(&self, subject: Subject<'_>, window: Window<'_>) -> Inactivity {
        let texts = self.texts();
        let parsed: Vec<Result<YamlDocument, String>> = texts
            .iter()
            .map(|(_, text)| YamlDocument::parse(text).map_err(|e| e.message))
            .collect();
        let sources: Vec<Source<'_>> = texts
            .iter()
            .zip(&parsed)
            .map(|((path, text), parsed)| Source {
                path,
                text,
                root: parsed.as_ref().map(|d| &d.root).map_err(Clone::clone),
            })
            .collect();
        Timeline::build(
            &sources,
            self.alias_bytes
                .as_deref()
                .or(self.aliases.as_deref().map(str::as_bytes)),
            self.merge_log.as_deref().map(str::as_bytes),
        )
        .measure(&subject, window)
    }
}
fn observation<'a>(id: &'a str, timestamp: Option<&'a str>) -> Subject<'a> {
    Subject {
        id,
        bound_to: Vec::new(),
        timestamp,
    }
}
fn staged(id: &str) -> Value {
    json!({"events_logged":[{"type":"observation","id":id,"routing":"staged","provenance":"user","summary":"Staged"}]})
}
fn codes(result: &Inactivity) -> Vec<&'static str> {
    result.diagnostics.iter().map(|d| d.code).collect()
}
/// Three sessions on three days; O01 staged in the first turn.
fn base() -> History {
    let mut h = History::default();
    h.start("2026-10-02_001", "2026-10-02T09:00Z")
        .log("2026-10-02_001", "2026-10-02T10:00Z", staged("O01"))
        .log(
            "2026-10-02_001",
            "2026-10-02T11:00Z",
            json!({"ai_actions":[{"action":"Unrelated work","provenance":"ai-executed","files_changed":[]}]}),
        )
        .start("2026-10-03_001", "2026-10-03T09:00Z")
        .log(
            "2026-10-03_001",
            "2026-10-03T10:00Z",
            json!({"summary":"Revisited O01 after the run"}),
        )
        .start("2026-10-04_001", "2026-10-04T09:00Z")
        .log("2026-10-04_001", "2026-10-04T10:00Z", json!({}))
        .log("2026-10-04_001", "2026-10-04T11:00Z", json!({}));
    h
}

#[test]
fn known_turn_and_day_counts_follow_the_latest_explicit_reference() {
    let h = base();
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, Some(2));
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(
        result.last_reference_turn.as_deref(),
        Some("2026-10-03_001#1")
    );
    assert_eq!(result.last_reference_date.as_deref(), Some("2026-10-03"));
    assert_eq!(result.reference_basis, Some("literal"));
    assert_eq!(result.history_status, "complete");
    assert!(result.diagnostics.is_empty());
    // Both references are shown with their source and turn identity.
    let shown: Vec<(String, Option<u64>, &str)> = result
        .evidence
        .iter()
        .map(|e| (e.session.clone().unwrap(), e.turn, e.basis.as_str()))
        .collect();
    assert_eq!(
        shown,
        vec![
            ("2026-10-02_001".into(), Some(1), "structured"),
            ("2026-10-03_001".into(), Some(1), "literal"),
        ]
    );
    let json = result.to_json();
    assert_eq!(
        json["evidence_sources"][0]["source"],
        "trace/sessions/2026-10-02_001.yaml"
    );
    assert_eq!(json["evidence_sources"][0]["field"], "events_logged[0].id");
    assert_eq!(
        json["evidence_sources"][1]["source"],
        "trace/pm_reasoning_log.yaml"
    );
}

#[test]
fn staging_turn_starts_the_count_without_later_references() {
    let h = base();
    let result = h.measure(
        observation("O02", Some("2026-10-02T10:30Z")),
        Window::default(),
    );
    // No reference: the timestamp places it between 10-02 turns 1 and 2.
    assert_eq!(result.turns_since_reference, Some(4));
    assert_eq!(result.session_days_since_reference, Some(2));
    assert_eq!(result.reference_basis, Some("staging_timestamp"));
    assert_eq!(result.history_status, "complete");
    // An equal timestamp is not a position.
    let equal = h.measure(
        observation("O02", Some("2026-10-02T11:00Z")),
        Window::default(),
    );
    assert_eq!(equal.turns_since_reference, None);
    assert_eq!(equal.session_days_since_reference, Some(2));
    assert_eq!(codes(&equal), ["history.staging_position"]);
    assert_eq!(equal.history_status, "ambiguous");
    // A date without a time of day cannot order turns on that date.
    let dated = h.measure(observation("O02", Some("2026-10-02")), Window::default());
    assert_eq!(dated.turns_since_reference, None);
    assert_eq!(dated.session_days_since_reference, Some(2));
    // The structured staging event is itself the attributable staging turn.
    let structured = h.measure(observation("O01", None), Window::default());
    assert_eq!(structured.turns_since_reference, Some(2));
}

#[test]
fn missing_creation_evidence_is_unknown_not_zero() {
    let h = base();
    let result = h.measure(observation("O09", None), Window::default());
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(result.reference_basis, None);
    assert_eq!(codes(&result), ["history.creation_evidence_missing"]);
    assert_eq!(result.history_status, "missing");
}

#[test]
fn bound_node_use_counts_as_a_structured_reference() {
    let mut h = base();
    h.log(
        "2026-10-04_001",
        "2026-10-04T12:00Z",
        json!({"events_logged":[{"type":"experiment","id":"N05","routing":"direct","provenance":"user","summary":"Ran it"}]}),
    );
    let mut subject = observation("O01", Some("2026-10-02T10:00Z"));
    subject.bound_to = vec!["N05"];
    let result = h.measure(subject, Window::default());
    assert_eq!(result.turns_since_reference, Some(0));
    assert_eq!(result.session_days_since_reference, Some(0));
    assert_eq!(
        result.last_reference_turn.as_deref(),
        Some("2026-10-04_001#3")
    );
    assert_eq!(result.reference_basis, Some("structured"));
    assert_eq!(result.evidence.last().unwrap().target, "N05");
}

#[test]
fn copied_audit_text_indexes_and_stale_records_are_not_new_references() {
    let mut h = base();
    h.log(
        "2026-10-04_001",
        "2026-10-04T12:00Z",
        json!({"open_threads":["Check O01 again"],
            "logic_revisions":[{"entry":"C01","field":"Statement","before":"See O01","after":"See O01 too","signal":"user-directive","provenance":"user"}]}),
    )
    // The thread is carried, not rewritten: an archived copy, not a reference.
    .log("2026-10-04_001", "2026-10-04T13:00Z", json!({}))
    .note(json!({"turn":"2026-10-04_001#4","notes":["Caller reason about O01", "{\"operation\":\"observation.mark_stale\",\"observation\":\"O01\"}"]}));
    h.raw.push((
        "trace/sessions/session_index.yaml".into(),
        json!({"sessions":[{"id":"2026-10-04_001","summary":"O01"}]}).to_string(),
    ));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(
        result.last_reference_turn.as_deref(),
        Some("2026-10-04_001#3")
    );
    assert_eq!(result.turns_since_reference, Some(1));
    assert!(
        result
            .evidence
            .iter()
            .all(|e| !e.field.contains("before") && !e.field.contains("after.after")),
        "{:?}",
        result.evidence
    );
    assert!(
        result
            .evidence
            .iter()
            .all(|e| !e.source.ends_with("session_index.yaml"))
    );
}

#[test]
fn semantic_mentions_and_substrings_are_not_id_references() {
    let mut h = base();
    h.log(
        "2026-10-04_001",
        "2026-10-04T12:00Z",
        json!({"key_context":[{"excerpt":"the observation about empty tables; O011, XO01 and O01a differ"}]}),
    );
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(
        result.last_reference_turn.as_deref(),
        Some("2026-10-03_001#1")
    );
    assert_eq!(result.turns_since_reference, Some(3));
}

#[test]
fn overlapping_sessions_keep_days_and_leave_turns_unknown() {
    let mut h = base();
    // A concurrent agent's session overlaps 2026-10-04_001.
    h.start("2026-10-04_002", "2026-10-04T09:30Z").log(
        "2026-10-04_002",
        "2026-10-04T10:30Z",
        json!({}),
    );
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(codes(&result), ["history.overlap"]);
    assert_eq!(result.history_status, "ambiguous");
    // Overlap entirely before the reference does not affect the count.
    let mut h = base();
    h.start("2026-10-02_002", "2026-10-02T09:30Z").log(
        "2026-10-02_002",
        "2026-10-02T10:30Z",
        json!({}),
    );
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, Some(2));
}

#[test]
fn equal_cross_session_timestamps_are_not_ordered_lexically() {
    let mut h = base();
    h.start("2026-10-04_002", "2026-10-04T11:00Z").log(
        "2026-10-04_002",
        "2026-10-04T11:00Z",
        json!({}),
    );
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(codes(&result), ["history.overlap"]);
}

#[test]
fn missing_turn_stamps_and_missing_sessions_leave_turns_unknown() {
    let mut h = base();
    h.log_with("2026-10-04_001", "2026-10-04T12:00Z", json!({}), false)
        .log("2026-10-04_001", "2026-10-04T13:00Z", json!({}));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(codes(&result), ["history.turn_stamp_missing"]);
    assert_eq!(result.history_status, "missing");

    let mut h = base();
    h.note(json!({"turn":"2026-10-05_001#1","session_metadata":{"session":"2026-10-05_001","before":{},"after":{"last_turn":"2026-10-05T10:00Z","turn_count":1}}}));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(codes(&result), ["history.session_missing"]);
    // A reference only a missing session holds has no attributable date.
    h.note(json!({"turn":"2026-10-05_001#1","notes":["Used O01"]}));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.session_days_since_reference, None);
    assert!(codes(&result).contains(&"history.reference_undated"));
}

#[test]
fn contradictory_metadata_makes_turn_order_ambiguous() {
    let mut h = base();
    h.reasoning[0]["session_metadata"]["after"]["turn_count"] = json!(7);
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(codes(&result), ["history.contradictory"]);
    assert_eq!(result.history_status, "ambiguous");
}

#[test]
fn empty_sessions_and_unlogged_dates_do_not_count() {
    let mut h = base();
    h.start("2026-10-05_001", "2026-10-05T09:00Z");
    h.start("2026-10-07_001", "2026-10-07T09:00Z").log(
        "2026-10-07_001",
        "2026-10-07T10:00Z",
        json!({}),
    );
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    // 10-04 and 10-07 have logged turns; 10-05 is empty and 10-06 unlogged.
    assert_eq!(result.session_days_since_reference, Some(2));
    assert_eq!(result.turns_since_reference, Some(3));
    let days: Vec<&str> = result
        .eligible_days
        .iter()
        .map(|(d, _)| d.as_str())
        .collect();
    assert_eq!(days, ["2026-10-04", "2026-10-07"]);
}

#[test]
fn legacy_sessions_count_days_but_not_turns() {
    let mut h = base();
    h.raw.push((
        "trace/sessions/2026-10-05_001.yaml".into(),
        "session:\n  id: \"2026-10-05_001\"\n  timestamp: \"2026-10-05\"\n  summary: \"Legacy work\"\nevents_logged:\n  - type: decision\n    id: N09\n".into(),
    ));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, Some(2));
    assert_eq!(codes(&result), ["history.legacy_session"]);
    // An undated legacy record makes the day count unknown too.
    h.raw.push((
        "trace/sessions/2026-10-06_001.yaml".into(),
        "session:\n  id: \"2026-10-06_001\"\n  summary: \"Undated work\"\n".into(),
    ));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.session_days_since_reference, None);
    assert!(codes(&result).contains(&"history.undated_turn"));
}

#[test]
fn legacy_import_aliases_without_occurrence_proof_keep_both_counts_unknown() {
    let mut h = base();
    h.start("2026-10-05_001", "2026-10-05T09:00Z").log(
        "2026-10-05_001",
        "2026-10-05T10:00Z",
        json!({"summary":"Peer revisited O03 and O07"}),
    );
    let alias = |original: &str, target: &str| {
        format!(
            "  - {}\n",
            json!({"source_key":"peer-fork","label":"peer","original":original,"target":target,"revision":"r1"})
        )
    };
    h.aliases = Some(format!(
        "format: ara.aliases/v1\naliases:\n{}{}",
        alias("2026-10-05_001", "2026-10-05_001"),
        alias("O03", "O01")
    ));
    // Redirects alone cannot prove whether a particular field is imported.
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(codes(&result), ["history.origin_unknown"]);
    assert!(!result.evidence.iter().any(|e| e.resolved_via.is_some()));
    // Unknown spellings also cannot be treated as authenticated local text.
    let result = h.measure(
        observation("O07", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(codes(&result), ["history.origin_unknown"]);
    assert_eq!(result.history_status, "ambiguous");
    assert_ne!(result.history_status, "complete");
    // A malformed alias ledger cannot authenticate anything.
    h.aliases = Some("format: nope\n".into());
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(codes(&result), ["history.alias_invalid"]);
}

#[test]
fn write_window_excludes_the_owning_turn_and_cuts_off_at_its_date() {
    let mut h = base();
    h.start("2026-10-05_001", "2026-10-05T09:00Z").log(
        "2026-10-05_001",
        "2026-10-05T10:00Z",
        json!({"summary":"Marking O01 stale"}),
    );
    let window = Window {
        exclude: Some(("2026-10-05_001", 1)),
        cutoff: Some("2026-10-05"),
    };
    let result = h.measure(observation("O01", Some("2026-10-02T10:00Z")), window);
    // The owner's own text is not a reference and its day proves nothing.
    assert_eq!(result.last_reference_date.as_deref(), Some("2026-10-03"));
    assert_eq!(result.session_days_since_reference, Some(1));
    let cut = h.measure(
        observation("O02", Some("2026-10-01T10:00Z")),
        Window {
            exclude: None,
            cutoff: Some("2026-10-03"),
        },
    );
    let days: Vec<&str> = cut.eligible_days.iter().map(|(d, _)| d.as_str()).collect();
    assert_eq!(days, ["2026-10-02", "2026-10-03"]);
}

#[test]
fn invalid_sources_are_reported_not_skipped() {
    let mut h = base();
    h.raw.push((
        "trace/sessions/2026-10-05_001.yaml".into(),
        "session: [unclosed".into(),
    ));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(codes(&result), ["history.invalid_source"]);
}

#[test]
fn reasoning_turns_beyond_the_session_are_contradictions_not_positions() {
    let mut h = base();
    h.note(json!({"turn":"2026-10-04_001#9","notes":["Used O01 later"]}));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(codes(&result), ["history.contradictory"]);
    assert_eq!(result.history_status, "ambiguous");
    // The reference still has its session's date for the day count.
    assert_eq!(result.session_days_since_reference, Some(0));
    // An empty session cannot own a turn either.
    let mut h = base();
    h.start("2026-10-05_001", "2026-10-05T09:00Z")
        .note(json!({"turn":"2026-10-05_001#1","notes":["Used O01"]}));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert!(codes(&result).contains(&"history.contradictory"));
}

#[test]
fn references_before_the_staging_instant_do_not_start_the_count() {
    // N05 was used in 10-02 turn 1, before O02 was staged at 10:30.
    let mut h = base();
    h.sessions.get_mut("2026-10-02_001").unwrap()["events_logged"][0]["id"] = json!("N05");
    let mut subject = observation("O02", Some("2026-10-02T10:30Z"));
    subject.bound_to = vec!["N05"];
    let result = h.measure(subject, Window::default());
    assert_eq!(result.turns_since_reference, Some(4));
    assert_eq!(result.session_days_since_reference, Some(2));
    assert_eq!(result.reference_basis, Some("staging_timestamp"));
    assert_eq!(result.last_reference_turn, None);
    assert_eq!(result.last_reference_date.as_deref(), Some("2026-10-02"));
    assert_eq!(result.evidence.len(), 1);
    assert_eq!(result.evidence[0].to_json()["status"], "before_staging");
    // A reference at the staging instant is the staging turn itself.
    let mut subject = observation("O02", Some("2026-10-02T10:00Z"));
    subject.bound_to = vec!["N05"];
    let result = h.measure(subject, Window::default());
    assert_eq!(
        result.last_reference_turn.as_deref(),
        Some("2026-10-02_001#1")
    );
    assert_eq!(result.turns_since_reference, Some(4));
    assert_eq!(result.reference_basis, Some("structured"));
}

#[test]
fn date_only_staging_never_starts_from_an_earlier_or_same_date_reference() {
    // N09 named on 10-03; O07 staged with the date 10-04 only.
    let mut h = base();
    h.log(
        "2026-10-03_001",
        "2026-10-03T11:00Z",
        json!({"key_context":[{"excerpt":"Checked N09"}]}),
    );
    let mut subject = observation("O07", Some("2026-10-04"));
    subject.bound_to = vec!["N09"];
    let result = h.measure(subject, Window::default());
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(codes(&result), ["history.staging_position"]);
    assert_eq!(result.session_days_since_reference, Some(0));
    assert_eq!(result.last_reference_date.as_deref(), Some("2026-10-04"));
    assert_eq!(result.last_reference_turn, None);
    assert_eq!(result.reference_basis, Some("staging_timestamp"));
    assert_eq!(result.evidence[0].to_json()["status"], "before_staging");
    assert_eq!(result.history_status, "ambiguous");
    // A reference on the staging date itself is ambiguous for turn order.
    let mut subject = observation("O07", Some("2026-10-03"));
    subject.bound_to = vec!["N09"];
    let result = h.measure(subject, Window::default());
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.last_reference_turn, None);
    assert_eq!(result.session_days_since_reference, Some(1));
    assert_eq!(codes(&result), ["history.staging_position"]);
    // A reference on a later date is provably after creation.
    let mut subject = observation("O07", Some("2026-10-02"));
    subject.bound_to = vec!["N09"];
    let result = h.measure(subject, Window::default());
    assert_eq!(result.turns_since_reference, Some(2));
    assert_eq!(
        result.last_reference_turn.as_deref(),
        Some("2026-10-03_001#2")
    );
}

#[test]
fn absurd_turn_counts_are_counted_compactly_not_materialised() {
    let mut h = base();
    h.raw.push((
        "trace/sessions/2026-10-05_001.yaml".into(),
        json!({"session":{"id":"2026-10-05_001","date":"2026-10-05","started":"2026-10-05T09:00Z","last_turn":"2026-10-05T10:00Z","turn_count":18446744073709551615u64,"summary":"Huge"}}).to_string(),
    ));
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    // The claimed turns still prove one logged date.
    assert_eq!(result.session_days_since_reference, Some(2));
    let missing = &result.diagnostics[0];
    assert_eq!(missing.code, "history.turn_stamp_missing");
    // Every turn but the last (stamped by last_turn) lacks a stamp.
    assert_eq!(missing.count, u64::MAX - 1);
    assert_eq!(
        missing.examples,
        ["2026-10-05_001#1", "2026-10-05_001#2", "2026-10-05_001#3"]
    );
}

#[test]
fn turn_counts_beyond_archived_evidence_and_unreadable_counts_fail_closed() {
    // The session archives its turns but claims more than the archive reaches.
    let mut h = base();
    h.sessions.get_mut("2026-10-04_001").unwrap()["session"]["turn_count"] = json!(5);
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert!(codes(&result).contains(&"history.contradictory"));
    // An unreadable count leaves both counts unknown.
    let mut h = base();
    h.sessions.get_mut("2026-10-04_001").unwrap()["session"]["turn_count"] = json!("many");
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(codes(&result), ["history.contradictory"]);
}

#[test]
fn undecodable_alias_bytes_are_a_diagnostic() {
    let mut h = base();
    h.alias_bytes = Some(vec![0xff, 0xfe, b'\n']);
    let result = h.measure(
        observation("O01", Some("2026-10-02T10:00Z")),
        Window::default(),
    );
    assert_eq!(result.turns_since_reference, None);
    assert_eq!(result.session_days_since_reference, None);
    assert_eq!(codes(&result), ["history.alias_invalid"]);
}

#[test]
fn yaml_aliases_in_reference_fields_make_both_counts_unknown() {
    for (summary, fields, archived) in [
        (
            "Unrelated",
            "key_context:\n  - turn: 1\n    excerpt: *recent\n",
            "",
        ),
        ("*recent", "key_context: []\n", ""),
        ("Unrelated", "open_threads: [*recent]\n", ""),
        ("Unrelated", "ai_suggestions_pending: *recent\n", ""),
        (
            "Unrelated",
            "key_context: []\n",
            "    session_metadata:\n      session: 2026-10-04_001\n      before:\n        turn_count: 0\n      after:\n        turn_count: 1\n        last_turn: '2026-10-04T10:00Z'\n        summary: *recent\n",
        ),
        (
            "Unrelated",
            "key_context: []\n",
            "    session_metadata:\n      session: 2026-10-04_001\n      before:\n        turn_count: 0\n      after:\n        turn_count: 1\n        last_turn: '2026-10-04T10:00Z'\n        summary: Unrelated\n        open_threads: [*recent]\n",
        ),
    ] {
        let mut h = History::default();
        h.raw.push((
            "trace/sessions/2026-10-04_001.yaml".into(),
            format!("recent: &recent 'Revisited O95'\nsession:\n  id: 2026-10-04_001\n  date: '2026-10-04'\n  started: '2026-10-04T09:00Z'\n  last_turn: '2026-10-04T10:00Z'\n  turn_count: 1\n  summary: {summary}\n{fields}"),
        ));
        if !archived.is_empty() {
            h.raw.push((
                "trace/pm_reasoning_log.yaml".into(),
                format!("recent: &recent 'Revisited O95'\nentries:\n  - turn: '2026-10-04_001#1'\n{archived}"),
            ));
        }
        let result = h.measure(
            observation("O95", Some("2026-10-01T10:00Z")),
            Window::default(),
        );
        assert_eq!(
            result.turns_since_reference, None,
            "{summary} {fields} {archived}"
        );
        assert_eq!(
            result.session_days_since_reference, None,
            "{summary} {fields} {archived}"
        );
        assert_ne!(result.history_status, "complete");
        assert!(
            codes(&result).contains(&"history.reference_alias"),
            "{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn nonlocal_and_differently_padded_tokens_are_not_inactivity_references() {
    for summary in [
        "Revisited peer:O01",
        "Read results/O01.csv",
        "Read https://example/O01",
        "Revisited O1",
        "Revisited O01suffix",
        "Revisited αO01",
        "Example:\n```\nO01\n```\nUnrelated work",
    ] {
        let mut h = base();
        h.start("2026-10-05_001", "2026-10-05T09:00Z").log(
            "2026-10-05_001",
            "2026-10-05T10:00Z",
            json!({"summary":summary}),
        );
        let result = h.measure(
            observation("O01", Some("2026-10-02T10:00Z")),
            Window::default(),
        );
        assert_eq!(
            result.last_reference_turn.as_deref(),
            Some("2026-10-03_001#1"),
            "{summary}"
        );
        assert_eq!(result.session_days_since_reference, Some(2), "{summary}");
        assert_eq!(result.history_status, "complete", "{summary}");
    }
}
