//! Real local Git fixtures for the snapshot adapter and the shared CLI merger.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

use ara_cli::merge::git;

const BASE_TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: First parent\n  - id: N02\n    type: question\n    title: Second parent\n";
const OURS_TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: First parent\n    children:\n      - id: N124\n        type: question\n        title: Our child\n        description: Our N124 prose must stay untouched.\n  - id: N02\n    type: question\n    title: Second parent\n";
const THEIRS_TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: First parent\n  - id: N02\n    type: question\n    title: Second parent\n    children:\n      - id: N124\n        type: question\n        title: Bob child\n        description: Bob references N124.\n        opaque_extension: {nested: [one, two], enabled: true}\n";

struct Repository {
    _owner: TempDir,
    root: PathBuf,
    artifact: PathBuf,
    relative: String,
    base: String,
    ours: String,
    theirs: String,
}
impl Repository {
    fn new(relative: &str) -> Self {
        Self::with_base_claim(relative, b"# Claims\n")
    }
    fn with_base_claim(relative: &str, base_claim: &[u8]) -> Self {
        Self::with_source_opaque(relative, base_claim, None)
    }
    fn with_source_opaque(relative: &str, base_claim: &[u8], source_opaque: Option<&[u8]>) -> Self {
        let owner = TempDir::new().unwrap();
        let root = owner.path().join("repository");
        fs::create_dir(&root).unwrap();
        let version = run_git(&root, &["--version"]);
        let text = String::from_utf8(version.stdout).unwrap();
        let numbers: Vec<_> = text
            .trim()
            .strip_prefix("git version ")
            .unwrap()
            .split('.')
            .collect();
        let major: u32 = numbers[0].parse().unwrap();
        let minor: u32 = numbers[1].parse().unwrap();
        assert!(
            (major, minor) >= (2, 45),
            "Git-mode integration fixtures require Git >=2.45 (found {text}); earlier versions cannot disable lazy fetching"
        );
        run_git(&root, &["init", "--initial-branch=ours"]);
        run_git(&root, &["config", "user.name", "ARA Git fixture"]);
        run_git(&root, &["config", "user.email", "fixture@example.invalid"]);
        run_git(&root, &["config", "commit.gpgsign", "false"]);
        let artifact = root.join(relative);
        write_file(
            &artifact,
            "trace/exploration_tree.yaml",
            BASE_TREE.as_bytes(),
        );
        write_file(
            &artifact,
            "logic/problem.md",
            b"# Problem\n\nShared framing.\n",
        );
        write_file(&artifact, "logic/claims.md", base_claim);
        write_file(&artifact, "src/opaque.bin", b"shared code\0\xff\n");
        write_file(
            &artifact,
            "logic/vendor.yaml",
            b"opaque unregistered layer\0\xff\n",
        );
        run_git(&root, &["add", "--all"]);
        run_git(&root, &["commit", "-m", "base"]);
        let base = git_text(&root, &["rev-parse", "HEAD"]);
        run_git(&root, &["checkout", "-b", "bob"]);
        write_file(
            &artifact,
            "trace/exploration_tree.yaml",
            THEIRS_TREE.as_bytes(),
        );
        write_file(&artifact, "logic/claims.md", b"# Claims\n\n## C05: Bob finding\n- **Statement**: Bob traced N124.\n- **Status**: hypothesis\n- **Proof**: [trace/exploration_tree.yaml:N124]\n- **Dependencies**: []\n");
        write_file(&artifact, "staging/observations.yaml", b"observations:\n  - id: O01\n    timestamp: '2026-10-01T12:00'\n    provenance: user\n    content: Bob observed N124.\n    context: imported branch\n    potential_type: claim\n    bound_to: [N124]\n    promoted: false\n    promoted_to: null\n    crystallized_via: null\n    stale: false\n");
        write_file(&artifact, "trace/sessions/2026-10-01_001.yaml", b"session:\n  id: 2026-10-01_001\n  date: '2026-10-01'\n  started: '2026-10-01T12:00'\n  last_turn: '2026-10-01T12:00'\n  turn_count: 1\n  summary: Bob appended a branch\nevents_logged:\n  - turn: 1\n    type: question\n    id: N124\n    routing: direct\n    provenance: user\n    summary: Bob node\nai_actions:\n  - turn: 1\n    action: captured N124\n    provenance: ai-executed\n    files_changed: [trace/exploration_tree.yaml]\nclaims_touched:\n  - id: C05\n    action: created\n    turn: 1\nlogic_revisions:\n  - turn: 1\n    entry: C05\n    field: Statement\n    before: pending N124\n    after: Bob traced N124.\n    signal: user-directive\n    provenance: user\nkey_context:\n  - turn: 1\n    excerpt: Remember N124\nopen_threads: [follow Bob branch]\nai_suggestions_pending: [review Bob finding]\n");
        write_file(&artifact, "trace/sessions/session_index.yaml", b"sessions:\n  - id: 2026-10-01_001\n    date: '2026-10-01'\n    summary: Bob appended a branch\n    turn_count: 1\n    events_count: 1\n    claims_touched: [C05]\n    open_threads: 1\n");
        if let Some(bytes) = source_opaque {
            write_file(&artifact, "src/opaque.bin", bytes);
        }
        run_git(&root, &["add", "--all"]);
        run_git(&root, &["commit", "-m", "Bob committed source"]);
        let theirs = git_text(&root, &["rev-parse", "HEAD"]);
        run_git(&root, &["checkout", "ours"]);
        write_file(
            &artifact,
            "trace/exploration_tree.yaml",
            OURS_TREE.as_bytes(),
        );
        run_git(&root, &["add", "--all"]);
        run_git(&root, &["commit", "-m", "our branch"]);
        let ours = git_text(&root, &["rev-parse", "HEAD"]);
        Self {
            _owner: owner,
            root,
            artifact,
            relative: relative.into(),
            base,
            ours,
            theirs,
        }
    }
    fn dirty(&self) {
        write_file(
            &self.artifact,
            "logic/problem.md",
            b"# Problem\n\nStaged local notes.\n",
        );
        run_git(
            &self.root,
            &["add", "--", &format!("{}/logic/problem.md", self.relative)],
        );
        write_file(
            &self.artifact,
            "logic/problem.md",
            b"# Problem\n\nStaged local notes.\nUnstaged local notes.\n",
        );
        write_file(
            &self.artifact,
            "logic/local.md",
            b"# Local untracked document\n\nOurs only.\n",
        );
        write_file(&self.artifact, "src/opaque.bin", b"dirty ours code\0\xff\n");
    }
    fn merge(&self, extra: &[&str]) -> Output {
        ara_command(&self.artifact)
            .args([
                "merge",
                "--git",
                "bob",
                "--as",
                "bob",
                "--source-key",
                "bob-fork",
                "--json",
            ])
            .args(extra)
            .output()
            .unwrap()
    }
}

fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root).env_clear();
    for name in ["PATH", "SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("GIT_AUTHOR_DATE", "2026-10-01T12:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-10-01T12:00:00Z")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            &format!("core.hooksPath={null}"),
        ]);
    command
}
fn run_git(root: &Path, args: &[&str]) -> Output {
    let output = git_command(root)
        .args(args)
        .output()
        .expect("Git fixture executable");
    assert!(
        output.status.success(),
        "Git fixture {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn git_text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(run_git(root, args).stdout)
        .unwrap()
        .trim_end_matches('\n')
        .into()
}
fn ara_command(artifact: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ara"));
    command.arg("-C").arg(artifact).env_remove("ARA_DIR");
    command
}
fn write_file(root: &Path, path: &str, bytes: &[u8]) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn artifact_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, relative: &Path, rows: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".ara" || entry.file_name() == ".git" {
                continue;
            }
            let path = relative.join(entry.file_name());
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                visit(root, &path, rows);
            } else if kind.is_file() {
                rows.insert(path.clone(), fs::read(root.join(path)).unwrap());
            }
        }
    }
    let mut rows = BTreeMap::new();
    visit(root, Path::new(""), &mut rows);
    rows
}
fn git_state(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut rows = BTreeMap::new();
    rows.insert(
        "all refs".into(),
        run_git(root, &["show-ref", "--head"]).stdout,
    );
    for name in [
        "HEAD",
        "index",
        "MERGE_HEAD",
        "MERGE_MSG",
        "MERGE_MODE",
        "AUTO_MERGE",
        "ORIG_HEAD",
        "FETCH_HEAD",
        "CHERRY_PICK_HEAD",
        "REBASE_HEAD",
    ] {
        let path = git_text(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        );
        if let Ok(bytes) = fs::read(path) {
            rows.insert(name.into(), bytes);
        }
    }
    rows
}
fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn pinned_raw_inputs_preserve_dirty_disk_index_refs_and_modes() {
    let repo = Repository::new("research [artifact]");
    repo.dirty();
    let disk = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    assert_eq!(inputs.provenance.repo_relative_root, repo.relative);
    assert_eq!(inputs.provenance.head, repo.ours);
    assert_eq!(inputs.provenance.theirs, repo.theirs);
    assert_eq!(inputs.provenance.base, repo.base);
    assert_eq!(
        fs::read(inputs.base_dir.join("trace/exploration_tree.yaml")).unwrap(),
        BASE_TREE.as_bytes()
    );
    assert_eq!(
        fs::read(inputs.theirs_dir.join("trace/exploration_tree.yaml")).unwrap(),
        THEIRS_TREE.as_bytes()
    );
    assert_eq!(
        fs::read(inputs.theirs_dir.join("src/opaque.bin")).unwrap(),
        b"shared code\0\xff\n"
    );
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
    git::recheck_head(&inputs).unwrap();
    let storage = inputs.base_dir.parent().unwrap().to_owned();
    assert!(!storage.starts_with(&repo.root));
    drop(inputs);
    assert!(
        !storage.exists(),
        "RAII must remove both materialized trees"
    );
}

#[test]
fn binary_merge_preserves_dirty_ours_and_exact_git_state_and_replays() {
    let repo = Repository::new("ara");
    repo.dirty();
    let before = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let dry = repo.merge(&["--dry-run"]);
    assert_exit(&dry, 0);
    let report: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(report["format"], "ara.merge/v1");
    assert_eq!(report["git"]["head"], repo.ours);
    assert_eq!(report["git"]["theirs"], repo.theirs);
    assert_eq!(report["git"]["base"], repo.base);
    assert_eq!(report["git"]["repo_relative_root"], "ara");
    assert_eq!(artifact_bytes(&repo.artifact), before);
    assert_eq!(git_state(&repo.root), state);
    let merged = repo.merge(&[]);
    assert_exit(&merged, 0);
    let tree = fs::read_to_string(repo.artifact.join("trace/exploration_tree.yaml")).unwrap();
    let view = ara_core::parse_dir(&repo.artifact).expect("merged artifact validates");
    let manifest = view.0;
    let ours = manifest
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "N124")
        .unwrap();
    let bob = manifest
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "N125")
        .unwrap();
    assert_eq!(ours.label.as_deref(), Some("Our child"));
    assert_eq!(bob.label.as_deref(), Some("Bob child"));
    assert!(tree.contains("Our N124 prose must stay untouched."));
    assert!(tree.contains("opaque_extension: {nested: [one, two], enabled: true}"));
    assert!(manifest.links.iter().any(|link| link.from.as_str() == "N02"
        && link.to.as_str() == "N125"
        && link.kind == ara_core::LinkKind::Child));
    assert_eq!(
        fs::read(repo.artifact.join("logic/problem.md")).unwrap(),
        before[Path::new("logic/problem.md")]
    );
    assert_eq!(
        fs::read(repo.artifact.join("logic/local.md")).unwrap(),
        before[Path::new("logic/local.md")]
    );
    assert_eq!(
        fs::read(repo.artifact.join("src/opaque.bin")).unwrap(),
        before[Path::new("src/opaque.bin")]
    );
    let observations = fs::read_to_string(repo.artifact.join("staging/observations.yaml")).unwrap();
    assert!(observations.contains("bound_to: [N125]"));
    let session =
        fs::read_to_string(repo.artifact.join("trace/sessions/2026-10-01_001.yaml")).unwrap();
    assert!(session.contains("id: N125"));
    let archive = ara_core::write::positions::YamlDocument::parse(&session)
        .unwrap()
        .root
        .to_json()
        .unwrap();
    assert_eq!(archive["logic_revisions"][0]["before"], "pending N124");
    assert_eq!(archive["logic_revisions"][0]["after"], "Bob traced N124.");
    assert!(session.contains("excerpt: Remember N125"));
    assert!(session.contains("open_threads: [follow Bob branch]"));
    assert!(session.contains("ai_suggestions_pending: [review Bob finding]"));
    assert_eq!(git_state(&repo.root), state);
    let resolve = ara_command(&repo.artifact)
        .args(["show", "bob:N124", "--identity", "--json"])
        .output()
        .unwrap();
    assert_exit(&resolve, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&resolve.stdout).unwrap()["entries"][0]["resolved_target"],
        "N125"
    );
    for address in ["bob:src/opaque.bin", "bob:logic/vendor.yaml"] {
        let resolved = ara_command(&repo.artifact)
            .args(["show", address, "--identity", "--json"])
            .output()
            .unwrap();
        assert_exit(&resolved, 0);
        assert_eq!(
            serde_json::from_slice::<Value>(&resolved.stdout).unwrap()["entries"][0]["resolved_target"],
            address.strip_prefix("bob:").unwrap()
        );
    }
    let after = artifact_bytes(&repo.artifact);
    assert_exit(&repo.merge(&[]), 0);
    assert_eq!(artifact_bytes(&repo.artifact), after);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn source_ref_can_move_but_only_the_pinned_commit_is_consumed() {
    let repo = Repository::new("ara");
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    run_git(&repo.root, &["update-ref", "refs/heads/bob", &repo.base]);
    git::recheck_head(&inputs).unwrap();
    assert_eq!(inputs.provenance.theirs, repo.theirs);
    assert_eq!(
        fs::read(inputs.theirs_dir.join("trace/exploration_tree.yaml")).unwrap(),
        THEIRS_TREE.as_bytes()
    );
    let after_reset = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    let error = git::check_source_lineage(&after_reset, &inputs.provenance).unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_source_lineage", 1));
}

#[test]
fn head_movement_rejects_the_precommit_context_without_disk_changes() {
    let repo = Repository::new("ara");
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    let disk = artifact_bytes(&repo.artifact);
    run_git(&repo.root, &["update-ref", "HEAD", &repo.theirs]);
    let error = git::recheck_head(&inputs).unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_head_changed", 1));
    assert_eq!(artifact_bytes(&repo.artifact), disk);
}

#[test]
fn recorded_source_advancement_accepts_descendants_and_rejects_rebase() {
    let repo = Repository::new("ara");
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    git::check_source_advancement(&inputs, &repo.base).unwrap();
    git::check_source_advancement(&inputs, &repo.theirs).unwrap();
    let error = git::check_source_advancement(&inputs, &repo.ours).unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_source_lineage", 1));
    let mut previous = inputs.provenance.clone();
    previous.repo_relative_root = "renamed-artifact".into();
    assert_eq!(
        git::check_source_lineage(&inputs, &previous)
            .unwrap_err()
            .code,
        "git_source_lineage"
    );
}

#[test]
fn annotated_tags_detached_head_ancestor_and_identical_revisions_work() {
    let repo = Repository::new("ara");
    run_git(
        &repo.root,
        &[
            "tag",
            "-a",
            "bob-tag",
            &repo.theirs,
            "-m",
            "source snapshot",
        ],
    );
    let tag = git::prepare_git_inputs(&repo.artifact, "bob-tag").unwrap();
    assert_eq!(tag.provenance.theirs, repo.theirs);
    run_git(&repo.root, &["checkout", "--detach", &repo.ours]);
    repo.dirty();
    let disk = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let ancestor = git::prepare_git_inputs(&repo.artifact, &repo.base).unwrap();
    assert_eq!(ancestor.provenance.base, repo.base);
    let same = git::prepare_git_inputs(&repo.artifact, "HEAD").unwrap();
    assert_eq!(same.provenance.base, repo.ours);
    assert_eq!(same.provenance.theirs, repo.ours);
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn linked_worktree_uses_its_own_head_and_index() {
    let repo = Repository::new("ara");
    let checkout = repo._owner.path().join("linked checkout");
    run_git(
        &repo.root,
        &[
            "worktree",
            "add",
            "--detach",
            checkout.to_str().unwrap(),
            &repo.ours,
        ],
    );
    write_file(
        &checkout.join("ara"),
        "logic/problem.md",
        b"# Problem\n\nLinked dirty state.\n",
    );
    let original_state = git_state(&repo.root);
    let linked_state = git_state(&checkout);
    let inputs = git::prepare_git_inputs(&checkout.join("ara"), "bob").unwrap();
    assert_eq!(inputs.provenance.head, repo.ours);
    assert_eq!(inputs.provenance.theirs, repo.theirs);
    assert_eq!(git_state(&checkout), linked_state);
    assert_eq!(git_state(&repo.root), original_state);
    assert_eq!(
        fs::read(checkout.join("ara/logic/problem.md")).unwrap(),
        b"# Problem\n\nLinked dirty state.\n"
    );
}

#[test]
fn nul_tree_paths_preserve_newlines_unicode_spaces_and_pathspec_metacharacters() {
    let repo = Repository::new("research [a]*?");
    run_git(&repo.root, &["checkout", "bob"]);
    for name in [
        "logic/name with spaces.md",
        "logic/new\nline.md",
        "logic/évidence [*?].md",
    ] {
        write_file(&repo.artifact, name, b"exact committed bytes\0\xff\n");
    }
    write_file(
        &repo.root,
        "research [a]*?suffix/trace/exploration_tree.yaml",
        b"must not be included",
    );
    write_file(
        &repo.artifact,
        "src/executable.sh",
        b"#!/bin/sh\nprintf exact\n",
    );
    run_git(&repo.root, &["add", "--all"]);
    run_git(
        &repo.root,
        &[
            "update-index",
            "--chmod=+x",
            &format!("{}/src/executable.sh", repo.relative),
        ],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            repo.artifact.join("src/executable.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    run_git(&repo.root, &["commit", "-m", "unusual raw paths"]);
    run_git(&repo.root, &["checkout", "ours"]);
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    for name in [
        "logic/name with spaces.md",
        "logic/new\nline.md",
        "logic/évidence [*?].md",
    ] {
        assert_eq!(
            fs::read(inputs.theirs_dir.join(name)).unwrap(),
            b"exact committed bytes\0\xff\n"
        );
    }
    assert_eq!(
        fs::read(inputs.theirs_dir.join("trace/exploration_tree.yaml")).unwrap(),
        THEIRS_TREE.as_bytes()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(inputs.theirs_dir.join("src/executable.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::metadata(inputs.theirs_dir.join("logic/new\nline.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
}

#[test]
fn git_mode_rejects_shallow_history_without_mutating_worktree_or_index() {
    let repo = Repository::new("ara");
    let shallow = repo._owner.path().join("shallow");
    let url = format!("file://{}", repo.root.display());
    run_git(
        repo._owner.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "--depth=1",
            "--branch=ours",
            &url,
            shallow.to_str().unwrap(),
        ],
    );
    let disk = artifact_bytes(&shallow.join("ara"));
    let state = git_state(&shallow);
    let error = git::prepare_git_inputs(&shallow.join("ara"), "HEAD").unwrap_err();
    assert_eq!(
        (error.code.as_str(), error.exit),
        ("git_shallow_history", 1)
    );
    assert_eq!(artifact_bytes(&shallow.join("ara")), disk);
    assert_eq!(git_state(&shallow), state);
}

fn commit_tree(root: &Path, tree: &str, parents: &[&str], message: &str) -> String {
    let mut args = vec!["commit-tree", tree];
    for parent in parents {
        args.extend(["-p", parent]);
    }
    args.extend(["-m", message]);
    git_text(root, &args)
}

#[test]
fn criss_cross_bases_and_unrelated_histories_fail_closed() {
    let repo = Repository::new("ara");
    let tree = git_text(
        &repo.root,
        &["rev-parse", &format!("{}^{{tree}}", repo.base)],
    );
    let a = commit_tree(&repo.root, &tree, &[&repo.base], "A");
    let b = commit_tree(&repo.root, &tree, &[&repo.base], "B");
    let left = commit_tree(&repo.root, &tree, &[&a, &b], "left criss-cross");
    let right = commit_tree(&repo.root, &tree, &[&b, &a], "right criss-cross");
    run_git(&repo.root, &["update-ref", "HEAD", &left]);
    let disk = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let error = git::prepare_git_inputs(&repo.artifact, &right).unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_ambiguous_base", 1));
    let orphan = commit_tree(&repo.root, &tree, &[], "orphan");
    let error = git::prepare_git_inputs(&repo.artifact, &orphan).unwrap_err();
    assert_eq!(
        (error.code.as_str(), error.exit),
        ("git_unrelated_history", 1)
    );
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

fn unmerged_entry(repo: &Repository, path: &str) {
    let base = git_text(
        &repo.root,
        &[
            "rev-parse",
            &format!(
                "{}:{}/trace/exploration_tree.yaml",
                repo.base, repo.relative
            ),
        ],
    );
    let theirs = git_text(
        &repo.root,
        &[
            "rev-parse",
            &format!(
                "{}:{}/trace/exploration_tree.yaml",
                repo.theirs, repo.relative
            ),
        ],
    );
    let mut child = git_command(&repo.root)
        .args(["update-index", "--index-info"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    write!(
        child.stdin.take().unwrap(),
        "0 {}\t{path}\n100644 {base} 1\t{path}\n100644 {theirs} 3\t{path}\n",
        "0".repeat(base.len())
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn unmerged_index_inside_artifact_rejects_but_outside_does_not() {
    let repo = Repository::new("ara");
    unmerged_entry(&repo, "unrelated.txt");
    let state = git_state(&repo.root);
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    assert_eq!(inputs.provenance.theirs, repo.theirs);
    assert_eq!(git_state(&repo.root), state);
    unmerged_entry(&repo, "ara/trace/exploration_tree.yaml");
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    let error = git::prepare_git_inputs(&repo.artifact, "bob").unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_unmerged_index", 1));
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn missing_or_relocated_historical_root_never_discovers_another_artifact() {
    let repo = Repository::new("ara");
    run_git(&repo.root, &["checkout", "bob"]);
    run_git(&repo.root, &["mv", "ara", "elsewhere"]);
    run_git(&repo.root, &["commit", "-m", "move source root"]);
    run_git(&repo.root, &["checkout", "ours"]);
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    let error = git::prepare_git_inputs(&repo.artifact, "bob").unwrap_err();
    assert_eq!(
        (error.code.as_str(), error.exit),
        ("git_artifact_missing", 1)
    );
    assert!(error.message.contains("\"ara\""));
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[cfg(unix)]
#[test]
fn committed_symlink_and_gitlink_blobs_are_never_followed_or_initialized() {
    use std::os::unix::fs::symlink;
    let repo = Repository::new("ara");
    run_git(&repo.root, &["checkout", "bob"]);
    symlink("../../outside", repo.artifact.join("logic/link.md")).unwrap();
    run_git(&repo.root, &["add", "--all"]);
    run_git(&repo.root, &["commit", "-m", "symlink source"]);
    let symlink_commit = git_text(&repo.root, &["rev-parse", "HEAD"]);
    run_git(&repo.root, &["checkout", "ours"]);
    let error = git::prepare_git_inputs(&repo.artifact, &symlink_commit).unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_unsafe_mode", 1));
    run_git(
        &repo.root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},ara/logic/submodule", repo.theirs),
        ],
    );
    run_git(&repo.root, &["commit", "-m", "gitlink source"]);
    let error = git::prepare_git_inputs(&repo.artifact, "HEAD").unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_unsafe_mode", 1));
    assert!(!repo.artifact.join("logic/submodule/.git").exists());
}

#[test]
fn invalid_revision_blob_revision_outside_and_unborn_repositories_are_operational_errors() {
    let repo = Repository::new("ara");
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    for reference in ["not-a-local-ref", "--help"] {
        assert_eq!(
            git::prepare_git_inputs(&repo.artifact, reference)
                .unwrap_err()
                .exit,
            2
        );
    }
    let blob = git_text(
        &repo.root,
        &[
            "rev-parse",
            &format!("{}:ara/trace/exploration_tree.yaml", repo.theirs),
        ],
    );
    assert_eq!(
        git::prepare_git_inputs(&repo.artifact, &blob)
            .unwrap_err()
            .exit,
        2
    );
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
    let outside = TempDir::new().unwrap();
    assert_eq!(
        git::prepare_git_inputs(outside.path(), "HEAD")
            .unwrap_err()
            .exit,
        2
    );
    let unborn = TempDir::new().unwrap();
    run_git(unborn.path(), &["init", "--initial-branch=empty"]);
    assert_eq!(
        git::prepare_git_inputs(unborn.path(), "HEAD")
            .unwrap_err()
            .exit,
        2
    );
    let bare = TempDir::new().unwrap();
    run_git(bare.path(), &["init", "--bare"]);
    assert_eq!(
        git::prepare_git_inputs(bare.path(), "HEAD")
            .unwrap_err()
            .code,
        "git_worktree_required"
    );
}

#[cfg(unix)]
fn executable(path: &Path, source: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn unsupported_or_missing_git_is_rejected_before_git_state_or_artifact_mutation() {
    let repo = Repository::new("ara");
    let tools = TempDir::new().unwrap();
    executable(
        &tools.path().join("git"),
        "#!/bin/sh\nprintf 'git version 2.44.0\\n'\n",
    );
    let disk = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--source-key",
            "bob-fork",
            "--json",
        ])
        .env("PATH", tools.path())
        .output()
        .unwrap();
    assert_exit(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("git_version_unsupported"));
    fs::remove_file(tools.path().join("git")).unwrap();
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--source-key",
            "bob-fork",
            "--json",
        ])
        .env("PATH", tools.path())
        .output()
        .unwrap();
    assert_exit(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("git_unavailable"));
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[cfg(unix)]
#[test]
fn missing_promisor_blob_cannot_invoke_fetch_helpers_or_leave_temporary_inputs() {
    let repo = Repository::new("ara");
    let tools = TempDir::new().unwrap();
    let temporary = TempDir::new().unwrap();
    let marker = tools.path().join("REMOTE_WAS_RUN");
    let helper = tools.path().join("git-remote-sentinel");
    executable(
        &helper,
        &format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexit 99\n",
            marker.display()
        ),
    );
    run_git(&repo.root, &["config", "core.repositoryformatversion", "1"]);
    run_git(&repo.root, &["config", "extensions.partialClone", "origin"]);
    run_git(&repo.root, &["config", "remote.origin.promisor", "true"]);
    run_git(
        &repo.root,
        &["config", "remote.origin.partialCloneFilter", "blob:none"],
    );
    run_git(
        &repo.root,
        &["config", "remote.origin.url", "sentinel::forbidden"],
    );
    run_git(&repo.root, &["config", "protocol.sentinel.allow", "always"]);
    let blob = git_text(
        &repo.root,
        &["rev-parse", &format!("{}:ara/src/opaque.bin", repo.theirs)],
    );
    fs::remove_file(
        repo.root
            .join(".git/objects")
            .join(&blob[..2])
            .join(&blob[2..]),
    )
    .unwrap();
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    let path = std::env::join_paths(
        std::iter::once(tools.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--source-key",
            "bob-fork",
            "--json",
        ])
        .env("PATH", path)
        .env("TMPDIR", temporary.path())
        .output()
        .unwrap();
    assert_exit(&output, 2);
    assert!(!marker.exists(), "No lazy-fetch remote helper may execute");
    assert!(String::from_utf8_lossy(&output.stderr).contains("git_object_unavailable"));
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[cfg(unix)]
#[test]
fn repository_helpers_filters_hooks_and_inherited_git_redirects_are_inert() {
    let repo = Repository::new("ara");
    repo.dirty();
    let tools = TempDir::new().unwrap();
    let marker = tools.path().join("HELPER_WAS_RUN");
    let helper = tools.path().join("forbidden-helper");
    executable(
        &helper,
        &format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexit 99\n",
            marker.display()
        ),
    );
    for (key, value) in [
        ("core.fsmonitor", helper.to_str().unwrap()),
        ("credential.helper", helper.to_str().unwrap()),
        ("filter.forbidden.clean", helper.to_str().unwrap()),
        ("filter.forbidden.smudge", helper.to_str().unwrap()),
        ("filter.forbidden.process", helper.to_str().unwrap()),
        ("filter.forbidden.required", "true"),
        ("core.hooksPath", tools.path().to_str().unwrap()),
    ] {
        run_git(&repo.root, &["config", key, value]);
    }
    executable(
        &tools.path().join("post-checkout"),
        &format!("#!/bin/sh\nprintf hook > '{}'\nexit 99\n", marker.display()),
    );
    write_file(&repo.root, ".git/info/attributes", b"* filter=forbidden\n");
    let wrong = TempDir::new().unwrap();
    run_git(wrong.path(), &["init", "--initial-branch=wrong"]);
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--as",
            "bob",
            "--source-key",
            "bob-fork",
            "--dry-run",
            "--json",
        ])
        .env("GIT_DIR", wrong.path().join(".git"))
        .env("GIT_WORK_TREE", wrong.path())
        .env("GIT_INDEX_FILE", wrong.path().join("redirected-index"))
        .env("GIT_OBJECT_DIRECTORY", wrong.path().join("missing-objects"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_0", &helper)
        .output()
        .unwrap();
    assert_exit(&output, 0);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["git"]["head"], repo.ours);
    assert_eq!(report["git"]["theirs"], repo.theirs);
    assert!(
        !marker.exists(),
        "No fsmonitor/filter/hook/credential helper may execute"
    );
    assert!(!wrong.path().join("redirected-index").exists());
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn directory_and_git_modes_are_mutually_exclusive_before_loading_inputs() {
    let repo = Repository::new("ara");
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--base",
            "unavailable",
            "--theirs",
            "unavailable",
            "--json",
        ])
        .output()
        .unwrap();
    assert_exit(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}

fn normalized_merge_metadata(mut value: Value) -> Value {
    if let Value::Object(object) = &mut value {
        object.remove("git_timings");
        object.remove("timings");
    }
    fn visit(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("git");
                object.remove("time");
                for value in object.values_mut() {
                    visit(value);
                }
            }
            Value::Array(rows) => {
                for row in rows {
                    visit(row);
                }
            }
            _ => {}
        }
    }
    visit(&mut value);
    value
}

#[test]
fn git_and_directory_modes_have_identical_content_mappings_and_conflict_evidence() {
    let base_claim = b"# Claims\n\n## C05: Bob finding\n- **Statement**: Shared baseline.\n- **Status**: hypothesis\n- **Proof**: []\n- **Dependencies**: []\n";
    for conflicting in [false, true] {
        let repo = if conflicting {
            Repository::with_base_claim("ara", base_claim)
        } else {
            Repository::new("ara")
        };
        repo.dirty();
        if conflicting {
            let claims = String::from_utf8(base_claim.to_vec())
                .unwrap()
                .replace("Shared baseline.", "Our divergent statement.");
            write_file(&repo.artifact, "logic/claims.md", claims.as_bytes());
        }
        let directory = TempDir::new().unwrap();
        for (path, bytes) in artifact_bytes(&repo.artifact) {
            write_file(directory.path(), path.to_str().unwrap(), &bytes);
        }
        let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
        let directory_merge = ara_command(directory.path())
            .args(["merge", "--base"])
            .arg(&inputs.base_dir)
            .arg("--theirs")
            .arg(&inputs.theirs_dir)
            .args(["--as", "bob", "--source-key", "bob-fork", "--json"])
            .output()
            .unwrap();
        let git_merge = repo.merge(&[]);
        let expected_exit = i32::from(conflicting);
        assert_exit(&directory_merge, expected_exit);
        assert_exit(&git_merge, expected_exit);
        let directory_report: Value = serde_json::from_slice(&directory_merge.stdout).unwrap();
        let git_report: Value = serde_json::from_slice(&git_merge.stdout).unwrap();
        assert_eq!(
            normalized_merge_metadata(directory_report),
            normalized_merge_metadata(git_report)
        );
        let directory_files = artifact_bytes(directory.path());
        let git_files = artifact_bytes(&repo.artifact);
        assert_eq!(
            directory_files.keys().collect::<Vec<_>>(),
            git_files.keys().collect::<Vec<_>>()
        );
        for (path, bytes) in git_files {
            let directory_bytes = &directory_files[&path];
            if path == Path::new("trace/merge_log.yaml") {
                let left: Value =
                    serde_saphyr::from_str(std::str::from_utf8(directory_bytes).unwrap()).unwrap();
                let right: Value =
                    serde_saphyr::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
                // Keep the full source inventory, every import mapping,
                // predecessor, and exact conflict candidate. Only metadata
                // recording transport/time may differ.
                assert_eq!(
                    normalized_merge_metadata(left),
                    normalized_merge_metadata(right)
                );
            } else {
                assert_eq!(
                    &bytes,
                    directory_bytes,
                    "content decision differs at {}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn advancing_committed_source_reuses_imports_and_reset_rejects_without_mutation() {
    let repo = Repository::new("ara");
    assert_exit(&repo.merge(&[]), 0);
    let first_resolve = ara_command(&repo.artifact)
        .args(["show", "bob:C05", "--identity", "--json"])
        .output()
        .unwrap();
    assert_exit(&first_resolve, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&first_resolve.stdout).unwrap()["entries"][0]["resolved_target"],
        "C01"
    );
    let source = repo._owner.path().join("Bob checkout");
    run_git(
        &repo.root,
        &["worktree", "add", source.to_str().unwrap(), "bob"],
    );
    let claims = fs::read_to_string(source.join("ara/logic/claims.md")).unwrap();
    write_file(
        &source.join("ara"),
        "logic/claims.md",
        claims
            .replace("Bob traced N124.", "Bob refined N124.")
            .as_bytes(),
    );
    run_git(&source, &["add", "--all"]);
    run_git(&source, &["commit", "-m", "advance Bob's mutable claim"]);
    let advanced = git_text(&source, &["rev-parse", "HEAD"]);
    let state = git_state(&repo.root);
    let output = repo.merge(&[]);
    assert_exit(&output, 0);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["git"]["theirs"], advanced);
    assert!(
        fs::read_to_string(repo.artifact.join("logic/claims.md"))
            .unwrap()
            .contains("Bob refined N125."),
        "actual claim source: {}\nreport: {report}",
        fs::read_to_string(repo.artifact.join("logic/claims.md")).unwrap()
    );
    let resolve = ara_command(&repo.artifact)
        .args(["show", "bob:N124", "--identity", "--json"])
        .output()
        .unwrap();
    assert_exit(&resolve, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&resolve.stdout).unwrap()["entries"][0]["resolved_target"],
        "N125"
    );
    let claim_resolve = ara_command(&repo.artifact)
        .args(["show", "bob:C05", "--identity", "--json"])
        .output()
        .unwrap();
    assert_exit(&claim_resolve, 0);
    assert_eq!(claim_resolve.stdout, first_resolve.stdout);
    assert_eq!(git_state(&repo.root), state);
    let disk = artifact_bytes(&repo.artifact);
    run_git(&repo.root, &["update-ref", "refs/heads/bob", &repo.base]);
    let state = git_state(&repo.root);
    let rejected = repo.merge(&[]);
    assert_exit(&rejected, 1);
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("git_source_lineage"));
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn conflicting_git_revision_replays_exact_candidates_and_cleans_private_inputs() {
    let base_claim = b"# Claims\n\n## C05: Bob finding\n- **Statement**: Shared baseline.\n- **Status**: hypothesis\n- **Proof**: []\n- **Dependencies**: []\n";
    let repo = Repository::with_base_claim("ara", base_claim);
    let ours = String::from_utf8(base_claim.to_vec())
        .unwrap()
        .replace("Shared baseline.", "Our divergent statement.");
    write_file(&repo.artifact, "logic/claims.md", ours.as_bytes());
    let state = git_state(&repo.root);
    let temporary = TempDir::new().unwrap();
    let invoke = || {
        ara_command(&repo.artifact)
            .args([
                "merge",
                "--git",
                "bob",
                "--as",
                "bob",
                "--source-key",
                "bob-fork",
                "--json",
            ])
            .env("TMPDIR", temporary.path())
            .output()
            .unwrap()
    };
    let first = invoke();
    assert_exit(&first, 1);
    let report: Value = serde_json::from_slice(&first.stdout).unwrap();
    let conflict = report["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|conflict| {
            conflict["field"]
                .as_str()
                .is_some_and(|field| field.eq_ignore_ascii_case("Statement"))
        })
        .unwrap();
    for (side, expected) in [
        ("base", "Shared baseline."),
        ("ours", "Our divergent statement."),
        ("theirs", "Bob traced N124."),
    ] {
        let bytes: Vec<u8> = serde_json::from_value(conflict[side]["bytes"].clone()).unwrap();
        assert!(
            String::from_utf8(bytes).unwrap().contains(expected),
            "missing exact {side} conflict evidence"
        );
    }
    assert!(
        fs::read_to_string(repo.artifact.join("logic/claims.md"))
            .unwrap()
            .contains("Our divergent statement.")
    );
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    let disk = artifact_bytes(&repo.artifact);
    let replay = invoke();
    assert_exit(&replay, 1);
    let replay_report: Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(report["conflicts"], replay_report["conflicts"]);
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[test]
fn malformed_committed_yaml_is_rejected_and_both_temporary_trees_are_removed() {
    let repo = Repository::new("ara");
    run_git(&repo.root, &["checkout", "bob"]);
    write_file(
        &repo.artifact,
        "trace/exploration_tree.yaml",
        b"tree: [unterminated\n",
    );
    run_git(&repo.root, &["add", "--all"]);
    run_git(&repo.root, &["commit", "-m", "invalid committed tree"]);
    run_git(&repo.root, &["checkout", "ours"]);
    let disk = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let temporary = TempDir::new().unwrap();
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--source-key",
            "bob-fork",
            "--json",
        ])
        .env("TMPDIR", temporary.path())
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[test]
fn raw_large_binary_blobs_keep_length_framing_and_all_content_bytes() {
    let repo = Repository::new("ara");
    run_git(&repo.root, &["checkout", "bob"]);
    let payload: Vec<u8> = (0..2 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect();
    write_file(&repo.artifact, "src/framed.bin", &payload);
    let lfs_pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 123456789\n";
    write_file(&repo.artifact, "src/lfs-pointer.bin", lfs_pointer);
    run_git(&repo.root, &["add", "--all"]);
    run_git(&repo.root, &["commit", "-m", "large raw framed blob"]);
    run_git(&repo.root, &["checkout", "ours"]);
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    assert_eq!(
        fs::read(inputs.theirs_dir.join("src/framed.bin")).unwrap(),
        payload
    );
    assert_eq!(
        fs::read(inputs.theirs_dir.join("src/lfs-pointer.bin")).unwrap(),
        lfs_pointer
    );
}

fn mktree(root: &Path, rows: &str) -> String {
    use std::io::Write;
    let mut child = git_command(root)
        .arg("mktree")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(rows.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end_matches('\n')
        .into()
}

#[test]
fn host_case_and_unicode_normalization_collisions_are_checked_for_directories() {
    let repo = Repository::new("ara");
    let blob = git_text(
        &repo.root,
        &[
            "rev-parse",
            &format!("{}:ara/trace/exploration_tree.yaml", repo.base),
        ],
    );
    let trace = git_text(
        &repo.root,
        &["rev-parse", &format!("{}:ara/trace", repo.base)],
    );
    let one = mktree(&repo.root, &format!("100644 blob {blob}\tone.md\n"));
    let two = mktree(&repo.root, &format!("100644 blob {blob}\ttwo.md\n"));
    for (first, second) in [("A", "a"), ("é", "e\u{301}")] {
        let logic = mktree(
            &repo.root,
            &format!("040000 tree {one}\t{first}\n040000 tree {two}\t{second}\n"),
        );
        let artifact = mktree(
            &repo.root,
            &format!("040000 tree {logic}\tlogic\n040000 tree {trace}\ttrace\n"),
        );
        let root = mktree(&repo.root, &format!("040000 tree {artifact}\tara\n"));
        let commit = commit_tree(
            &repo.root,
            &root,
            &[&repo.base],
            "host collision candidates",
        );
        let probe = TempDir::new().unwrap();
        fs::create_dir(probe.path().join(first)).unwrap();
        let aliases = fs::create_dir(probe.path().join(second)).is_err();
        let result = git::prepare_git_inputs(&repo.artifact, &commit);
        if aliases {
            let error = result.unwrap_err();
            assert_eq!((error.code.as_str(), error.exit), ("git_path_collision", 1));
        } else {
            let inputs = result.unwrap();
            assert_eq!(
                fs::read(inputs.theirs_dir.join(format!("logic/{first}/one.md"))).unwrap(),
                BASE_TREE.as_bytes()
            );
            assert_eq!(
                fs::read(inputs.theirs_dir.join(format!("logic/{second}/two.md"))).unwrap(),
                BASE_TREE.as_bytes()
            );
        }
    }
}

#[test]
fn raw_reserved_tree_path_is_rejected_before_any_escaped_write() {
    let repo = Repository::new("ara");
    let blob = git_text(
        &repo.root,
        &[
            "rev-parse",
            &format!("{}:ara/trace/exploration_tree.yaml", repo.base),
        ],
    );
    let trace = git_text(
        &repo.root,
        &["rev-parse", &format!("{}:ara/trace", repo.base)],
    );
    let artifact = mktree(
        &repo.root,
        &format!("100644 blob {blob}\t.git\n040000 tree {trace}\ttrace\n"),
    );
    let root = mktree(&repo.root, &format!("040000 tree {artifact}\tara\n"));
    let commit = commit_tree(&repo.root, &root, &[&repo.base], "reserved historical path");
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    let error = git::prepare_git_inputs(&repo.artifact, &commit).unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_unsafe_path", 1));
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn nested_submodule_artifact_cannot_be_used_as_an_implicit_repository() {
    let repo = Repository::new("ara");
    let outer = TempDir::new().unwrap();
    run_git(outer.path(), &["init", "--initial-branch=outer"]);
    run_git(
        outer.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            repo.root.to_str().unwrap(),
            "nested",
        ],
    );
    let error = git::prepare_git_inputs(&outer.path().join("nested/ara"), "HEAD").unwrap_err();
    assert_eq!((error.code.as_str(), error.exit), ("git_submodule", 1));
}

#[cfg(unix)]
#[test]
fn truncated_blob_child_is_reaped_and_materialization_is_cleaned_on_failure() {
    let repo = Repository::new("ara");
    let tools = TempDir::new().unwrap();
    let temporary = TempDir::new().unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let pid_file = tools.path().join("child.pid");
    executable(
        &tools.path().join("git"),
        &format!(
            "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = cat-file ]; then\n    printf '%s\\n' \"$$\" > '{}'\n    IFS= read -r oid\n    printf '%s blob 20\\nshort' \"$oid\"\n    exit 99\n  fi\ndone\nexec '{}' \"$@\"\n",
            pid_file.display(),
            real_git.display()
        ),
    );
    let path = std::env::join_paths(
        std::iter::once(tools.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let state = git_state(&repo.root);
    let disk = artifact_bytes(&repo.artifact);
    let output = ara_command(&repo.artifact)
        .args([
            "merge",
            "--git",
            "bob",
            "--source-key",
            "bob-fork",
            "--json",
        ])
        .env("PATH", path)
        .env("TMPDIR", temporary.path())
        .output()
        .unwrap();
    assert_exit(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("git_object_unavailable"));
    let pid = fs::read_to_string(pid_file).unwrap();
    let alive = Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(
        !alive.success(),
        "catchable materialization failure must reap its blob child"
    );
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    assert_eq!(artifact_bytes(&repo.artifact), disk);
    assert_eq!(git_state(&repo.root), state);
}

#[test]
fn only_exact_shared_writer_temporary_paths_are_excluded_from_git_inputs() {
    let repo = Repository::new("ara");
    run_git(&repo.root, &["checkout", "bob"]);
    write_file(
        &repo.artifact,
        "logic/.ara-write-123-456",
        b"private staged bytes\n",
    );
    write_file(
        &repo.artifact,
        "logic/.ara-write-99-100/descendant.bin",
        b"private subtree bytes\n",
    );
    write_file(
        &repo.artifact,
        "logic/.ara-write-123-456-extra",
        b"not a writer temporary file\n",
    );
    write_file(
        &repo.artifact,
        "logic/.ara-write-123-x",
        b"also not a writer temporary file\n",
    );
    run_git(&repo.root, &["add", "--all"]);
    run_git(
        &repo.root,
        &["commit", "-m", "tracked temporary and lookalike names"],
    );
    let private_blob = git_text(
        &repo.root,
        &["rev-parse", "bob:ara/logic/.ara-write-123-456"],
    );
    run_git(&repo.root, &["checkout", "ours"]);
    // This excluded blob is deliberately unavailable. Materialization must not
    // request it, and must not broaden the exact exclusion to prefix lookalikes.
    fs::remove_file(
        repo.root
            .join(".git/objects")
            .join(&private_blob[..2])
            .join(&private_blob[2..]),
    )
    .unwrap();
    let inputs = git::prepare_git_inputs(&repo.artifact, "bob").unwrap();
    assert!(!inputs.theirs_dir.join("logic/.ara-write-123-456").exists());
    assert!(!inputs.theirs_dir.join("logic/.ara-write-99-100").exists());
    assert_eq!(
        fs::read(inputs.theirs_dir.join("logic/.ara-write-123-456-extra")).unwrap(),
        b"not a writer temporary file\n"
    );
    assert_eq!(
        fs::read(inputs.theirs_dir.join("logic/.ara-write-123-x")).unwrap(),
        b"also not a writer temporary file\n"
    );
}

#[test]
fn incoming_opaque_change_conflicts_with_dirty_ours_without_touching_git_state() {
    let incoming = b"incoming opaque code\0\xff\n";
    let repo = Repository::with_source_opaque("ara", b"# Claims\n", Some(incoming));
    repo.dirty();
    let before = artifact_bytes(&repo.artifact);
    let state = git_state(&repo.root);
    let output = repo.merge(&[]);
    assert_exit(&output, 1);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let conflict = report["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["path"] == "src/opaque.bin")
        .unwrap();
    let bytes =
        |side: &str| serde_json::from_value::<Vec<u8>>(conflict[side]["bytes"].clone()).unwrap();
    assert_eq!(bytes("base"), b"shared code\0\xff\n");
    assert_eq!(bytes("ours"), before[Path::new("src/opaque.bin")]);
    assert_eq!(bytes("theirs"), incoming);
    assert_eq!(
        fs::read(repo.artifact.join("src/opaque.bin")).unwrap(),
        before[Path::new("src/opaque.bin")]
    );
    assert_eq!(git_state(&repo.root), state);
    let merged = artifact_bytes(&repo.artifact);
    let replay = repo.merge(&[]);
    assert_exit(&replay, 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&replay.stdout).unwrap()["conflicts"],
        report["conflicts"]
    );
    assert_eq!(artifact_bytes(&repo.artifact), merged);
}
