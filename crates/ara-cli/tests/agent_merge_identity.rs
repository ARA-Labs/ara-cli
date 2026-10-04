//! Binary-level identity consumers for accepted native writer archives.
use ara_core::write::{self, ArtifactSnapshot, EntrySelector, WorkingArtifact, WriteOperation};
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

const DOCUMENT: &str = "logic/solution/architecture.md";
const ORIGINAL: &str = "# Architecture\n\n## Parent\nContainer body.\n### A/B\nLiteral child body.\n### A\nNested parent body.\n#### B\nNested child body.\n";
fn put(root: &Path, path: &str, bytes: &[u8]) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}
fn seed(root: &Path) {
    put(
        root,
        "trace/exploration_tree.yaml",
        b"tree: [{id: N01, type: question, title: Unrelated}]\n",
    );
    put(root, DOCUMENT, ORIGINAL.as_bytes());
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
fn writer_rename(root: &Path) {
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    for value in [
        json!({"op":"session.start","id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Accepted writer fixture"}),
        json!({"op":"session.log","session":"2026-10-01_001","timestamp":"2026-10-01T10:01"}),
    ] {
        write::plan_operation(&mut working, &serde_json::from_value(value).unwrap()).unwrap();
    }
    let target = EntrySelector::Document {
        document: DOCUMENT.into(),
        heading: vec!["Architecture".into(), "Parent".into()],
        entry: None,
    };
    let range = write::logic::resolve(&working, &target).unwrap().range;
    let expected = write::source::digest(working.text(DOCUMENT).unwrap()[range].as_bytes());
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryRename {
            target,
            name: "Renamed parent".into(),
            expected,
            references: vec![],
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: Some("user-directive".into()),
            provenance: Some("user".into()),
        },
    )
    .unwrap();
    for revision in std::mem::take(&mut working.revisions) {
        write::sessions::append_revision(
            &mut working,
            &revision.session,
            revision.turn,
            &revision.record,
        )
        .unwrap();
    }
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
    let ledger = working
        .yaml("trace/logic_mutations.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    let colliding = ledger["mutations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["from"] == "logic/solution/architecture.md:Architecture/Parent/A/B")
        .collect::<Vec<_>>();
    assert_eq!(colliding.len(), 2);
    assert_ne!(colliding[0]["from_selector"], colliding[1]["from_selector"]);
    for (path, bytes) in &working.files {
        put(root, path, bytes);
    }
}
fn show_vectors(root: &Path) {
    let whole = run(root, &["show", "--document", DOCUMENT, "--full"]);
    assert!(whole["entries"][0]["content"].as_str().unwrap().contains(
        "### A/B\nLiteral child body.\n### A\nNested parent body.\n#### B\nNested child body."
    ));
    for ancestor in ["Parent", "Renamed parent"] {
        let parent = run(
            root,
            &[
                "show",
                "--document",
                DOCUMENT,
                "--heading",
                "Architecture",
                "--heading",
                ancestor,
                "--full",
            ],
        );
        assert_eq!(
            parent["entries"][0]["heading"],
            json!(["Architecture", "Renamed parent"])
        );
        assert!(
            parent["entries"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Literal child body.")
        );
        assert!(
            parent["entries"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Nested child body.")
        );
    }
    assert_eq!(run(root, &["show", "N01"])["entries"][0]["id"], "N01");
    assert!(
        run(root, &["ls"])["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == "N01")
    );
    for ancestor in ["Parent", "Renamed parent"] {
        for (tail, body) in [
            (vec!["A/B"], "Literal child body.\n"),
            (vec!["A", "B"], "Nested child body.\n"),
        ] {
            let mut args = vec![
                "show",
                "--document",
                DOCUMENT,
                "--heading",
                "Architecture",
                "--heading",
                ancestor,
            ];
            for part in &tail {
                args.extend(["--heading", *part]);
            }
            args.push("--full");
            let actual = run(root, &args);
            assert_eq!(actual["entries"][0]["content"], body);
            let mut current = vec!["Architecture", "Renamed parent"];
            current.extend(tail);
            assert_eq!(actual["entries"][0]["heading"], json!(current));
        }
    }
    // Reject only the requested ambiguous legacy display locator.
    for address in [
        "logic/solution/architecture.md:Architecture/Parent/A/B",
        "logic/solution/architecture.md#Architecture/Renamed parent/A/B",
    ] {
        let output = ara(root)
            .args(["resolve", address, "--json"])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("merge.redirect_ambiguous")
                || String::from_utf8_lossy(&output.stdout).contains("merge.redirect_ambiguous")
        );
    }
    assert_eq!(run(root, &["show", "N01"])["entries"][0]["id"], "N01");
}
fn frozen(root: &Path) -> BTreeMap<String, Vec<u8>> {
    ArtifactSnapshot::load_complete(root)
        .unwrap()
        .files
        .into_iter()
        .filter(|(_, file)| file.existed)
        .map(|(path, file)| (path, file.bytes))
        .collect()
}
#[test]
fn accepted_colliding_writer_vectors_survive_actual_reads_merge_and_identical_replay() {
    let owner = TempDir::new().unwrap();
    let base = owner.path().join("base");
    let ours = owner.path().join("ours");
    let source = owner.path().join("source");
    for root in [&base, &ours, &source] {
        seed(root);
    }
    writer_rename(&source);
    show_vectors(&source);
    let source_before = frozen(&source);
    let base_before = frozen(&base);
    let merge = || {
        run(
            &ours,
            &[
                "merge",
                "--base",
                base.to_str().unwrap(),
                "--theirs",
                source.to_str().unwrap(),
                "--as",
                "bob",
                "--source-key",
                "literal-vector-source",
                "--no-duplicate-check",
            ],
        )
    };
    let first = merge();
    assert_eq!(first["unresolved_count"], 0);
    assert_eq!(first["committed"], true);
    show_vectors(&ours);
    assert_eq!(
        fs::read(ours.join(DOCUMENT)).unwrap(),
        source_before[DOCUMENT]
    );
    let destination_before = frozen(&ours);
    let replay = merge();
    assert_eq!(replay["unresolved_count"], 0);
    assert_eq!(replay["changed_paths"], json!([]));
    assert_eq!(frozen(&ours), destination_before);
    assert_eq!(frozen(&source), source_before);
    assert_eq!(frozen(&base), base_before);
    show_vectors(&ours);
}

// Plan 04: peer-feedback diamond through the real binary, in directory and
// local-Git mode. Fork A's original claim must keep one canonical identity.
const PEER_TREE: &str =
    "tree:\n  - id: N01\n    type: question\n    title: root\n    children: []\n";
const PEER_CLAIMS: &str = "# Claims\n\n## C01: shared\n- **Statement**: base\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";
fn peer_claim(title: &str, statement: &str) -> String {
    format!(
        "\n## C77: {title}\n- **Statement**: {statement}\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n"
    )
}
fn peer_seed(root: &Path) {
    put(root, "trace/exploration_tree.yaml", PEER_TREE.as_bytes());
    put(root, "logic/claims.md", PEER_CLAIMS.as_bytes());
}
fn copy_tree(from: &Path, to: &Path) {
    for (path, bytes) in frozen(from) {
        put(to, &path, &bytes);
    }
}
fn peer_import(ours: &Path, base: &Path, theirs: &Path, key: &str) -> Value {
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
        ],
    )
}
fn resolved(root: &Path, address: &str) -> String {
    run(root, &["resolve", address])["id"]
        .as_str()
        .unwrap()
        .into()
}
fn claim_headings(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("logic/claims.md"))
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("## "))
        .map(str::to_owned)
        .collect()
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
struct Diamond {
    _owner: TempDir,
    seed: std::path::PathBuf,
    a1: std::path::PathBuf,
    b1: std::path::PathBuf,
    b2: std::path::PathBuf,
    canonical: std::path::PathBuf,
}
fn diamond(a_change: impl Fn(&Path), b_change: impl Fn(&Path)) -> Diamond {
    let owner = TempDir::new().unwrap();
    let path = |name: &str| owner.path().join(name);
    let (seed, a1, b1, b2, canonical) = (
        path("seed"),
        path("A1"),
        path("B1"),
        path("B2"),
        path("canonical"),
    );
    peer_seed(&seed);
    copy_tree(&seed, &a1);
    a_change(&a1);
    copy_tree(&seed, &b1);
    b_change(&b1);
    copy_tree(&b1, &b2);
    copy_tree(&seed, &canonical);
    Diamond {
        _owner: owner,
        seed,
        a1,
        b1,
        b2,
        canonical,
    }
}
fn append_claim(root: &Path, title: &str, statement: &str) {
    let mut text = fs::read_to_string(root.join("logic/claims.md")).unwrap();
    text.push_str(&peer_claim(title, statement));
    put(root, "logic/claims.md", text.as_bytes());
}

#[test]
fn diamond_peer_feedback_return_import_keeps_original_identities() {
    let d = diamond(
        |a| append_claim(a, "fork A finding", "A says x"),
        |b| append_claim(b, "fork B finding", "B says y"),
    );
    assert_eq!(
        peer_import(&d.canonical, &d.seed, &d.b1, "fork-b")["unresolved_count"],
        0
    );
    assert_eq!(
        peer_import(&d.canonical, &d.seed, &d.a1, "fork-a")["unresolved_count"],
        0
    );
    assert_eq!(
        peer_import(&d.b2, &d.seed, &d.a1, "fork-a")["unresolved_count"],
        0
    );
    let before = resolved(&d.canonical, "fork-a:C77");
    assert_eq!(before, "C03");
    assert_eq!(resolved(&d.b2, "fork-a:C77"), "C78");
    let canonical_claims = fs::read(d.canonical.join("logic/claims.md")).unwrap();

    let report = peer_import(&d.canonical, &d.b1, &d.b2, "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    assert_eq!(report["committed"], true);
    assert_eq!(
        claim_headings(&d.canonical),
        [
            "## C01: shared",
            "## C02: fork B finding",
            "## C03: fork A finding"
        ]
    );
    assert_eq!(
        fs::read(d.canonical.join("logic/claims.md")).unwrap(),
        canonical_claims
    );
    assert_eq!(resolved(&d.canonical, "fork-a:C77"), before);
    assert_eq!(resolved(&d.canonical, "fork-b:C77"), "C02");
    assert_eq!(resolved(&d.canonical, "fork-b:C78"), "C03");
    assert_eq!(
        record_kinds(&d.canonical, "fork-a"),
        ["enrollment", "revision"]
    );
    let destination = frozen(&d.canonical);
    let replay = peer_import(&d.canonical, &d.b1, &d.b2, "fork-b");
    assert_eq!(replay["unresolved_count"], 0);
    assert_eq!(replay["changed_paths"], json!([]));
    assert_eq!(frozen(&d.canonical), destination);
}

#[test]
fn independent_imports_with_different_wall_clock_times_return_cleanly() {
    let d = diamond(
        |a| {
            let text = fs::read_to_string(a.join("logic/claims.md")).unwrap();
            put(
                a,
                "logic/claims.md",
                text.replace("Statement**: base", "Statement**: A refined")
                    .as_bytes(),
            );
        },
        |b| {
            let mut text = fs::read_to_string(b.join("logic/claims.md")).unwrap();
            text.push_str(&peer_claim("fork B finding", "B says y").replace("C77", "C02"));
            put(b, "logic/claims.md", text.as_bytes());
        },
    );
    peer_import(&d.canonical, &d.seed, &d.b1, "fork-b");
    peer_import(&d.canonical, &d.seed, &d.a1, "fork-a");
    // Import timestamps have one-second resolution; force a different event time.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    peer_import(&d.b2, &d.seed, &d.a1, "fork-a");
    let time = |root: &Path| {
        fs::read_to_string(root.join("trace/merge_log.yaml"))
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("  - "))
            .map(|row| serde_json::from_str::<Value>(row).unwrap())
            .find(|row| row["kind"] == "revision" && row["source_key"] == "fork-a")
            .unwrap()["time"]
            .clone()
    };
    assert_ne!(time(&d.canonical), time(&d.b2));
    let report = peer_import(&d.canonical, &d.b1, &d.b2, "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    assert_eq!(
        claim_headings(&d.canonical),
        ["## C01: shared", "## C02: fork B finding"]
    );
    assert_eq!(resolved(&d.canonical, "fork-a:C01"), "C01");
    assert_eq!(
        record_kinds(&d.canonical, "fork-a"),
        ["enrollment", "revision"]
    );
}

fn git(root: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout).unwrap()
}
fn git_import(root: &Path, reference: &str, key: &str) -> Value {
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
fn local_git_diamond_matches_directory_identities_and_content() {
    let directory = diamond(
        |a| append_claim(a, "fork A finding", "A says x"),
        |b| append_claim(b, "fork B finding", "B says y"),
    );
    peer_import(
        &directory.canonical,
        &directory.seed,
        &directory.b1,
        "fork-b",
    );
    peer_import(
        &directory.canonical,
        &directory.seed,
        &directory.a1,
        "fork-a",
    );
    peer_import(&directory.b2, &directory.seed, &directory.a1, "fork-a");
    peer_import(&directory.canonical, &directory.b1, &directory.b2, "fork-b");

    // One worktree per fork keeps each destination's private journal separate.
    let owner = TempDir::new().unwrap();
    let root = &owner.path().join("canonical");
    let fork_b = &owner.path().join("fork-b-worktree");
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "--initial-branch=seed"]);
    git(root, &["config", "user.name", "ARA fixture"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    put(root, ".gitignore", b".ara/\n");
    peer_seed(root);
    git(root, &["add", "--all"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    for (branch, title, statement) in [
        ("fork-a", "fork A finding", "A says x"),
        ("fork-b", "fork B finding", "B says y"),
    ] {
        git(root, &["checkout", "-q", "-b", branch, "seed"]);
        append_claim(root, title, statement);
        git(root, &["commit", "-q", "-am", branch]);
    }
    git(root, &["checkout", "-q", "-b", "canonical", "seed"]);
    git(
        root,
        &["worktree", "add", "-q", fork_b.to_str().unwrap(), "fork-b"],
    );
    assert_eq!(git_import(root, "fork-b", "fork-b")["unresolved_count"], 0);
    assert_eq!(git_import(root, "fork-a", "fork-a")["unresolved_count"], 0);
    assert_eq!(
        git_import(fork_b, "fork-a", "fork-a")["unresolved_count"],
        0
    );
    assert_eq!(resolved(fork_b, "fork-a:C77"), "C78");
    let report = git_import(root, "fork-b", "fork-b");
    assert_eq!(report["unresolved_count"], 0);
    assert!(report["git"]["theirs"].is_string());
    assert_eq!(
        fs::read(root.join("logic/claims.md")).unwrap(),
        fs::read(directory.canonical.join("logic/claims.md")).unwrap()
    );
    for address in ["fork-a:C77", "fork-b:C77", "fork-b:C78"] {
        assert_eq!(
            resolved(root, address),
            resolved(&directory.canonical, address),
            "{address}"
        );
    }
    assert_eq!(record_kinds(root, "fork-a"), ["enrollment", "revision"]);
    // Git ancestry is still required: a rewritten fork-b history is refused.
    git(fork_b, &["checkout", "-q", "-b", "rewritten", "seed"]);
    append_claim(fork_b, "rewritten", "not a descendant");
    git(fork_b, &["commit", "-q", "-am", "rewritten"]);
    let output = ara(root)
        .args([
            "merge",
            "--git",
            "rewritten",
            "--as",
            "fork-b",
            "--source-key",
            "fork-b",
            "--json",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("git_source_lineage"), "{text}");
}

#[test]
fn forged_inherited_alias_rejects_before_destination_mutation() {
    let d = diamond(
        |a| append_claim(a, "fork A finding", "A says x"),
        |b| append_claim(b, "fork B finding", "B says y"),
    );
    peer_import(&d.canonical, &d.seed, &d.b1, "fork-b");
    peer_import(&d.canonical, &d.seed, &d.a1, "fork-a");
    // B claims an entry is fork A's C77 without any backing source fact.
    let mut text = fs::read_to_string(d.b2.join("logic/claims.md")).unwrap();
    text.push_str(&peer_claim("fork A finding", "A says x").replace("C77", "C78"));
    put(&d.b2, "logic/claims.md", text.as_bytes());
    put(
        &d.b2,
        "trace/aliases.yaml",
        b"format: ara.aliases/v1\naliases:\n  - {\"source_key\":\"fork-a\",\"label\":\"fork-a\",\"original\":\"C77\",\"target\":\"C78\",\"revision\":\"forged\"}\n",
    );
    let before = frozen(&d.canonical);
    let output = ara(&d.canonical)
        .args([
            "merge",
            "--base",
            d.b1.to_str().unwrap(),
            "--theirs",
            d.b2.to_str().unwrap(),
            "--as",
            "fork-b",
            "--source-key",
            "fork-b",
            "--json",
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("merge.alias_conflict"), "{text}");
    assert!(text.contains("trace/aliases.yaml"), "{text}");
    assert_eq!(frozen(&d.canonical), before);
    assert_eq!(resolved(&d.canonical, "fork-a:C77"), "C03");
}
