//! Format-fix applier: turns the drift [`crate::lint`] detects into in-place
//! rewrites of `trace/exploration_tree.yaml` / `logic/claims.md`, but applies an
//! edit only after a per-rule **safety guard** proves it is semantically sound.
//!
//! # Design
//!
//! All edits and guard checks happen **in memory** on the file text; a file is
//! written to disk only once its edits pass the guard, so a rejected fix can
//! never leave a corrupted source file.
//!
//! The applier runs a **fixpoint loop**: detect → pick one fixable candidate →
//! apply it to a copy of the text → re-parse and guard → commit or discard →
//! re-detect on the (possibly edited) text and repeat. Running one candidate per
//! iteration and re-detecting from scratch is what gives idempotence and lets
//! later rules see the byte-offset / line shifts an earlier fix produced (e.g.
//! an ARA001 root→tree rewrite re-indents the block, shifting the columns
//! ARA002/ARA003 point at).
//!
//! # Guards
//!
//! The re-parse uses the crate's public [`parse_sources`]: the applier only ever
//! edits `exploration_tree.yaml` / `claims.md`, and the extra `parse_dir` layers
//! (logic sections, evidence) read *other* files these edits never touch, so
//! comparing `parse_sources` output is sufficient to prove the edit's effect.
//!
//! - **ARA001** (structural, semantic no-op): accept only if the manifest is
//!   *unchanged* (`mc == mb`). This is what protects the re-indent.
//! - **ARA002 / ARA003** (alias rename, value-recovering): accept only if exactly
//!   one node's target field goes `None → Some` and nothing else differs.
//! - **ARA004** (claim-header rewrite, value-recovering): accept only if exactly
//!   one claim appears (with the header's title), additively, and no node/link
//!   changes.
//!
//! When a guard is ambiguous for an edge case the applier prefers the **safe**
//! choice — discard and report the drift as detected-but-not-applied.

use std::path::Path;

use serde::Serialize;

use crate::lint::{FixCandidate, LintDiagnostic, LintFile, LintReport, LintRuleId, check_sources};
use crate::manifest::{Claim, Manifest, Node, NodeFields, is_canonical_id};
use crate::parse::parse_sources;
use crate::report::{Diagnostic, ParseReport};

/// Safety backstop on the fixpoint loop. Each iteration applies or discards
/// exactly one candidate; applies only ever *reduce* the remaining drift, so a
/// real artifact terminates far below this bound.
const MAX_ITERS: usize = 1000;

/// A fix that was applied to a source file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppliedFix {
    /// The rule whose drift was fixed.
    pub rule: LintRuleId,
    /// The file that was edited.
    pub file: LintFile,
    /// Short human-readable description of the edit.
    pub description: String,
}

/// A fixable drift that was detected but deliberately **not** applied, because
/// the safety guard rejected the edit (or it could not be rendered).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkippedFix {
    /// The rule whose drift was left in place.
    pub rule: LintRuleId,
    /// The file the drift lives in.
    pub file: LintFile,
    /// Why the fix was not applied.
    pub reason: String,
}

/// The outcome of a [`fix_dir`] pass.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixOutcome {
    /// Fixes that were applied, in application order.
    pub applied: Vec<AppliedFix>,
    /// Fixable drift detected but discarded by a guard, with the reason.
    pub skipped: Vec<SkippedFix>,
    /// Format-lint report re-run on the post-fix text (what still remains).
    ///
    /// This reflects the **in-memory** post-fix text. When `errors` is non-empty
    /// an intended write did not reach disk, so for those files the on-disk drift
    /// still stands even though `remaining` shows it resolved — callers must treat
    /// a non-empty `errors` as a failure (the CLI keys exit code 2 off it) rather
    /// than trusting `remaining`/`applied` for the un-written files.
    pub remaining: LintReport,
    /// The files that were actually rewritten on disk.
    pub changed_files: Vec<LintFile>,
    /// I/O failures while writing fixes back: `(file, error message)`. Non-empty
    /// ⇔ at least one intended write did not reach disk.
    pub errors: Vec<(LintFile, String)>,
}

impl FixOutcome {
    /// True when nothing was applied and no file changed.
    pub fn is_noop(&self) -> bool {
        self.applied.is_empty() && self.changed_files.is_empty()
    }

    /// True when an intended write failed to reach disk.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Detects fixable drift in the ARA artifact at `dir`, applies the **safe** fixes
/// to `trace/exploration_tree.yaml` / `logic/claims.md` in place, and returns a
/// [`FixOutcome`]. Native only.
///
/// Edits and guard validation run entirely in memory; a file is written only
/// after its edits pass, so a rejected fix never corrupts a source file. Running
/// `fix_dir` twice is a no-op the second time (idempotent).
pub fn fix_dir(dir: &Path) -> FixOutcome {
    let tree_path = dir.join("trace/exploration_tree.yaml");
    let claims_path = dir.join("logic/claims.md");
    let orig_tree = std::fs::read_to_string(&tree_path).unwrap_or_default();
    let orig_claims = std::fs::read_to_string(&claims_path).ok();

    let mut applier = Applier::new(orig_tree.clone(), orig_claims.clone());
    applier.run();

    // Write back only the files that actually changed. A successful write is
    // recorded in `changed_files`; a failed write is recorded in `errors` so the
    // caller never mistakes an un-written file for clean.
    let mut changed_files = Vec::new();
    let mut errors = Vec::new();
    if applier.tree != orig_tree {
        match std::fs::write(&tree_path, &applier.tree) {
            Ok(()) => changed_files.push(LintFile::Tree),
            Err(e) => errors.push((LintFile::Tree, e.to_string())),
        }
    }
    if let Some(new_claims) = &applier.claims
        && orig_claims.as_deref() != Some(new_claims.as_str())
    {
        match std::fs::write(&claims_path, new_claims) {
            Ok(()) => changed_files.push(LintFile::Claims),
            Err(e) => errors.push((LintFile::Claims, e.to_string())),
        }
    }

    // Re-detect on the final in-memory text: what remains is exactly the fixable
    // drift we chose not to apply (applied fixes are gone), so the skip list is
    // built directly from it, annotated with the reason recorded during the run.
    let remaining = check_sources(&applier.tree, applier.claims.as_deref());
    let skipped = remaining
        .diagnostics()
        .iter()
        .filter(|d| d.fixable)
        .map(|d| SkippedFix {
            rule: d.rule,
            file: d.file,
            reason: applier.reason_for(d),
        })
        .collect();

    FixOutcome {
        applied: applier.applied,
        skipped,
        remaining,
        changed_files,
        errors,
    }
}

/// The result shape [`parse_sources`] returns; aliased for the guard helpers.
type ParseResult = Result<(Manifest, ParseReport), ParseReport>;

/// Which recovering alias field a targeted guard is validating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AliasField {
    /// ARA002: `dead_end.why_failed`.
    WhyFailed,
    /// ARA003: `decision.rationale`.
    Rationale,
}

/// In-memory applier state driving the fixpoint loop.
struct Applier {
    /// Current `exploration_tree.yaml` text.
    tree: String,
    /// Current `claims.md` text, when the file exists.
    claims: Option<String>,
    /// Applied fixes, in order.
    applied: Vec<AppliedFix>,
    /// Candidates rejected in the current pass: `(rule, file, line, reason)`. Line
    /// numbers are stable across every fix kind (all edits are single-line or
    /// keep the line count), so `(rule, file, line)` uniquely keys a candidate.
    /// Cleared whenever a fix is applied, so previously-rejected candidates get
    /// re-evaluated against the new state (e.g. an ARA004 claim recovery may
    /// resolve the error that had blocked an ARA002 rename).
    failed: Vec<(LintRuleId, LintFile, usize, String)>,
}

impl Applier {
    fn new(tree: String, claims: Option<String>) -> Self {
        Self {
            tree,
            claims,
            applied: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// Runs the detect → apply/discard → re-detect fixpoint to completion.
    fn run(&mut self) {
        for _ in 0..MAX_ITERS {
            let report = check_sources(&self.tree, self.claims.as_deref());
            let Some(diag) = report
                .diagnostics()
                .iter()
                .find(|d| d.fixable && d.fix.is_some() && !self.is_failed(d))
                .cloned()
            else {
                break;
            };
            if self.step(&diag) {
                // A fix landed: text (and thus the parse baseline) changed, so
                // reconsider anything we had rejected earlier.
                self.failed.clear();
            }
        }
    }

    /// Attempts one candidate. Returns `true` iff it was applied.
    fn step(&mut self, diag: &LintDiagnostic) -> bool {
        let base = parse_sources(&self.tree, self.claims.as_deref());
        let Some((new_tree, new_claims)) = self.render_candidate(diag) else {
            self.fail(
                diag,
                "fix candidate could not be rendered onto the source text",
            );
            return false;
        };
        let cand = parse_sources(&new_tree, new_claims.as_deref());

        let accept = match diag.rule {
            LintRuleId::RootDialect => guard_ara001(&base, &cand),
            LintRuleId::DeadEndReasonAlias => guard_alias(&base, &cand, AliasField::WhyFailed),
            LintRuleId::DecisionRationaleAlias => guard_alias(&base, &cand, AliasField::Rationale),
            LintRuleId::ClaimHeaderStyle => {
                self.guard_ara004(diag, &base, &cand, new_claims.as_deref(), &new_tree)
            }
        };
        if !accept {
            self.fail(diag, guard_reason(diag.rule));
            return false;
        }

        // Idempotence backstop: the same drift must not survive at this line, or
        // the loop could re-detect and re-apply it forever.
        let recheck = check_sources(&new_tree, new_claims.as_deref());
        let line = diag_line(diag);
        if recheck
            .diagnostics()
            .iter()
            .any(|d| d.rule == diag.rule && diag_line(d) == line)
        {
            self.fail(diag, "fix did not eliminate the drift (non-idempotent)");
            return false;
        }

        self.tree = new_tree;
        self.claims = new_claims;
        self.applied.push(AppliedFix {
            rule: diag.rule,
            file: diag.file,
            description: applied_desc(diag.rule),
        });
        true
    }

    /// Renders `diag`'s fix candidate onto the current text, returning the edited
    /// `(tree, claims)` pair. `None` if the offsets don't fit the text.
    fn render_candidate(&self, diag: &LintDiagnostic) -> Option<(String, Option<String>)> {
        let fix = diag.fix.as_ref()?;
        match diag.file {
            LintFile::Tree => Some((apply_fix_to_text(&self.tree, fix)?, self.claims.clone())),
            LintFile::Claims => {
                let claims = self.claims.as_deref()?;
                Some((self.tree.clone(), Some(apply_fix_to_text(claims, fix)?)))
            }
        }
    }

    /// ARA004 targeted guard. See the module docs for the invariant it relies on:
    /// the edit only changes one `## C\d+` header separator, so the parsed claim
    /// set can only *grow* by that one claim (body boundaries are unchanged), the
    /// tree is untouched (nodes/links identical), and bindings can only gain
    /// edges to the recovered claim.
    fn guard_ara004(
        &self,
        diag: &LintDiagnostic,
        base: &ParseResult,
        cand: &ParseResult,
        new_claims: Option<&str>,
        new_tree: &str,
    ) -> bool {
        // The edited artifact must be fully valid.
        let Ok((mc, _)) = cand else {
            return false;
        };
        // No new errors: fixing a separator may only resolve a dangling reference.
        // (cand is Ok here, so this is trivially satisfied, but assert it anyway.)
        if !errors_subset(cand, base) {
            return false;
        }

        // The pre-fix claim set, isolated from the tree so a dangling reference in
        // the full base parse (the main ARA004 case) can't hide it.
        let Some(base_claims) = claims_only(self.claims.as_deref()) else {
            return false;
        };
        // The recovered id/title, read from the rewritten header line.
        let Some((rec_id, rec_title)) = header_at(new_claims, diag_line(diag)) else {
            return false;
        };

        // Genuine recovery: absent before, present after with the header's title.
        if base_claims.iter().any(|c| c.id.as_str() == rec_id) {
            return false;
        }
        let Some(rc) = mc.claims.iter().find(|c| c.id.as_str() == rec_id) else {
            return false;
        };
        if rc.title != rec_title {
            return false;
        }

        // Additive: dropping the recovered claim reproduces the base set exactly
        // (same order, nothing else changed).
        let mc_minus: Vec<Claim> = mc
            .claims
            .iter()
            .filter(|c| c.id.as_str() != rec_id)
            .cloned()
            .collect();
        if mc_minus != base_claims {
            return false;
        }

        // Nodes/links cannot change (the tree text is untouched); assert it
        // against a claims-independent parse of the same tree.
        let Ok((tb, _)) = parse_sources(new_tree, None) else {
            return false;
        };
        mc.nodes == tb.nodes && mc.links == tb.links
    }

    /// True if `diag`'s candidate was already rejected in the current pass.
    fn is_failed(&self, diag: &LintDiagnostic) -> bool {
        let key = (diag.rule, diag.file, diag_line(diag));
        self.failed.iter().any(|(r, f, l, _)| (*r, *f, *l) == key)
    }

    /// Records a rejection reason for `diag` (first reason per candidate wins).
    fn fail(&mut self, diag: &LintDiagnostic, reason: impl Into<String>) {
        if !self.is_failed(diag) {
            self.failed
                .push((diag.rule, diag.file, diag_line(diag), reason.into()));
        }
    }

    /// Looks up the recorded rejection reason for a remaining diagnostic, falling
    /// back to a generic per-rule reason.
    fn reason_for(&self, diag: &LintDiagnostic) -> String {
        let key = (diag.rule, diag.file, diag_line(diag));
        self.failed
            .iter()
            .find(|(r, f, l, _)| (*r, *f, *l) == key)
            .map(|(_, _, _, reason)| reason.clone())
            .unwrap_or_else(|| guard_reason(diag.rule))
    }
}

// ---- guards ---------------------------------------------------------------

/// ARA001 structural guard: the root→tree rewrite must be a semantic no-op.
fn guard_ara001(base: &ParseResult, cand: &ParseResult) -> bool {
    match (base, cand) {
        (Ok((mb, _)), Ok((mc, _))) => mc == mb,
        _ => false,
    }
}

/// ARA002/ARA003 targeted guard: exactly one node's target field goes
/// `None → Some`, and nothing else differs.
fn guard_alias(base: &ParseResult, cand: &ParseResult, field: AliasField) -> bool {
    let (Ok((mb, _)), Ok((mc, _))) = (base, cand) else {
        return false;
    };
    if mc.nodes.len() != mb.nodes.len() {
        return false;
    }
    if mb.nodes.iter().zip(&mc.nodes).any(|(a, b)| a.id != b.id) {
        return false;
    }

    let diffs: Vec<usize> = (0..mb.nodes.len())
        .filter(|&i| mb.nodes[i] != mc.nodes[i])
        .collect();
    if diffs.len() != 1 {
        return false;
    }
    let i = diffs[0];

    // The value must be recovered: `None` in base, `Some` in cand.
    if field_is_some(&mb.nodes[i], field) || !field_is_some(&mc.nodes[i], field) {
        return false;
    }

    // Resetting that one recovered field to `None` must reproduce base exactly —
    // proof that nothing else moved and the value landed in the right place.
    let mut mc2 = (*mc).clone();
    clear_field(&mut mc2.nodes[i], field);
    mc2 == *mb
}

/// True when `node`'s `field` is populated.
fn field_is_some(node: &Node, field: AliasField) -> bool {
    match (field, &node.fields) {
        (AliasField::WhyFailed, NodeFields::DeadEnd { why_failed, .. }) => why_failed.is_some(),
        (AliasField::Rationale, NodeFields::Decision { rationale, .. }) => rationale.is_some(),
        _ => false,
    }
}

/// Clears `node`'s `field` (no-op if the node isn't the matching kind).
fn clear_field(node: &mut Node, field: AliasField) {
    match (field, &mut node.fields) {
        (AliasField::WhyFailed, NodeFields::DeadEnd { why_failed, .. }) => *why_failed = None,
        (AliasField::Rationale, NodeFields::Decision { rationale, .. }) => *rationale = None,
        _ => {}
    }
}

/// True iff every distinct error occurs no more often in `cand` than in `base`
/// (renames/recoveries may only resolve errors, never introduce one).
fn errors_subset(cand: &ParseResult, base: &ParseResult) -> bool {
    let candidate_errors = errors_of(cand);
    let base_errors = errors_of(base);

    for (index, error) in candidate_errors.iter().enumerate() {
        if candidate_errors[..index].contains(error) {
            continue;
        }

        let candidate_count = candidate_errors[index..]
            .iter()
            .filter(|other| *other == error)
            .count();
        let base_count = base_errors
            .iter()
            .filter(|other| *other == error)
            .count();
        if candidate_count > base_count {
            return false;
        }
    }

    true
}

/// The error diagnostics of a parse result (present on both `Ok` and `Err`).
fn errors_of(result: &ParseResult) -> &[Diagnostic] {
    match result {
        Ok((_, report)) => report.errors(),
        Err(report) => report.errors(),
    }
}

/// Parses `claims` isolated from any tree (`tree: []`), returning just the claim
/// set. This yields the claims even when the real artifact's tree references a
/// not-yet-recovered claim (which would make the full parse error). `None` when
/// the claims themselves fail to parse (e.g. a claim→claim dependency error).
fn claims_only(claims: Option<&str>) -> Option<Vec<Claim>> {
    match parse_sources("tree: []\n", claims) {
        Ok((m, _)) => Some(m.claims),
        Err(_) => None,
    }
}

// ---- text edits -----------------------------------------------------------

/// Applies a single [`FixCandidate`] to `text`, returning the edited text.
fn apply_fix_to_text(text: &str, fix: &FixCandidate) -> Option<String> {
    match fix {
        FixCandidate::ReplaceInLine {
            line,
            start_col,
            end_col,
            replacement,
        } => apply_replace_in_line(text, *line, *start_col, *end_col, replacement),
        FixCandidate::RewriteRootToTree {
            root_line,
            root_indent,
            block_end_line,
        } => apply_root_to_tree(text, *root_line, *root_indent, *block_end_line),
    }
}

/// Replaces the byte range `[start, end)` on 0-based `line` with `repl`.
/// Splitting/joining on `'\n'` round-trips the exact text (including a trailing
/// newline and any `\r` from CRLF, which sits past the edited span).
fn apply_replace_in_line(
    text: &str,
    line: usize,
    start: usize,
    end: usize,
    repl: &str,
) -> Option<String> {
    let mut segs: Vec<String> = text.split('\n').map(str::to_string).collect();
    let seg = segs.get_mut(line)?;
    if start > end || end > seg.len() || !seg.is_char_boundary(start) || !seg.is_char_boundary(end)
    {
        return None;
    }
    seg.replace_range(start..end, repl);
    Some(segs.join("\n"))
}

/// Rewrites a top-level `root:` single-node map into a one-element `tree:` list:
/// rename the key, add one indent level to every block line, and turn the first
/// block content line into the list element with a `- ` marker.
fn apply_root_to_tree(
    text: &str,
    root_line: usize,
    root_indent: usize,
    block_end_line: usize,
) -> Option<String> {
    let mut segs: Vec<String> = text.split('\n').map(str::to_string).collect();
    if root_line >= segs.len() || block_end_line > segs.len() || block_end_line <= root_line {
        return None;
    }

    // 1. `root` → `tree` at the key's indent.
    {
        let seg = &mut segs[root_line];
        let end = root_indent + "root".len();
        if end > seg.len() || !seg.is_char_boundary(root_indent) || &seg[root_indent..end] != "root"
        {
            return None;
        }
        seg.replace_range(root_indent..end, "tree");
    }

    // 2. Indent the block by one level; the first content line becomes the list
    //    element (`- ` marker inserted after its existing indentation). Blank
    //    lines are left untouched so no trailing whitespace is introduced.
    let mut first_seen = false;
    for seg in segs.iter_mut().take(block_end_line).skip(root_line + 1) {
        if seg.trim().is_empty() {
            continue;
        }
        if first_seen {
            seg.insert_str(0, "  ");
        } else {
            first_seen = true;
            let ws = leading_spaces(seg);
            seg.insert_str(ws, "- ");
        }
    }

    Some(segs.join("\n"))
}

/// Counts leading ASCII spaces.
fn leading_spaces(s: &str) -> usize {
    s.len() - s.trim_start_matches(' ').len()
}

/// The claim id+title of a canonical `## C\d+: title` header at 0-based `line`,
/// mirroring the claims parser so a recovered title compares equal.
fn header_at(claims: Option<&str>, line: usize) -> Option<(String, String)> {
    let l = claims?.split('\n').nth(line)?;
    let rest = l.trim_start().strip_prefix("## ")?;
    let (raw_id, raw_title) = rest.split_once(':')?;
    let id = raw_id.trim();
    if !is_canonical_id(id, 'C') {
        return None;
    }
    let title = raw_title.trim();
    if title.is_empty() {
        return None;
    }
    Some((id.to_string(), title.to_string()))
}

/// The 0-based source line a diagnostic's fix targets (used to key candidates).
fn diag_line(diag: &LintDiagnostic) -> usize {
    match &diag.fix {
        Some(FixCandidate::ReplaceInLine { line, .. }) => *line,
        Some(FixCandidate::RewriteRootToTree { root_line, .. }) => *root_line,
        None => usize::MAX,
    }
}

/// Human-readable description of an applied fix.
fn applied_desc(rule: LintRuleId) -> String {
    match rule {
        LintRuleId::RootDialect => {
            "rewrote top-level `root:` single node into a one-element `tree:` list".to_string()
        }
        LintRuleId::DeadEndReasonAlias => {
            "renamed `reason:` to `why_failed:` on a dead_end node".to_string()
        }
        LintRuleId::DecisionRationaleAlias => {
            "renamed `justification:` to `rationale:` on a decision node".to_string()
        }
        LintRuleId::ClaimHeaderStyle => "rewrote dash claim-header separator to `: `".to_string(),
    }
}

/// Generic reason recorded when a rule's guard rejects a candidate.
fn guard_reason(rule: LintRuleId) -> String {
    match rule {
        LintRuleId::RootDialect => {
            "root→tree rewrite would change the parsed manifest; left unchanged".to_string()
        }
        LintRuleId::DeadEndReasonAlias | LintRuleId::DecisionRationaleAlias => {
            "alias rename would change more than the recovered field; left unchanged".to_string()
        }
        LintRuleId::ClaimHeaderStyle => {
            "claim-header rewrite would change more than the recovered claim; left unchanged"
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NodeId;

    /// Builds a temp ARA artifact with the given tree YAML and optional claims.
    fn artifact(tree_yaml: &str, claims_md: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("trace")).unwrap();
        std::fs::write(dir.path().join("trace/exploration_tree.yaml"), tree_yaml).unwrap();
        if let Some(claims) = claims_md {
            std::fs::create_dir_all(dir.path().join("logic")).unwrap();
            std::fs::write(dir.path().join("logic/claims.md"), claims).unwrap();
        }
        dir
    }

    fn read_tree(dir: &tempfile::TempDir) -> String {
        std::fs::read_to_string(dir.path().join("trace/exploration_tree.yaml")).unwrap()
    }

    fn read_claims(dir: &tempfile::TempDir) -> String {
        std::fs::read_to_string(dir.path().join("logic/claims.md")).unwrap()
    }

    fn parse_errors(errors: &[(&str, &str)]) -> ParseResult {
        let mut report = ParseReport::default();
        for &(path, message) in errors {
            report.error(path, message);
        }
        Err(report)
    }

    #[test]
    fn errors_subset_accepts_equal_multisets() {
        let base = parse_errors(&[
            ("nodes[N01]", "duplicate node id"),
            ("links[0].target", "unknown node N99"),
        ]);
        let cand = parse_errors(&[
            ("nodes[N01]", "duplicate node id"),
            ("links[0].target", "unknown node N99"),
        ]);

        assert!(errors_subset(&cand, &base));
    }

    #[test]
    fn errors_subset_accepts_removed_errors() {
        let base = parse_errors(&[
            ("nodes[N01]", "duplicate node id"),
            ("links[0].target", "unknown node N99"),
        ]);
        let cand = parse_errors(&[("links[0].target", "unknown node N99")]);

        assert!(errors_subset(&cand, &base));
    }

    #[test]
    fn errors_subset_rejects_same_size_with_different_identity() {
        let base = parse_errors(&[("nodes[N01]", "duplicate node id")]);
        let different_path = parse_errors(&[("nodes[N02]", "duplicate node id")]);
        let different_message = parse_errors(&[("nodes[N01]", "unknown node id")]);

        assert!(!errors_subset(&different_path, &base));
        assert!(!errors_subset(&different_message, &base));
    }

    #[test]
    fn errors_subset_rejects_duplicate_candidate_occurrence() {
        let base = parse_errors(&[("nodes[N01]", "duplicate node id")]);
        let cand = parse_errors(&[
            ("nodes[N01]", "duplicate node id"),
            ("nodes[N01]", "duplicate node id"),
        ]);

        assert!(!errors_subset(&cand, &base));
    }

    // ---- ARA001 -----------------------------------------------------------

    #[test]
    fn ara001_root_rewritten_to_tree_preserving_manifest() {
        let yaml = "\
root:
  id: N01
  type: question
  title: q
  children:
    - id: N02
      type: experiment
      result: 28.4 BLEU
";
        let before = parse_sources(yaml, None).expect("root parses").0;
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::RootDialect);
        assert_eq!(outcome.changed_files, vec![LintFile::Tree]);
        assert!(outcome.remaining.is_empty());

        let after_text = read_tree(&dir);
        assert!(after_text.starts_with("tree:\n"), "got: {after_text}");
        // Rewritten YAML is well-formed and re-parses to the same manifest.
        let after = parse_sources(&after_text, None)
            .expect("rewritten parses")
            .0;
        assert_eq!(before.nodes, after.nodes);
        assert_eq!(before.links, after.links);
        assert_eq!(before, after);
    }

    #[test]
    fn ara001_expected_reindented_text() {
        let yaml = "root:\n  id: RQ\n  type: question\n  children:\n    - id: N02\n";
        let dir = artifact(yaml, None);
        fix_dir(dir.path());
        assert_eq!(
            read_tree(&dir),
            "tree:\n  - id: RQ\n    type: question\n    children:\n      - id: N02\n"
        );
    }

    #[test]
    fn ara001_guard_discards_when_manifest_would_differ() {
        // Directly exercise the load-bearing guard: two DIFFERENT valid manifests
        // must be rejected, an identical one accepted.
        let base = parse_sources("tree:\n  - id: N01\n    type: question\n", None);
        let different = parse_sources("tree:\n  - id: N99\n    type: question\n", None);
        let same = parse_sources("tree:\n  - id: N01\n    type: question\n", None);
        assert!(!guard_ara001(&base, &different));
        assert!(guard_ara001(&base, &same));
    }

    // ---- ARA002 / ARA003 --------------------------------------------------

    #[test]
    fn ara002_reason_recovered_as_why_failed() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: it diverged
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::DeadEndReasonAlias);
        assert!(read_tree(&dir).contains("why_failed: it diverged"));

        let (m, _) = parse_sources(&read_tree(&dir), None).expect("ok");
        match &m.nodes[0].fields {
            NodeFields::DeadEnd { why_failed, .. } => {
                assert_eq!(why_failed.as_deref(), Some("it diverged"));
            }
            other => panic!("expected DeadEnd fields, got {other:?}"),
        }
    }

    #[test]
    fn ara003_justification_recovered_as_rationale() {
        let yaml = "\
tree:
  - id: N01
    type: decision
    justification: cheaper to train
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::DecisionRationaleAlias);

        let (m, _) = parse_sources(&read_tree(&dir), None).expect("ok");
        match &m.nodes[0].fields {
            NodeFields::Decision { rationale, .. } => {
                assert_eq!(rationale.as_deref(), Some("cheaper to train"));
            }
            other => panic!("expected Decision fields, got {other:?}"),
        }
    }

    #[test]
    fn alias_guard_discards_multi_node_change() {
        // Two nodes' fields change → not "exactly one recovered field" → discard.
        let base = parse_sources(
            "tree:\n  - id: N01\n    type: dead_end\n  - id: N02\n    type: dead_end\n",
            None,
        );
        let cand = parse_sources(
            "tree:\n  - id: N01\n    type: dead_end\n    why_failed: a\n  - id: N02\n    type: dead_end\n    why_failed: b\n",
            None,
        );
        assert!(!guard_alias(&base, &cand, AliasField::WhyFailed));

        // A single recovered field is accepted.
        let base1 = parse_sources("tree:\n  - id: N01\n    type: dead_end\n", None);
        let cand1 = parse_sources(
            "tree:\n  - id: N01\n    type: dead_end\n    why_failed: a\n",
            None,
        );
        assert!(guard_alias(&base1, &cand1, AliasField::WhyFailed));
    }

    // ---- ARA004 -----------------------------------------------------------

    #[test]
    fn ara004_dash_header_recovers_claim() {
        // Standalone claim (not referenced) that silently disappears today.
        let yaml = "tree:\n  - id: N01\n    type: question\n";
        let claims = "## C01 — Attention is all you need\n- **Statement**: yes\n";
        let dir = artifact(yaml, Some(claims));

        let before = parse_sources(yaml, Some(claims)).expect("ok").0;
        assert!(before.claims.is_empty(), "dash header must not parse today");

        let outcome = fix_dir(dir.path());
        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::ClaimHeaderStyle);
        assert_eq!(outcome.changed_files, vec![LintFile::Claims]);

        let after_claims = read_claims(&dir);
        assert!(after_claims.starts_with("## C01: Attention is all you need\n"));
        let (m, _) = parse_sources(&read_tree(&dir), Some(&after_claims)).expect("ok");
        assert_eq!(m.claims.len(), 1);
        assert_eq!(m.claims[0].id, crate::manifest::ClaimId::new("C01"));
        assert_eq!(m.claims[0].title, "Attention is all you need");
    }

    #[test]
    fn ara004_recovers_referenced_claim_and_resolves_dangling_error() {
        // A node references C01 whose header is dash-separated → base parse errors
        // (dangling reference). The fix recovers the claim and the binding.
        let yaml = "\
tree:
  - id: N01
    type: experiment
    evidence: [C01]
";
        let claims = "## C01 - Faster training\n- **Statement**: yes\n";
        let dir = artifact(yaml, Some(claims));

        assert!(
            parse_sources(yaml, Some(claims)).is_err(),
            "dangling C01 must error before the fix"
        );

        let outcome = fix_dir(dir.path());
        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::ClaimHeaderStyle);

        let (m, report) =
            parse_sources(&read_tree(&dir), Some(&read_claims(&dir))).expect("ok now");
        assert!(report.is_ok());
        assert_eq!(m.claims.len(), 1);
        assert_eq!(m.bindings.len(), 1);
        assert_eq!(m.bindings[0].claim, crate::manifest::ClaimId::new("C01"));
    }

    // ---- idempotence / safety --------------------------------------------

    #[test]
    fn fix_dir_is_idempotent() {
        let yaml = "\
root:
  id: N01
  type: question
  children:
    - id: N02
      type: dead_end
      reason: diverged
    - id: N03
      type: decision
      justification: cheaper
";
        let claims = "## C01 — A claim\n- **Statement**: yes\n";
        let dir = artifact(yaml, Some(claims));

        let first = fix_dir(dir.path());
        assert!(!first.applied.is_empty());
        let tree_after_first = read_tree(&dir);
        let claims_after_first = read_claims(&dir);

        let second = fix_dir(dir.path());
        assert!(
            second.applied.is_empty(),
            "second run must apply nothing, got: {:?}",
            second.applied
        );
        assert!(second.changed_files.is_empty());
        assert_eq!(
            read_tree(&dir),
            tree_after_first,
            "tree must be byte-identical"
        );
        assert_eq!(
            read_claims(&dir),
            claims_after_first,
            "claims must be byte-identical"
        );
    }

    #[test]
    fn discarded_fix_leaves_file_unchanged_and_parseable() {
        // A duplicate node id makes the base parse error, so the ARA002 alias
        // guard (which requires a clean baseline) discards the rename. The file
        // must be left byte-identical — no partial/corrupt write.
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: x
  - id: N01
    type: insight
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert!(outcome.applied.is_empty());
        assert!(outcome.changed_files.is_empty());
        assert!(
            outcome
                .skipped
                .iter()
                .any(|s| s.rule == LintRuleId::DeadEndReasonAlias)
        );
        assert_eq!(read_tree(&dir), yaml, "file must be untouched");
        // Still valid text (the pre-existing duplicate-id error is unrelated).
        assert_eq!(read_tree(&dir).lines().count(), yaml.lines().count());
    }

    #[test]
    fn happy_path_reports_no_write_errors() {
        let yaml = "root:\n  id: N01\n  type: question\n";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());
        assert!(!outcome.applied.is_empty());
        assert!(
            outcome.errors.is_empty(),
            "clean write must record no errors"
        );
        assert!(!outcome.has_errors());
    }

    #[cfg(unix)]
    #[test]
    fn write_failure_is_surfaced_in_errors() {
        use std::os::unix::fs::PermissionsExt;

        let yaml = "root:\n  id: N01\n  type: question\n";
        let dir = artifact(yaml, None);
        let tree_path = dir.path().join("trace/exploration_tree.yaml");

        // Make the tree file read-only so the write-back fails (non-root).
        let mut perms = std::fs::metadata(&tree_path).unwrap().permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&tree_path, perms).unwrap();

        // Probe whether we can still write despite the read-only bit (i.e. running
        // as root, where the permission is bypassed); skip the assertion if so.
        if std::fs::OpenOptions::new()
            .write(true)
            .open(&tree_path)
            .is_ok()
        {
            eprintln!("skipping: write not denied (likely running as root)");
            return;
        }

        let outcome = fix_dir(dir.path());

        assert!(outcome.has_errors());
        assert!(
            outcome.errors.iter().any(|(f, _)| *f == LintFile::Tree),
            "tree write failure must be surfaced, got: {:?}",
            outcome.errors
        );
        // The write failed, so the file must NOT be marked changed and the drift
        // is still on disk (no false "clean").
        assert!(!outcome.changed_files.contains(&LintFile::Tree));
        assert_eq!(read_tree(&dir), yaml, "on-disk file must be untouched");

        // Restore write permission so TempDir cleanup succeeds.
        let mut perms = std::fs::metadata(&tree_path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&tree_path, perms).unwrap();
    }

    #[test]
    fn clean_artifact_is_a_noop() {
        let yaml = "tree:\n  - id: N01\n    type: question\n";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());
        assert!(outcome.is_noop());
        assert!(outcome.applied.is_empty());
        assert!(outcome.skipped.is_empty());
        assert_eq!(read_tree(&dir), yaml);
    }

    #[test]
    fn combined_ara001_and_alias_fixes_both_apply() {
        // ARA001 re-indent shifts the `reason:` column; the fixpoint re-detects
        // ARA002 on the rewritten text and still fixes it.
        let yaml = "\
root:
  id: N01
  type: question
  children:
    - id: N02
      type: dead_end
      reason: diverged
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        let rules: Vec<LintRuleId> = outcome.applied.iter().map(|a| a.rule).collect();
        assert!(rules.contains(&LintRuleId::RootDialect));
        assert!(rules.contains(&LintRuleId::DeadEndReasonAlias));
        assert!(outcome.remaining.is_empty());

        let (m, _) = parse_sources(&read_tree(&dir), None).expect("ok");
        assert_eq!(m.nodes[0].id, NodeId::new("N01"));
        match &m.nodes[1].fields {
            NodeFields::DeadEnd { why_failed, .. } => {
                assert_eq!(why_failed.as_deref(), Some("diverged"));
            }
            other => panic!("expected DeadEnd, got {other:?}"),
        }
    }
}
