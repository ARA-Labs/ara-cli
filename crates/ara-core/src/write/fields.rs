//! Authoring field schemas. Existing unrecognized source fields remain opaque.
use super::{Fields, WriteError};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Claim,
    Heuristic,
    Concept,
    RelatedWork,
    Experiment,
    Problem,
    Solution,
}

pub fn kind(document: &str) -> Result<EntryKind, WriteError> {
    match document {
        "logic/claims.md" => Ok(EntryKind::Claim),
        "logic/solution/heuristics.md" => Ok(EntryKind::Heuristic),
        "logic/concepts.md" => Ok(EntryKind::Concept),
        "logic/related_work.md" => Ok(EntryKind::RelatedWork),
        "logic/experiments.md" => Ok(EntryKind::Experiment),
        "logic/problem.md" => Ok(EntryKind::Problem),
        p if super::documents::allowed(p) => Ok(EntryKind::Solution),
        _ => Err(WriteError::semantic(
            "write.document",
            "Not a mutable logic document",
        )),
    }
}

pub fn canonical(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase().replace('_', " ");
    match name.as_str() {
        "falsification criteria" => "falsification".into(),
        "boundary conditions" => "boundary".into(),
        "related concepts" => "related".into(),
        _ => name,
    }
}

fn registry(kind: EntryKind) -> &'static [&'static str] {
    match kind {
        EntryKind::Claim => &[
            "Statement",
            "Conditions",
            "Sources",
            "Status",
            "Provenance",
            "Falsification",
            "Proof",
            "Evidence basis",
            "Dependencies",
            "Tags",
            "Merged into",
            "Last revised",
        ],
        EntryKind::Heuristic => &[
            "Rationale",
            "Source",
            "Sources",
            "Status",
            "Provenance",
            "Sensitivity",
            "Bounds",
            "Code ref",
            "Tags",
            "Last revised",
        ],
        EntryKind::Concept => &[
            "Definition",
            "Role",
            "Appears in",
            "Notation",
            "Boundary",
            "Related",
            "Provenance",
            "Sources",
            "Last revised",
        ],
        EntryKind::RelatedWork => &[
            "DOI",
            "Type",
            "Delta",
            "Claims affected",
            "Adopted elements",
            "Bounds",
            "Baseline",
            "What",
            "How ARA extends",
            "Key finding from prior work",
            "Sources",
            "Provenance",
            "Last revised",
        ],
        EntryKind::Experiment => &[
            "Verifies",
            "Evidence",
            "Run",
            "Setup",
            "Procedure",
            "Metrics",
            "Expected outcome",
            "Baselines",
            "Dependencies",
            "Question",
            "Prediction (directional)",
            "Predictions (directional)",
            "Falsification condition",
            "Status",
            "Evidence output",
            "Sources",
            "Provenance",
            "Last revised",
        ],
        EntryKind::Problem => &[
            "Statement",
            "Evidence",
            "Implication",
            "Caused by",
            "Existing attempts",
            "Why they fail",
            "Why it matters",
            "Insight",
            "Derived from",
            "Enables",
            "Conditions",
            "Sources",
            "Provenance",
            "Last revised",
        ],
        EntryKind::Solution => &[
            "Statement",
            "Definition",
            "Description",
            "Rationale",
            "Role",
            "Constraint",
            "Architecture",
            "Mechanism",
            "Algorithm",
            "Inputs",
            "Outputs",
            "Procedure",
            "Conditions",
            "Bounds",
            "Sources",
            "Status",
            "Provenance",
            "Code ref",
            "Appears in",
            "Dependencies",
            "Tags",
            "Last revised",
        ],
    }
}

pub fn spelling(kind: EntryKind, name: &str) -> Option<&'static str> {
    let key = canonical(name);
    registry(kind)
        .iter()
        .copied()
        .find(|label| canonical(label) == key)
}

pub fn validate(
    kind: EntryKind,
    fields: &Fields,
    creation: bool,
    revision: bool,
) -> Result<Fields, WriteError> {
    let mut result = BTreeMap::new();
    for (name, value) in fields {
        let label = spelling(kind, name).ok_or_else(|| {
            WriteError::semantic("write.field", format!("Unknown field {name}")).at(name)
        })?;
        let key = canonical(label);
        if result.contains_key(label) {
            return Err(WriteError::semantic(
                "write.field_duplicate",
                format!("Duplicate canonical field {label}"),
            )
            .at(name));
        }
        if key == "last revised" && !revision {
            return Err(WriteError::semantic(
                "write.revision_required",
                "Last revised is owned by logic.revise",
            )
            .at(name));
        }
        if key == "merged into" && !revision {
            return Err(WriteError::semantic(
                "write.revision_required",
                "Merged into requires logic.revise with session history",
            )
            .at(name));
        }
        let list = value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string));
        let valid = match key.as_str() {
            "dependencies" => {
                list && value.as_array().is_some_and(|items| {
                    items.iter().all(|v| {
                        typed_id(
                            v.as_str().unwrap(),
                            if kind == EntryKind::Experiment {
                                "E"
                            } else {
                                "C"
                            },
                        )
                    })
                })
            }
            "merged into" => value.as_str().is_some_and(|id| typed_id(id, "C")),
            "source" | "sources" | "proof" | "tags" | "code ref" | "appears in" | "related"
            | "verifies" | "evidence" | "run" | "procedure" | "metrics" | "expected outcome"
            | "baselines" | "claims affected" | "adopted elements" => value.is_string() || list,
            "weight" => value.is_string() || value.is_number(),
            _ => value.is_string(),
        };
        if !valid {
            return Err(WriteError::semantic(
                "write.field_type",
                format!("Invalid type or reference for {label}"),
            )
            .at(name));
        }
        let scalar = value.as_str();
        let vocabulary_ok = match key.as_str() {
            "status" if kind == EntryKind::Claim => scalar.is_some_and(|s| {
                matches!(
                    s,
                    "hypothesis"
                        | "untested"
                        | "testing"
                        | "supported"
                        | "weakened"
                        | "refuted"
                        | "withdrawn"
                )
            }),
            "status" if kind == EntryKind::Heuristic => {
                scalar.is_some_and(|s| matches!(s, "active" | "weakened" | "retired"))
            }
            "provenance" => scalar.is_some_and(|s| {
                matches!(s, "user" | "ai-suggested" | "ai-executed" | "user-revised")
            }),
            "sensitivity" => scalar.is_some_and(|s| {
                matches!(
                    s,
                    "low" | "medium" | "high" | "unknown" | "Not specified in paper"
                )
            }),
            _ => true,
        };
        if !vocabulary_ok {
            return Err(WriteError::semantic(
                "write.field_value",
                format!("Unknown authored value for {label}"),
            )
            .at(name));
        }
        result.insert(label.into(), value.clone());
    }
    if creation {
        let required: &[&str] = match kind {
            EntryKind::Claim => &[
                "Statement",
                "Conditions",
                "Status",
                "Provenance",
                "Falsification",
            ],
            EntryKind::Heuristic => &["Rationale", "Sensitivity", "Code ref"],
            _ => &[],
        };
        for name in required {
            if !result.contains_key(*name) {
                return Err(WriteError::semantic(
                    "write.field_required",
                    format!("Required field {name}"),
                )
                .at(*name));
            }
        }
    }
    Ok(result)
}

pub fn typed_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.bytes().all(|b| b.is_ascii_digit())
            && suffix.bytes().any(|b| b != b'0')
    })
}

pub fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => serde_json::to_string(value).expect("JSON value is serializable"),
    }
}

/// Every caller line, including the final empty one, has its own continuation.
/// Structural separators are LF; no caller bytes are trimmed or normalized.
pub fn render(name: &str, value: &Value) -> String {
    let text = value_text(value);
    let mut out = format!("- **{name}**:\n");
    for line in text.split('\n') {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Field lines of a newly created claim or heuristic block, in the fixed
/// registry order regardless of surrounding entries or input order. Callers
/// pass fields already accepted by [`validate`], whose keys are registry labels.
/// Inline lines end with `eol` (the target file's convention); continuation
/// values keep LF, the only separator the continuation reader decodes.
pub fn render_created(kind: EntryKind, fields: &Fields, eol: &str) -> String {
    debug_assert!(
        fields
            .keys()
            .all(|name| registry(kind).contains(&name.as_str())),
        "creation fields must be validated registry labels"
    );
    let mut out = String::new();
    for label in registry(kind) {
        if let Some(value) = fields.get(*label) {
            out.push_str(&render_created_field(label, value, eol));
        }
    }
    out
}

/// One new field. Dependencies render as the bracketed typed-ID list the claim
/// reader already parses. Other lists keep compact JSON: no current reader of a
/// flexible list field (Proof, Sources, Tags, Code ref) decodes a plain comma
/// list back to the same list, so comma-joining would lose structure. Scalars
/// are inline only when the source reader decodes the inline line to the exact
/// caller text; anything else keeps the lossless continuation form.
fn render_created_field(label: &str, value: &Value, eol: &str) -> String {
    let inline = match value {
        Value::Array(items) if canonical(label) == "dependencies" => {
            let ids: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            Some(format!("[{}]", ids.join(", ")))
        }
        Value::Array(_) => Some(value_text(value)),
        Value::String(text) => Some(text.clone()),
        _ => None,
    };
    match inline.and_then(|text| inline_line(label, &text, eol)) {
        Some(line) => line,
        None => render(label, value),
    }
}

/// The exact `- **label**: text` line to write, if the source field reader
/// decodes those bytes back to exactly `text`. Multiline, empty and
/// outer-whitespace values fail.
fn inline_line(label: &str, text: &str, eol: &str) -> Option<String> {
    if text.is_empty() || text.contains(['\n', '\r']) || text.trim() != text {
        return None;
    }
    let line = format!("- **{label}**: {text}{eol}");
    let parsed = crate::markdown::fields(&line, 0..line.len());
    let exact = parsed.len() == 1
        && parsed[0].name == label
        && parsed[0].range == (0..line.len())
        && crate::markdown::decode_field(&parsed[0]) == text;
    exact.then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Decode every field of a rendered block through the source reader.
    fn decoded(block: &str) -> Vec<(String, String)> {
        crate::markdown::fields(block, 0..block.len())
            .iter()
            .map(|f| {
                (
                    f.name.to_owned(),
                    crate::markdown::decode_field(f).into_owned(),
                )
            })
            .collect()
    }

    fn fields(value: Value) -> Fields {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn created_fields_follow_registry_order_not_input_or_alphabetical_order() {
        let input = fields(json!({
            "tags": ["x"],
            "Dependencies": [],
            "Proof": [],
            "Falsification criteria": "F text",
            "Status": "supported",
            "Provenance": "user",
            "Evidence basis": "E01 table",
            "Sources": "paper §3",
            "Conditions": "C text",
            "statement": "S text",
        }));
        let valid = validate(EntryKind::Claim, &input, true, false).unwrap();
        let block = render_created(EntryKind::Claim, &valid, "\n");
        let names: Vec<_> = decoded(&block).into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "Statement",
                "Conditions",
                "Sources",
                "Status",
                "Provenance",
                "Falsification",
                "Proof",
                "Evidence basis",
                "Dependencies",
                "Tags",
            ]
        );
        assert_eq!(
            block,
            "- **Statement**: S text\n- **Conditions**: C text\n- **Sources**: paper §3\n\
             - **Status**: supported\n- **Provenance**: user\n- **Falsification**: F text\n\
             - **Proof**: []\n- **Evidence basis**: E01 table\n- **Dependencies**: []\n\
             - **Tags**: [\"x\"]\n"
        );

        let heuristic = fields(json!({
            "Tags": "a, b",
            "Code ref": "src/run.rs",
            "Bounds": "Only synthetic data",
            "Sensitivity": "unknown",
            "Provenance": "user",
            "Status": "active",
            "Sources": ["doi:1"],
            "Source": "paper.pdf p3",
            "Rationale": "Reason",
        }));
        let valid = validate(EntryKind::Heuristic, &heuristic, true, false).unwrap();
        let names: Vec<_> = decoded(&render_created(EntryKind::Heuristic, &valid, "\n"))
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            names,
            [
                "Rationale",
                "Source",
                "Sources",
                "Status",
                "Provenance",
                "Sensitivity",
                "Bounds",
                "Code ref",
                "Tags",
            ]
        );
    }

    #[test]
    fn created_scalars_are_inline_only_when_the_reader_returns_exact_text() {
        let inline = [
            "plain",
            "雪 café ünïcode",
            "a, b",
            "[]",
            "none",
            "[\"x\"]",
            "{\"k\": 1}",
            ": leading colon",
            "**bold**: text",
            "<!-- not a comment line -->",
            "```",
            "## not a heading",
        ];
        for text in inline {
            let block = render_created_field("Statement", &json!(text), "\n");
            assert_eq!(block, format!("- **Statement**: {text}\n"));
            assert_eq!(decoded(&block), [("Statement".to_owned(), text.to_owned())]);
        }
        let continued = [
            "",
            " leading",
            "trailing ",
            "\u{a0}nbsp",
            "a\nb",
            "final newline\n",
            "a\r\nb\r\n",
            "\n\n",
            "lone\rcarriage",
        ];
        for text in continued {
            let block = render_created_field("Statement", &json!(text), "\n");
            assert!(block.starts_with("- **Statement**:\n  "), "{block:?}");
            assert_eq!(decoded(&block), [("Statement".to_owned(), text.to_owned())]);
        }
    }

    #[test]
    fn created_lists_are_lossless_and_dependencies_use_typed_brackets() {
        for (ids, expected) in [
            (json!([]), "- **Dependencies**: []\n"),
            (json!(["C03"]), "- **Dependencies**: [C03]\n"),
            (json!(["C03", "C04"]), "- **Dependencies**: [C03, C04]\n"),
        ] {
            assert_eq!(render_created_field("Dependencies", &ids, "\n"), expected);
        }
        let lists = [
            json!([]),
            json!(["evaluation", "experimental-design"]),
            json!(["a, b", "c"]),
            json!(["[x]", "\"q\"", "back\\slash", "", "none", " pad "]),
            json!(["雪", "multi\nline", "crlf\r\n"]),
        ];
        for name in ["Proof", "Sources", "Tags", "Code ref"] {
            for list in &lists {
                let block = render_created_field(name, list, "\n");
                assert!(!block.contains("\n  "), "{block:?}");
                let (label, text) = decoded(&block).remove(0);
                assert_eq!(label, name);
                assert_eq!(&serde_json::from_str::<Value>(&text).unwrap(), list);
            }
        }
    }
}
