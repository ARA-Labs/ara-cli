//! `ara`: command-line entry point for the ARA viewer runtime.
//!
//! Installed via `cargo install ara-cli`, this ships a binary named `ara`.
//! Stage 1 provides `ara validate`; Stage 2 adds `ara layout`; Stage 4 adds
//! `ara serve`.

mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use ara_core::{LayoutOptions, ParseReport, parse_and_layout_dir, parse_dir};
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
    /// Compute a layered DAG layout and emit the positioned manifest as JSON.
    Layout(LayoutArgs),
    /// Serve an ARA directory with a live-reloading web viewer.
    Serve(serve::ServeArgs),
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
    /// Also run layout and report node/edge counts plus bounds.
    #[arg(long)]
    layout: bool,
}

#[derive(clap::Args)]
struct LayoutArgs {
    /// Path to the ARA artifact directory (containing `trace/` and `logic/`).
    dir: PathBuf,
    /// Emit the positioned manifest as JSON.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate(args) => validate(args),
        Command::Layout(args) => layout_cmd(args),
        Command::Serve(args) => serve::run(args),
    }
}

fn validate(args: ValidateArgs) -> ExitCode {
    if args.layout {
        return validate_with_layout(&args);
    }

    let report = match parse_dir(&args.dir) {
        Ok((_manifest, report)) => report,
        Err(report) => report,
    };

    emit_report(&args.dir, &report, args.json, args.strict)
}

fn validate_with_layout(args: &ValidateArgs) -> ExitCode {
    let opts = LayoutOptions::default();
    match parse_and_layout_dir(&args.dir, &opts) {
        Ok((manifest, report)) => {
            let code = emit_report(&args.dir, &report, args.json, args.strict);
            println!(
                "layout: {} node(s), {} edge(s), bounds: {:.1}×{:.1}",
                manifest.nodes.len(),
                manifest.links.len(),
                manifest.bounds.map_or(0.0, |b| b.width),
                manifest.bounds.map_or(0.0, |b| b.height),
            );
            code
        }
        Err(report) => emit_report(&args.dir, &report, args.json, args.strict),
    }
}

fn layout_cmd(args: LayoutArgs) -> ExitCode {
    let opts = LayoutOptions::default();
    match parse_and_layout_dir(&args.dir, &opts) {
        Ok((manifest, _report)) => {
            if args.json {
                match serde_json::to_string_pretty(&manifest) {
                    Ok(json) => {
                        println!("{json}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize manifest: {e}");
                        ExitCode::FAILURE
                    }
                }
            } else {
                println!(
                    "{}: {} node(s), {} edge(s), bounds: {:.1}×{:.1}",
                    args.dir.display(),
                    manifest.nodes.len(),
                    manifest.links.len(),
                    manifest.bounds.map_or(0.0, |b| b.width),
                    manifest.bounds.map_or(0.0, |b| b.height),
                );
                ExitCode::SUCCESS
            }
        }
        Err(report) => {
            for diagnostic in report.errors() {
                println!("{diagnostic}");
            }
            println!(
                "{}: layout skipped — {} error(s)",
                args.dir.display(),
                report.errors().len(),
            );
            ExitCode::FAILURE
        }
    }
}

fn emit_report(dir: &std::path::Path, report: &ParseReport, json: bool, strict: bool) -> ExitCode {
    if json {
        match serde_json::to_string_pretty(report) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("error: failed to serialize report: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        print_human(dir, report, strict);
    }

    let failed = !report.is_ok() || (strict && !report.warnings().is_empty());
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
