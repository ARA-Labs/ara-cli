//! Native inputs and transaction adapter for one source-aware merge planner.
pub mod git;
use crate::agent::ReadOptions;
use crate::output::AgentError;
use ara_core::merge::{self, MergeOptions};
use ara_core::write::{ArtifactLock, ArtifactSnapshot};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, clap::Args)]
pub struct MergeArgs {
    #[command(subcommand)]
    pub command: Option<MergeCommand>,
    #[arg(long, conflicts_with = "git", requires = "theirs")]
    pub base: Option<PathBuf>,
    #[arg(long, conflicts_with = "git", requires = "base")]
    pub theirs: Option<PathBuf>,
    #[arg(long,conflicts_with_all=["base","theirs"])]
    pub git: Option<String>,
    #[arg(long = "as")]
    pub label: Option<String>,
    #[arg(long)]
    pub source_key: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub no_duplicate_check: bool,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Subcommand)]
pub enum MergeCommand {
    Resolve(ConflictArgs),
    Repair(RepairArgs),
}
#[derive(Debug, clap::Args)]
pub struct ConflictArgs {
    pub conflict: String,
    #[arg(long,value_parser=["ours","theirs","base"])]
    pub take: String,
    #[arg(long)]
    pub session: String,
    #[arg(long)]
    pub turn: u64,
    #[arg(long)]
    pub signal: String,
    #[arg(long)]
    pub provenance: String,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct RepairArgs {
    #[arg(long)]
    pub conflict_file: PathBuf,
    #[arg(long,value_parser=["reject_incoming","restore_base"])]
    pub decision: String,
    #[arg(long)]
    pub expected_current: String,
    #[arg(long)]
    pub session: String,
    #[arg(long)]
    pub turn: u64,
    #[arg(long)]
    pub signal: String,
    #[arg(long)]
    pub provenance: String,
    #[arg(long)]
    pub reason: String,
    #[command(flatten)]
    pub output: ReadOptions,
}
#[derive(Debug, clap::Args)]
pub struct ResolveArgs {
    pub address: String,
    #[command(flatten)]
    pub output: ReadOptions,
}
pub fn convert_error(error: merge::MergeError) -> AgentError {
    let exit = error.exit_code();
    let details = serde_json::to_value(&error).ok().map(Box::new);
    AgentError {
        exit,
        code: error.code,
        message: error.message,
        id: None,
        line: None,
        details,
    }
}
fn fingerprint(snapshot: &ArtifactSnapshot) -> Vec<(&str, &str)> {
    snapshot
        .files
        .iter()
        .map(|(path, file)| (path.as_str(), file.digest.as_str()))
        .collect()
}
fn disjoint_roots(roots: &[&Path]) -> Result<(), AgentError> {
    for (i, left) in roots.iter().enumerate() {
        for right in roots.iter().skip(i + 1) {
            if left.starts_with(right) || right.starts_with(left) {
                return Err(AgentError::semantic(
                    "overlapping_inputs",
                    "Base, ours, and theirs must be nonoverlapping roots",
                ));
            }
        }
    }
    Ok(())
}
pub fn run(root: &Path, args: &MergeArgs) -> Result<Value, AgentError> {
    if let Some(command) = &args.command {
        return match command {
            MergeCommand::Resolve(resolution) => resolve_conflict(root, resolution),
            MergeCommand::Repair(repair) => repair_protected(root, repair),
        };
    }
    let operation_started = std::time::Instant::now();
    let git_inputs = args
        .git
        .as_deref()
        .map(|reference| git::prepare_git_inputs(root, reference))
        .transpose()?;
    let (base_root, theirs_root) = if let Some(inputs) = &git_inputs {
        (inputs.base_dir.clone(), inputs.theirs_dir.clone())
    } else {
        let base = args.base.as_deref().ok_or_else(|| {
            AgentError::semantic("missing_merge_mode", "Supply --base and --theirs, or --git")
        })?;
        let theirs = args
            .theirs
            .as_deref()
            .ok_or_else(|| AgentError::semantic("missing_merge_mode", "Supply --theirs"))?;
        (
            crate::context::validate(base)?,
            crate::context::validate(theirs)?,
        )
    };
    disjoint_roots(&[root, &base_root, &theirs_root])?;
    crate::context::ensure_no_pending_transaction(&base_root)?;
    crate::context::ensure_no_pending_transaction(&theirs_root)?;
    let base = ArtifactSnapshot::load_complete(&base_root).map_err(crate::write::convert_error)?;
    let theirs =
        ArtifactSnapshot::load_complete(&theirs_root).map_err(crate::write::convert_error)?;
    let lock = if args.dry_run {
        None
    } else {
        Some(ArtifactLock::acquire(root).map_err(crate::write::convert_error)?)
    };
    if args.dry_run {
        crate::context::ensure_no_pending_transaction(root)?;
    } else {
        ara_core::write::journal::recover(root).map_err(crate::write::convert_error)?;
    }
    let ours = ArtifactSnapshot::load_complete(root).map_err(crate::write::convert_error)?;
    if let Some(inputs) = &git_inputs {
        git::recheck_head(inputs)?;
    }
    let source_key = args.source_key.as_ref().ok_or_else(|| {
        AgentError::semantic(
            "source_identity_required",
            "Use --source-key with a stable fork identity; --as is only a display label",
        )
    })?;
    let history = merge::source_history(&ours, source_key).map_err(convert_error)?;
    let predecessor = if let Some(history) = history {
        if let Some(inputs) = &git_inputs {
            if let Some(previous) = history.git.as_ref() {
                git::check_source_lineage(inputs, previous)?;
            } else {
                return Err(AgentError::semantic(
                    "source_identity_conflict",
                    "Source was enrolled without Git provenance; explicit reviewed reenrollment required",
                ));
            }
            Some(history.fingerprint)
        } else {
            None
        }
    } else {
        None
    };
    let label = args
        .label
        .clone()
        .or_else(|| args.git.clone())
        .or_else(|| {
            theirs_root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .ok_or_else(|| AgentError::semantic("invalid_label", "Supply --as"))?;
    let options = MergeOptions {
        source_key: source_key.clone(),
        label,
        time: crate::write::now(),
        git: git_inputs.as_ref().map(|inputs| inputs.provenance.clone()),
        predecessor,
    };
    let loaded_at = std::time::Instant::now();
    let mut planned_at = None;
    let mut validated_at = None;
    let mut plan =
        merge::plan_merge_with_observer(&base, &ours, &theirs, &options, |phase| match phase {
            merge::MergePhase::PlanningFinished => planned_at = Some(std::time::Instant::now()),
            merge::MergePhase::ValidationFinished => validated_at = Some(std::time::Instant::now()),
        })
        .map_err(convert_error)?;
    let plan_returned = std::time::Instant::now();
    let planned_at = planned_at.unwrap_or(plan_returned);
    let validated_at = validated_at.unwrap_or(plan_returned);
    let recheck_started = std::time::Instant::now();
    let fresh_base =
        ArtifactSnapshot::load_complete(&base_root).map_err(crate::write::convert_error)?;
    let fresh_theirs =
        ArtifactSnapshot::load_complete(&theirs_root).map_err(crate::write::convert_error)?;
    if fingerprint(&base) != fingerprint(&fresh_base)
        || fingerprint(&theirs) != fingerprint(&fresh_theirs)
    {
        return Err(AgentError::semantic(
            "stale_merge_input",
            "A source changed during capture; no destination changes committed",
        ));
    }
    if let Some(inputs) = &git_inputs {
        git::recheck_head(inputs)?;
    }
    plan.working
        .plan_missing_directories()
        .map_err(crate::write::convert_error)?;
    let rechecked_at = std::time::Instant::now();
    let should_commit = !args.dry_run
        && (!plan.working.changed_paths().is_empty() || !plan.working.created_dirs.is_empty());
    if should_commit {
        plan.working
            .include_operational_ignore()
            .map_err(crate::write::convert_error)?;
    }
    let duplicate_check =
        !args.no_duplicate_check && std::env::var("ARA_NO_DUPLICATE_CHECK").as_deref() != Ok("1");
    let validation = &plan.validation;
    let advice = || {
        let started = std::time::Instant::now();
        let candidates = duplicate_check.then(|| {
            crate::write::duplicate_candidates_from_manifests(
                validation.base_manifest.as_ref(),
                &validation.candidate_manifest,
            )
        });
        (candidates, started.elapsed().as_secs_f64() * 1000.0)
    };
    let commit = || {
        let started = std::time::Instant::now();
        if should_commit {
            ara_core::write::transaction::commit(&plan.working)
                .map_err(crate::write::convert_error)?;
            Ok(started.elapsed().as_secs_f64() * 1000.0)
        } else {
            Ok(0.0)
        }
    };
    let (commit_ms, candidates, advisory_ms) =
        if should_commit && duplicate_check && validation.candidate_manifest.nodes.len() >= 1000 {
            std::thread::scope(|scope| {
                let worker = scope.spawn(advice);
                let committed = commit();
                let advised = worker.join();
                let commit_ms = committed?;
                let (candidates, advisory_ms) =
                    advised.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                Ok::<_, AgentError>((commit_ms, candidates, advisory_ms))
            })?
        } else {
            let commit_ms = commit()?;
            let (candidates, advisory_ms) = advice();
            (commit_ms, candidates, advisory_ms)
        };
    plan.report.committed = !args.dry_run;
    plan.report.dry_run = args.dry_run;
    let mut report = serde_json::to_value(&plan.report).expect("merge report serialization");
    if !plan.working.created_dirs.is_empty() {
        report.as_object_mut().unwrap().insert(
            "created_directories".into(),
            json!(plan.working.created_dirs),
        );
    }
    if let Some(candidates) = candidates {
        match candidates {
            Ok(candidates) => {
                report
                    .as_object_mut()
                    .unwrap()
                    .insert("duplicate_candidates".into(), json!(candidates));
            }
            Err(error) => {
                report.as_object_mut().unwrap().insert(
                    "advisories".into(),
                    json!([format!("duplicate_advisory_unavailable: {error}")]),
                );
            }
        }
    }
    report.as_object_mut().unwrap().insert("timings".into(),json!({
        "load_ms":loaded_at.duration_since(operation_started).as_secs_f64()*1000.0,
        "planning_ms":planned_at.duration_since(loaded_at).as_secs_f64()*1000.0,
        "validation_ms":validated_at.duration_since(planned_at).as_secs_f64()*1000.0,
        "input_recheck_ms":rechecked_at.duration_since(recheck_started).as_secs_f64()*1000.0,
        "commit_ms":commit_ms,"advisory_ms":advisory_ms,"operation_ms":operation_started.elapsed().as_secs_f64()*1000.0,
    }));
    if let Some(inputs) = &git_inputs {
        report.as_object_mut().unwrap().insert(
            "git_timings".into(),
            serde_json::to_value(&inputs.timings).expect("Git timings serialize"),
        );
    }
    drop(lock);
    Ok(report)
}
pub fn resolve(root: &Path, args: &ResolveArgs) -> Result<Value, AgentError> {
    let snapshot =
        ArtifactSnapshot::load_with_identities(root).map_err(crate::write::convert_error)?;
    let target = merge::resolve(&snapshot, &args.address).map_err(convert_error)?;
    Ok(json!({"format":"ara.resolve/v1","address":args.address,"id":target}))
}
fn resolve_conflict(root: &Path, args: &ConflictArgs) -> Result<Value, AgentError> {
    let lock = ArtifactLock::acquire(root).map_err(crate::write::convert_error)?;
    ara_core::write::journal::recover(root).map_err(crate::write::convert_error)?;
    let snapshot = ArtifactSnapshot::load_complete(root).map_err(crate::write::convert_error)?;
    let mut working = merge::plan_resolution(
        &snapshot,
        &args.conflict,
        &args.take,
        &args.session,
        args.turn,
        &args.signal,
        &args.provenance,
    )
    .map_err(convert_error)?;
    working
        .include_operational_ignore()
        .map_err(crate::write::convert_error)?;
    working
        .plan_missing_directories()
        .map_err(crate::write::convert_error)?;
    ara_core::write::transaction::commit(&working).map_err(crate::write::convert_error)?;
    let changed_paths = working.changed_paths();
    drop(lock);
    Ok(
        json!({"format":"ara.merge/v1","committed":true,"conflict":args.conflict,"take":args.take,"changed_paths":changed_paths}),
    )
}
fn repair_protected(root: &Path, args: &RepairArgs) -> Result<Value, AgentError> {
    let bytes = std::fs::read(&args.conflict_file).map_err(|e| AgentError::io(e.to_string()))?;
    let conflict: merge::MergeConflict = serde_json::from_slice(&bytes)
        .map_err(|e| AgentError::semantic("invalid_conflict_record", e.to_string()))?;
    let lock = ArtifactLock::acquire(root).map_err(crate::write::convert_error)?;
    ara_core::write::journal::recover(root).map_err(crate::write::convert_error)?;
    let snapshot = ArtifactSnapshot::load_complete(root).map_err(crate::write::convert_error)?;
    let mut working = merge::plan_protected_resolution(
        &snapshot,
        &conflict,
        &args.decision,
        &args.expected_current,
        &args.session,
        args.turn,
        &args.signal,
        &args.provenance,
        &args.reason,
    )
    .map_err(convert_error)?;
    working
        .include_operational_ignore()
        .map_err(crate::write::convert_error)?;
    working
        .plan_missing_directories()
        .map_err(crate::write::convert_error)?;
    ara_core::write::transaction::commit(&working).map_err(crate::write::convert_error)?;
    let changed_paths = working.changed_paths();
    drop(lock);
    Ok(
        json!({"format":"ara.merge/v1","committed":true,"decision":args.decision,"changed_paths":changed_paths}),
    )
}
