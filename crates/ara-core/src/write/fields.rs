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
