//! Which validation results still allow structural reads.
//!
//! Structural reads (`find`, `ls`, `show`) need a
//! complete representation, not a valid artifact. An error is read through
//! only when its code is in [`READ_THROUGH`]: a dangling reference whose
//! source value is kept while no edge or binding is invented for it. Every
//! other error refuses, so parse loss, missing structure, duplicate or
//! ambiguous identities, cycles, contradictory parents and corrupt identity
//! history stay refusals, and a new rule refuses until it is classified here.
//! Read-through also needs every typed native document to be represented: no
//! `ARA229` fence hides one, no claim-like heading in `logic/claims.md`
//! fails to parse, and no code fence left open at EOF hides one, whether or
//! not any dangling ID names it. A dropped claim looks exactly like a dangling `ARA107`
//! reference, so that parse loss refuses as it did before. The report keeps
//! each diagnostic's original severity. Writes never use this tolerance.
//!
//! An `ARA217`/`ARA218` warning about one optional-layer file (observations,
//! taste log, sessions, experiments, heuristics, the knowledge registry) does
//! not refuse: the read goes ahead and [`with_coverage`] names the file under
//! `incomplete`. Node-scoped `ARA217`, `ARA222` and `ARA226` still refuse.
use crate::output::AgentError;
use ara_core::{ClaimId, Diagnostic, Manifest, ParseReport, RuleCode};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Native documents whose level-two sections are entries.
const TYPED_DOCUMENTS: [&str; 5] = [
    "logic/claims.md",
    "logic/solution/heuristics.md",
    "logic/concepts.md",
    "logic/related_work.md",
    "logic/experiments.md",
];

/// Errors that leave every source entry represented.
const READ_THROUGH: [RuleCode; 3] = [
    // A node's `evidence:` names an absent claim. The node keeps the source
    // value; only the binding is missing, as when `logic/claims.md` is absent.
    RuleCode::UnknownEvidenceClaim,
    // `also_depends_on:` names an absent node. The node keeps the source
    // value; no dependency edge is created.
    RuleCode::UnknownDependencyNode,
    // A claim's `Dependencies:` names an absent claim. The claim keeps the ID
    // verbatim; nothing resolves it.
    RuleCode::UnknownClaimDependency,
];

/// Warnings meaning an optional layer or annotation was not fully represented.
const UNREPRESENTED: [RuleCode; 4] = [
    RuleCode::MalformedAgentLayer,
    RuleCode::DuplicateAgentId,
    RuleCode::MalformedSameAs,
    RuleCode::MalformedNodeAnnotation,
];

/// Optional-layer files whose `ARA217`/`ARA218` warnings read through.
const OPTIONAL_LAYER_FILES: [&str; 5] = [
    "staging/observations.yaml",
    "trace/taste_log.yaml",
    "logic/experiments.md",
    "logic/solution/heuristics.md",
    // An invalid registry leaves the registered documents unloaded.
    "PAPER.md.knowledge_paths",
];

/// Session records and their index, one optional layer.
const SESSIONS: &str = "trace/sessions/";

/// A diagnostic path without its `:line` suffix.
fn diagnostic_file(path: &str) -> &str {
    match path.rsplit_once(':') {
        Some((file, line)) if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) => file,
        _ => path,
    }
}

/// The optional-layer file a warning leaves incomplete, if it reads through.
fn optional_layer(diagnostic: &Diagnostic) -> Option<&str> {
    if !matches!(
        diagnostic.code,
        RuleCode::MalformedAgentLayer | RuleCode::DuplicateAgentId
    ) {
        return None;
    }
    let file = diagnostic_file(&diagnostic.path);
    (OPTIONAL_LAYER_FILES.contains(&file) || file.starts_with(SESSIONS)).then_some(file)
}

/// `{path, codes, reason}` for each optional-layer file a read could not
/// fully represent, in first-diagnostic order. `reason` joins the first line
/// of each distinct message.
pub fn incomplete(report: &ParseReport) -> Vec<Value> {
    let mut files: Vec<(&str, Vec<RuleCode>, Vec<&str>)> = Vec::new();
    for diagnostic in report.warnings() {
        let Some(file) = optional_layer(diagnostic) else {
            continue;
        };
        let index = match files.iter().position(|(path, ..)| *path == file) {
            Some(index) => index,
            None => {
                files.push((file, Vec::new(), Vec::new()));
                files.len() - 1
            }
        };
        let (_, codes, reasons) = &mut files[index];
        if !codes.contains(&diagnostic.code) {
            codes.push(diagnostic.code);
        }
        let reason = diagnostic.message.lines().next().unwrap_or_default();
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
    }
    files
        .into_iter()
        .map(
            |(path, codes, reasons)| json!({"path":path,"codes":codes,"reason":reasons.join("; ")}),
        )
        .collect()
}

/// A structural read result with `complete: false` and `incomplete` added
/// when an optional layer was not fully represented; unchanged otherwise.
pub fn with_coverage(mut value: Value, report: &ParseReport) -> Value {
    let incomplete = incomplete(report);
    if !incomplete.is_empty()
        && let Value::Object(object) = &mut value
    {
        object.insert("complete".into(), json!(false));
        object.insert("incomplete".into(), json!(incomplete));
    }
    value
}

/// Whether every optional layer and node annotation was represented: none of
/// the [`UNREPRESENTED`] warnings is present. `status` uses this as part of
/// `complete`; structural reads refuse with `incomplete_artifact` without it.
pub fn representable(report: &ParseReport) -> bool {
    !report
        .warnings()
        .iter()
        .any(|diagnostic| UNREPRESENTED.contains(&diagnostic.code))
}

/// [`representable`], ignoring optional-layer warnings: every node and node
/// annotation was represented, so counts stay meaningful.
pub fn nodes_representable(report: &ParseReport) -> bool {
    !report.warnings().iter().any(|diagnostic| {
        UNREPRESENTED.contains(&diagnostic.code) && optional_layer(diagnostic).is_none()
    })
}

/// Native ID prefixes whose next ID is unknown because their optional-layer
/// file is incomplete: a dropped entry may already hold the next number.
pub fn incomplete_prefixes(report: &ParseReport) -> BTreeMap<char, &str> {
    let mut prefixes = BTreeMap::new();
    for diagnostic in report.warnings() {
        let prefix = match optional_layer(diagnostic) {
            Some("staging/observations.yaml") => 'O',
            Some("trace/taste_log.yaml") => 'T',
            Some("logic/experiments.md") => 'E',
            Some("logic/solution/heuristics.md") => 'H',
            _ => continue,
        };
        prefixes
            .entry(prefix)
            .or_insert_with(|| diagnostic_file(&diagnostic.path));
    }
    prefixes
}

/// Distinct error codes in report order, filtered by `keep`.
fn error_codes(report: &ParseReport, keep: impl Fn(RuleCode) -> bool) -> Vec<RuleCode> {
    let mut codes = Vec::new();
    for diagnostic in report.errors() {
        if keep(diagnostic.code) && !codes.contains(&diagnostic.code) {
            codes.push(diagnostic.code);
        }
    }
    codes
}

/// `path:line: reason` for each typed document that lost entries.
fn unrepresented_documents(
    report: &ParseReport,
    sources: &BTreeMap<String, String>,
    manifest: Option<&Manifest>,
) -> Vec<String> {
    let mut reasons: Vec<String> = report
        .warnings()
        .iter()
        .filter(|d| {
            d.code == RuleCode::UnclosedFrontmatter
                && d.path
                    .rsplit_once(':')
                    .is_some_and(|(path, _)| TYPED_DOCUMENTS.contains(&path))
        })
        .map(|d| format!("{}: unclosed leading `---` hides its entries", d.path))
        .collect();
    if let Some(claims) = sources.get("logic/claims.md") {
        reasons.extend(ara_core::unparsed_claim_headings(claims).into_iter().map(
            |(line, heading)| {
                format!("logic/claims.md:{line}: claim heading `{heading}` does not parse")
            },
        ));
        reasons.extend(ara_core::fenced_claim_headings(claims).into_iter().map(
            |(line, heading)| {
                format!(
                    "logic/claims.md:{line}: unclosed code fence hides claim heading `{}`",
                    heading.trim()
                )
            },
        ));
    }
    let (dangling, unidentified) = dangling_claims(report, manifest);
    reasons.extend(unidentified);
    if let Some(claims) = sources.get("logic/claims.md") {
        reasons.extend(dangling_claim_headings(&dangling, claims));
    }
    reasons
}

/// The claim ID a dangling-reference message names: its last backticked
/// token, when that is a canonical claim ID.
fn dangling_id(message: &str) -> Option<&str> {
    message
        .rsplit('`')
        .nth(1)
        .filter(|id| ClaimId::new(*id).is_canonical())
}

/// Dangling claim IDs, plus a reason for each ARA107/ARA109 diagnostic whose
/// ID cannot be identified, so a reworded message fails closed. Claim
/// dependencies come from the manifest; evidence IDs (whose bindings the
/// parser does not keep) come from the diagnostics.
fn dangling_claims(
    report: &ParseReport,
    manifest: Option<&Manifest>,
) -> (BTreeSet<String>, Vec<String>) {
    let mut ids = BTreeSet::new();
    if let Some(manifest) = manifest {
        let live: BTreeSet<&str> = manifest.claims.iter().map(|c| c.id.as_str()).collect();
        ids.extend(
            manifest
                .claims
                .iter()
                .flat_map(|claim| &claim.deps)
                .map(ClaimId::as_str)
                .filter(|id| !live.contains(id))
                .map(str::to_owned),
        );
    }
    let mut unidentified = Vec::new();
    for diagnostic in report.errors().iter().filter(|d| {
        matches!(
            d.code,
            RuleCode::UnknownEvidenceClaim | RuleCode::UnknownClaimDependency
        )
    }) {
        match dangling_id(&diagnostic.message) {
            Some(id) => {
                ids.insert(id.to_owned());
            }
            None => unidentified.push(format!(
                "{}: cannot identify the dangling claim to rule out a dropped claim heading",
                diagnostic.path
            )),
        }
    }
    (ids, unidentified)
}

/// Heading-like lines of `logic/claims.md` that name a dangling claim ID.
///
/// A claim the parser dropped (inside an unclosed code fence, lowercase,
/// bold, at level three, spaced `C 01`) is reported exactly like a dangling
/// reference. Any line starting with `#` (after leading whitespace) that has
/// the ID as a whole token, case-insensitively or with spaces removed, is
/// therefore treated as lost source. This includes a parsed claim heading
/// whose title names the dangling ID (`## C02: Extends C01`), which refuses
/// conservatively. Fences and indentation are ignored on purpose; prose and
/// field lines are not headings and never match.
fn dangling_claim_headings(dangling: &BTreeSet<String>, claims: &str) -> Vec<String> {
    let names = |text: &str, id: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|token| token.eq_ignore_ascii_case(id))
    };
    claims
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with('#'))
        .filter_map(|(index, line)| {
            let compact: String = line.split_whitespace().collect();
            let id = dangling
                .iter()
                .find(|id| names(line, id) || names(&compact, id))?;
            Some(format!(
                "logic/claims.md:{}: heading-like line names dangling claim `{id}` but is not a claim",
                index + 1
            ))
        })
        .collect()
}

/// The structural-read refusal for a load, if any: `invalid_artifact` for a
/// blocking error, or for a read-through error while a typed document lost
/// entries, and `incomplete_artifact` for an unrepresented node annotation.
/// Optional-layer warnings never refuse ([`with_coverage`]).
pub fn read_refusal(
    report: &ParseReport,
    sources: &BTreeMap<String, String>,
    manifest: Option<&Manifest>,
) -> Option<AgentError> {
    let errors = error_codes(report, |code| !READ_THROUGH.contains(&code));
    if !errors.is_empty() {
        return Some(refusal("invalid_artifact", report, errors));
    }
    if !report.is_ok() {
        let unrepresented = unrepresented_documents(report, sources, manifest);
        if !unrepresented.is_empty() {
            let mut error = refusal("invalid_artifact", report, error_codes(report, |_| true));
            if let Some(Value::Object(details)) = error.details.as_deref_mut() {
                details.insert("unrepresented".into(), json!(unrepresented));
            }
            return Some(error);
        }
    }
    let unrepresented: Vec<RuleCode> = UNREPRESENTED
        .into_iter()
        .filter(|code| {
            report
                .warnings()
                .iter()
                .any(|d| d.code == *code && optional_layer(d).is_none())
        })
        .collect();
    (!unrepresented.is_empty()).then(|| refusal("incomplete_artifact", report, unrepresented))
}

/// A refusal naming its blocking rule codes, with a next step.
pub fn refusal(code: &str, report: &ParseReport, blocking: Vec<RuleCode>) -> AgentError {
    AgentError {
        details: Some(Box::new(json!({
            "blocking": blocking,
            "hint": "Run `ara check` to list each diagnostic with its rule code and location. Dangling references alone do not block reads; a source read (`ara show --document <path> --source`) still works.",
        }))),
        summary: Some(summary(report).into()),
        ..AgentError::semantic(code, report.to_string())
    }
}

/// Error and warning counts with their rule codes, for text output.
fn summary(report: &ParseReport) -> String {
    let codes = |diagnostics: &[ara_core::Diagnostic]| {
        diagnostics
            .iter()
            .map(|d| d.code.to_string())
            .collect::<Vec<_>>()
    };
    let (errors, warnings) = (codes(report.errors()), codes(report.warnings()));
    let errors: Vec<&str> = errors.iter().map(String::as_str).collect();
    let warnings: Vec<&str> = warnings.iter().map(String::as_str).collect();
    format!(
        "the artifact cannot be read structurally: {}",
        crate::brief::summary(&errors, &warnings)
    )
}

#[cfg(test)]
mod tests {
    use super::dangling_id;

    #[test]
    fn dangling_ids_come_only_from_backticked_canonical_claims() {
        assert_eq!(
            dangling_id("evidence references unknown claim `C01`"),
            Some("C01")
        );
        assert_eq!(
            dangling_id("dependency references unknown claim `C98`"),
            Some("C98")
        );
        // Reworded or unexpected messages yield nothing, which fails closed.
        assert_eq!(dangling_id("evidence references unknown claim C01"), None);
        assert_eq!(dangling_id("unknown claim `N01`"), None);
    }
}
