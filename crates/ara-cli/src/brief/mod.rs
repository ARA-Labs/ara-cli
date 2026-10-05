//! Brief default text for agent reads.
//!
//! Each read command builds its `ara.<command>/v1` value; without `--json`
//! this module renders that value as address-led text. Data that only the
//! text needs (a native section for `show C04`, write selectors, document
//! summaries, status rule codes) is computed only in brief mode and lives in
//! `display` objects that JSON output never carries. Diagnostics print once
//! per command on stderr as counts and codes.
mod lists;
mod show;

use crate::output::AgentError;
use serde_json::Value;
pub(crate) use show::quote;
use std::collections::BTreeMap;
use std::io::Write;

/// Ranked candidates a text error lists; `--json` keeps the full list.
const TEXT_CANDIDATES: usize = 10;
/// Distinct rule codes a text summary names per severity; counts stay exact.
const TEXT_CODES: usize = 12;

/// Read formats rendered as brief text; other commands keep their text.
pub(crate) fn handles(format: &str) -> bool {
    matches!(
        format,
        "ara.status/v1"
            | "ara.ls/v1"
            | "ara.show/v1"
            | "ara.path/v1"
            | "ara.refs/v1"
            | "ara.open/v1"
            | "ara.find/v1"
    )
}

/// Write a read result to stdout and its diagnostics summary to stderr.
pub(crate) fn print(value: &Value) {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    // A reader that stops early (`| head`) is not an error.
    if let Err(error) = render(value, &mut out).and_then(|()| out.flush())
        && error.kind() != std::io::ErrorKind::BrokenPipe
    {
        panic!("stdout output: {error}");
    }
    if value["format"] != "ara.status/v1"
        && let Some(summary) = diagnostics(&value["diagnostics"])
    {
        eprintln!("{summary}");
    }
}

pub(crate) fn render(value: &Value, out: &mut impl Write) -> std::io::Result<()> {
    match value["format"].as_str().unwrap_or("") {
        "ara.show/v1" => {
            for block in show::blocks(value) {
                block.write(out)?;
            }
            Ok(())
        }
        "ara.find/v1" => lists::find(value, out),
        "ara.ls/v1" => lists::ls(value, out),
        "ara.status/v1" => lists::status(value, out),
        "ara.path/v1" => lists::path(value, out),
        "ara.refs/v1" => lists::refs(value, out),
        "ara.open/v1" => lists::open(value, out),
        _ => writeln!(out, "{value:#}"),
    }
}

/// Rule codes with repeat counts, sorted by code: `ARA206×2, ARA208`.
/// Past [`TEXT_CODES`] distinct codes, the rest are counted, not named.
fn codes(codes: &[&str]) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    for code in codes {
        *counts.entry(code).or_default() += 1;
    }
    let omitted = counts.len().saturating_sub(TEXT_CODES);
    let mut named: Vec<String> = counts
        .into_iter()
        .take(TEXT_CODES)
        .map(|(code, count)| {
            if count > 1 {
                format!("{code}×{count}")
            } else {
                code.to_owned()
            }
        })
        .collect();
    if omitted > 0 {
        named.push(format!("… {omitted} more codes"));
    }
    named.join(", ")
}

/// `N errors (codes), M warnings (codes)` from rule codes, one per
/// diagnostic, ending in a pointer to `ara check` when there is any.
pub(crate) fn summary(errors: &[&str], warnings: &[&str]) -> String {
    let part = |found: &[&str], name: &str| {
        if found.is_empty() {
            format!("0 {name}")
        } else {
            format!("{} {name} ({})", found.len(), codes(found))
        }
    };
    let mut text = format!("{}, {}", part(errors, "errors"), part(warnings, "warnings"));
    if !errors.is_empty() || !warnings.is_empty() {
        text.push_str("; run `ara check` for locations");
    }
    text
}

/// The `code` of each diagnostic row under `key`.
fn row_codes<'a>(diagnostics: &'a Value, key: &str) -> Vec<&'a str> {
    diagnostics[key]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["code"].as_str().unwrap_or("?"))
        .collect()
}
/// One line naming error and warning counts and codes, or `None` when the
/// load reported nothing.
pub(crate) fn diagnostics(diagnostics: &Value) -> Option<String> {
    let errors = row_codes(diagnostics, "errors");
    let warnings = row_codes(diagnostics, "warnings");
    (!errors.is_empty() || !warnings.is_empty())
        .then(|| format!("diagnostics: {}", summary(&errors, &warnings)))
}

/// A text error: the code and message, then the actionable details a JSON
/// caller would read (hint, candidates, blocking codes, file-access roots).
pub(crate) fn error(error: &AgentError) -> String {
    let message = error.summary.as_deref().unwrap_or(&error.message);
    let mut text = format!("error [{}]: {message}", error.code);
    let Some(details) = error.details.as_deref() else {
        return text;
    };
    let list = |key: &str| {
        details[key].as_array().map(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map_or_else(|| item.to_string(), str::to_owned)
                })
                .collect::<Vec<_>>()
        })
    };
    if let Some(hint) = details["hint"].as_str() {
        text.push_str(&format!("\n  hint: {hint}"));
    }
    if let Some(candidates) = list("candidates") {
        if candidates.is_empty() {
            text.push_str("\n  candidates: none");
        } else {
            text.push_str("\n  candidates:");
            for candidate in candidates.iter().take(TEXT_CANDIDATES) {
                text.push_str(&format!("\n    {candidate}"));
            }
            let hidden = candidates.len().saturating_sub(TEXT_CANDIDATES);
            if hidden > 0 || details["capped"] == true {
                let more = if details["capped"] == true { "+" } else { "" };
                text.push_str(&format!(
                    "\n    … {hidden}{more} more (`--json` lists up to 40)"
                ));
            }
        }
    }
    for key in ["blocking", "file_access"] {
        if let Some(items) = list(key) {
            text.push_str(&format!("\n  {key}: {}", items.join(" ")));
        }
    }
    if let Some(items) = list("unrepresented") {
        for item in items {
            text.push_str(&format!("\n  unrepresented: {item}"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diagnostics_deduplicate_display_but_keep_counts() {
        let value = json!({
            "errors": [{"code":"ARA107"},{"code":"ARA107"},{"code":"ARA109"}],
            "warnings": [{"code":"ARA206"}],
        });
        let summary = diagnostics(&value).unwrap();
        assert!(summary.contains("3 errors (ARA107×2, ARA109)"), "{summary}");
        assert!(summary.contains("1 warnings (ARA206)"), "{summary}");
        assert_eq!(summary.matches("ARA107").count(), 1);
        assert!(diagnostics(&json!({"errors":[],"warnings":[]})).is_none());
    }

    #[test]
    fn diagnostic_summaries_name_a_bounded_number_of_codes() {
        let many: Vec<String> = (100..130).map(|code| format!("ARA{code}")).collect();
        let mut rows: Vec<Value> = many.iter().map(|code| json!({"code": code})).collect();
        rows.push(json!({"code": "ARA100"}));
        let summary = diagnostics(&json!({"errors": rows, "warnings": []})).unwrap();
        // The count keeps every diagnostic; the names stop at the cap.
        assert!(summary.contains("31 errors (ARA100×2, "), "{summary}");
        assert_eq!(summary.matches("ARA1").count(), TEXT_CODES);
        assert!(summary.contains(&format!("… {} more codes", 30 - TEXT_CODES)));
    }

    #[test]
    fn text_errors_keep_actionable_details() {
        let error = AgentError {
            details: Some(Box::new(json!({
                "candidates": ["C04", "logic/claims.md#h/Claims/C05"],
                "capped": true,
                "blocking": ["ARA100"],
                "hint": "Run ara check",
            }))),
            ..AgentError::semantic("unknown_id", "Unknown")
        };
        let text = super::error(&error);
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        assert!(lines.contains(&"C04"));
        assert!(lines.contains(&"logic/claims.md#h/Claims/C05"));
        assert!(text.contains("ARA100") && text.contains("Run ara check"));
    }
}
