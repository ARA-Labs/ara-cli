//! Local-only Git plumbing for the directory merger. Never reconstructs ours.
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use ara_core::merge::GitMergeProvenance;
use serde::Serialize;
use tempfile::TempDir;

use crate::output::AgentError;

// Git 2.45 introduced --no-lazy-fetch; it sets fetch_if_missing=0 as well as
// GIT_NO_LAZY_FETCH. Earlier Git must not be allowed to contact promisor remotes.
// https://github.com/git/git/blob/v2.45.0/Documentation/RelNotes/2.45.0.txt
const MIN_GIT_VERSION: (u32, u32) = (2, 45);
const STDERR_LIMIT: usize = 4096;
const RECORD_LIMIT: usize = 64 * 1024;
const QUERY_LIMIT: usize = 1024 * 1024;
const TREE_FILE: &str = "trace/exploration_tree.yaml";

/// Measured native input-capture wall spans, never source identity metadata.
///
/// Tree capture and blob materialization run concurrently for each snapshot.
/// Their cumulative spans overlap and include stream backpressure; adding them
/// does not give total elapsed time. Use total_materialize_ms/total_setup_ms for
/// enclosing durations. merge_base_ms includes the complete ancestry traversal.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GitInputTimings {
    pub ref_resolution_ms: f64,
    pub merge_base_ms: f64,
    pub tree_capture_ms: f64,
    pub blob_materialize_ms: f64,
    pub total_materialize_ms: f64,
    pub total_setup_ms: f64,
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

/// Owns both private input trees for the entire planning/commit lifetime.
#[derive(Debug)]
pub struct GitMergeInputs {
    pub base_dir: PathBuf,
    pub theirs_dir: PathBuf,
    pub provenance: GitMergeProvenance,
    pub timings: GitInputTimings,
    repository: PathBuf,
    _temporary_root: TempDir,
}

/// Pins ancestry, validates local history, and extracts raw committed bytes.
/// The destination, index, refs, and Git merge state are never written.
pub fn prepare_git_inputs(
    destination: &Path,
    reference: &str,
) -> Result<GitMergeInputs, AgentError> {
    let setup_started = Instant::now();
    let mut timings = GitInputTimings::default();
    let destination = fs::canonicalize(destination)
        .map_err(|_| AgentError::io("Cannot open the destination artifact directory"))?;
    if !destination.is_dir() {
        return Err(AgentError::setup(
            "git_worktree_required",
            "Git merge requires an artifact directory inside a non-bare Git working tree",
        ));
    }
    check_version(&destination)?;
    let inside = query(&destination, &["rev-parse", "--is-inside-work-tree"])?;
    if trim_line(&inside)? != b"true" {
        return Err(AgentError::setup(
            "git_worktree_required",
            "Git merge requires a non-bare Git working tree",
        ));
    }
    let top = query(
        &destination,
        &["rev-parse", "--path-format=absolute", "--show-toplevel"],
    )?;
    let repository = fs::canonicalize(path_from_bytes(trim_line(&top)?)?)
        .map_err(|_| AgentError::io("Cannot open the Git working-tree root"))?;
    let relative = destination.strip_prefix(&repository).map_err(|_| {
        AgentError::setup(
            "git_worktree_required",
            "The selected artifact is outside the Git working tree",
        )
    })?;
    let repo_relative_root = relative_path(relative)?;
    let superproject = query(
        &repository,
        &["rev-parse", "--show-superproject-working-tree"],
    )?;
    if !superproject.is_empty() && !trim_line(&superproject)?.is_empty() {
        return Err(AgentError::semantic(
            "git_submodule",
            "An artifact inside a submodule is not supported by Git merge",
        ));
    }
    require_complete_history(&repository)?;
    let resolution_started = Instant::now();
    let head = resolve_commit(&repository, "HEAD", "HEAD is unborn or unavailable locally")?;
    let theirs = resolve_commit(
        &repository,
        reference,
        "The requested Git revision is not a locally available commit",
    )?;
    timings.ref_resolution_ms = elapsed_ms(resolution_started);
    let ancestry_started = Instant::now();
    // Walking both complete ancestor graphs catches missing commits even when
    // merge-base could answer before encountering an absent ancestor.
    let mut history = GitChild::spawn(&repository, &["rev-list", &head, &theirs, "--"], false)?;
    io::copy(&mut history.stdout, &mut io::sink())
        .map_err(|_| AgentError::io("Cannot read local Git ancestry"))?;
    history.finish("Local Git ancestry is incomplete or unreadable")?;
    let mut bases = GitChild::spawn(&repository, &["merge-base", "--all", &head, &theirs], false)?;
    let data = read_bounded(&mut bases.stdout, QUERY_LIMIT, "Git merge-base output")?;
    let (status, stderr) = bases.complete()?;
    if status.code() == Some(1) && data.is_empty() {
        return Err(AgentError::semantic(
            "git_unrelated_history",
            "HEAD and the pinned source commit have no common ancestor",
        ));
    }
    require_success(status, &stderr, "Cannot obtain a local Git merge base")?;
    let base_ids: Vec<&[u8]> = data
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    if base_ids.len() != 1 {
        return Err(AgentError::semantic(
            "git_ambiguous_base",
            "Git merge requires exactly one merge base; criss-cross ancestry is unsupported",
        ));
    }
    let base = object_id(base_ids[0])?;
    timings.merge_base_ms = elapsed_ms(ancestry_started);
    check_unmerged(&repository, &repo_relative_root)?;

    let materialization_started = Instant::now();
    // Validate the OS temporary location before creating anything. In particular,
    // an inherited TMPDIR inside the artifact cannot cause a dry-run mutation.
    let temporary_parent = fs::canonicalize(std::env::temp_dir())
        .map_err(|_| AgentError::io("Cannot open the OS temporary directory"))?;
    if temporary_parent.starts_with(&repository)
        || temporary_parent.starts_with(&destination)
        || temporary_parent
            .components()
            .any(|part| part.as_os_str() == OsStr::new(".git"))
    {
        return Err(AgentError::setup(
            "unsafe_temporary_directory",
            "Git merge needs an OS temporary directory outside the repository and artifact",
        ));
    }
    let temporary_root = tempfile::Builder::new()
        .prefix("ara-git-")
        .tempdir_in(temporary_parent)
        .map_err(|_| AgentError::io("Cannot create private Git input storage"))?;
    set_private_permissions(temporary_root.path())?;
    let base_dir = temporary_root.path().join("base");
    let theirs_dir = temporary_root.path().join("theirs");
    fs::create_dir(&base_dir)
        .and_then(|()| fs::create_dir(&theirs_dir))
        .map_err(|_| AgentError::io("Cannot create private Git input trees"))?;
    materialize(
        &repository,
        &base,
        &repo_relative_root,
        &base_dir,
        &mut timings,
    )?;
    materialize(
        &repository,
        &theirs,
        &repo_relative_root,
        &theirs_dir,
        &mut timings,
    )?;
    timings.total_materialize_ms = elapsed_ms(materialization_started);
    timings.total_setup_ms = elapsed_ms(setup_started);
    Ok(GitMergeInputs {
        base_dir,
        theirs_dir,
        provenance: GitMergeProvenance {
            repo_relative_root,
            head,
            theirs,
            base,
        },
        timings,
        repository,
        _temporary_root: temporary_root,
    })
}

/// Call under the destination writer lock, and again immediately before commit.
/// Movement of the source ref is harmless; movement of HEAD invalidates ancestry.
pub fn recheck_head(inputs: &GitMergeInputs) -> Result<(), AgentError> {
    let current = resolve_commit(
        &inputs.repository,
        "HEAD",
        "HEAD became unavailable during Git merge",
    )?;
    if current != inputs.provenance.head {
        return Err(AgentError::semantic(
            "git_head_changed",
            "HEAD changed after Git merge inputs were captured; no merge may be committed",
        ));
    }
    check_unmerged(&inputs.repository, &inputs.provenance.repo_relative_root)
}

/// Proves advancement of an already enrolled source key without treating a ref
/// name, filesystem location, or repository URL as portable fork identity.
pub fn check_source_advancement(
    inputs: &GitMergeInputs,
    previous_theirs: &str,
) -> Result<(), AgentError> {
    validated_object_id(previous_theirs.as_bytes())?;
    if previous_theirs == inputs.provenance.theirs {
        return Ok(());
    }
    require_complete_history(&inputs.repository)?;
    let previous = resolve_commit(
        &inputs.repository,
        previous_theirs,
        "The enrolled source commit is unavailable locally; its ancestry cannot be proved",
    )?;
    if previous != previous_theirs {
        return Err(AgentError::semantic(
            "git_source_lineage",
            "The recorded source revision is not a full immutable commit ID",
        ));
    }
    // prepare_git_inputs already traversed the complete pinned source graph.
    // Do not walk its entire history a second time for source-key enrollment.
    let mut child = GitChild::spawn(
        &inputs.repository,
        &[
            "merge-base",
            "--is-ancestor",
            &previous,
            &inputs.provenance.theirs,
        ],
        false,
    )?;
    io::copy(&mut child.stdout, &mut io::sink())
        .map_err(|_| AgentError::io("Cannot read Git source ancestry proof"))?;
    let (status, stderr) = child.complete()?;
    if status.code() == Some(1) {
        return Err(AgentError::semantic(
            "git_source_lineage",
            "The pinned source does not descend from the previously imported commit; reset, rebase, or source-key reuse requires explicit new enrollment",
        ));
    }
    require_success(status, &stderr, "Cannot prove local Git source advancement")
}

/// Checks the artifact context as well as the immutable source revision.
pub fn check_source_lineage(
    inputs: &GitMergeInputs,
    previous: &GitMergeProvenance,
) -> Result<(), AgentError> {
    if previous.repo_relative_root != inputs.provenance.repo_relative_root {
        return Err(AgentError::semantic(
            "git_source_lineage",
            "The enrolled Git source used a different repository-relative artifact root",
        ));
    }
    check_source_advancement(inputs, &previous.theirs)
}

fn check_version(directory: &Path) -> Result<(), AgentError> {
    // Do not pass the new --no-lazy-fetch flag until the executable version is
    // known. --version cannot resolve objects or invoke repository machinery.
    let mut command = git_command(directory, false);
    command.arg("--version");
    let mut child = GitChild::from_command(command, false)?;
    let bytes = read_bounded(&mut child.stdout, RECORD_LIMIT, "Git version output")?;
    child.finish("Cannot determine the Git executable version")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| malformed("Git version"))?;
    let version = text
        .strip_prefix("git version ")
        .and_then(|s| s.split_whitespace().next())
        .ok_or_else(|| malformed("Git version"))?;
    let mut components = version.split('.');
    let major = components.next().and_then(|s| s.parse::<u32>().ok());
    let minor = components.next().and_then(|s| s.parse::<u32>().ok());
    if major.zip(minor).is_none_or(|pair| pair < MIN_GIT_VERSION) {
        return Err(AgentError::setup(
            "git_version_unsupported",
            "Git merge requires Git 2.45 or newer to guarantee that missing objects cannot trigger lazy fetching",
        ));
    }
    Ok(())
}

fn require_complete_history(repository: &Path) -> Result<(), AgentError> {
    let shallow = query(repository, &["rev-parse", "--is-shallow-repository"])?;
    match trim_line(&shallow)? {
        b"false" => Ok(()),
        b"true" => Err(AgentError::semantic(
            "git_shallow_history",
            "Git merge requires complete local ancestry; shallow repositories are unsupported and will not be fetched",
        )),
        _ => Err(malformed("Git shallow-history response")),
    }
}

fn resolve_commit(repository: &Path, reference: &str, context: &str) -> Result<String, AgentError> {
    // --end-of-options prevents a reference from becoming a command option.
    let expression = format!("{reference}^{{commit}}");
    let mut child = GitChild::spawn(
        repository,
        &["rev-parse", "--verify", "--end-of-options", &expression],
        false,
    )?;
    let bytes = read_bounded(&mut child.stdout, RECORD_LIMIT, "Git commit resolution")?;
    child.finish(context)?;
    object_id(trim_line(&bytes)?)
}

fn check_unmerged(repository: &Path, prefix: &str) -> Result<(), AgentError> {
    let mut child = GitChild::spawn(
        repository,
        &["ls-files", "--unmerged", "-z", "--full-name"],
        false,
    )?;
    let mut record = Vec::new();
    let mut inside = false;
    while read_record(&mut child.stdout, 0, &mut record)? {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| malformed("unmerged Git index record"))?;
        let path = &record[tab + 1..];
        if descendant(path, prefix.as_bytes()).is_some() || path == prefix.as_bytes() {
            inside = true;
        }
    }
    child.finish("Cannot inspect Git index conflicts")?;
    if inside {
        return Err(AgentError::semantic(
            "git_unmerged_index",
            "The selected artifact has unmerged index entries; resolve them before an ARA merge",
        ));
    }
    Ok(())
}

fn materialize(
    repository: &Path,
    commit: &str,
    prefix: &str,
    root: &Path,
    timings: &mut GitInputTimings,
) -> Result<(), AgentError> {
    let tree_started = Instant::now();
    let mut tree = GitChild::spawn(
        repository,
        &["ls-tree", "-r", "-z", "--full-tree", commit, "--"],
        false,
    )?;
    let blobs_started = Instant::now();
    let mut batch = GitChild::spawn(repository, &["cat-file", "--batch"], true)?;
    let mut input = batch
        .child
        .stdin
        .take()
        .ok_or_else(|| AgentError::io("Cannot open Git blob input"))?;
    let mut directories = BTreeSet::new();
    directories.insert(root.to_path_buf());
    let mut record = Vec::new();
    let mut header = Vec::new();
    let mut found_tree = false;
    // Stream tree metadata and blob payloads; only created directory names
    // remain in memory for host-filesystem collision checks.
    while read_record(&mut tree.stdout, 0, &mut record)? {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| malformed("Git tree record"))?;
        let [mode, kind, oid] =
            three_fields(&record[..tab]).ok_or_else(|| malformed("Git tree metadata"))?;
        let path = &record[tab + 1..];
        let selected = descendant(path, prefix.as_bytes());
        // A symlink/gitlink at an ancestor cannot be silently mistaken for a
        // missing historical artifact. No object outside the prefix is copied.
        let ancestor = !prefix.is_empty()
            && (path == prefix.as_bytes() || descendant(prefix.as_bytes(), path).is_some());
        if ancestor {
            return Err(AgentError::semantic(
                "git_unsafe_mode",
                "The historical artifact root traverses a symlink, gitlink, or non-directory entry",
            ));
        }
        let Some(relative) = selected else {
            continue;
        };
        let path = safe_blob_path(relative)?;
        if path.components().any(|part| {
            part.as_os_str()
                .to_str()
                .is_some_and(ara_core::write::source::private_name)
        }) {
            continue;
        }
        let executable = match (mode, kind) {
            (b"100644", b"blob") => false,
            (b"100755", b"blob") => true,
            _ => {
                return Err(AgentError::semantic(
                    "git_unsafe_mode",
                    "Git inputs support only regular 100644/100755 blobs, not symlinks, gitlinks, or special modes",
                ));
            }
        };
        let oid = validated_object_id(oid)?;
        found_tree |= relative == TREE_FILE.as_bytes();
        ensure_parents(root, &path, &mut directories)?;
        let target = root.join(&path);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| materialization_error(error, "Cannot create a private Git blob"))?;
        writeln!(input, "{oid}")
            .and_then(|()| input.flush())
            .map_err(|_| AgentError::io("Cannot request a local Git blob"))?;
        if !read_record(&mut batch.stdout, b'\n', &mut header)? {
            return Err(AgentError::setup(
                "git_object_unavailable",
                "Git stopped before returning the requested local blob",
            ));
        }
        if header.strip_suffix(b" missing") == Some(oid.as_bytes()) {
            return Err(AgentError::setup(
                "git_object_unavailable",
                format!("Required blob {oid} is unavailable locally; fetching is disabled"),
            ));
        }
        let [returned, kind, size] =
            three_fields(&header).ok_or_else(|| malformed("Git blob frame"))?;
        if returned != oid.as_bytes() || kind != b"blob" {
            return Err(malformed("Git blob frame"));
        }
        let size = std::str::from_utf8(size)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| malformed("Git blob size"))?;
        let copied = io::copy(&mut (&mut batch.stdout).take(size), &mut file)
            .map_err(|_| AgentError::io("Cannot stream a local Git blob into private storage"))?;
        if copied != size {
            return Err(AgentError::setup(
                "git_object_unavailable",
                "Git returned an incomplete local blob payload",
            ));
        }
        let mut terminator = [0];
        batch
            .stdout
            .read_exact(&mut terminator)
            .map_err(|_| malformed("Git blob terminator"))?;
        if terminator[0] != b'\n' {
            return Err(malformed("Git blob terminator"));
        }
        file.flush()
            .map_err(|_| AgentError::io("Cannot finish writing a private Git blob"))?;
        set_blob_permissions(&file, executable)?;
    }
    tree.finish("A committed Git tree is missing or unreadable locally")?;
    timings.tree_capture_ms += elapsed_ms(tree_started);
    if !found_tree {
        let display_root = if prefix.is_empty() { "." } else { prefix };
        return Err(AgentError::semantic(
            "git_artifact_missing",
            format!(
                "The committed artifact at repository-relative root {display_root:?} is missing {TREE_FILE}"
            ),
        ));
    }
    // Closing stdin asks batch to finish; consume EOF and verify its exit status.
    drop(input);
    let mut extra = [0];
    if batch
        .stdout
        .read(&mut extra)
        .map_err(|_| AgentError::io("Cannot finish reading Git blob output"))?
        != 0
    {
        return Err(malformed("unexpected Git blob output"));
    }
    batch.finish("A local Git blob could not be materialized")?;
    timings.blob_materialize_ms += elapsed_ms(blobs_started);
    Ok(())
}

fn descendant<'a>(path: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    if prefix.is_empty() {
        return Some(path);
    }
    path.strip_prefix(prefix)?.strip_prefix(b"/")
}

fn safe_blob_path(bytes: &[u8]) -> Result<PathBuf, AgentError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        AgentError::semantic("git_unsafe_path", "Git artifact paths must be valid UTF-8")
    })?;
    if text.is_empty()
        || text.contains('\\')
        || text.contains(':')
        || text.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git")
        })
    {
        return Err(AgentError::semantic(
            "git_unsafe_path",
            "A committed artifact path is absolute, reserved, or contains traversal components",
        ));
    }
    let path = PathBuf::from(text);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(AgentError::semantic(
            "git_unsafe_path",
            "A committed artifact path escapes its private input tree",
        ));
    }
    Ok(path)
}

fn relative_path(path: &Path) -> Result<String, AgentError> {
    let mut parts = Vec::new();
    for part in path.components() {
        let Component::Normal(part) = part else {
            return Err(AgentError::semantic(
                "git_unsafe_path",
                "The artifact root must have a safe repository-relative path",
            ));
        };
        let text = part.to_str().ok_or_else(|| {
            AgentError::semantic(
                "git_unsafe_path",
                "The artifact root must have a UTF-8 repository-relative path",
            )
        })?;
        parts.push(text);
    }
    Ok(parts.join("/"))
}

fn ensure_parents(
    root: &Path,
    relative: &Path,
    directories: &mut BTreeSet<PathBuf>,
) -> Result<(), AgentError> {
    let parent = relative
        .parent()
        .ok_or_else(|| malformed("Git blob parent path"))?;
    let mut path = root.to_path_buf();
    for part in parent.components() {
        path.push(part);
        if !directories.contains(&path) {
            // create_dir (not create_dir_all) detects host case/normalization
            // aliases even when two logical names have disjoint descendants.
            fs::create_dir(&path).map_err(|error| {
                materialization_error(error, "Cannot create a private Git input directory")
            })?;
            directories.insert(path.clone());
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| AgentError::io("Cannot inspect a private Git input directory"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AgentError::semantic(
                "git_unsafe_path",
                "A private Git input parent is not a regular directory",
            ));
        }
    }
    Ok(())
}

fn materialization_error(error: io::Error, context: &str) -> AgentError {
    if error.kind() == io::ErrorKind::AlreadyExists {
        AgentError::semantic(
            "git_path_collision",
            "Committed Git paths collide on the host filesystem",
        )
    } else {
        AgentError::io(context)
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), AgentError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| AgentError::io("Cannot secure private Git input storage"))
}
#[cfg(not(unix))]
fn set_private_permissions(_: &Path) -> Result<(), AgentError> {
    Err(AgentError::setup(
        "git_platform_unsupported",
        "Git snapshot privacy currently requires a Unix host",
    ))
}

#[cfg(unix)]
fn set_blob_permissions(file: &File, executable: bool) -> Result<(), AgentError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(if executable {
        0o755
    } else {
        0o644
    }))
    .map_err(|_| AgentError::io("Cannot preserve committed Git blob permissions"))
}
#[cfg(not(unix))]
fn set_blob_permissions(_: &File, _: bool) -> Result<(), AgentError> {
    Err(AgentError::setup(
        "git_platform_unsupported",
        "Git snapshot permission preservation currently requires a Unix host",
    ))
}

fn object_id(bytes: &[u8]) -> Result<String, AgentError> {
    validated_object_id(bytes).map(str::to_owned)
}

fn validated_object_id(bytes: &[u8]) -> Result<&str, AgentError> {
    if !matches!(bytes.len(), 40 | 64) || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(malformed("immutable Git object ID"));
    }
    std::str::from_utf8(bytes).map_err(|_| malformed("immutable Git object ID"))
}

fn three_fields(bytes: &[u8]) -> Option<[&[u8]; 3]> {
    let mut fields = bytes.split(|byte| *byte == b' ');
    let result = [fields.next()?, fields.next()?, fields.next()?];
    fields.next().is_none().then_some(result)
}

fn trim_line(bytes: &[u8]) -> Result<&[u8], AgentError> {
    // Remove exactly Git's output newline, not whitespace in a pathname.
    bytes
        .strip_suffix(b"\n")
        .ok_or_else(|| malformed("Git line response"))
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf, AgentError> {
    use std::os::unix::ffi::OsStrExt;
    Ok(PathBuf::from(OsStr::from_bytes(bytes)))
}
#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf, AgentError> {
    Ok(PathBuf::from(
        std::str::from_utf8(bytes).map_err(|_| malformed("Git working-tree path"))?,
    ))
}

fn malformed(context: &str) -> AgentError {
    AgentError::setup(
        "git_invalid_output",
        format!("Malformed or incomplete {context}"),
    )
}

fn query(directory: &Path, arguments: &[&str]) -> Result<Vec<u8>, AgentError> {
    let mut child = GitChild::spawn(directory, arguments, false)?;
    let bytes = read_bounded(&mut child.stdout, QUERY_LIMIT, "Git discovery output")?;
    child.finish("Cannot read the required local Git repository data")?;
    Ok(bytes)
}

fn read_bounded(
    reader: &mut impl Read,
    limit: usize,
    context: &str,
) -> Result<Vec<u8>, AgentError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AgentError::io(format!("Cannot read {context}")))?;
    if bytes.len() > limit {
        return Err(malformed(context));
    }
    Ok(bytes)
}

fn read_record(
    reader: &mut impl BufRead,
    delimiter: u8,
    record: &mut Vec<u8>,
) -> Result<bool, AgentError> {
    record.clear();
    let count = reader
        .take(RECORD_LIMIT as u64 + 1)
        .read_until(delimiter, record)
        .map_err(|_| AgentError::io("Cannot read Git plumbing output"))?;
    if count == 0 {
        return Ok(false);
    }
    if count > RECORD_LIMIT || record.last().copied() != Some(delimiter) {
        return Err(malformed("Git plumbing record"));
    }
    record.pop();
    Ok(true)
}

fn git_command(directory: &Path, no_lazy_fetch: bool) -> Command {
    let mut command = Command::new("git");
    command.current_dir(directory).env_clear();
    // Only OS executable lookup/platform bootstrap survives. No GIT_*, SSH_*,
    // trace, index/object redirection, config injection, loader, or prompt env.
    for name in ["PATH", "SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    command
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_SYSTEM", null)
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_GRAFT_FILE", null)
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_PAGER", "")
        .arg("--no-pager")
        .arg("--no-replace-objects")
        .arg("--no-optional-locks");
    if no_lazy_fetch {
        command.arg("--no-lazy-fetch");
    }
    for setting in [
        "protocol.allow=never",
        "credential.helper=",
        "credential.interactive=false",
        "core.fsmonitor=false",
        "core.untrackedCache=false",
        "gc.auto=0",
        "maintenance.auto=false",
        "core.commitGraph=false",
        "advice.graftFileDeprecated=false",
    ] {
        command.args(["-c", setting]);
    }
    command.arg("-c").arg(format!("core.hooksPath={null}"));
    command.arg("-c").arg(format!("core.attributesFile={null}"));
    command
}

/// Drains bounded stderr concurrently and kills/reaps on every early return.
struct GitChild {
    child: Child,
    stdout: BufReader<ChildStdout>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    reaped: bool,
}
impl GitChild {
    fn spawn(directory: &Path, arguments: &[&str], input: bool) -> Result<Self, AgentError> {
        let mut command = git_command(directory, true);
        command.args(arguments);
        Self::from_command(command, input)
    }
    fn from_command(mut command: Command, input: bool) -> Result<Self, AgentError> {
        command
            .stdin(if input { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                AgentError::setup(
                    "git_unavailable",
                    "Git merge requires the Git executable (version 2.45 or newer) on PATH",
                )
            } else {
                AgentError::io("Cannot launch the local Git executable")
            }
        })?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AgentError::io("Cannot open Git output"));
            }
        };
        let Some(mut stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AgentError::io("Cannot open Git diagnostics"));
        };
        let drain = thread::Builder::new()
            .name("ara-git-stderr".into())
            .spawn(move || {
                let mut captured = Vec::with_capacity(STDERR_LIMIT);
                let mut block = [0; 4096];
                loop {
                    let count = stderr.read(&mut block)?;
                    if count == 0 {
                        break;
                    }
                    let keep = count.min(STDERR_LIMIT - captured.len());
                    captured.extend_from_slice(&block[..keep]);
                }
                Ok(captured)
            });
        match drain {
            Ok(stderr) => Ok(Self {
                child,
                stdout: BufReader::new(stdout),
                stderr: Some(stderr),
                reaped: false,
            }),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(AgentError::io("Cannot capture Git diagnostics"))
            }
        }
    }
    fn complete(&mut self) -> Result<(ExitStatus, Vec<u8>), AgentError> {
        let status = self
            .child
            .wait()
            .map_err(|_| AgentError::io("Cannot reap the Git subprocess"))?;
        self.reaped = true;
        let stderr = self
            .stderr
            .take()
            .ok_or_else(|| AgentError::io("Git diagnostics were already consumed"))?
            .join()
            .map_err(|_| AgentError::io("Cannot join the Git diagnostic reader"))?
            .map_err(|_| AgentError::io("Cannot read Git diagnostics"))?;
        Ok((status, stderr))
    }
    fn finish(&mut self, context: &str) -> Result<(), AgentError> {
        let (status, stderr) = self.complete()?;
        require_success(status, &stderr, context)
    }
}
impl Drop for GitChild {
    fn drop(&mut self) {
        if !self.reaped {
            // Batch stdin lives in the caller; killing rather than waiting for
            // EOF avoids an early-error deadlock even before its local drop.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(stderr) = self.stderr.take() {
            let _ = stderr.join();
        }
    }
}

fn require_success(status: ExitStatus, stderr: &[u8], context: &str) -> Result<(), AgentError> {
    if status.success() {
        return Ok(());
    }
    let detail = sanitized_stderr(stderr);
    let message = if detail.is_empty() {
        context.to_owned()
    } else {
        format!("{context}: {detail}")
    };
    Err(AgentError::setup("git_object_unavailable", message))
}

fn sanitized_stderr(bytes: &[u8]) -> String {
    // Never expose arbitrary paths (including private input paths) or controls.
    // Keep only a small useful subset of Git's fixed diagnostics; otherwise the
    // operation context is sufficient and cannot leak a credential or URL.
    let text = String::from_utf8_lossy(bytes);
    for (needle, message) in [
        (
            "not a git repository",
            "destination is not a Git repository",
        ),
        ("bad object", "a required local object is missing"),
        ("missing blob", "a required local blob is missing"),
        ("could not read", "required local Git data is unreadable"),
        (
            "unknown revision",
            "the requested local revision is unavailable",
        ),
        (
            "Needed a single revision",
            "the requested local revision is unavailable",
        ),
        (
            "dubious ownership",
            "Git rejected repository ownership; configure Git safe.directory explicitly",
        ),
        ("transport", "remote transport is disabled"),
    ] {
        if text.contains(needle) {
            return message.into();
        }
    }
    String::new()
}
