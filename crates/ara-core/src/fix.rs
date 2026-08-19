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
//! Guards re-parse the base and candidate with [`parse_sources_detailed`], which
//! retains normalized manifests even when semantic errors remain. Fatal syntax
//! or top-level-shape outcomes are always rejected. The common invariant for
//! value recovery is: the candidate introduces no new error occurrences, and
//! the manifest delta is exactly the recovery targeted by the rule.
//!
//! - **ARA001** (structural, semantic no-op): accept only clean normalized
//!   outcomes with an *unchanged* manifest (`mc == mb`). This protects the
//!   re-indent and deliberately remains stricter than the recovering guards.
//! - **ARA002 / ARA003** (alias rename, value-recovering): after diagnostic
//!   containment, accept only if one node's target field goes `None → Some` and
//!   clearing that field reproduces the base manifest exactly.
//! - **ARA004** (claim-header rewrite, value-recovering): after diagnostic
//!   containment, accept only if one header-matching claim appears, nodes and
//!   links remain identical, and removing that claim plus its bindings
//!   reproduces the base claim and binding sets exactly.
//!
//! When a guard is ambiguous for an edge case the applier prefers the **safe**
//! choice — discard and report the drift as detected-but-not-applied.

use std::path::Path;

use serde::Serialize;

use crate::lint::{FixCandidate, LintDiagnostic, LintFile, LintReport, LintRuleId, check_sources};
use crate::manifest::{Node, NodeFields, is_canonical_id};
use crate::parse::{ParseOutcome, parse_sources_detailed};
use crate::report::Diagnostic;

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

/// Which recovering alias field a targeted guard is validating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AliasField {
    /// ARA002: `dead_end.why_failed`.
    WhyFailed,
    /// ARA003: `decision.rationale`.
    Rationale,
    /// ARA005: `pivot.prior_direction` (alias `from:`).
    PriorDirection,
    /// ARA006: `pivot.new_direction` (alias `to:`).
    NewDirection,
    /// ARA007: `pivot.reason` (alias `trigger:`).
    PivotReason,
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
        let base = parse_sources_detailed(&self.tree, self.claims.as_deref());
        let Some((new_tree, new_claims)) = self.render_candidate(diag) else {
            self.fail(
                diag,
                "fix candidate could not be rendered onto the source text",
            );
            return false;
        };
        let cand = parse_sources_detailed(&new_tree, new_claims.as_deref());

        let accept = match diag.rule {
            LintRuleId::RootDialect => guard_ara001(&base, &cand),
            LintRuleId::DeadEndReasonAlias => guard_alias(&base, &cand, AliasField::WhyFailed),
            LintRuleId::DecisionRationaleAlias => guard_alias(&base, &cand, AliasField::Rationale),
            LintRuleId::PivotFromAlias => guard_alias(&base, &cand, AliasField::PriorDirection),
            LintRuleId::PivotToAlias => guard_alias(&base, &cand, AliasField::NewDirection),
            LintRuleId::PivotTriggerAlias => guard_alias(&base, &cand, AliasField::PivotReason),
            LintRuleId::ClaimHeaderStyle => {
                self.guard_ara004(diag, &base, &cand, new_claims.as_deref())
            }
        };
        if !accept {
            self.fail(diag, guard_rejection_reason(diag.rule, &base, &cand));
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

    /// ARA004 targeted guard: no new error occurrence may appear, and removing
    /// the one recovered header-matching claim plus every binding to it must
    /// reproduce the base claims and bindings exactly. Nodes and links cannot
    /// change.
    fn guard_ara004(
        &self,
        diag: &LintDiagnostic,
        base: &ParseOutcome,
        cand: &ParseOutcome,
        new_claims: Option<&str>,
    ) -> bool {
        let (ParseOutcome::Normalized(mb, _), ParseOutcome::Normalized(mc, _)) = (base, cand)
        else {
            return false;
        };
        if !errors_subset(cand, base) {
            return false;
        }

        let Some((rec_id, rec_title)) = header_at(new_claims, diag_line(diag)) else {
            return false;
        };

        // Genuine targeted recovery: the header id was absent before and occurs
        // after the edit with exactly the title rendered on that header.
        if mb.claims.iter().any(|claim| claim.id.as_str() == rec_id) {
            return false;
        }
        let Some(recovered_index) = mc
            .claims
            .iter()
            .position(|claim| claim.id.as_str() == rec_id)
        else {
            return false;
        };
        if mc.claims[recovered_index].title != rec_title {
            return false;
        }

        let mut claims_without_recovered = mc.claims.clone();
        claims_without_recovered.remove(recovered_index);
        if claims_without_recovered != mb.claims {
            return false;
        }
        if mc.nodes != mb.nodes || mc.links != mb.links {
            return false;
        }

        let mut bindings_without_recovered = mc.bindings.clone();
        bindings_without_recovered.retain(|binding| binding.claim.as_str() != rec_id);
        bindings_without_recovered == mb.bindings
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

/// ARA001 structural guard: both parses must be clean normalized outcomes and
/// the root→tree rewrite must be a semantic no-op.
fn guard_ara001(base: &ParseOutcome, cand: &ParseOutcome) -> bool {
    match (base, cand) {
        (ParseOutcome::Normalized(mb, rb), ParseOutcome::Normalized(mc, rc)) => {
            rb.is_ok() && rc.is_ok() && mc == mb
        }
        _ => false,
    }
}

/// ARA002/ARA003/ARA005–ARA007 targeted guard: after proving no new error
/// occurrence appears, exactly one node's target field goes `None → Some`, and
/// nothing else differs.
fn guard_alias(base: &ParseOutcome, cand: &ParseOutcome, field: AliasField) -> bool {
    let (ParseOutcome::Normalized(mb, _), ParseOutcome::Normalized(mc, _)) = (base, cand) else {
        return false;
    };
    if !errors_subset(cand, base) {
        return false;
    }
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
    let mut mc2 = mc.clone();
    clear_field(&mut mc2.nodes[i], field);
    mc2 == *mb
}

/// True when `node`'s `field` is populated.
fn field_is_some(node: &Node, field: AliasField) -> bool {
    match (field, &node.fields) {
        (AliasField::WhyFailed, NodeFields::DeadEnd { why_failed, .. }) => why_failed.is_some(),
        (AliasField::Rationale, NodeFields::Decision { rationale, .. }) => rationale.is_some(),
        (AliasField::PriorDirection, NodeFields::Pivot { prior_direction, .. }) => {
            prior_direction.is_some()
        }
        (AliasField::NewDirection, NodeFields::Pivot { new_direction, .. }) => {
            new_direction.is_some()
        }
        (AliasField::PivotReason, NodeFields::Pivot { reason, .. }) => reason.is_some(),
        _ => false,
    }
}

/// Clears `node`'s `field` (no-op if the node isn't the matching kind).
fn clear_field(node: &mut Node, field: AliasField) {
    match (field, &mut node.fields) {
        (AliasField::WhyFailed, NodeFields::DeadEnd { why_failed, .. }) => *why_failed = None,
        (AliasField::Rationale, NodeFields::Decision { rationale, .. }) => *rationale = None,
        (AliasField::PriorDirection, NodeFields::Pivot { prior_direction, .. }) => {
            *prior_direction = None
        }
        (AliasField::NewDirection, NodeFields::Pivot { new_direction, .. }) => {
            *new_direction = None
        }
        (AliasField::PivotReason, NodeFields::Pivot { reason, .. }) => *reason = None,
        _ => {}
    }
}

/// True iff every distinct error occurs no more often in `cand` than in `base`.
/// Each repeated candidate occurrence must have a matching base occurrence.
fn errors_subset(cand: &ParseOutcome, base: &ParseOutcome) -> bool {
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
        let base_count = base_errors.iter().filter(|other| *other == error).count();
        if candidate_count > base_count {
            return false;
        }
    }

    true
}

/// Error diagnostics retained by either a normalized or fatal parse outcome.
fn errors_of(result: &ParseOutcome) -> &[Diagnostic] {
    match result {
        ParseOutcome::Normalized(_, report) | ParseOutcome::Fatal(report) => report.errors(),
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
        LintRuleId::PivotFromAlias => {
            "renamed `from:` to `prior_direction:` on a pivot node".to_string()
        }
        LintRuleId::PivotToAlias => {
            "renamed `to:` to `new_direction:` on a pivot node".to_string()
        }
        LintRuleId::PivotTriggerAlias => {
            "renamed `trigger:` to `reason:` on a pivot node".to_string()
        }
    }
}

/// Specific reason for a guard rejection when the candidate regresses parse
/// errors; otherwise the rule's semantic-delta reason.
fn guard_rejection_reason(rule: LintRuleId, base: &ParseOutcome, cand: &ParseOutcome) -> String {
    let normalized = matches!(
        (base, cand),
        (
            ParseOutcome::Normalized(_, _),
            ParseOutcome::Normalized(_, _)
        )
    );
    if normalized && !errors_subset(cand, base) {
        return match rule {
            LintRuleId::DeadEndReasonAlias
            | LintRuleId::DecisionRationaleAlias
            | LintRuleId::PivotFromAlias
            | LintRuleId::PivotToAlias
            | LintRuleId::PivotTriggerAlias => {
                "alias rename would introduce a new parse error occurrence; left unchanged"
                    .to_string()
            }
            LintRuleId::ClaimHeaderStyle => {
                "claim-header rewrite would introduce a new parse error occurrence; left unchanged"
                    .to_string()
            }
            LintRuleId::RootDialect => guard_reason(rule),
        };
    }

    guard_reason(rule)
}

/// Generic reason recorded when a rule's guard rejects a candidate.
fn guard_reason(rule: LintRuleId) -> String {
    match rule {
        LintRuleId::RootDialect => {
            "root→tree rewrite would change the parsed manifest; left unchanged".to_string()
        }
        LintRuleId::DeadEndReasonAlias
        | LintRuleId::DecisionRationaleAlias
        | LintRuleId::PivotFromAlias
        | LintRuleId::PivotToAlias
        | LintRuleId::PivotTriggerAlias => {
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
    use crate::parse::parse_sources;
    use crate::report::ParseReport;

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

    fn parse_errors(errors: &[(&str, &str)]) -> ParseOutcome {
        let mut report = ParseReport::default();
        for &(path, message) in errors {
            report.error(path, message);
        }
        ParseOutcome::Fatal(report)
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
        let base = parse_sources_detailed("tree:\n  - id: N01\n    type: question\n", None);
        let different = parse_sources_detailed("tree:\n  - id: N99\n    type: question\n", None);
        let same = parse_sources_detailed("tree:\n  - id: N01\n    type: question\n", None);
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
    fn alias_fixes_apply_with_unrelated_duplicate_id_error() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: diverged
  - id: N02
    type: decision
    justification: cheaper
  - id: N03
    type: question
  - id: N03
    type: insight
";
        let before = parse_sources(yaml, None).expect_err("duplicate id must remain an error");
        let before_errors = before.errors().to_vec();
        let dir = artifact(yaml, None);

        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 2);
        assert_eq!(
            outcome
                .applied
                .iter()
                .map(|fix| fix.rule)
                .collect::<Vec<_>>(),
            vec![
                LintRuleId::DeadEndReasonAlias,
                LintRuleId::DecisionRationaleAlias,
            ]
        );
        assert_eq!(outcome.changed_files, vec![LintFile::Tree]);
        let fixed = read_tree(&dir);
        assert!(fixed.contains("why_failed: diverged"));
        assert!(fixed.contains("rationale: cheaper"));
        assert!(!fixed.contains("\n    reason:"));
        assert!(!fixed.contains("\n    justification:"));

        let after = parse_sources(&fixed, None).expect_err("duplicate id must remain an error");
        assert_eq!(after.errors(), before_errors);
    }

    #[test]
    fn alias_fix_applies_on_kept_duplicate_id_occurrence() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: diverged
  - id: N01
    type: question
";
        let before = parse_sources(yaml, None).expect_err("duplicate id must remain an error");
        let before_errors = before.errors().to_vec();
        let dir = artifact(yaml, None);

        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::DeadEndReasonAlias);
        assert_eq!(outcome.changed_files, vec![LintFile::Tree]);
        let fixed = read_tree(&dir);
        assert!(fixed.contains("why_failed: diverged"));
        assert!(!fixed.contains("\n    reason:"));
        let after = parse_sources(&fixed, None).expect_err("duplicate id must remain an error");
        assert_eq!(after.errors(), before_errors);
    }

    #[test]
    fn alias_fix_rejects_dropped_duplicate_id_occurrence_byte_identically() {
        let yaml = "\
tree:
  - id: N01
    type: question
  - id: N01
    type: dead_end
    reason: diverged
";
        assert!(
            parse_sources(yaml, None).is_err(),
            "duplicate id must remain an error"
        );
        let dir = artifact(yaml, None);

        let outcome = fix_dir(dir.path());

        assert!(outcome.applied.is_empty());
        assert!(outcome.changed_files.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].rule, LintRuleId::DeadEndReasonAlias);
        assert_eq!(read_tree(&dir), yaml);
    }

    #[test]
    fn alias_guard_discards_multi_node_change() {
        // Two nodes' fields change → not "exactly one recovered field" → discard.
        let base = parse_sources_detailed(
            "tree:\n  - id: N01\n    type: dead_end\n  - id: N02\n    type: dead_end\n",
            None,
        );
        let cand = parse_sources_detailed(
            "tree:\n  - id: N01\n    type: dead_end\n    why_failed: a\n  - id: N02\n    type: dead_end\n    why_failed: b\n",
            None,
        );
        assert!(!guard_alias(&base, &cand, AliasField::WhyFailed));

        // A single recovered field is accepted.
        let base1 = parse_sources_detailed("tree:\n  - id: N01\n    type: dead_end\n", None);
        let cand1 = parse_sources_detailed(
            "tree:\n  - id: N01\n    type: dead_end\n    why_failed: a\n",
            None,
        );
        assert!(guard_alias(&base1, &cand1, AliasField::WhyFailed));
    }

    // ---- ARA005 / ARA006 / ARA007 (pivot aliases) -------------------------

    #[test]
    fn ara005_from_recovered_as_prior_direction() {
        let yaml = "\
tree:
  - id: N01
    type: pivot
    from: dense retrieval
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::PivotFromAlias);
        assert!(outcome.skipped.is_empty());
        assert!(read_tree(&dir).contains("prior_direction: dense retrieval"));

        let (m, report) = parse_sources(&read_tree(&dir), None).expect("ok");
        assert!(
            report.warnings().is_empty(),
            "unknown-field warning must be gone, got: {report}"
        );
        match &m.nodes[0].fields {
            NodeFields::Pivot { prior_direction, .. } => {
                assert_eq!(prior_direction.as_deref(), Some("dense retrieval"));
            }
            other => panic!("expected Pivot fields, got {other:?}"),
        }
    }

    #[test]
    fn ara006_to_recovered_as_new_direction() {
        let yaml = "\
tree:
  - id: N01
    type: pivot
    to: sparse retrieval
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::PivotToAlias);
        assert!(read_tree(&dir).contains("new_direction: sparse retrieval"));

        let (m, report) = parse_sources(&read_tree(&dir), None).expect("ok");
        assert!(
            report.warnings().is_empty(),
            "unknown-field warning must be gone, got: {report}"
        );
        match &m.nodes[0].fields {
            NodeFields::Pivot { new_direction, .. } => {
                assert_eq!(new_direction.as_deref(), Some("sparse retrieval"));
            }
            other => panic!("expected Pivot fields, got {other:?}"),
        }
    }

    #[test]
    fn ara007_trigger_recovered_as_reason() {
        let yaml = "\
tree:
  - id: N01
    type: pivot
    trigger: latency budget
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].rule, LintRuleId::PivotTriggerAlias);
        assert!(read_tree(&dir).contains("reason: latency budget"));

        let (m, report) = parse_sources(&read_tree(&dir), None).expect("ok");
        assert!(
            report.warnings().is_empty(),
            "unknown-field warning must be gone, got: {report}"
        );
        match &m.nodes[0].fields {
            NodeFields::Pivot { reason, .. } => {
                assert_eq!(reason.as_deref(), Some("latency budget"));
            }
            other => panic!("expected Pivot fields, got {other:?}"),
        }
    }

    #[test]
    fn pivot_alias_rejects_when_canonical_and_alias_coexist_byte_identically() {
        // A node carrying BOTH the alias and its canonical key is auto-rejected:
        // the rename would duplicate the canonical key, so the guard records a
        // SkippedFix with a precise reason and never writes.
        let cases = [
            (
                "\
tree:
  - id: N01
    type: pivot
    prior_direction: canonical
    from: alias
",
                LintRuleId::PivotFromAlias,
            ),
            (
                "\
tree:
  - id: N01
    type: pivot
    new_direction: canonical
    to: alias
",
                LintRuleId::PivotToAlias,
            ),
            (
                "\
tree:
  - id: N01
    type: pivot
    reason: canonical
    trigger: alias
",
                LintRuleId::PivotTriggerAlias,
            ),
        ];

        for (yaml, rule) in cases {
            let dir = artifact(yaml, None);
            let outcome = fix_dir(dir.path());

            assert!(outcome.applied.is_empty(), "{rule:?}");
            assert!(outcome.changed_files.is_empty(), "{rule:?}");
            let skipped = outcome
                .skipped
                .iter()
                .find(|skipped| skipped.rule == rule)
                .unwrap_or_else(|| panic!("{rule:?}: {:?}", outcome.skipped));
            assert_eq!(
                skipped.reason,
                "alias rename would change more than the recovered field; left unchanged",
                "{rule:?}"
            );
            assert_eq!(read_tree(&dir), yaml, "{rule:?}");
        }
    }

    #[test]
    fn pivot_multi_alias_fixpoint_recovers_all_fields() {
        // from+to+trigger on one pivot all rename in a single fix run.
        let yaml = "\
tree:
  - id: N01
    type: pivot
    from: dense retrieval
    to: sparse retrieval
    trigger: latency budget
";
        let dir = artifact(yaml, None);
        let outcome = fix_dir(dir.path());

        assert_eq!(outcome.applied.len(), 3);
        let rules: Vec<LintRuleId> = outcome.applied.iter().map(|fix| fix.rule).collect();
        for rule in [
            LintRuleId::PivotFromAlias,
            LintRuleId::PivotToAlias,
            LintRuleId::PivotTriggerAlias,
        ] {
            assert!(rules.contains(&rule), "missing {rule}, got: {rules:?}");
        }
        assert!(outcome.skipped.is_empty());

        let fixed = read_tree(&dir);
        assert!(fixed.contains("prior_direction: dense retrieval"));
        assert!(fixed.contains("new_direction: sparse retrieval"));
        assert!(fixed.contains("reason: latency budget"));
        assert!(!fixed.contains("\n    from:"));
        assert!(!fixed.contains("\n    to:"));
        assert!(!fixed.contains("\n    trigger:"));

        let (m, report) = parse_sources(&fixed, None).expect("ok");
        assert!(report.warnings().is_empty(), "got: {report}");
        assert_eq!(
            m.nodes[0].fields,
            NodeFields::Pivot {
                prior_direction: Some("dense retrieval".to_string()),
                new_direction: Some("sparse retrieval".to_string()),
                reason: Some("latency budget".to_string()),
                lesson: None,
            }
        );
    }

    #[test]
    fn pivot_alias_fix_second_run_is_noop() {
        let yaml = "\
tree:
  - id: N01
    type: pivot
    from: dense retrieval
    to: sparse retrieval
    trigger: latency budget
";
        let dir = artifact(yaml, None);

        let first = fix_dir(dir.path());
        assert_eq!(first.applied.len(), 3);
        let tree_after_first = read_tree(&dir);

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
    }

    #[test]
    fn pivot_alias_guard_validates_single_recovery() {
        // A single recovered pivot field is accepted...
        let base = parse_sources_detailed("tree:\n  - id: N01\n    type: pivot\n", None);
        let cand = parse_sources_detailed(
            "tree:\n  - id: N01\n    type: pivot\n    prior_direction: a\n",
            None,
        );
        assert!(guard_alias(&base, &cand, AliasField::PriorDirection));

        // ...but not when the field was already populated in base...
        let cand2 = parse_sources_detailed(
            "tree:\n  - id: N01\n    type: pivot\n    prior_direction: b\n",
            None,
        );
        assert!(!guard_alias(&cand, &cand2, AliasField::PriorDirection));

        // ...and a recovery lands in the right field only.
        assert!(!guard_alias(&base, &cand, AliasField::NewDirection));
        assert!(!guard_alias(&base, &cand, AliasField::PivotReason));
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

    #[test]
    fn mixed_recovering_rules_apply_with_persisting_duplicate_id_error() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: diverged
    evidence: [C01]
  - id: N02
    type: question
  - id: N02
    type: insight
";
        let claims = "## C01 — Recovered claim\n- **Statement**: supported\n";
        let before =
            parse_sources(yaml, Some(claims)).expect_err("both semantic errors must be present");
        let duplicate_errors = before
            .errors()
            .iter()
            .filter(|error| error.message == "duplicate node id")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(duplicate_errors.len(), 1);
        assert!(
            before
                .errors()
                .iter()
                .any(|error| error.message == "evidence references unknown claim `C01`")
        );
        let dir = artifact(yaml, Some(claims));

        let outcome = fix_dir(dir.path());

        assert_eq!(
            outcome
                .applied
                .iter()
                .map(|fix| fix.rule)
                .collect::<Vec<_>>(),
            vec![LintRuleId::DeadEndReasonAlias, LintRuleId::ClaimHeaderStyle,]
        );
        assert_eq!(
            outcome.changed_files,
            vec![LintFile::Tree, LintFile::Claims]
        );
        assert!(outcome.skipped.is_empty());
        assert!(outcome.remaining.is_empty());
        let fixed_tree = read_tree(&dir);
        let fixed_claims = read_claims(&dir);
        assert!(fixed_tree.contains("why_failed: diverged"));
        assert!(fixed_claims.starts_with("## C01: Recovered claim\n"));
        let after = parse_sources(&fixed_tree, Some(&fixed_claims))
            .expect_err("duplicate id must remain an error");
        assert_eq!(after.errors(), duplicate_errors);
    }

    #[test]
    fn ara004_guard_requires_exact_recovered_binding_delta() {
        let yaml = "\
tree:
  - id: N01
    type: experiment
    evidence: [C01, C02]
";
        let claims = "\
## C01 — Recovered
- **Statement**: one

## C02: Existing
- **Statement**: two
";
        let lint = check_sources(yaml, Some(claims));
        let diag = lint
            .diagnostics()
            .iter()
            .find(|diag| diag.rule == LintRuleId::ClaimHeaderStyle)
            .expect("ARA004 candidate");
        let fixed_claims = apply_fix_to_text(claims, diag.fix.as_ref().unwrap()).unwrap();
        let base = parse_sources_detailed(yaml, Some(claims));
        let candidate = parse_sources_detailed(yaml, Some(&fixed_claims));
        let applier = Applier::new(yaml.to_string(), Some(claims.to_string()));

        assert!(applier.guard_ara004(diag, &base, &candidate, Some(&fixed_claims)));
        let (
            ParseOutcome::Normalized(base_manifest, _),
            ParseOutcome::Normalized(candidate_manifest, _),
        ) = (&base, &candidate)
        else {
            panic!("both artifacts must normalize");
        };
        let mut bindings_without_recovered = candidate_manifest.bindings.clone();
        bindings_without_recovered.retain(|binding| binding.claim.as_str() != "C01");
        assert_eq!(bindings_without_recovered, base_manifest.bindings);

        let mut perturbed_manifest = candidate_manifest.clone();
        perturbed_manifest
            .bindings
            .push(base_manifest.bindings[0].clone());
        let perturbed = match candidate {
            ParseOutcome::Normalized(_, report) => {
                ParseOutcome::Normalized(perturbed_manifest, report)
            }
            ParseOutcome::Fatal(_) => unreachable!(),
        };
        assert!(!applier.guard_ara004(diag, &base, &perturbed, Some(&fixed_claims)));
    }

    #[test]
    fn speedrun_claim_headers_fix_on_erroring_artifact() {
        let yaml = include_str!(
            "../tests/fixtures/corpus/speedrun/nanogpt-speedrun/trace/exploration_tree.yaml"
        );
        let claims =
            include_str!("../tests/fixtures/corpus/speedrun/nanogpt-speedrun/logic/claims.md");
        let pre_fix =
            parse_sources(yaml, Some(claims)).expect_err("referenced claims must be absent");
        assert!(
            pre_fix
                .errors()
                .iter()
                .any(|error| error.message.contains("evidence references unknown claim")),
            "expected absent referenced-claim errors, got: {pre_fix}"
        );
        let ParseOutcome::Normalized(base, _) = parse_sources_detailed(yaml, Some(claims)) else {
            panic!("speedrun fixture must normalize despite semantic errors");
        };
        assert!(base.claims.is_empty());
        let dir = artifact(yaml, Some(claims));

        let first = fix_dir(dir.path());

        // 10 claim-header rewrites plus 2 pivot `trigger:`→`reason:` recoveries
        // (ARA007): the fixture's two pivot nodes carry the pre-canonical alias.
        assert_eq!(first.applied.len(), 12);
        assert_eq!(
            first
                .applied
                .iter()
                .filter(|fix| fix.rule == LintRuleId::ClaimHeaderStyle)
                .count(),
            10
        );
        assert_eq!(
            first
                .applied
                .iter()
                .filter(|fix| fix.rule == LintRuleId::PivotTriggerAlias)
                .count(),
            2
        );
        assert_eq!(
            first.changed_files,
            vec![LintFile::Tree, LintFile::Claims]
        );
        let fixed_tree = read_tree(&dir);
        let fixed_claims = read_claims(&dir);
        let (manifest, report) =
            parse_sources(&fixed_tree, Some(&fixed_claims)).expect("fixed fixture must parse");
        assert!(report.is_ok());
        assert_eq!(
            manifest
                .claims
                .iter()
                .map(|claim| claim.id.as_str())
                .collect::<Vec<_>>(),
            (1..=10)
                .map(|number| format!("C{number:02}"))
                .collect::<Vec<_>>()
        );
        for claim in &manifest.claims {
            assert!(
                fixed_claims.contains(&format!("## {}: {}", claim.id, claim.title)),
                "missing canonical header for {}",
                claim.id
            );
        }
        assert_eq!(
            manifest.claims[0].title,
            "16× Training Speedup Through Incremental Optimization"
        );
        assert_eq!(
            manifest.claims[0].statement.as_deref(),
            Some(
                "Human-authored optimizations compress GPT-2 124M training (val_loss ≤ 3.28) from 49.5 min to 3.1 min across 21 records, achieving a 16.1× wall-clock speedup on 8×H100."
            )
        );
        // The two pivot `trigger:` aliases were recovered into `reason:`; clearing
        // exactly those recovered fields must reproduce the base nodes.
        let recovered: Vec<&Node> = manifest
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    &node.fields,
                    NodeFields::Pivot {
                        reason: Some(_),
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(recovered.len(), 2, "got: {recovered:?}");
        let mut without_recovered = manifest.nodes.clone();
        for node in &mut without_recovered {
            if let NodeFields::Pivot { reason, .. } = &mut node.fields {
                *reason = None;
            }
        }
        assert_eq!(without_recovered, base.nodes);
        assert_eq!(manifest.links, base.links);
        assert!(manifest.bindings.iter().all(|binding| {
            manifest
                .claims
                .iter()
                .any(|claim| claim.id == binding.claim)
        }));
        assert_eq!(
            manifest
                .bindings
                .iter()
                .filter(|binding| {
                    !manifest
                        .claims
                        .iter()
                        .any(|claim| claim.id == binding.claim)
                })
                .collect::<Vec<_>>(),
            base.bindings.iter().collect::<Vec<_>>()
        );

        let second = fix_dir(dir.path());
        assert!(second.applied.is_empty());
        assert!(second.changed_files.is_empty());
        assert_eq!(read_tree(&dir), fixed_tree);
        assert_eq!(read_claims(&dir), fixed_claims);
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
    fn ara001_rejects_error_bearing_normalized_artifact_byte_identically() {
        let yaml = "\
root:
  id: N01
  type: question
  children:
    - id: N01
      type: insight
";
        assert!(matches!(
            parse_sources_detailed(yaml, None),
            ParseOutcome::Normalized(_, ref report) if !report.is_ok()
        ));
        let dir = artifact(yaml, None);

        let outcome = fix_dir(dir.path());

        assert!(outcome.applied.is_empty());
        assert!(outcome.changed_files.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].rule, LintRuleId::RootDialect);
        assert_eq!(read_tree(&dir), yaml);
    }

    #[test]
    fn alias_fixes_reject_when_canonical_and_alias_fields_coexist_byte_identically() {
        let cases = [
            (
                "\
tree:
  - id: N01
    type: dead_end
    why_failed: canonical
    reason: alias
",
                LintRuleId::DeadEndReasonAlias,
            ),
            (
                "\
tree:
  - id: N01
    type: decision
    rationale: canonical
    justification: alias
",
                LintRuleId::DecisionRationaleAlias,
            ),
        ];

        for (yaml, rule) in cases {
            let dir = artifact(yaml, None);
            let outcome = fix_dir(dir.path());

            assert!(outcome.applied.is_empty(), "{rule:?}");
            assert!(outcome.changed_files.is_empty(), "{rule:?}");
            assert!(
                outcome.skipped.iter().any(|skipped| skipped.rule == rule),
                "{rule:?}: {:?}",
                outcome.skipped
            );
            assert_eq!(read_tree(&dir), yaml, "{rule:?}");
        }
    }

    #[test]
    fn fatal_alias_artifact_is_rejected_byte_identically() {
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    reason: diverged
  - broken: [
";
        assert!(matches!(
            parse_sources_detailed(yaml, None),
            ParseOutcome::Fatal(_)
        ));
        let dir = artifact(yaml, None);

        let outcome = fix_dir(dir.path());

        assert!(outcome.applied.is_empty());
        assert!(outcome.changed_files.is_empty());
        assert!(
            outcome
                .skipped
                .iter()
                .any(|skipped| skipped.rule == LintRuleId::DeadEndReasonAlias)
        );
        assert_eq!(read_tree(&dir), yaml);
    }

    #[test]
    fn ara004_rejects_new_unknown_dependency_byte_identically() {
        let yaml = "tree:\n  - id: N01\n    type: question\n";
        let claims = "\
## C01 — Recovered claim
- **Statement**: value
- **Dependencies**: [C99]
";
        let dir = artifact(yaml, Some(claims));

        let outcome = fix_dir(dir.path());

        assert!(outcome.applied.is_empty());
        assert!(outcome.changed_files.is_empty());
        assert!(
            outcome
                .skipped
                .iter()
                .any(|skipped| skipped.rule == LintRuleId::ClaimHeaderStyle)
        );
        assert_eq!(
            outcome.skipped[0].reason,
            "claim-header rewrite would introduce a new parse error occurrence; left unchanged"
        );
        assert_eq!(read_tree(&dir), yaml);
        assert_eq!(read_claims(&dir), claims);
    }

    #[test]
    fn ara004_rejects_fatal_tree_byte_identically() {
        let yaml = "tree: [\n";
        let claims = "## C01 — Recovered claim\n- **Statement**: value\n";
        assert!(matches!(
            parse_sources_detailed(yaml, Some(claims)),
            ParseOutcome::Fatal(_)
        ));
        let dir = artifact(yaml, Some(claims));

        let outcome = fix_dir(dir.path());

        assert!(outcome.applied.is_empty());
        assert!(outcome.changed_files.is_empty());
        assert!(
            outcome
                .skipped
                .iter()
                .any(|skipped| skipped.rule == LintRuleId::ClaimHeaderStyle)
        );
        assert_eq!(read_tree(&dir), yaml);
        assert_eq!(read_claims(&dir), claims);
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
