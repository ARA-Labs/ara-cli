//! Versioned output and the agent-command 0/1/2 exit contract.
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    io::{BufWriter, Write},
    process::ExitCode,
};

pub const EXCERPT_CHARS: usize = 160;

#[derive(Debug, Clone, Serialize)]
pub struct AgentError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Box<Value>>,
    #[serde(skip)]
    pub exit: u8,
}
impl AgentError {
    pub fn semantic(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            id: None,
            line: None,
            details: None,
            exit: 1,
        }
    }
    pub fn setup(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            exit: 2,
            ..Self::semantic(code, message)
        }
    }
    pub fn io(message: impl Into<String>) -> Self {
        Self::setup("io_error", message)
    }
    pub fn unknown(id: &str) -> Self {
        Self {
            id: Some(id.into()),
            ..Self::semantic(
                "unknown_id",
                format!("Unknown or ambiguous selector `{id}`"),
            )
        }
    }
}
impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AgentError {}

pub fn excerpt(text: &str) -> String {
    let mut output = String::with_capacity(text.len().min(EXCERPT_CHARS * 4));
    let mut count = 0;
    let mut space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            space = !output.is_empty();
            continue;
        }
        for next in space
            .then_some(' ')
            .into_iter()
            .chain(std::iter::once(character))
        {
            if count == EXCERPT_CHARS {
                let end = output
                    .char_indices()
                    .nth(EXCERPT_CHARS - 1)
                    .map_or(output.len(), |(offset, _)| offset);
                output.truncate(end);
                output.push('…');
                return output;
            }
            output.push(next);
            count += 1;
        }
        space = false;
    }
    output
}

pub fn project(value: &mut Value, fields: Option<&str>) -> Result<(), AgentError> {
    let Some(fields) = fields else {
        return Ok(());
    };
    let names = fields.split(',').map(str::trim).collect::<Vec<_>>();
    if names.iter().any(|name| name.is_empty()) {
        return Err(AgentError::semantic(
            "unknown_field",
            "--fields requires nonempty field names",
        ));
    }
    let object = value.as_object_mut().ok_or_else(|| {
        AgentError::semantic("invalid_projection", "Projection requires an object")
    })?;
    if names.iter().all(|name| object.contains_key(*name)) {
        object.retain(|name, _| {
            matches!(
                name.as_str(),
                "format"
                    | "id"
                    | "key"
                    | "committed"
                    | "dry_run"
                    | "operations"
                    | "bindings"
                    | "changed_paths"
                    | "created_directories"
                    | "no_op"
                    | "turn"
                    | "target"
                    | "unresolved_count"
                    | "complete"
                    | "diagnostics"
                    | "advisories"
                    | "duplicate_candidates"
            ) || names.contains(&name.as_str())
        });
        return Ok(());
    }
    let arrays = [
        "entries",
        "steps",
        "items",
        "results",
        "structured",
        "prose",
    ];
    if !arrays
        .iter()
        .any(|key| object.get(*key).is_some_and(Value::is_array))
    {
        return Err(AgentError::semantic(
            "unknown_field",
            "Unknown projected field",
        ));
    }
    let format = object.get("format").and_then(Value::as_str).unwrap_or("");
    for name in &names {
        let present = arrays
            .iter()
            .filter_map(|key| object.get(*key).and_then(Value::as_array))
            .flatten()
            .any(|row| row.get(*name).is_some());
        if !present && !known_row_field(format, name) {
            return Err(AgentError::semantic(
                "unknown_field",
                format!("Unknown projected field `{name}`"),
            ));
        }
    }
    for key in arrays {
        if let Some(rows) = object.get_mut(key).and_then(Value::as_array_mut) {
            for row in rows {
                if let Some(row) = row.as_object_mut() {
                    row.retain(|name, _| {
                        matches!(name.as_str(), "id" | "key" | "kind" | "source")
                            || names.contains(&name.as_str())
                    });
                }
            }
        }
    }
    Ok(())
}
fn known_row_field(format: &str, name: &str) -> bool {
    if format == "ara.find/v1" {
        return matches!(
            name,
            "id" | "key" | "kind" | "source" | "score" | "excerpt" | "entry"
        );
    }
    if format == "ara.refs/v1" {
        return matches!(
            name,
            "id" | "source" | "field" | "literal" | "certainty" | "range" | "context"
        );
    }
    let node = matches!(
        name,
        "id" | "key"
            | "kind"
            | "source"
            | "label"
            | "title"
            | "support_level"
            | "description"
            | "thinking"
            | "provenance"
            | "timestamp"
            | "fields"
            | "evidence_notes"
            | "isolated"
            | "pos"
            | "same_as"
            | "artifacts"
            | "concepts"
            | "source_fields"
            | "relations"
    );
    if format == "ara.path/v1" {
        return node;
    }
    node || matches!(
        name,
        "statement"
            | "status"
            | "proof"
            | "deps"
            | "proof_content"
            | "falsification"
            | "conditions"
            | "sources"
            | "tags"
            | "last_revised"
            | "body"
            | "source_file"
            | "content"
            | "context"
            | "potential_type"
            | "promoted_to"
            | "crystallized_via"
            | "bound_to"
            | "promoted"
            | "stale"
            | "extra"
            | "date"
            | "started"
            | "last_turn"
            | "summary"
            | "turn_count"
            | "events_logged"
            | "ai_actions"
            | "claims_touched"
            | "logic_revisions"
            | "key_context"
            | "open_threads"
            | "ai_suggestions_pending"
            | "metadata_extra"
            | "rationale"
            | "sensitivity"
            | "code_ref"
            | "evidence_output"
            | "question"
            | "setup"
            | "prediction"
            | "target"
            | "comment"
            | "tag"
            | "object"
            | "term"
            | "notation"
            | "definition"
            | "boundary"
            | "related"
            | "cite"
            | "doi"
            | "what_changed"
            | "why"
            | "adopted"
            | "claims_affected"
            | "name"
            | "file"
            | "image"
            | "claims"
            | "document"
            | "heading"
            | "heading_path"
            | "address"
            | "digest"
            | "reasons"
    )
}

pub fn emit(
    format: &str,
    result: Result<Value, AgentError>,
    json_mode: bool,
    fields: Option<&str>,
) -> ExitCode {
    let result = result.and_then(|mut value| {
        project(&mut value, fields)?;
        Ok(value)
    });
    match result {
        Ok(value) => {
            if json_mode {
                let stdout = std::io::stdout();
                let mut writer = BufWriter::new(stdout.lock());
                serde_json::to_writer(&mut writer, &value).expect("JSON output");
                writer.write_all(b"\n").expect("stdout output");
                writer.flush().expect("stdout output");
            } else {
                print_human(&value);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json_mode {
                eprintln!("{}", json!({"format":format,"error":error}));
            } else {
                eprintln!("error [{}]: {}", error.code, error.message);
            }
            ExitCode::from(error.exit)
        }
    }
}

fn print_human(value: &Value) {
    if let Some(warnings) = value["diagnostics"]["warnings"].as_array() {
        for warning in warnings {
            eprintln!(
                "warning [{}] {}: {}",
                warning["code"].as_str().unwrap_or(""),
                warning["path"].as_str().unwrap_or(""),
                warning["message"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(candidates) = value["duplicate_candidates"].as_array() {
        for candidate in candidates {
            eprintln!(
                "warning: {} and {} may repeat the same finding (lexical similarity {:.3}; not equivalence)",
                candidate["left"].as_str().unwrap_or(""),
                candidate["right"].as_str().unwrap_or(""),
                candidate["similarity"].as_f64().unwrap_or(0.0)
            );
        }
    }
    if let Some(advisories) = value["advisories"].as_array() {
        for advisory in advisories {
            eprintln!(
                "warning: {}",
                advisory.as_str().unwrap_or("Advisory unavailable")
            );
        }
    }
    if value.get("operations").is_some()
        && let Some(id) = value["id"].as_str()
    {
        println!("{id}");
        return;
    }
    if let Some(rows) = ["entries", "steps", "items", "results"]
        .into_iter()
        .find_map(|name| value.get(name).and_then(Value::as_array))
    {
        if value["format"] == "ara.show/v1" {
            for row in rows {
                if row["kind"] == "source_document"
                    && let Some(content) = row["content"].as_str()
                {
                    print!("{content}");
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(row).expect("entry serialization")
                    );
                }
            }
        } else {
            for row in rows {
                let id = row
                    .get("id")
                    .or_else(|| row.get("key"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let kind = row.get("kind").and_then(Value::as_str).unwrap_or("");
                let title = [
                    "title",
                    "term",
                    "cite",
                    "name",
                    "excerpt",
                    "statement",
                    "summary",
                    "content",
                ]
                .iter()
                .find_map(|key| row.get(*key).and_then(Value::as_str))
                .unwrap_or("");
                println!("{id}\t{kind}\t{title}");
            }
        }
        return;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("JSON value serialization")
    );
}
