//! Shared discovery for agent commands. Reads never provision operational files.
use std::path::{Path, PathBuf};

use crate::output::AgentError;

pub fn discover(explicit: Option<&Path>) -> Result<PathBuf, AgentError> {
    discover_with_mode(explicit, false)
}

pub fn discover_writer(explicit: Option<&Path>) -> Result<PathBuf, AgentError> {
    discover_with_mode(explicit, true)
}
/// Explicit source inspection also supports a partially initialized knowledge
/// root; structural discovery still requires an exploration tree.
pub fn discover_source(explicit: Option<&Path>) -> Result<PathBuf, AgentError> {
    let cwd = std::env::current_dir().map_err(|error| AgentError::io(error.to_string()))?;
    let selected = explicit
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("ARA_DIR").map(PathBuf::from));
    if let Some(selected) = selected {
        let selected = cwd.join(selected);
        let metadata = std::fs::symlink_metadata(&selected)
            .map_err(|error| AgentError::io(format!("{}: {error}", selected.display())))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AgentError::setup(
                "invalid_artifact_path",
                "Source root must be a real directory",
            ));
        }
        let root = selected
            .canonicalize()
            .map_err(|error| AgentError::io(error.to_string()))?;
        ensure_no_pending_transaction(&root)?;
        Ok(root)
    } else {
        discover(None)
    }
}

fn discover_with_mode(explicit: Option<&Path>, writer: bool) -> Result<PathBuf, AgentError> {
    let cwd = std::env::current_dir().map_err(|e| AgentError::io(e.to_string()))?;
    if let Some(path) = explicit {
        return validate_with_mode(&cwd.join(path), writer);
    }
    if let Some(path) = std::env::var_os("ARA_DIR") {
        return validate_with_mode(&cwd.join(path), writer);
    }
    for ancestor in cwd.ancestors() {
        for candidate in [ancestor.to_path_buf(), ancestor.join("ara")] {
            if candidate.join("trace/exploration_tree.yaml").is_file() {
                return validate_with_mode(&candidate, writer);
            }
        }
    }
    Err(AgentError::setup(
        "discovery_failed",
        "No ARA found; use -C or ARA_DIR",
    ))
}

pub fn validate(path: &Path) -> Result<PathBuf, AgentError> {
    validate_with_mode(path, false)
}

fn validate_with_mode(path: &Path, writer: bool) -> Result<PathBuf, AgentError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| AgentError::io(format!("{}: {error}", path.display())))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AgentError::setup(
            "invalid_artifact_path",
            "Artifact root must be a real directory",
        ));
    }
    let root = path
        .canonicalize()
        .map_err(|e| AgentError::io(format!("{}: {e}", path.display())))?;
    if !root.join("trace/exploration_tree.yaml").is_file() {
        return Err(AgentError::setup(
            "invalid_artifact_path",
            format!(
                "{} does not contain trace/exploration_tree.yaml",
                root.display()
            ),
        ));
    }
    if !writer {
        ensure_no_pending_transaction(&root)?;
    }
    Ok(root)
}

pub fn ensure_no_pending_transaction(root: &Path) -> Result<(), AgentError> {
    if ara_core::write::journal::pending_prepared(root).map_err(crate::write::convert_error)? {
        return Err(AgentError::setup(
            "pending_transaction",
            "A prepared transaction requires recovery by an ara writer before reading",
        ));
    }
    Ok(())
}
