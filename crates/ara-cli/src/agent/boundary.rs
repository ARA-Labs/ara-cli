//! Native document boundary for reads and its `invalid_document` error.
use crate::output::AgentError;
use serde_json::json;

/// Native document reads. `PAPER.md` `knowledge_paths` entries extend them.
const NATIVE_DOCUMENTS: [&str; 4] = [
    "PAPER.md",
    "logic/**/*.md",
    "trace/**/*.yaml",
    "staging/observations.yaml",
];

pub fn knowledge_document(path: &str) -> bool {
    path == "PAPER.md"
        || path.starts_with("logic/") && path.ends_with(".md")
        || path.starts_with("trace/") && path.ends_with(".yaml")
        || path == "staging/observations.yaml"
}

/// `invalid_document` with a machine-readable hint: which documents `ara`
/// reads natively and which roots belong to the agent's own file tools.
pub fn invalid_document() -> AgentError {
    AgentError {
        details: Some(Box::new(json!({
            "hint": "ara reads PAPER.md, logic/, trace/, staging/observations.yaml and paths registered in PAPER.md knowledge_paths. Read rubric/, evidence/ and src/ files directly with your file tools (grep, read).",
            "native_documents": NATIVE_DOCUMENTS,
            "registered_documents": "PAPER.md knowledge_paths",
            "file_access": ara_core::FILE_ACCESS_ROOTS,
        }))),
        ..AgentError::semantic(
            "invalid_document",
            "Document outside the knowledge boundary",
        )
    }
}
