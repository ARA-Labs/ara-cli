//! Native optional-layer readers. Pure parsers operate on strings and preserve
//! unknown values; filesystem policy lives in NativeLoad.

use crate::manifest::*;
use crate::parse::NativeLoad;
use crate::{ParseReport, RuleCode};
use std::collections::{BTreeMap, BTreeSet};

type Map = BTreeMap<String, SourceValue>;

fn malformed(report: &mut ParseReport, file: &str, detail: impl Into<String>) {
    report.warn(RuleCode::MalformedAgentLayer, file, detail);
}

fn document(source: &str, file: &str, report: &mut ParseReport) -> Option<Map> {
    match serde_saphyr::from_str::<SourceValue>(source) {
        Ok(SourceValue::Mapping(map)) => Some(map),
        Ok(_) => {
            malformed(report, file, "expected a mapping document");
            None
        }
        Err(e) => {
            malformed(report, file, format!("invalid YAML: {e}"));
            None
        }
    }
}

fn rows(source: &str, key: &str, file: &str, report: &mut ParseReport) -> Vec<Map> {
    let Some(mut doc) = document(source, file, report) else {
        return Vec::new();
    };
    let Some(SourceValue::Sequence(rows)) = doc.remove(key) else {
        malformed(report, file, format!("expected `{key}` sequence"));
        return Vec::new();
    };
    rows.into_iter()
        .filter_map(|row| match row {
            SourceValue::Mapping(map) => Some(map),
            _ => {
                malformed(report, file, "entry is not a mapping");
                None
            }
        })
        .collect()
}

fn string(map: &mut Map, key: &str, file: &str, report: &mut ParseReport) -> Option<String> {
    match map.remove(key) {
        Some(SourceValue::String(s)) => Some(s),
        Some(SourceValue::Null) | None => None,
        Some(value) => {
            map.insert(key.to_string(), value);
            malformed(report, file, format!("`{key}` must be text"));
            None
        }
    }
}
fn boolean(map: &mut Map, key: &str, file: &str, report: &mut ParseReport) -> Option<bool> {
    match map.remove(key) {
        Some(SourceValue::Bool(v)) => Some(v),
        Some(SourceValue::Null) | None => None,
        Some(value) => {
            map.insert(key.to_string(), value);
            malformed(report, file, format!("`{key}` must be boolean"));
            None
        }
    }
}
fn sequence(map: &mut Map, key: &str, file: &str, report: &mut ParseReport) -> Vec<SourceValue> {
    match map.remove(key) {
        Some(SourceValue::Sequence(v)) => v,
        Some(SourceValue::Null) | None => Vec::new(),
        Some(value) => {
            map.insert(key.to_string(), value);
            malformed(report, file, format!("`{key}` must be a sequence"));
            Vec::new()
        }
    }
}
fn duplicate(id: &str, seen: &mut BTreeSet<String>, file: &str, report: &mut ParseReport) {
    if !seen.insert(id.to_string()) {
        report.warn(
            RuleCode::DuplicateAgentId,
            file,
            format!("duplicate layer id `{id}`; all entries retained"),
        );
    }
}

pub(crate) fn parse_observations(
    source: &str,
    file: &str,
    report: &mut ParseReport,
) -> Vec<Observation> {
    let mut seen = BTreeSet::new();
    rows(source, "observations", file, report)
        .into_iter()
        .filter_map(|mut map| {
            let Some(id) = string(&mut map, "id", file, report) else {
                malformed(report, file, "observation has no id");
                return None;
            };
            if !is_canonical_id(&id, 'O') {
                malformed(report, file, format!("invalid observation id `{id}`"));
                return None;
            }
            let Some(content) = string(&mut map, "content", file, report) else {
                malformed(report, file, format!("observation `{id}` has no content"));
                return None;
            };
            duplicate(&id, &mut seen, file, report);
            let bound_to = sequence(&mut map, "bound_to", file, report)
                .into_iter()
                .filter_map(|value| match value {
                    SourceValue::String(s) => Some(NodeId::new(s)),
                    _ => {
                        malformed(report, file, "bound_to entry must be text");
                        None
                    }
                })
                .collect();
            let promoted_to = string(&mut map, "promoted_to", file, report);
            if let Some(destination) = &promoted_to {
                let canonical = destination.split_once(':').is_some_and(|(path, id)| {
                    (path == "logic/claims.md" && is_canonical_id(id, 'C'))
                        || (path == "logic/solution/heuristics.md" && is_canonical_id(id, 'H'))
                        || (path == "logic/concepts.md" && !id.trim().is_empty())
                        || (path == "logic/solution/constraints.md" && !id.trim().is_empty())
                        || (path == "trace/exploration_tree.yaml" && is_canonical_id(id, 'N'))
                });
                if !canonical {
                    report.warn(
                        RuleCode::MalformedPromotionDestination,
                        file,
                        format!("malformed promotion destination `{destination}` (retained)"),
                    );
                }
            }
            Some(Observation {
                id: ObservationId::new(id),
                source_file: file.to_string(),
                content,
                timestamp: string(&mut map, "timestamp", file, report),
                provenance: string(&mut map, "provenance", file, report),
                context: string(&mut map, "context", file, report),
                potential_type: string(&mut map, "potential_type", file, report),
                bound_to,
                promoted: boolean(&mut map, "promoted", file, report),
                promoted_to,
                crystallized_via: string(&mut map, "crystallized_via", file, report),
                stale: boolean(&mut map, "stale", file, report),
                extra: map,
            })
        })
        .collect()
}

pub(crate) fn parse_session(source: &str, file: &str, report: &mut ParseReport) -> Option<Session> {
    let mut doc = document(source, file, report)?;
    let Some(SourceValue::Mapping(mut meta)) = doc.remove("session") else {
        malformed(report, file, "missing session metadata mapping");
        return None;
    };
    let Some(id) = string(&mut meta, "id", file, report) else {
        malformed(report, file, "session has no id");
        return None;
    };
    let turn_count = match meta.remove("turn_count") {
        Some(SourceValue::Integer(v)) if v >= 0 => Some(v as u64),
        Some(SourceValue::Unsigned(v)) => Some(v),
        Some(SourceValue::Null) | None => None,
        Some(v) => {
            meta.insert("turn_count".into(), v);
            malformed(report, file, "turn_count must be nonnegative integer");
            None
        }
    };
    Some(Session {
        id: SessionId::new(id),
        source_file: file.to_string(),
        body: source.to_string(),
        date: string(&mut meta, "date", file, report),
        started: string(&mut meta, "started", file, report),
        last_turn: string(&mut meta, "last_turn", file, report),
        turn_count,
        summary: string(&mut meta, "summary", file, report),
        events_logged: sequence(&mut doc, "events_logged", file, report),
        ai_actions: sequence(&mut doc, "ai_actions", file, report),
        claims_touched: sequence(&mut doc, "claims_touched", file, report),
        logic_revisions: sequence(&mut doc, "logic_revisions", file, report),
        key_context: sequence(&mut doc, "key_context", file, report),
        open_threads: sequence(&mut doc, "open_threads", file, report),
        ai_suggestions_pending: sequence(&mut doc, "ai_suggestions_pending", file, report),
        metadata_extra: meta,
        extra: doc,
    })
}

pub(crate) fn parse_taste(source: &str, file: &str, report: &mut ParseReport) -> Vec<TasteComment> {
    let mut seen = BTreeSet::new();
    rows(source, "entries", file, report)
        .into_iter()
        .filter_map(|mut map| {
            let Some(id) = string(&mut map, "id", file, report) else {
                malformed(report, file, "taste entry has no id");
                return None;
            };
            if !is_canonical_id(&id, 'T') {
                malformed(report, file, format!("invalid taste id `{id}`"));
                return None;
            }
            let Some(target) = string(&mut map, "target", file, report) else {
                malformed(report, file, "taste entry has no target");
                return None;
            };
            let Some(comment) = string(&mut map, "comment", file, report) else {
                malformed(report, file, "taste entry has no comment");
                return None;
            };
            duplicate(&id, &mut seen, file, report);
            Some(TasteComment {
                id: TasteId::new(id),
                source_file: file.to_string(),
                target,
                comment,
                timestamp: string(&mut map, "timestamp", file, report),
                tag: string(&mut map, "tag", file, report),
                object: string(&mut map, "object", file, report),
                extra: map,
            })
        })
        .collect()
}

fn markdown_entries(
    source: &str,
    prefix: char,
    file: &str,
    report: &mut ParseReport,
) -> Vec<(String, String, String, Vec<SourceField>)> {
    let mut seen = BTreeSet::new();
    crate::markdown::sections(source)
        .into_iter()
        .filter_map(|section| {
            let Some((id, title)) = section.heading.split_once(':') else {
                if looks_like_entry(section.heading, prefix) {
                    malformed(report, file, "malformed entry heading");
                }
                return None;
            };
            let id = id.trim();
            let title = title.trim();
            if !is_canonical_id(id, prefix) || title.is_empty() {
                if looks_like_entry(id, prefix) {
                    malformed(report, file, "malformed entry heading");
                }
                return None;
            }
            duplicate(id, &mut seen, file, report);
            let fields = crate::markdown::fields(source, section.body_range)
                .into_iter()
                .map(|f| SourceField {
                    name: f.name.to_string(),
                    value: crate::markdown::decode_field(&f).into_owned(),
                })
                .collect();
            Some((
                id.to_string(),
                title.to_string(),
                source[section.range].to_string(),
                fields,
            ))
        })
        .collect()
}
fn looks_like_entry(heading: &str, prefix: char) -> bool {
    heading
        .strip_prefix(prefix)
        .and_then(|rest| rest.as_bytes().first())
        .is_some_and(u8::is_ascii_digit)
}
fn field(fields: &[SourceField], names: &[&str]) -> Option<String> {
    fields
        .iter()
        .rev()
        .find(|f| names.iter().any(|name| f.name.eq_ignore_ascii_case(name)))
        .map(|f| f.value.clone())
}

pub(crate) fn parse_heuristics(
    source: &str,
    file: &str,
    report: &mut ParseReport,
) -> Vec<Heuristic> {
    markdown_entries(source, 'H', file, report)
        .into_iter()
        .map(|(id, title, body, fields)| Heuristic {
            id: HeuristicId::new(id),
            title,
            source_file: file.to_string(),
            body,
            rationale: field(&fields, &["Rationale"]),
            sources: field(&fields, &["Sources"]),
            status: field(&fields, &["Status"]),
            provenance: field(&fields, &["Provenance"]),
            sensitivity: field(&fields, &["Sensitivity"]),
            code_ref: field(&fields, &["Code ref"]),
            last_revised: field(&fields, &["Last revised"]),
            source_fields: fields,
        })
        .collect()
}
pub(crate) fn parse_experiments(
    source: &str,
    file: &str,
    report: &mut ParseReport,
) -> Vec<ExperimentPlan> {
    markdown_entries(source, 'E', file, report)
        .into_iter()
        .map(|(id, title, body, fields)| ExperimentPlan {
            id: ExperimentId::new(id),
            title,
            source_file: file.to_string(),
            body,
            status: field(&fields, &["Status"]),
            evidence_output: field(&fields, &["Evidence output"]),
            question: field(&fields, &["Question"]),
            setup: field(&fields, &["Setup"]),
            prediction: field(
                &fields,
                &[
                    "Prediction",
                    "Predictions",
                    "Prediction (directional)",
                    "Predictions (directional)",
                ],
            ),
            falsification: field(
                &fields,
                &[
                    "Falsification",
                    "Falsification criteria",
                    "Falsification condition",
                ],
            ),
            provenance: field(&fields, &["Provenance"]),
            last_revised: field(&fields, &["Last revised"]),
            source_fields: fields,
        })
        .collect()
}

pub(crate) fn read_layers(dir: &std::path::Path, load: &mut NativeLoad, manifest: &mut Manifest) {
    load.read_source(dir, "trace/pm_reasoning_log.yaml", false);
    load.read_source(dir, "trace/logic_mutations.yaml", false);
    let observations = "staging/observations.yaml";
    if let Some(source) = load.read_source(dir, observations, false) {
        manifest.observations = parse_observations(&source, observations, &mut load.report);
    }
    let taste = "trace/taste_log.yaml";
    if let Some(source) = load.read_source(dir, taste, false) {
        manifest.taste_comments = parse_taste(&source, taste, &mut load.report);
    }
    let heuristic = "logic/solution/heuristics.md";
    if let Some(source) = load.sources.get(heuristic) {
        manifest.heuristics = parse_heuristics(source, heuristic, &mut load.report);
    }
    let experiment = "logic/experiments.md";
    if let Some(source) = load.read_source(dir, experiment, false) {
        manifest.experiment_plans = parse_experiments(&source, experiment, &mut load.report);
    }
    let paths = load.list_files(dir, "trace/sessions");
    let mut seen = BTreeSet::new();
    for path in paths {
        if path.ends_with("session_index.yaml")
            || !(path.ends_with(".yaml") || path.ends_with(".yml"))
        {
            continue;
        }
        if let Some(source) = load.read_source(dir, &path, false)
            && let Some(session) = parse_session(&source, &path, &mut load.report)
        {
            duplicate(session.id.as_str(), &mut seen, &path, &mut load.report);
            manifest.sessions.push(session);
        }
    }
    manifest.sessions.sort_by(|a, b| {
        a.date
            .as_deref()
            .unwrap_or(a.id.as_str())
            .cmp(b.date.as_deref().unwrap_or(b.id.as_str()))
            .then_with(|| session_sequence(a.id.as_str()).cmp(&session_sequence(b.id.as_str())))
            .then_with(|| a.source_file.cmp(&b.source_file))
    });
    let index = "trace/sessions/session_index.yaml";
    if let Some(source) = load.read_source(dir, index, false) {
        let mut index_seen = BTreeSet::new();
        for mut row in rows(&source, "sessions", index, &mut load.report) {
            let Some(id) = string(&mut row, "id", index, &mut load.report) else {
                malformed(&mut load.report, index, "index row has no id");
                continue;
            };
            duplicate(&id, &mut index_seen, index, &mut load.report);
            if !seen.contains(&id) {
                load.report.warn(
                    RuleCode::DanglingSessionIndex,
                    index,
                    format!("session index references absent record `{id}`"),
                );
            }
        }
    }
}
fn session_sequence(id: &str) -> u64 {
    id.rsplit_once('_')
        .and_then(|(_, n)| n.parse().ok())
        .unwrap_or(0)
}

/// Roots an agent reads and searches with ordinary file tools. Native reads,
/// writes and merge entries never cover them; merge treats their files as
/// external read-only bytes.
pub const FILE_ACCESS_ROOTS: [&str; 3] = ["rubric/", "evidence/", "src/"];

/// Whether `path` lies under one of [`FILE_ACCESS_ROOTS`].
pub fn file_access_path(path: &str) -> bool {
    FILE_ACCESS_ROOTS.iter().any(|root| path.starts_with(root))
}

/// Whether `path` is a bare file-access root (`rubric`) or lies under one.
pub fn file_access_location(path: &str) -> bool {
    file_access_path(path)
        || FILE_ACCESS_ROOTS
            .iter()
            .any(|root| root.strip_suffix('/') == Some(path))
}

/// Proposed explicit knowledge-document registry in PAPER frontmatter. This
/// validates identity, not filesystem existence; native loading separately
/// rejects symlink components. File-access roots cannot be registered.
pub fn knowledge_paths(paper: &str) -> Result<Vec<String>, String> {
    let Some(yaml) = crate::paper::extract_frontmatter(paper) else {
        if paper
            .trim_start_matches('\u{feff}')
            .lines()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| line.trim() == "---")
        {
            return Err("unterminated PAPER frontmatter".into());
        }
        return Ok(Vec::new());
    };
    let doc = serde_saphyr::from_str::<SourceValue>(yaml)
        .map_err(|e| format!("invalid PAPER frontmatter: {e}"))?;
    let SourceValue::Mapping(mut map) = doc else {
        return Err("PAPER frontmatter must be a mapping".into());
    };
    let Some(value) = map.remove("knowledge_paths") else {
        return Ok(Vec::new());
    };
    let SourceValue::Sequence(values) = value else {
        return Err("knowledge_paths must be a sequence".into());
    };
    let mut seen = BTreeSet::new();
    let mut paths = Vec::with_capacity(values.len());
    for value in values {
        let SourceValue::String(path) = value else {
            return Err("knowledge_paths entries must be text".into());
        };
        let first = path.split('/').next().unwrap_or("");
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path.contains(':')
            || path.chars().any(char::is_control)
            || !path.ends_with(".md")
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || file_access_location(&path)
            || matches!(first, ".git" | ".ara" | "trace" | "staging")
        {
            return Err(format!("unsafe registered knowledge path `{path}`"));
        }
        if !seen.insert(path.clone()) {
            return Err(format!("duplicate registered knowledge path `{path}`"));
        }
        paths.push(path);
    }
    Ok(paths)
}
