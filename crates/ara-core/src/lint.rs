//! Format-lint layer: detects canonicalizable *drift* in an ARA artifact's raw
//! source text and emits diagnostics paired with data-only fix candidates.
//!
//! This is deliberately **separate** from [`crate::parse`]: `parse_sources` /
//! `parse_dir` normalize an artifact into a [`crate::Manifest`] and are tolerant
//! by design (aliases like `reason:`/`justification:` are accepted silently).
//! This module instead works on the *unparsed* text so it can point at the exact
//! line/byte span a later applier must rewrite, and it is **not** wired into
//! parsing — `ara validate` behavior is unchanged.
//!
//! Each [`LintDiagnostic`] carries a [`LintRuleId`], a human message, the file
//! it lives in, and (when fixable) a [`FixCandidate`] describing the edit as
//! data. Applying those candidates is step 2's job; this layer only detects.
//!
//! Scanning is regex-free string work, matching the rest of the crate (see
//! [`crate::manifest::is_canonical_id`]).

use serde::Serialize;

#[cfg(feature = "native")]
use crate::manifest::is_canonical_id;

/// A format-lint rule identifier. Serializes to its `ARA0NN` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LintRuleId {
    /// `ARA001`: top-level `root:` single-node dialect (canonical is `tree:`).
    #[serde(rename = "ARA001")]
    RootDialect,
    /// `ARA002`: `reason:` key on a `dead_end` node (canonical `why_failed:`).
    #[serde(rename = "ARA002")]
    DeadEndReasonAlias,
    /// `ARA003`: `justification:` key on a `decision` node (canonical
    /// `rationale:`).
    #[serde(rename = "ARA003")]
    DecisionRationaleAlias,
    /// `ARA004`: claim header with a dash separator instead of a colon.
    #[serde(rename = "ARA004")]
    ClaimHeaderStyle,
}

impl LintRuleId {
    /// The stable `ARA0NN` code string.
    pub fn as_str(&self) -> &'static str {
        match self {
            LintRuleId::RootDialect => "ARA001",
            LintRuleId::DeadEndReasonAlias => "ARA002",
            LintRuleId::DecisionRationaleAlias => "ARA003",
            LintRuleId::ClaimHeaderStyle => "ARA004",
        }
    }
}

impl std::fmt::Display for LintRuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which source file a diagnostic (and its fix) applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LintFile {
    /// `trace/exploration_tree.yaml`.
    Tree,
    /// `logic/claims.md`.
    Claims,
}

impl LintFile {
    /// The file's path relative to the artifact root.
    pub fn relative_path(&self) -> &'static str {
        match self {
            LintFile::Tree => "trace/exploration_tree.yaml",
            LintFile::Claims => "logic/claims.md",
        }
    }
}

/// A fix described as data, so a later step can apply it as a surgical text
/// edit without re-deriving the location.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FixCandidate {
    /// Replace the byte range `[start_col, end_col)` on 0-based line `line` with
    /// `replacement`. Columns are byte offsets within that line. Used for the
    /// line-level renames (ARA002/ARA003) and the claim-header rewrite (ARA004).
    ReplaceInLine {
        /// 0-based line index.
        line: usize,
        /// Byte offset within the line where the replaced span begins.
        start_col: usize,
        /// Byte offset within the line where the replaced span ends (exclusive).
        end_col: usize,
        /// Text to substitute for `[start_col, end_col)`.
        replacement: String,
    },
    /// ARA001 structural rewrite: turn the top-level `root:` single-node map into
    /// a one-element `tree:` list. The re-indent/re-emit algorithm is step 2's
    /// job; this candidate carries the block's location so the applier can read
    /// and transform it deterministically.
    RewriteRootToTree {
        /// 0-based line index of the top-level `root:` key.
        root_line: usize,
        /// Leading-space indentation of the `root:` key (canonically `0`).
        root_indent: usize,
        /// 0-based line index one past the last line of the `root:` block
        /// (exclusive); the block runs `[root_line, block_end_line)`.
        block_end_line: usize,
    },
}

/// One format-lint finding.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LintDiagnostic {
    /// The rule that fired.
    pub rule: LintRuleId,
    /// Human-readable explanation of the drift.
    pub message: String,
    /// The file the drift lives in.
    pub file: LintFile,
    /// Whether a canonical fix is known (mirrors `fix.is_some()`).
    pub fixable: bool,
    /// The edit to apply, when known.
    pub fix: Option<FixCandidate>,
}

/// The outcome of a format-lint pass: findings in source order.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LintReport {
    /// All diagnostics, tree file first then claims, each in source order.
    pub diagnostics: Vec<LintDiagnostic>,
}

impl LintReport {
    /// All diagnostics.
    pub fn diagnostics(&self) -> &[LintDiagnostic] {
        &self.diagnostics
    }

    /// True when no drift was detected.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Number of diagnostics carrying a fix candidate.
    pub fn fixable(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.fixable).count()
    }
}

/// Reads `trace/exploration_tree.yaml` and `logic/claims.md` from `dir` and
/// runs the format-lint rules over their raw text. Native only.
///
/// Missing files are tolerated: an absent `claims.md` (or `exploration_tree.yaml`)
/// simply contributes no diagnostics rather than erroring or panicking. This
/// never reads the parse layer — it is a pure text pass.
#[cfg(feature = "native")]
pub fn check_dir(dir: &std::path::Path) -> LintReport {
    let mut diagnostics = Vec::new();
    if let Ok(text) = std::fs::read_to_string(dir.join("trace/exploration_tree.yaml")) {
        diagnostics.extend(lint_tree(&text));
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("logic/claims.md")) {
        diagnostics.extend(lint_claims(&text));
    }
    LintReport { diagnostics }
}

/// A parsed YAML mapping-key line.
#[cfg(feature = "native")]
struct KeyLine {
    /// The key name (the token before `:`).
    key: String,
    /// The scalar value after `:` (trimmed); empty for block keys.
    value: String,
    /// True when the line is a `- ` list item.
    is_list_item: bool,
    /// Byte offset within the line of the key's first character. Sibling keys of
    /// one node map share this column, so it doubles as the node's indent key.
    key_col: usize,
}

/// A recorded occurrence of a context-scoped key (`reason:` / `justification:`).
#[cfg(feature = "native")]
struct KeyHit {
    line: usize,
    col: usize,
}

/// One node map encountered while scanning, retained after it leaves the stack
/// so its keys can be resolved against its (possibly later-declared) `type:`.
#[cfg(feature = "native")]
struct NodeFrame {
    /// The column at which this node's direct keys live.
    key_indent: usize,
    /// The node's `type:`, once seen.
    ty: Option<String>,
    /// `reason:` keys directly on this node.
    reason_hits: Vec<KeyHit>,
    /// `justification:` keys directly on this node.
    justification_hits: Vec<KeyHit>,
}

/// Counts leading ASCII spaces (YAML indentation is spaces, never tabs).
#[cfg(feature = "native")]
fn leading_spaces(s: &str) -> usize {
    s.len() - s.trim_start_matches(' ').len()
}

/// Parses a line into a [`KeyLine`] when it is a `word: ...` mapping entry
/// (optionally introduced by `- `). Returns `None` for blanks, comments, scalar
/// list items, and block-scalar continuations (free text that happens to hold a
/// colon), so those never masquerade as node keys.
#[cfg(feature = "native")]
fn parse_key_line(line: &str) -> Option<KeyLine> {
    let indent = leading_spaces(line);
    let after = &line[indent..];
    if after.is_empty() || after.starts_with('#') {
        return None;
    }

    let (is_list_item, content, base) = match after.strip_prefix("- ") {
        Some(rest) => {
            let extra = leading_spaces(rest);
            (true, &rest[extra..], indent + 2 + extra)
        }
        None => (false, after, indent),
    };

    let colon = content.find(':')?;
    let key = &content[..colon];
    // A real key is a single bare identifier token; reject free text (which
    // contains spaces) and other punctuation so block scalars never match.
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    // YAML requires a space (or end of line) after a mapping colon; this also
    // rejects scalars like `http://x` that carry an inner colon.
    let after_colon = &content[colon + 1..];
    if !(after_colon.is_empty() || after_colon.starts_with(' ')) {
        return None;
    }

    Some(KeyLine {
        key: key.to_string(),
        value: after_colon.trim().to_string(),
        is_list_item,
        key_col: base,
    })
}

/// Returns the exclusive end line of the `root:` block: the first later line at
/// indent 0 (a new top-level key), or the end of the file. Blank lines are
/// skipped so trailing blanks are not treated as a boundary.
#[cfg(feature = "native")]
fn root_block_end(lines: &[&str], root_line: usize) -> usize {
    let mut j = root_line + 1;
    while j < lines.len() {
        let l = lines[j];
        if l.trim().is_empty() {
            j += 1;
            continue;
        }
        if leading_spaces(l) == 0 {
            break;
        }
        j += 1;
    }
    j
}

/// Runs the tree-file rules (ARA001/ARA002/ARA003) over raw YAML text.
///
/// ARA002/ARA003 are context-scoped: a `reason:`/`justification:` key is only
/// flagged when it sits directly on a `dead_end`/`decision` node. A stack of
/// node frames (keyed by their direct-key column) tracks which node owns each
/// key line; frames are retained so a `type:` declared after the aliased key is
/// still resolved. Node maps are recognized by their `- ` list items, matching
/// the `tree:`/`children:` list dialect.
#[cfg(feature = "native")]
fn lint_tree(text: &str) -> Vec<LintDiagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diags = Vec::new();
    let mut frames: Vec<NodeFrame> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let Some(kl) = parse_key_line(line) else {
            continue;
        };

        // ARA001: a top-level `root:` key uses the single-node dialect.
        if !kl.is_list_item && kl.key_col == 0 && kl.key == "root" {
            diags.push(LintDiagnostic {
                rule: LintRuleId::RootDialect,
                message: "top-level `root:` uses the single-node dialect; canonical form is a \
                          `tree:` list with one element"
                    .to_string(),
                file: LintFile::Tree,
                fixable: true,
                fix: Some(FixCandidate::RewriteRootToTree {
                    root_line: i,
                    root_indent: 0,
                    block_end_line: root_block_end(&lines, i),
                }),
            });
            continue;
        }

        // Close any frames deeper than this key (dedent).
        while let Some(&top) = stack.last() {
            if frames[top].key_indent > kl.key_col {
                stack.pop();
            } else {
                break;
            }
        }

        if kl.is_list_item {
            // A list item at the same column as the current node is its sibling:
            // close the current node before opening the new one.
            if let Some(&top) = stack.last()
                && frames[top].key_indent == kl.key_col
            {
                stack.pop();
            }
            let idx = frames.len();
            frames.push(NodeFrame {
                key_indent: kl.key_col,
                ty: None,
                reason_hits: Vec::new(),
                justification_hits: Vec::new(),
            });
            stack.push(idx);
        }

        // Attribute the key to the node whose direct keys live at this column.
        if let Some(&top) = stack.last()
            && frames[top].key_indent == kl.key_col
        {
            match kl.key.as_str() {
                "type" => frames[top].ty = Some(kl.value.clone()),
                "reason" => frames[top].reason_hits.push(KeyHit {
                    line: i,
                    col: kl.key_col,
                }),
                "justification" => frames[top].justification_hits.push(KeyHit {
                    line: i,
                    col: kl.key_col,
                }),
                _ => {}
            }
        }
    }

    // Resolve context-scoped hits against each node's type.
    for f in &frames {
        if f.ty.as_deref() == Some("dead_end") {
            for hit in &f.reason_hits {
                diags.push(LintDiagnostic {
                    rule: LintRuleId::DeadEndReasonAlias,
                    message: "`reason:` on a dead_end node is an alias; canonical key is \
                              `why_failed:`"
                        .to_string(),
                    file: LintFile::Tree,
                    fixable: true,
                    fix: Some(FixCandidate::ReplaceInLine {
                        line: hit.line,
                        start_col: hit.col,
                        end_col: hit.col + "reason".len(),
                        replacement: "why_failed".to_string(),
                    }),
                });
            }
        }
        if f.ty.as_deref() == Some("decision") {
            for hit in &f.justification_hits {
                diags.push(LintDiagnostic {
                    rule: LintRuleId::DecisionRationaleAlias,
                    message: "`justification:` on a decision node is an alias; canonical key is \
                              `rationale:`"
                        .to_string(),
                    file: LintFile::Tree,
                    fixable: true,
                    fix: Some(FixCandidate::ReplaceInLine {
                        line: hit.line,
                        start_col: hit.col,
                        end_col: hit.col + "justification".len(),
                        replacement: "rationale".to_string(),
                    }),
                });
            }
        }
    }

    diags
}

/// Runs the claims-file rule (ARA004) over raw Markdown text.
#[cfg(feature = "native")]
fn lint_claims(text: &str) -> Vec<LintDiagnostic> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| claim_header_drift(line, i))
        .collect()
}

/// Detects a claim header whose id/title separator is a dash instead of a colon
/// (`## C01 — Title` / `## C01 - Title`) and returns the fix that rewrites the
/// separator to `: `. Non-claim `##` headers (id not `^C\d+$`) and canonical
/// colon headers are left untouched.
#[cfg(feature = "native")]
fn claim_header_drift(line: &str, line_idx: usize) -> Option<LintDiagnostic> {
    let ws = leading_spaces(line);
    let rest = line[ws..].strip_prefix("## ")?;
    let id_start = ws + 3; // "## " is three bytes.

    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if !is_canonical_id(&id, 'C') {
        return None;
    }
    let id_end = id_start + id.len();

    // Inspect the separator immediately after the id.
    let tail = &line[id_end..];
    let trimmed = tail.trim_start();
    let leading_ws = tail.len() - trimmed.len();
    let sep = trimmed.chars().next()?;
    // A colon is already canonical; only dash separators drift.
    if !matches!(sep, '—' | '–' | '-') {
        return None;
    }

    // The title follows the separator (and any spaces); require it be non-empty
    // so degenerate `## C01 -` lines are not "fixed" into `## C01: `.
    let after_sep = &trimmed[sep.len_utf8()..];
    let title = after_sep.trim_start();
    if title.is_empty() {
        return None;
    }
    let title_ws = after_sep.len() - title.len();
    let title_start = id_end + leading_ws + sep.len_utf8() + title_ws;

    Some(LintDiagnostic {
        rule: LintRuleId::ClaimHeaderStyle,
        message: "claim header uses a dash separator; canonical form is `## <id>: <title>`"
            .to_string(),
        file: LintFile::Claims,
        fixable: true,
        fix: Some(FixCandidate::ReplaceInLine {
            line: line_idx,
            start_col: id_end,
            end_col: title_start,
            replacement: ": ".to_string(),
        }),
    })
}

// The test suite drives the native-only scanning entry points, so it compiles
// only when the `native` feature is on (it is, by default).
#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;

    /// Extracts the single diagnostic for `rule`, asserting exactly one fired.
    fn only(diags: Vec<LintDiagnostic>, rule: LintRuleId) -> LintDiagnostic {
        let mut hits: Vec<LintDiagnostic> = diags.into_iter().filter(|d| d.rule == rule).collect();
        assert_eq!(hits.len(), 1, "expected exactly one {rule}, got {hits:?}");
        hits.pop().unwrap()
    }

    // ---- ARA001 -----------------------------------------------------------

    #[test]
    fn ara001_root_dialect_is_detected() {
        let yaml = "\
root:
  id: N01
  type: question
  title: q
";
        let diags = lint_tree(yaml);
        let d = only(diags, LintRuleId::RootDialect);
        assert!(d.fixable);
        match &d.fix {
            Some(FixCandidate::RewriteRootToTree {
                root_line,
                root_indent,
                block_end_line,
            }) => {
                assert_eq!(*root_line, 0);
                assert_eq!(*root_indent, 0);
                assert_eq!(*block_end_line, 4); // all four lines belong to the block
            }
            other => panic!("expected RewriteRootToTree, got {other:?}"),
        }
    }

    #[test]
    fn ara001_tree_dialect_not_flagged() {
        let yaml = "tree:\n  - id: N01\n    type: question\n";
        assert!(
            lint_tree(yaml)
                .iter()
                .all(|d| d.rule != LintRuleId::RootDialect)
        );
    }

    #[test]
    fn ara001_block_end_stops_at_next_top_level_key() {
        let yaml = "\
root:
  id: N01
  type: question
meta: trailing
";
        let d = only(lint_tree(yaml), LintRuleId::RootDialect);
        match &d.fix {
            Some(FixCandidate::RewriteRootToTree { block_end_line, .. }) => {
                assert_eq!(*block_end_line, 3); // stops at `meta:` on line 3
            }
            other => panic!("expected RewriteRootToTree, got {other:?}"),
        }
    }

    // ---- ARA002 -----------------------------------------------------------

    #[test]
    fn ara002_reason_on_dead_end_is_detected_and_fixable() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: it diverged
";
        let d = only(lint_tree(yaml), LintRuleId::DeadEndReasonAlias);
        assert!(d.fixable);
        assert_eq!(d.file, LintFile::Tree);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine {
                line,
                start_col,
                end_col,
                replacement,
            }) => {
                assert_eq!(*line, 3); // 0-based: the `reason:` line
                assert_eq!(*start_col, 4); // key column under a 2-space list item
                assert_eq!(*end_col, 4 + "reason".len());
                assert_eq!(replacement, "why_failed");
            }
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    #[test]
    fn ara002_type_after_reason_still_resolves() {
        // `type:` declared *after* the aliased key must still be attributed.
        let yaml = "\
tree:
  - id: N01
    reason: it diverged
    type: dead_end
";
        let d = only(lint_tree(yaml), LintRuleId::DeadEndReasonAlias);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine { line, .. }) => assert_eq!(*line, 2),
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    #[test]
    fn ara002_reason_on_non_dead_end_not_flagged() {
        let yaml = "\
tree:
  - id: N01
    type: experiment
    reason: some prose
";
        assert!(
            lint_tree(yaml)
                .iter()
                .all(|d| d.rule != LintRuleId::DeadEndReasonAlias)
        );
    }

    #[test]
    fn ara002_canonical_why_failed_not_flagged() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    why_failed: it diverged
";
        assert!(lint_tree(yaml).is_empty());
    }

    #[test]
    fn ara002_siblings_scoped_independently() {
        // A dead_end sibling's reason fires; a decision sibling's reason does not.
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: x
  - id: N02
    type: decision
    reason: y
";
        let diags = lint_tree(yaml);
        let d = only(diags, LintRuleId::DeadEndReasonAlias);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine { line, .. }) => assert_eq!(*line, 3),
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    #[test]
    fn ara002_reason_on_nested_dead_end_child_is_detected() {
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: dead_end
        reason: nested
";
        let d = only(lint_tree(yaml), LintRuleId::DeadEndReasonAlias);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine {
                line, start_col, ..
            }) => {
                assert_eq!(*line, 6);
                assert_eq!(*start_col, 8); // deeper nesting → deeper key column
            }
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    // ---- ARA003 -----------------------------------------------------------

    #[test]
    fn ara003_justification_on_decision_is_detected() {
        let yaml = "\
tree:
  - id: N01
    type: decision
    justification: cheaper
";
        let d = only(lint_tree(yaml), LintRuleId::DecisionRationaleAlias);
        assert!(d.fixable);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine {
                line,
                start_col,
                end_col,
                replacement,
            }) => {
                assert_eq!(*line, 3);
                assert_eq!(*start_col, 4);
                assert_eq!(*end_col, 4 + "justification".len());
                assert_eq!(replacement, "rationale");
            }
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    #[test]
    fn ara003_justification_on_non_decision_not_flagged() {
        let yaml = "\
tree:
  - id: N01
    type: experiment
    justification: some prose
";
        assert!(
            lint_tree(yaml)
                .iter()
                .all(|d| d.rule != LintRuleId::DecisionRationaleAlias)
        );
    }

    // ---- ARA004 -----------------------------------------------------------

    #[test]
    fn ara004_em_dash_header_is_detected() {
        let md = "## C01 — Attention is all you need";
        let d = only(lint_claims(md), LintRuleId::ClaimHeaderStyle);
        assert!(d.fixable);
        assert_eq!(d.file, LintFile::Claims);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine {
                line,
                start_col,
                end_col,
                replacement,
            }) => {
                assert_eq!(*line, 0);
                assert_eq!(*start_col, 6); // right after "## C01"
                assert_eq!(replacement, ": ");
                // Splicing the replacement yields the canonical header.
                let fixed = format!("{}{}{}", &md[..*start_col], replacement, &md[*end_col..]);
                assert_eq!(fixed, "## C01: Attention is all you need");
            }
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    #[test]
    fn ara004_hyphen_header_is_detected() {
        let md = "## C02 - Faster training";
        let d = only(lint_claims(md), LintRuleId::ClaimHeaderStyle);
        match &d.fix {
            Some(FixCandidate::ReplaceInLine {
                start_col,
                end_col,
                replacement,
                ..
            }) => {
                let fixed = format!("{}{}{}", &md[..*start_col], replacement, &md[*end_col..]);
                assert_eq!(fixed, "## C02: Faster training");
            }
            other => panic!("expected ReplaceInLine, got {other:?}"),
        }
    }

    #[test]
    fn ara004_colon_header_not_flagged() {
        assert!(lint_claims("## C01: Attention is all you need").is_empty());
    }

    #[test]
    fn ara004_non_claim_dash_header_not_flagged() {
        // The id is not `^C\d+$`, so a dash-separated section header is left alone.
        assert!(lint_claims("## Overview — background").is_empty());
    }

    #[test]
    fn ara004_hyphen_in_title_with_colon_not_flagged() {
        // The separator is a colon; a hyphen later in the title is irrelevant.
        assert!(lint_claims("## C01: Multi-head attention").is_empty());
    }

    // ---- check_dir --------------------------------------------------------

    #[test]
    fn check_dir_tolerates_missing_claims_and_does_not_panic() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CTR: AtomicUsize = AtomicUsize::new(0);

        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ara_lint_test_{}_{n}", std::process::id()));
        std::fs::create_dir_all(dir.join("trace")).unwrap();
        std::fs::write(
            dir.join("trace/exploration_tree.yaml"),
            "root:\n  id: N01\n  type: question\n",
        )
        .unwrap();
        // Deliberately no `logic/claims.md`.

        let report = check_dir(&dir);
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|d| d.rule == LintRuleId::RootDialect)
        );
        assert_eq!(report.fixable(), report.diagnostics().len());
        assert!(!report.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn check_dir_missing_tree_yields_empty_report() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CTR: AtomicUsize = AtomicUsize::new(0);

        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ara_lint_empty_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let report = check_dir(&dir);
        assert!(report.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
