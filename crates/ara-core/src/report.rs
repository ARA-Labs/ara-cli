//! Diagnostics produced by parsing/validation.
//!
//! A [`Diagnostic`] carries a **logical** path (e.g. `nodes[N07].evidence[0]`),
//! not a source `line:column` — `serde-saphyr` does not expose reliable spans
//! through serde, so line numbers are intentionally not promised.
//!
//! Every diagnostic also carries the [`RuleCode`] of the check that produced it.
//! The code is deliberately **not** part of the `Display` text or the serde
//! output, so `ara validate`'s human and `--json` output stay byte-stable; only
//! `ara check` renders it.

use serde::Serialize;

use crate::rules::RuleCode;

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => f.write_str("error"),
            Severity::Warning => f.write_str("warning"),
        }
    }
}

/// A single diagnostic: rule code, severity, logical path, and message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnostic {
    /// The rule that produced this diagnostic. Not serialized and not shown by
    /// `Display` (see the module docs); `ara check` renders it explicitly.
    #[serde(skip)]
    pub code: RuleCode,
    pub severity: Severity,
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}: {}", self.severity, self.path, self.message)
    }
}

/// The outcome of a parse: separated errors and warnings.
///
/// A parse "succeeds" (`is_ok`) when there are no errors — warnings do not
/// block success but **must** still be surfaced by callers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ParseReport {
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
}

impl ParseReport {
    #[cfg(feature = "native")]
    pub(crate) fn append(&mut self, mut other: Self) {
        self.errors.append(&mut other.errors);
        self.warnings.append(&mut other.warnings);
    }
    /// Records an error produced by rule `code`.
    pub(crate) fn error(
        &mut self,
        code: RuleCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        debug_assert_eq!(code.default_severity(), Severity::Error, "{code}");
        self.errors.push(Diagnostic {
            code,
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
        });
    }

    /// Records a warning produced by rule `code`.
    pub(crate) fn warn(
        &mut self,
        code: RuleCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        debug_assert_eq!(code.default_severity(), Severity::Warning, "{code}");
        self.warnings.push(Diagnostic {
            code,
            severity: Severity::Warning,
            path: path.into(),
            message: message.into(),
        });
    }

    /// All errors, in the order they were recorded.
    pub fn errors(&self) -> &[Diagnostic] {
        &self.errors
    }

    /// All warnings, in the order they were recorded.
    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }

    /// True when there are no errors (warnings are allowed).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

impl std::fmt::Display for ParseReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for d in &self.errors {
            writeln!(f, "{d}")?;
        }
        for d in &self.warnings {
            writeln!(f, "{d}")?;
        }
        write!(
            f,
            "{} error(s), {} warning(s)",
            self.errors.len(),
            self.warnings.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_and_accessors() {
        let mut r = ParseReport::default();
        assert!(r.is_ok());
        r.warn(RuleCode::EmptyTree, "document", "empty");
        assert!(r.is_ok()); // warnings don't block
        assert_eq!(r.warnings().len(), 1);
        r.error(RuleCode::DuplicateNodeId, "nodes[N01]", "duplicate node id");
        assert!(!r.is_ok());
        assert_eq!(r.errors().len(), 1);
    }

    #[test]
    fn diagnostic_display() {
        let d = Diagnostic {
            code: RuleCode::UnknownEvidenceClaim,
            severity: Severity::Error,
            path: "nodes[N07].evidence[0]".into(),
            message: "unknown claim".into(),
        };
        assert_eq!(
            d.to_string(),
            "error: nodes[N07].evidence[0]: unknown claim"
        );
    }

    #[test]
    fn code_is_not_serialized() {
        let mut r = ParseReport::default();
        r.error(RuleCode::DuplicateNodeId, "nodes[N01]", "duplicate node id");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(
            json,
            r#"{"errors":[{"severity":"error","path":"nodes[N01]","message":"duplicate node id"}],"warnings":[]}"#
        );
        assert_eq!(r.errors()[0].code, RuleCode::DuplicateNodeId);
    }
}
