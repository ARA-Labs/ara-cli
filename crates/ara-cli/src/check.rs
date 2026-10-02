//! `ara check`: a linter/format-checker that composes two diagnostic sources
//! over an ARA artifact directory.
//!
//! It merges the **validate** layer ([`parse_dir`] errors/warnings, none of which
//! are fixable) with the **format-lint** layer ([`check_dir`], whose diagnostics
//! each carry a safe fix). Every finding from either layer is rendered with its
//! stable [`RuleCode`] (`ARA0xx` format, `ARA1xx` errors, `ARA2xx` warnings).
//! Without `--fix` it only reports and, like `ruff check`, exits non-zero when a
//! fixable issue remains. With `--fix` it applies the safe fixes in place
//! ([`fix_dir`]), re-checks the now-fixed directory, and reports the post-fix
//! state.
//!
//! # Exit codes (contract)
//!
//! - `0` — no errors and no unfixed fixable issues (and, under `--strict`, no
//!   warnings). In `--fix` mode this is judged on the post-fix state.
//! - `1` — errors present, or fixable issues remain unfixed.
//! - `2` — internal failure the CLI cannot recover from: the target is not a
//!   readable directory, `trace/exploration_tree.yaml` is unreadable, JSON
//!   serialization failed, or (`--fix`) a fix write failed
//!   ([`FixOutcome::has_errors`]). A *readable* artifact that merely has parse
//!   errors is exit `1`, not `2`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ara_core::{
    AppliedFix, Diagnostic, FixOutcome, LintDiagnostic, LintFile, LintReport, ParseReport,
    RuleCode, Severity, SkippedFix, check_dir, fix_dir, parse_dir,
};
use serde::Serialize;

/// Exit code for an internal failure the CLI cannot recover from (see module docs).
const EXIT_INTERNAL: u8 = 2;

#[derive(clap::Args)]
pub struct CheckArgs {
    /// Path to the ARA artifact directory (containing `trace/` and `logic/`).
    dir: PathBuf,
    /// Apply the safe format fixes in place, then re-check the fixed directory.
    #[arg(long)]
    fix: bool,
    /// Treat warnings as errors (affects the exit code only).
    #[arg(long)]
    strict: bool,
    /// Emit the composed report as JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

/// Runs `ara check`. See the module docs for the exit-code contract.
pub fn run(args: CheckArgs) -> ExitCode {
    // Up-front path checks map "the CLI can't do its job" to exit 2, keeping a
    // readable-but-invalid artifact (exit 1) distinct from a bad target.
    if !args.dir.is_dir() {
        eprintln!(
            "error: {} is not a directory (or does not exist)",
            args.dir.display()
        );
        return ExitCode::from(EXIT_INTERNAL);
    }
    let tree_path = args.dir.join("trace/exploration_tree.yaml");
    if let Err(e) = std::fs::read_to_string(&tree_path) {
        eprintln!("error: cannot read {}: {e}", tree_path.display());
        return ExitCode::from(EXIT_INTERNAL);
    }

    if args.fix {
        check_fix(&args)
    } else {
        check_only(&args)
    }
}

/// No-`--fix` path: parse + format-lint, report, and fail on any error or any
/// unfixed fixable issue.
fn check_only(args: &CheckArgs) -> ExitCode {
    let report = parse_report(&args.dir);
    let lint = check_dir(&args.dir);

    if args.json {
        let composed = CheckReport::new(&args.dir, &report, lint.diagnostics(), None, args.strict);
        let code = emit_json(&composed);
        // A serialization failure already returned exit 2; otherwise the 0/1
        // decision must match the human path.
        if code != ExitCode::SUCCESS {
            return code;
        }
        return decide(&report, &lint, args.strict);
    }

    print_human(&args.dir, &report, lint.diagnostics(), None, args.strict);
    decide(&report, &lint, args.strict)
}

/// `--fix` path: apply the safe fixes, then re-check the fixed directory. The
/// exit code reflects the post-fix state; a failed write forces exit 2.
fn check_fix(args: &CheckArgs) -> ExitCode {
    let outcome = fix_dir(&args.dir);
    // Re-check on disk: validate needs a fresh parse, and reading the lint back
    // from disk keeps the report honest even if a write failed (that case exits 2
    // below regardless).
    let report = parse_report(&args.dir);
    let lint = check_dir(&args.dir);

    if args.json {
        let composed = CheckReport::new(
            &args.dir,
            &report,
            lint.diagnostics(),
            Some(&outcome),
            args.strict,
        );
        let code = emit_json(&composed);
        // A serialization failure already returned exit 2; otherwise a failed
        // write must also surface as exit 2.
        if code != ExitCode::SUCCESS {
            return code;
        }
        return fix_exit(&outcome, &report, &lint, args.strict);
    }

    print_human(
        &args.dir,
        &report,
        lint.diagnostics(),
        Some(&outcome),
        args.strict,
    );
    fix_exit(&outcome, &report, &lint, args.strict)
}

/// Reads the parse report for `dir`, collapsing the `Ok`/`Err` result (both carry
/// a [`ParseReport`]) into the report itself.
fn parse_report(dir: &Path) -> ParseReport {
    match parse_dir(dir) {
        Ok((_manifest, report)) => report,
        Err(report) => report,
    }
}

/// Exit decision for the no-fix path: fail on any error, any fixable lint issue,
/// or (under `--strict`) any warning.
fn decide(report: &ParseReport, lint: &LintReport, strict: bool) -> ExitCode {
    if failed(report, lint, strict) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Exit decision for the `--fix` path: a failed write is exit 2; otherwise the
/// post-fix state decides between 0 and 1.
fn fix_exit(
    outcome: &FixOutcome,
    report: &ParseReport,
    lint: &LintReport,
    strict: bool,
) -> ExitCode {
    if outcome.has_errors() {
        return ExitCode::from(EXIT_INTERNAL);
    }
    decide(report, lint, strict)
}

/// True when the run should exit non-zero: a parse error, an unfixed fixable lint
/// issue, or (under `--strict`) a remaining warning.
fn failed(report: &ParseReport, lint: &LintReport, strict: bool) -> bool {
    !report.is_ok() || lint.fixable() > 0 || (strict && !report.warnings().is_empty())
}

// ---- human rendering ------------------------------------------------------

/// Renders the composed report as human-readable text: applied fixes (fix mode),
/// then validate errors/warnings, then annotated lint diagnostics, then skipped
/// fixes (fix mode), then a one-line summary.
fn print_human(
    dir: &Path,
    report: &ParseReport,
    lint: &[LintDiagnostic],
    outcome: Option<&FixOutcome>,
    strict: bool,
) {
    if let Some(outcome) = outcome {
        for a in &outcome.applied {
            println!(
                "fixed {} in {}: {}",
                a.rule,
                a.file.relative_path(),
                a.description
            );
        }
    }

    for d in report.errors() {
        print_validate_line(d);
    }
    for d in report.warnings() {
        print_validate_line(d);
    }
    for d in lint {
        print_lint_line(d);
    }

    if let Some(outcome) = outcome {
        for s in &outcome.skipped {
            println!(
                "skipped {} in {}: {}",
                s.rule,
                s.file.relative_path(),
                s.reason
            );
        }
        for (file, msg) in &outcome.errors {
            println!("error: could not write {}: {msg}", file.relative_path());
        }
    }

    print_summary_line(dir, report, lint, outcome, strict);
}

/// Prints one validate diagnostic prefixed with its rule code
/// (e.g. `ARA107 error: <path>: <message>`). The text after the code is exactly
/// what `ara validate` prints for the same diagnostic.
fn print_validate_line(d: &Diagnostic) {
    println!("{} {d}", d.code);
}

/// Prints one lint diagnostic, annotated with its rule id and a `[fixable]`
/// marker when a safe fix is known (e.g. `ARA002 [fixable]: <file>: <message>`).
fn print_lint_line(d: &LintDiagnostic) {
    let marker = if d.fixable { " [fixable]" } else { "" };
    println!(
        "{}{marker}: {}: {}",
        d.rule,
        d.file.relative_path(),
        d.message
    );
}

/// Prints the trailing `PASS`/`FAIL` summary line, adapting to fix vs. no-fix.
fn print_summary_line(
    dir: &Path,
    report: &ParseReport,
    lint: &[LintDiagnostic],
    outcome: Option<&FixOutcome>,
    strict: bool,
) {
    let errors = report.errors().len();
    let warnings = report.warnings().len();
    let fixable = lint.iter().filter(|d| d.fixable).count();
    let strict_note = if strict { " [--strict]" } else { "" };

    match outcome {
        Some(outcome) if outcome.has_errors() => {
            // A write failed: the tool could not finish its job (exit 2).
            println!(
                "{}: ERROR — {} fix write(s) failed{strict_note}",
                dir.display(),
                outcome.errors.len(),
            );
        }
        Some(outcome) => {
            let pass = !failed_counts(errors, fixable, warnings, strict);
            let status = if pass { "PASS" } else { "FAIL" };
            println!(
                "{}: {status} — applied {} fix(es); {errors} error(s), {warnings} warning(s), \
                 {fixable} fixable issue(s) remaining{strict_note}",
                dir.display(),
                outcome.applied.len(),
            );
        }
        None => {
            let pass = !failed_counts(errors, fixable, warnings, strict);
            let status = if pass { "PASS" } else { "FAIL" };
            let hint = if fixable > 0 {
                " — run `ara check --fix` to apply the fixable ones"
            } else {
                ""
            };
            println!(
                "{}: {status} — {errors} error(s), {warnings} warning(s), \
                 {fixable} fixable issue(s){hint}{strict_note}",
                dir.display(),
            );
        }
    }
}

/// The pass/fail decision expressed over the summary counts (kept in step with
/// [`failed`], which the exit code uses).
fn failed_counts(errors: usize, fixable: usize, warnings: usize, strict: bool) -> bool {
    errors > 0 || fixable > 0 || (strict && warnings > 0)
}

// ---- JSON rendering -------------------------------------------------------

/// Serializes `report` to pretty JSON on stdout, returning [`ExitCode::SUCCESS`]
/// on success or exit 2 on a serialization failure.
fn emit_json(report: &CheckReport) -> ExitCode {
    match serde_json::to_string_pretty(report) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to serialize check report: {e}");
            ExitCode::from(EXIT_INTERNAL)
        }
    }
}

/// Machine-readable `ara check` report for CI annotation. Combines the validate
/// report, the (post-fix, in fix mode) lint diagnostics, an optional fix summary,
/// and a roll-up.
#[derive(Serialize)]
struct CheckReport<'a> {
    /// The artifact directory that was checked.
    dir: String,
    /// Validate-layer diagnostics (`errors` + `warnings`), each with its rule
    /// code; none are fixable.
    validate: ValidateSection<'a>,
    /// Format-lint diagnostics, each with its rule id, `fixable` flag, and fix.
    lint: &'a [LintDiagnostic],
    /// Present only in `--fix` mode: what the fixer applied/skipped, the files it
    /// rewrote, and any write errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<FixSummary<'a>>,
    /// Roll-up counts and the resulting pass/fail decision.
    summary: Summary,
}

impl<'a> CheckReport<'a> {
    fn new(
        dir: &Path,
        validate: &'a ParseReport,
        lint: &'a [LintDiagnostic],
        outcome: Option<&'a FixOutcome>,
        strict: bool,
    ) -> Self {
        let errors = validate.errors().len();
        let warnings = validate.warnings().len();
        let fixable = lint.iter().filter(|d| d.fixable).count();
        let write_errors = outcome.is_some_and(FixOutcome::has_errors);
        Self {
            dir: dir.display().to_string(),
            validate: ValidateSection::from(validate),
            lint,
            fix: outcome.map(FixSummary::from),
            summary: Summary {
                errors,
                warnings,
                fixable,
                strict,
                write_errors,
                passed: !failed_counts(errors, fixable, warnings, strict),
            },
        }
    }
}

/// The validate portion of a [`CheckReport`]: the `ara validate --json` shape
/// with a `rule` code added to every diagnostic.
#[derive(Serialize)]
struct ValidateSection<'a> {
    errors: Vec<CodedDiagnostic<'a>>,
    warnings: Vec<CodedDiagnostic<'a>>,
}

impl<'a> From<&'a ParseReport> for ValidateSection<'a> {
    fn from(report: &'a ParseReport) -> Self {
        Self {
            errors: report.errors().iter().map(CodedDiagnostic::from).collect(),
            warnings: report
                .warnings()
                .iter()
                .map(CodedDiagnostic::from)
                .collect(),
        }
    }
}

/// A validate diagnostic as rendered by `check --json`. `rule` uses the same key
/// as a lint diagnostic, so CI can group findings from both layers by rule.
#[derive(Serialize)]
struct CodedDiagnostic<'a> {
    rule: RuleCode,
    severity: Severity,
    path: &'a str,
    message: &'a str,
}

impl<'a> From<&'a Diagnostic> for CodedDiagnostic<'a> {
    fn from(d: &'a Diagnostic) -> Self {
        Self {
            rule: d.code,
            severity: d.severity,
            path: &d.path,
            message: &d.message,
        }
    }
}

/// The `--fix` portion of a [`CheckReport`].
#[derive(Serialize)]
struct FixSummary<'a> {
    /// Fixes applied in place, in application order.
    applied: &'a [AppliedFix],
    /// Fixable drift detected but discarded by a guard, with the reason.
    skipped: &'a [SkippedFix],
    /// The files actually rewritten on disk.
    changed_files: &'a [LintFile],
    /// Write-back failures as `[file, message]`. Non-empty ⇒ the run exits 2.
    errors: &'a [(LintFile, String)],
}

impl<'a> From<&'a FixOutcome> for FixSummary<'a> {
    fn from(o: &'a FixOutcome) -> Self {
        Self {
            applied: &o.applied,
            skipped: &o.skipped,
            changed_files: &o.changed_files,
            errors: &o.errors,
        }
    }
}

/// Roll-up counts plus the pass/fail decision, so CI can key off one object.
#[derive(Serialize)]
struct Summary {
    /// Number of validate errors (in fix mode: remaining after the fix).
    errors: usize,
    /// Number of validate warnings (in fix mode: remaining after the fix).
    warnings: usize,
    /// Number of unfixed fixable lint issues.
    fixable: usize,
    /// Whether `--strict` was set (warnings then count against `passed`).
    strict: bool,
    /// Whether a fix write failed (`--fix` only); when true the run exits 2.
    write_errors: bool,
    /// The exit-0 vs. exit-1 decision (errors/fixable/strict-warnings). Does not
    /// account for `write_errors`, which independently forces exit 2.
    passed: bool,
}
