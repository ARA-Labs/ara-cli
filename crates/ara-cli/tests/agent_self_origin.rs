//! Plan 04 canonical -> worker -> canonical round trip through the real binary,
//! in directory and local-Git mode. Each destination names its own `--self-key`.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: root\n    children: []\n";
const CLAIMS: &str = "# Claims\n\n## C01: shared\n- **Statement**: base\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";

fn put(root: &Path, path: &str, bytes: &[u8]) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}
fn seed(root: &Path) {
    // Every CLI write adds this rule; forks share it from the seed.
    put(root, ".gitignore", b".ara/\n");
    put(root, "trace/exploration_tree.yaml", TREE.as_bytes());
    put(root, "logic/claims.md", CLAIMS.as_bytes());
}
fn claims(root: &Path) -> String {
    fs::read_to_string(root.join("logic/claims.md")).unwrap()
}
fn add_claim(root: &Path, id: &str, title: &str, statement: &str) {
    let text = claims(root)
        + &format!(
            "\n## {id}: {title}\n- **Statement**: {statement}\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n"
        );
    put(root, "logic/claims.md", text.as_bytes());
}
/// Edits go through the CLI so each checkout's write journal stays consistent.
fn set_statement(root: &Path, id: &str, statement: &str) {
    run(
        root,
        &["edit", id, "--set", &format!("Statement={statement}")],
    );
}
fn section(root: &Path, heading: &str) -> String {
    let text = claims(root);
    let start = text.find(heading).unwrap();
    let rest = &text[start + heading.len()..];
    rest[..rest.find("\n## ").unwrap_or(rest.len())].to_owned()
}
fn headings(root: &Path) -> Vec<String> {
    claims(root)
        .lines()
        .filter(|line| line.starts_with("## "))
        .map(str::to_owned)
        .collect()
}
fn frozen(root: &Path) -> BTreeMap<String, Vec<u8>> {
    ara_core::write::ArtifactSnapshot::load_complete(root)
        .unwrap()
        .files
        .into_iter()
        .filter(|(_, file)| file.existed)
        .map(|(path, file)| (path, file.bytes))
        .collect()
}
fn copy_tree(from: &Path, to: &Path) {
    for (path, bytes) in frozen(from) {
        put(to, &path, &bytes);
    }
}
fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .env_remove("ARA_DIR")
        .env("ARA_NO_DUPLICATE_CHECK", "1")
        .arg("-C")
        .arg(root);
    command
}
fn run(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}
fn import(ours: &Path, base: &Path, theirs: &Path, key: &str, own: &str) -> Value {
    run(
        ours,
        &[
            "merge",
            "--base",
            base.to_str().unwrap(),
            "--theirs",
            theirs.to_str().unwrap(),
            "--as",
            key,
            "--source-key",
            key,
            "--self-key",
            own,
        ],
    )
}
fn resolved(root: &Path, address: &str) -> String {
    run(root, &["resolve", address])["id"]
        .as_str()
        .unwrap()
        .into()
}
fn record_kinds(root: &Path, key: &str) -> Vec<String> {
    fs::read_to_string(root.join("trace/merge_log.yaml"))
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("  - "))
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .filter(|row| row["source_key"] == key)
        .map(|row| row["kind"].as_str().unwrap().to_owned())
        .collect()
}
const WORKER: [&str; 4] = [
    "## C01: shared",
    "## C77: fork B finding",
    "## C78: fork A finding",
    "## C79: canonical synthesis",
];
const CANONICAL: [&str; 4] = [
    "## C01: shared",
    "## C02: fork B finding",
    "## C03: fork A finding",
    "## C04: canonical synthesis",
];
fn canonical_authors(root: &Path) {
    let added = run(
        root,
        &[
            "claim",
            "add",
            "--title",
            "canonical synthesis",
            "--set",
            "Statement=PM combines A and B",
            "--set",
            "Status=hypothesis",
            "--set",
            "Provenance=user",
            "--set",
            "Conditions=none",
            "--set",
            "Falsification criteria=none",
            "--set",
            "Proof=[]",
            "--set",
            "Dependencies=[]",
            "--set",
            "Tags=[]",
        ],
    );
    assert_eq!(added["id"], "C04");
}
fn worker_checks(worker: &Path) {
    assert_eq!(headings(worker), WORKER);
    assert!(section(worker, "## C77: fork B finding").contains("B says y"));
    for (address, expected) in [
        ("canonical:C02", "C77"),
        ("canonical:C03", "C78"),
        ("canonical:C04", "C79"),
        ("fork-a:C77", "C78"),
    ] {
        assert_eq!(resolved(worker, address), expected, "{address}");
    }
    assert_eq!(record_kinds(worker, "fork-b"), ["self_identity"]);
}
fn canonical_checks(canonical: &Path) {
    assert_eq!(headings(canonical), CANONICAL);
    assert!(section(canonical, "## C04: canonical synthesis").contains("B agrees"));
    assert!(section(canonical, "## C02: fork B finding").contains("PM refined B"));
    for (address, expected) in [
        ("fork-b:C77", "C02"),
        ("fork-b:C78", "C03"),
        ("fork-b:C79", "C04"),
        ("fork-a:C77", "C03"),
    ] {
        assert_eq!(resolved(canonical, address), expected, "{address}");
    }
    assert_eq!(record_kinds(canonical, "canonical"), ["self_identity"]);
}

struct Directory {
    _owner: TempDir,
    canonical: PathBuf,
}
fn directory_round_trip() -> Directory {
    let owner = TempDir::new().unwrap();
    let path = |name: &str| owner.path().join(name);
    let (seed_dir, a1, b1, worker, canonical) = (
        path("seed"),
        path("A1"),
        path("B1"),
        path("B"),
        path("canonical"),
    );
    seed(&seed_dir);
    copy_tree(&seed_dir, &a1);
    add_claim(&a1, "C77", "fork A finding", "A says x");
    copy_tree(&seed_dir, &b1);
    add_claim(&b1, "C77", "fork B finding", "B says y");
    copy_tree(&b1, &worker);
    copy_tree(&seed_dir, &canonical);
    import(&canonical, &seed_dir, &b1, "fork-b", "canonical");
    import(&canonical, &seed_dir, &a1, "fork-a", "canonical");
    canonical_authors(&canonical);
    let c2 = path("C2");
    copy_tree(&canonical, &c2);

    // 1. B absorbs canonical: its own C77 comes back as canonical's C02.
    let report = import(&worker, &seed_dir, &c2, "canonical", "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    worker_checks(&worker);
    let worker_before = frozen(&worker);
    let replay = import(&worker, &seed_dir, &c2, "canonical", "fork-b");
    assert_eq!(replay["changed_paths"], json!([]));
    assert_eq!(frozen(&worker), worker_before);
    // A different self key is refused without touching the worker.
    let output = ara(&worker)
        .args([
            "merge",
            "--base",
            seed_dir.to_str().unwrap(),
            "--theirs",
            c2.to_str().unwrap(),
            "--as",
            "canonical",
            "--source-key",
            "canonical",
            "--self-key",
            "fork-x",
            "--json",
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("merge.self_identity_conflict"), "{text}");
    assert_eq!(frozen(&worker), worker_before);

    // 2. Canonical refines B's claim; with C2 as trusted base it applies to C77.
    set_statement(&canonical, "C02", "PM refined B");
    let report = import(&worker, &c2, &canonical, "canonical", "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    assert!(section(&worker, "## C77: fork B finding").contains("PM refined B"));
    assert_eq!(headings(&worker), WORKER);

    // 3. Canonical receives B back: its own C04 returns without a duplicate.
    let b2 = path("B2");
    copy_tree(&worker, &b2);
    let report = import(&canonical, &b1, &b2, "fork-b", "canonical");
    assert_eq!(report["unresolved_count"], 0);
    assert_eq!(headings(&canonical), CANONICAL);

    // 4. B's later edit of canonical's claim has B2 as its trusted base.
    set_statement(&worker, "C79", "PM combines A and B; B agrees");
    let report = import(&canonical, &b2, &worker, "fork-b", "canonical");
    assert_eq!(report["unresolved_count"], 0);
    canonical_checks(&canonical);
    let canonical_before = frozen(&canonical);
    let replay = import(&canonical, &b2, &worker, "fork-b", "canonical");
    assert_eq!(replay["changed_paths"], json!([]));
    assert_eq!(frozen(&canonical), canonical_before);
    Directory {
        _owner: owner,
        canonical,
    }
}

#[test]
fn directory_canonical_worker_canonical_round_trip_has_no_duplicates() {
    directory_round_trip();
}

fn git(root: &Path, args: &[&str]) {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = std::process::Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_DATE", "2026-10-01T12:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-10-01T12:00:00Z")
        .args(["-c", &format!("core.hooksPath={null}")])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn git_import(root: &Path, reference: &str, key: &str, own: &str) -> Value {
    let report = run(
        root,
        &[
            "merge",
            "--git",
            reference,
            "--as",
            key,
            "--source-key",
            key,
            "--self-key",
            own,
        ],
    );
    git(root, &["add", "--all"]);
    git(
        root,
        &["commit", "-q", "-m", &format!("import {reference}")],
    );
    report
}

#[test]
fn local_git_round_trip_matches_directory_mode() {
    let directory = directory_round_trip();
    let owner = TempDir::new().unwrap();
    let root = &owner.path().join("canonical");
    let worker = &owner.path().join("fork-b-worktree");
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "--initial-branch=seed"]);
    git(root, &["config", "user.name", "ARA fixture"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    seed(root);
    git(root, &["add", "--all"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    for (branch, title, statement) in [
        ("fork-a", "fork A finding", "A says x"),
        ("fork-b", "fork B finding", "B says y"),
    ] {
        git(root, &["checkout", "-q", "-b", branch, "seed"]);
        add_claim(root, "C77", title, statement);
        git(root, &["commit", "-q", "-am", branch]);
    }
    git(root, &["checkout", "-q", "-b", "canonical", "seed"]);
    git(
        root,
        &["worktree", "add", "-q", worker.to_str().unwrap(), "fork-b"],
    );
    git_import(root, "fork-b", "fork-b", "canonical");
    git_import(root, "fork-a", "fork-a", "canonical");
    canonical_authors(root);
    git(root, &["commit", "-q", "-am", "canonical synthesis"]);

    let report = git_import(worker, "canonical", "canonical", "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    assert!(report["git"]["theirs"].is_string());
    worker_checks(worker);
    set_statement(root, "C02", "PM refined B");
    git(root, &["commit", "-q", "-am", "canonical refines B"]);
    let report = git_import(worker, "canonical", "canonical", "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    assert!(section(worker, "## C77: fork B finding").contains("PM refined B"));

    assert_eq!(
        git_import(root, "fork-b", "fork-b", "canonical")["unresolved_count"],
        0
    );
    assert_eq!(headings(root), CANONICAL);
    set_statement(worker, "C79", "PM combines A and B; B agrees");
    git(worker, &["commit", "-q", "-am", "worker agrees"]);
    let report = git_import(root, "fork-b", "fork-b", "canonical");
    assert_eq!(report["unresolved_count"], 0);
    canonical_checks(root);
    assert_eq!(claims(root), claims(&directory.canonical));
    for address in ["fork-b:C77", "fork-b:C79", "fork-a:C77", "canonical:C04"] {
        assert_eq!(
            resolved(root, address),
            resolved(&directory.canonical, address),
            "{address}"
        );
    }
}
