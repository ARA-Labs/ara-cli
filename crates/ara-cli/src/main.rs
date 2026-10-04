//! `ara`: command-line entry point for the ARA viewer runtime.
//!
//! Installed via `cargo install ara-cli`, this ships a binary named `ara`.
//! Stage 1 provides `ara validate`; Stage 2 adds `ara layout`; Stage 4 adds
//! `ara serve`. `ara check` composes the validate and format-lint layers into a
//! linter/format-checker with an optional `--fix`.

mod check;
mod check_config;
mod serve;
use ara_cli::{agent, context, merge, output, search, snapshot, write};

use std::path::PathBuf;
use std::process::ExitCode;

use ara_core::{LayoutOptions, ParseReport, parse_and_layout_dir, parse_dir};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ara", version, about = "ARA viewer runtime")]
struct Cli {
    /// Select the ARA directory for agent commands.
    #[arg(short = 'C', global = true)]
    directory: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and validate an ARA artifact directory.
    Validate(ValidateArgs),
    /// Compute a layered DAG layout and emit the positioned manifest as JSON.
    Layout(LayoutArgs),
    /// Lint an ARA artifact (validate + format checks), optionally auto-fixing.
    Check(check::CheckArgs),
    /// Serve an ARA directory with a live-reloading web viewer.
    Serve(serve::ServeArgs),
    /// Summarize layer counts, diagnostics, and next-ID advice.
    Status(agent::ReadOptions),
    /// List source-order entries with intersecting structural filters.
    Ls(agent::ListArgs),
    /// Retrieve complete entries or bounded source documents.
    Show(agent::ShowArgs),
    /// Follow nesting from the root through a node.
    Path(agent::IdArgs),
    /// Report structured references and possible prose mentions.
    Refs(agent::IdArgs),
    /// List unfinished work without changing its state.
    Open(agent::ReadOptions),
    /// Rank knowledge entries with offline BM25 keyword search.
    Find(FindArgs),
    /// Append nodes or validated dependency edges.
    Add(write::AddArgs),
    /// Edit mutable logic or permitted metadata fields.
    Edit(write::EditArgs),
    /// Create and update claims.
    Claim(write::LogicArgs),
    /// Create and update heuristics.
    Heuristic(write::LogicArgs),
    /// Append an observation without promoting it.
    Stage(write::StageArgs),
    /// Atomically create a target and record promotion pointers.
    Promote(write::PromoteArgs),
    /// Start sessions and append complete turns.
    Session(write::SessionArgs),
    /// Annotate a finding without collapsing either node.
    Link(write::LinkArgs),
    /// Apply an all-or-none typed JSONL batch.
    Apply(write::ApplyArgs),
    /// Merge complete knowledge layers from directories or local Git objects.
    Merge(merge::MergeArgs),
    /// Resolve a source-qualified imported identity.
    Resolve(merge::ResolveArgs),
    /// Capture the complete artifact into a new verified package directory.
    Snapshot(snapshot::SnapshotArgs),
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
#[derive(clap::Args)]
struct FindArgs {
    query: String,
    #[arg(long = "type")]
    kind: Option<String>,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[command(flatten)]
    output: agent::ReadOptions,
}

fn agent_command(
    directory: Option<&std::path::Path>,
    format: &str,
    options: &agent::ReadOptions,
    writer: bool,
    command: impl FnOnce(&std::path::Path) -> Result<serde_json::Value, output::AgentError>,
) -> ExitCode {
    let result = if writer {
        context::discover_writer(directory)
    } else {
        context::discover(directory)
    }
    .and_then(|root| command(&root));
    let incomplete = result.as_ref().is_ok_and(|value| {
        value.get("complete") == Some(&serde_json::Value::Bool(false))
            || value
                .get("unresolved_count")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|count| count > 0)
    });
    let exit = output::emit(format, result, options.json, options.fields.as_deref());
    if exit == ExitCode::SUCCESS && incomplete {
        ExitCode::FAILURE
    } else {
        exit
    }
}

fn argument_command(args: &[std::ffi::OsString]) -> Option<&str> {
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let argument = argument.to_str()?;
        if argument == "-C" {
            arguments.next();
            continue;
        }
        if !argument.starts_with('-') {
            return Some(argument);
        }
    }
    None
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let args = std::env::args_os().collect::<Vec<_>>();
            let command = argument_command(&args).unwrap_or("cli");
            let legacy = matches!(command, "validate" | "layout" | "check" | "serve");
            if error.exit_code() != 0 && !legacy && args.iter().any(|arg| arg == "--json") {
                return output::emit(
                    &format!("ara.{command}/v1"),
                    Err(output::AgentError::setup(
                        "argument_error",
                        error.to_string(),
                    )),
                    true,
                    None,
                );
            }
            error.exit();
        }
    };
    let directory = cli.directory.as_deref();
    let heuristic = matches!(&cli.command, Command::Heuristic(_));
    match cli.command {
        Command::Validate(args) => validate(args),
        Command::Layout(args) => layout_cmd(args),
        Command::Check(args) => check::run(args),
        Command::Serve(args) => serve::run(args),
        Command::Status(args) => {
            agent_command(directory, "ara.status/v1", &args, false, agent::status)
        }
        Command::Ls(args) => agent_command(directory, "ara.ls/v1", &args.output, false, |root| {
            agent::list(root, &args)
        }),
        Command::Show(args) => {
            if args.source {
                let result =
                    context::discover_source(directory).and_then(|root| agent::show(&root, &args));
                output::emit(
                    "ara.show/v1",
                    result,
                    args.output.json,
                    args.output.fields.as_deref(),
                )
            } else {
                agent_command(directory, "ara.show/v1", &args.output, false, |root| {
                    agent::show(root, &args)
                })
            }
        }
        Command::Path(args) => {
            agent_command(directory, "ara.path/v1", &args.output, false, |root| {
                agent::path(root, &args)
            })
        }
        Command::Refs(args) => {
            agent_command(directory, "ara.refs/v1", &args.output, false, |root| {
                agent::refs(root, &args)
            })
        }
        Command::Open(args) => agent_command(directory, "ara.open/v1", &args, false, |root| {
            agent::open(root, &args)
        }),
        Command::Find(args) => {
            agent_command(directory, "ara.find/v1", &args.output, false, |root| {
                let artifact = agent::Artifact::load(root)?;
                let hits = search::run_search_with_documents(
                    &artifact.manifest,
                    artifact.searchable_documents(),
                    &args.query,
                    args.kind.as_deref(),
                    args.limit,
                )
                .map_err(|message| output::AgentError::semantic("invalid_search", message))?;
                let mut results = serde_json::to_value(hits).expect("search result serialization");
                if args.output.full {
                    for hit in results.as_array_mut().unwrap() {
                        let id = hit
                            .get("id")
                            .or_else(|| hit.get("key"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned);
                        if let Some(id) = id {
                            let mut show = agent::show_loaded(
                                &artifact,
                                &agent::ShowArgs {
                                    ids: vec![id],
                                    output: agent::ReadOptions {
                                        full: true,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                            )?;
                            hit.as_object_mut().unwrap().insert(
                                "entry".into(),
                                show["entries"].as_array_mut().unwrap().remove(0),
                            );
                        }
                    }
                }
                Ok(serde_json::json!({"format":"ara.find/v1","results":results}))
            })
        }
        Command::Add(args) => {
            let options = match &args.command {
                write::AddCommand::Node(node) => &node.output,
                write::AddCommand::Edge(edge) => &edge.output,
            };
            agent_command(directory, "ara.add/v1", options, true, |root| {
                write::add(root, &args)
            })
        }
        Command::Edit(args) => {
            agent_command(directory, "ara.edit/v1", &args.output, true, |root| {
                write::edit(root, &args)
            })
        }
        Command::Claim(args) | Command::Heuristic(args) => {
            let options = match &args.command {
                write::LogicCommand::Add(add) => &add.output,
                write::LogicCommand::Set(set) => &set.output,
            };
            agent_command(
                directory,
                if heuristic {
                    "ara.heuristic/v1"
                } else {
                    "ara.claim/v1"
                },
                options,
                true,
                |root| write::logic(root, &args, heuristic),
            )
        }
        Command::Stage(args) => {
            agent_command(directory, "ara.stage/v1", &args.output, true, |root| {
                write::stage(root, &args)
            })
        }
        Command::Promote(args) => {
            agent_command(directory, "ara.promote/v1", &args.output, true, |root| {
                write::promote(root, &args)
            })
        }
        Command::Session(args) => {
            let options = match &args.command {
                write::SessionCommand::Start(start) => &start.output,
                write::SessionCommand::Log(log) => &log.output,
            };
            agent_command(directory, "ara.session/v1", options, true, |root| {
                write::session(root, &args)
            })
        }
        Command::Link(args) => {
            agent_command(directory, "ara.link/v1", &args.output, true, |root| {
                write::link(root, &args)
            })
        }
        Command::Apply(args) => output::emit(
            "ara.apply/v1",
            write::apply_discover(directory, &args),
            args.output.json,
            args.output.fields.as_deref(),
        ),
        Command::Merge(args) => {
            let options = match &args.command {
                Some(merge::MergeCommand::Resolve(resolution)) => &resolution.output,
                Some(merge::MergeCommand::Repair(repair)) => &repair.output,
                None => &args.output,
            };
            agent_command(directory, "ara.merge/v1", options, !args.dry_run, |root| {
                merge::run(root, &args)
            })
        }
        Command::Snapshot(args) => {
            agent_command(directory, snapshot::FORMAT, &args.options, true, |root| {
                snapshot::run(root, &args)
            })
        }
        Command::Resolve(args) => {
            if args.output.json {
                agent_command(directory, "ara.resolve/v1", &args.output, false, |root| {
                    merge::resolve(root, &args)
                })
            } else {
                match context::discover(directory).and_then(|root| merge::resolve(&root, &args)) {
                    Ok(value) => {
                        println!("{}", value["id"].as_str().unwrap());
                        ExitCode::SUCCESS
                    }
                    Err(error) => output::emit("ara.resolve/v1", Err(error), false, None),
                }
            }
        }
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
