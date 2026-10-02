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
//! An optional `.ara-check.toml` ([`crate::check_config`]) tunes the rule set:
//! ignored rules vanish from both outputs and from the exit decision, severity
//! overrides move findings between errors and warnings, and `unfixable` rules are
//! reported but never rewritten. Without a config file the behavior is the
//! built-in default, unchanged.
//!
//! # Exit codes (contract)
//!
//! - `0` — no error-severity findings (and, under `--strict`, no warnings). In
//!   `--fix` mode this is judged on the post-fix state.
//! - `1` — error-severity findings present: validate errors, or format-lint
//!   issues (which default to `error`, so an unfixed fixable issue fails).
//! - `2` — internal failure the CLI cannot recover from: the target is not a
//!   readable directory, `trace/exploration_tree.yaml` is unreadable, the config
//!   file is unreadable or invalid, JSON serialization failed, or (`--fix`) a fix
//!   write failed ([`FixOutcome::has_errors`]). A *readable* artifact that merely
//!   has parse errors is exit `1`, not `2`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ara_core::{
    AppliedFix, Diagnostic, FixOutcome, LintDiagnostic, LintFile, ParseReport, RuleCode, Severity,
    SkippedFix, check_dir, fix_dir_with, parse_dir,
};
use serde::Serialize;

use crate::check_config::{self, CheckConfig};

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
    /// Use this config file instead of discovering `.ara-check.toml`.
    #[arg(long, value_name = "PATH", conflicts_with = "no_config")]
    config: Option<PathBuf>,
    /// Ignore any `.ara-check.toml` and use the built-in rule settings.
    #[arg(long)]
    no_config: bool,
}

/// The config in effect for a run, plus where it came from (for `--json`).
struct LoadedConfig {
    config: CheckConfig,
    path: Option<PathBuf>,
}

/// Resolves the config for this run: `--no-config` → built-in; `--config` →
/// that file; otherwise the discovered `.ara-check.toml`, if any.
fn load_config(args: &CheckArgs) -> Result<LoadedConfig, check_config::ConfigError> {
    let path = if args.no_config {
        None
    } else {
        args.config
            .clone()
            .or_else(|| check_config::discover(&args.dir))
    };
    let config = match &path {
        Some(p) => CheckConfig::load(p)?,
        None => CheckConfig::default(),
    };
    Ok(LoadedConfig { config, path })
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
    let loaded = match load_config(&args) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };

    if args.fix {
        check_fix(&args, &loaded)
    } else {
        check_only(&args, &loaded)
    }
}

/// No-`--fix` path: parse + format-lint, report, and fail on any error-severity
/// finding (an unfixed fixable issue is one by default).
fn check_only(args: &CheckArgs, loaded: &LoadedConfig) -> ExitCode {
    let findings = Findings::collect(&args.dir, &loaded.config);

    if args.json {
        let composed = CheckReport::new(&args.dir, loaded, &findings, None, args.strict);
        let code = emit_json(&composed);
        // A serialization failure already returned exit 2; otherwise the 0/1
        // decision must match the human path.
        if code != ExitCode::SUCCESS {
            return code;
        }
        return decide(&findings, args.strict);
    }

    print_human(&args.dir, &findings, None, args.strict);
    decide(&findings, args.strict)
}

/// `--fix` path: apply the safe fixes the config allows, then re-check the fixed
/// directory. The exit code reflects the post-fix state; a failed write forces
/// exit 2.
fn check_fix(args: &CheckArgs, loaded: &LoadedConfig) -> ExitCode {
    let outcome = fix_dir_with(&args.dir, |rule| loaded.config.is_fixable(rule));
    // Re-check on disk: validate needs a fresh parse, and reading the lint back
    // from disk keeps the report honest even if a write failed (that case exits 2
    // below regardless).
    let findings = Findings::collect(&args.dir, &loaded.config);

    if args.json {
        let composed = CheckReport::new(&args.dir, loaded, &findings, Some(&outcome), args.strict);
        let code = emit_json(&composed);
        // A serialization failure already returned exit 2; otherwise a failed
        // write must also surface as exit 2.
        if code != ExitCode::SUCCESS {
            return code;
        }
        return fix_exit(&outcome, &findings, args.strict);
    }

    print_human(&args.dir, &findings, Some(&outcome), args.strict);
    fix_exit(&outcome, &findings, args.strict)
}

/// Reads the parse report for `dir`, collapsing the `Ok`/`Err` result (both carry
/// a [`ParseReport`]) into the report itself.
fn parse_report(dir: &Path) -> ParseReport {
    match parse_dir(dir) {
        Ok((_manifest, report)) => report,
        Err(report) => report,
    }
}

/// Exit decision: fail on any error-severity finding or (under `--strict`) any
/// warning.
fn decide(findings: &Findings, strict: bool) -> ExitCode {
    if findings.failed(strict) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Exit decision for the `--fix` path: a failed write is exit 2; otherwise the
/// post-fix state decides between 0 and 1.
fn fix_exit(outcome: &FixOutcome, findings: &Findings, strict: bool) -> ExitCode {
    if outcome.has_errors() {
        return ExitCode::from(EXIT_INTERNAL);
    }
    decide(findings, strict)
}

// ---- config application ---------------------------------------------------

/// The findings of one run with the config applied: disabled rules dropped,
/// severities overridden (validate findings re-bucketed accordingly), and each
/// lint finding's `fixable` flag narrowed to what `--fix` may apply.
struct Findings {
    /// Validate findings at effective severity `error`, in report order.
    errors: Vec<Diagnostic>,
    /// Validate findings at effective severity `warning`, in report order.
    warnings: Vec<Diagnostic>,
    /// Enabled format-lint findings, in source order.
    lint: Vec<LintFinding>,
}

/// One format-lint finding as reported by `check`. Serializes as the
/// [`LintDiagnostic`] plus a `severity` key that appears only when the config
/// overrides the rule's default (`error`), so the no-config JSON is unchanged.
#[derive(Serialize)]
struct LintFinding {
    #[serde(flatten)]
    diag: LintDiagnostic,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<Severity>,
}

impl LintFinding {
    fn severity(&self) -> Severity {
        self.severity
            .unwrap_or_else(|| self.diag.rule.code().default_severity())
    }
}

impl Findings {
    /// Parses + format-lints `dir` and applies `config`.
    fn collect(dir: &Path, config: &CheckConfig) -> Self {
        Self::resolve(&parse_report(dir), check_dir(dir).diagnostics(), config)
    }

    fn resolve(report: &ParseReport, lint: &[LintDiagnostic], config: &CheckConfig) -> Self {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        for d in report.errors().iter().chain(report.warnings()) {
            if !config.is_enabled(d.code) {
                continue;
            }
            let severity = config.severity(d.code);
            let d = Diagnostic {
                severity,
                ..d.clone()
            };
            match severity {
                Severity::Error => errors.push(d),
                Severity::Warning => warnings.push(d),
            }
        }
        let lint = lint
            .iter()
            .filter(|d| config.is_enabled(d.rule.code()))
            .map(|d| {
                let code = d.rule.code();
                let mut diag = d.clone();
                if !config.is_fixable(code) {
                    // Keep `fixable` mirroring `fix.is_some()`.
                    diag.fixable = false;
                    diag.fix = None;
                }
                let severity = config.severity(code);
                LintFinding {
                    diag,
                    severity: (severity != code.default_severity()).then_some(severity),
                }
            })
            .collect();
        Self {
            errors,
            warnings,
            lint,
        }
    }

    /// Roll-up counts: `errors` / `warnings` count every non-fixable finding at
    /// that severity (validate findings plus any `unfixable` lint finding);
    /// `fixable` counts the lint findings `--fix` may still apply.
    fn counts(&self) -> Counts {
        let mut counts = Counts {
            errors: self.errors.len(),
            warnings: self.warnings.len(),
            fixable: 0,
        };
        for l in &self.lint {
            match (l.diag.fixable, l.severity()) {
                (true, _) => counts.fixable += 1,
                (false, Severity::Error) => counts.errors += 1,
                (false, Severity::Warning) => counts.warnings += 1,
            }
        }
        counts
    }

    /// True when the run should exit non-zero: any finding at severity `error`
    /// (including a default-severity lint issue), or under `--strict` any
    /// finding at all.
    fn failed(&self, strict: bool) -> bool {
        let any_lint_error = self.lint.iter().any(|l| l.severity() == Severity::Error);
        let any = !self.errors.is_empty() || !self.warnings.is_empty() || !self.lint.is_empty();
        !self.errors.is_empty() || any_lint_error || (strict && any)
    }
}

/// See [`Findings::counts`].
struct Counts {
    errors: usize,
    warnings: usize,
    fixable: usize,
}

// ---- human rendering ------------------------------------------------------

/// Renders the composed report as human-readable text: applied fixes (fix mode),
/// then validate errors/warnings, then annotated lint diagnostics, then skipped
/// fixes (fix mode), then a one-line summary.
fn print_human(dir: &Path, findings: &Findings, outcome: Option<&FixOutcome>, strict: bool) {
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

    for d in findings.errors.iter().chain(&findings.warnings) {
        print_validate_line(d);
    }
    for l in &findings.lint {
        print_lint_line(l);
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

    print_summary_line(dir, findings, outcome, strict);
}

/// Prints one validate diagnostic prefixed with its rule code
/// (e.g. `ARA107 error: <path>: <message>`). The text after the code is exactly
/// what `ara validate` prints for the same diagnostic.
fn print_validate_line(d: &Diagnostic) {
    println!("{} {d}", d.code);
}

/// Prints one lint diagnostic, annotated with its rule id, its severity when the
/// config overrides the default, and a `[fixable]` marker when `--fix` may apply
/// it (e.g. `ARA002 [fixable]: <file>: <message>`,
/// `ARA002 warning [fixable]: ...`).
fn print_lint_line(l: &LintFinding) {
    let d = &l.diag;
    let severity = l.severity.map(|s| format!(" {s}")).unwrap_or_default();
    let marker = if d.fixable { " [fixable]" } else { "" };
    println!(
        "{}{severity}{marker}: {}: {}",
        d.rule,
        d.file.relative_path(),
        d.message
    );
}

/// Prints the trailing `PASS`/`FAIL` summary line, adapting to fix vs. no-fix.
fn print_summary_line(dir: &Path, findings: &Findings, outcome: Option<&FixOutcome>, strict: bool) {
    let Counts {
        errors,
        warnings,
        fixable,
    } = findings.counts();
    let pass = !findings.failed(strict);
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
            let status = if pass { "PASS" } else { "FAIL" };
            println!(
                "{}: {status} — applied {} fix(es); {errors} error(s), {warnings} warning(s), \
                 {fixable} fixable issue(s) remaining{strict_note}",
                dir.display(),
                outcome.applied.len(),
            );
        }
        None => {
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
    /// The `.ara-check.toml` in effect; absent when none was loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<String>,
    /// Validate-layer diagnostics (`errors` + `warnings`), each with its rule
    /// code and effective severity; none are fixable.
    validate: ValidateSection<'a>,
    /// Format-lint diagnostics, each with its rule id, `fixable` flag, and fix
    /// (plus `severity` when the config overrides it).
    lint: &'a [LintFinding],
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
        loaded: &LoadedConfig,
        findings: &'a Findings,
        outcome: Option<&'a FixOutcome>,
        strict: bool,
    ) -> Self {
        let Counts {
            errors,
            warnings,
            fixable,
        } = findings.counts();
        let write_errors = outcome.is_some_and(FixOutcome::has_errors);
        Self {
            dir: dir.display().to_string(),
            config: loaded.path.as_ref().map(|p| p.display().to_string()),
            validate: ValidateSection::from(findings),
            lint: &findings.lint,
            fix: outcome.map(FixSummary::from),
            summary: Summary {
                errors,
                warnings,
                fixable,
                strict,
                write_errors,
                passed: !findings.failed(strict),
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

impl<'a> From<&'a Findings> for ValidateSection<'a> {
    fn from(findings: &'a Findings) -> Self {
        Self {
            errors: findings.errors.iter().map(CodedDiagnostic::from).collect(),
            warnings: findings
                .warnings
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
    /// Number of non-fixable findings at severity `error`: validate errors plus
    /// any `unfixable` lint issue (in fix mode: remaining after the fix).
    errors: usize,
    /// Number of non-fixable findings at severity `warning` (in fix mode:
    /// remaining after the fix).
    warnings: usize,
    /// Number of unfixed lint issues `--fix` may apply.
    fixable: usize,
    /// Whether `--strict` was set (warnings then count against `passed`).
    strict: bool,
    /// Whether a fix write failed (`--fix` only); when true the run exits 2.
    write_errors: bool,
    /// The exit-0 vs. exit-1 decision (any error-severity finding, or under
    /// `--strict` any finding). Does not account for `write_errors`, which
    /// independently forces exit 2.
    passed: bool,
}
