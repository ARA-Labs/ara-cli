//! `ara`: command-line entry point for the ARA viewer runtime.
//!
//! Installed via `cargo install ara-cli`, this ships a binary named `ara`.
//! Stage 1 provides `ara validate`; `serve` lands in a later stage.

use std::path::PathBuf;
use std::process::ExitCode;

use ara_core::{ParseReport, parse_dir};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ara", version, about = "ARA viewer runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and validate an ARA artifact directory.
    Validate(ValidateArgs),
}

#[derive(clap::Args)]
struct ValidateArgs {
    /// Path to the ARA artifact directory (containing `trace/` and `logic/`).
    dir: PathBuf,
    /// Emit the diagnostics report as JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
    /// Treat warnings as errors (affects the exit code only).
    #[arg(long)]
    strict: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate(args) => validate(args),
    }
}

fn validate(args: ValidateArgs) -> ExitCode {
    // Both arms carry a report; `Err` simply means it contains errors.
    let report = match parse_dir(&args.dir) {
        Ok((_manifest, report)) => report,
        Err(report) => report,
    };

    if args.json {
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("error: failed to serialize report: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        print_human(&args.dir, &report, args.strict);
    }

    // `--strict` promotes warnings to failures for the exit code.
    let failed = !report.is_ok() || (args.strict && !report.warnings().is_empty());
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_human(dir: &std::path::Path, report: &ParseReport, strict: bool) {
    for diagnostic in report.errors() {
        println!("{diagnostic}");
    }
    for diagnostic in report.warnings() {
        println!("{diagnostic}");
    }
    let failed = !report.is_ok() || (strict && !report.warnings().is_empty());
    let status = if failed { "FAIL" } else { "PASS" };
    let strict_note = if strict { " [--strict]" } else { "" };
    println!(
        "{}: {status} — {} error(s), {} warning(s){strict_note}",
        dir.display(),
        report.errors().len(),
        report.warnings().len(),
    );
}
