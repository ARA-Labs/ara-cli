//! Plan 04b: the ara-eval phase-4 runner's canonical-feedback flow through the
//! real binary. Forks own external files, every external conflict is
//! acknowledged with `merge resolve` on both sides (writing positional
//! reasoning entries and same-day sessions), and a never-written fork without a
//! `.gitignore` absorbs canonical.
use assert_cmd::Command;
use serde_json::Value;
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
fn seed(root: &Path, gitignore: bool) {
    if gitignore {
        put(root, ".gitignore", b".ara/\n");
    }
    put(root, "trace/exploration_tree.yaml", TREE.as_bytes());
    put(root, "logic/claims.md", CLAIMS.as_bytes());
}
fn headings(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("logic/claims.md"))
        .unwrap()
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
/// A published snapshot: every nonprivate file, without the write journal.
fn snapshot(from: &Path, to: &Path) {
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
fn json(root: &Path, args: &[&str], code: i32) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .code(code)
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
/// `ara merge` that may leave external-file conflicts (exit 1) but never fails.
fn import(ours: &Path, base: &Path, theirs: &Path, key: &str, own: &str) -> Value {
    let args = [
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
    ];
    let output = ara(ours).args(args).arg("--json").output().unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    assert!(
        report["committed"] == true && matches!(output.status.code(), Some(0 | 1)),
        "{args:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for conflict in report["conflicts"].as_array().unwrap() {
        assert!(
            matches!(
                conflict["kind"].as_str(),
                Some("external_read_only" | "opaque_file" | "imported_unresolved")
            ),
            "{conflict}"
        );
    }
    report
}
/// Acknowledge every local external-file conflict as the runner does: one
/// session per conflict, `merge resolve --take ours`.
fn acknowledge(root: &Path, report: &Value) {
    for conflict in report["conflicts"].as_array().unwrap() {
        if conflict["allowed"] != serde_json::json!(["ours"]) {
            continue;
        }
        let id = conflict["id"].as_str().unwrap();
        let session = json(
            root,
            &["session", "start", "--summary", &format!("ack {id}")],
            0,
        );
        let session = session["id"].as_str().unwrap();
        json(
            root,
            &[
                "merge",
                "resolve",
                id,
                "--take",
                "ours",
                "--session",
                session,
                "--turn",
                "1",
                "--signal",
                "user-directive",
                "--provenance",
                "ai-executed",
            ],
            0,
        );
    }
}
fn resolved(root: &Path, address: &str) -> String {
    json(root, &["resolve", address], 0)["id"]
        .as_str()
        .unwrap()
        .into()
}
fn sessions(root: &Path) -> usize {
    fs::read_dir(root.join("trace/sessions"))
        .map(|entries| {
            entries
                .filter(|entry| {
                    entry
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("20")
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn canonical_feedback_with_external_files_sessions_and_reasoning_round_trips() {
    let owner = TempDir::new().unwrap();
    let path = |name: &str| -> PathBuf { owner.path().join(name) };
    let (seed_dir, canonical, fork_a) = (path("seed"), path("canonical"), path("fork-a"));
    seed(&seed_dir, true);
    let a1 = path("A1");
    snapshot(&seed_dir, &a1);
    put(&a1, "evidence/a.json", b"{}\n");
    let text = fs::read_to_string(a1.join("logic/claims.md")).unwrap()
        + "\n## C77: fork A finding\n- **Statement**: A says x\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";
    put(&a1, "logic/claims.md", text.as_bytes());
    snapshot(&a1, &fork_a);
    snapshot(&seed_dir, &canonical);

    let report = import(&canonical, &seed_dir, &a1, "fork-a", "canonical");
    acknowledge(&canonical, &report);
    put(&canonical, "src/c.py", b"print('canonical')\n");
    let k1 = path("K1");
    snapshot(&canonical, &k1);
    let report = import(&fork_a, &seed_dir, &k1, "canonical", "fork-a");
    acknowledge(&fork_a, &report);
    let a2 = path("A2");
    snapshot(&fork_a, &a2);
    // Defect 1: fork A owns evidence/a.json; its return used to fail with
    // merge.ambiguous_origin on a historical identity.
    import(&canonical, &a1, &a2, "fork-a", "canonical");
    assert_eq!(
        headings(&canonical),
        ["## C01: shared", "## C02: fork A finding"]
    );

    // Both sides acknowledge new external files independently, so each
    // appends reasoning entries at the same positions before exchanging.
    put(&canonical, "src/d.py", b"print('d')\n");
    let k2 = path("K2");
    snapshot(&canonical, &k2);
    put(&fork_a, "evidence/b.json", b"{\"b\": 1}\n");
    let a3 = path("A3");
    snapshot(&fork_a, &a3);
    let report = import(&canonical, &a2, &a3, "fork-a", "canonical");
    acknowledge(&canonical, &report);
    let report = import(&fork_a, &k1, &k2, "canonical", "fork-a");
    acknowledge(&fork_a, &report);
    let k3 = path("K3");
    snapshot(&canonical, &k3);
    import(&fork_a, &k2, &k3, "canonical", "fork-a");
    let a4 = path("A4");
    snapshot(&fork_a, &a4);
    // Defect 3a: positional reasoning entries relocated on both routes used to
    // reject as protected_inherited_entry.
    let before = sessions(&canonical);
    import(&canonical, &a3, &a4, "fork-a", "canonical");
    assert_eq!(
        headings(&canonical),
        ["## C01: shared", "## C02: fork A finding"]
    );
    assert_eq!(resolved(&canonical, "fork-a:C77"), "C02");
    assert_eq!(sessions(&canonical), before + 1);
    let replay = import(&canonical, &a3, &a4, "fork-a", "canonical");
    assert_eq!(replay["changed_paths"], serde_json::json!([]));

    // Defect 2: a fork with one same-day session absorbs canonical's several.
    let fork_c = path("fork-c");
    snapshot(&seed_dir, &fork_c);
    json(&fork_c, &["session", "start", "--summary", "own"], 0);
    let k4 = path("K4");
    snapshot(&canonical, &k4);
    let report = import(&fork_c, &seed_dir, &k4, "canonical", "fork-c");
    assert_eq!(sessions(&fork_c), sessions(&canonical) + 1, "{report}");
    assert_eq!(headings(&fork_c), headings(&canonical));

    // Defect 3b: a never-written fork has no .gitignore to alias.
    let bare = path("bare-seed");
    seed(&bare, false);
    let fork_d = path("fork-d");
    snapshot(&bare, &fork_d);
    // The incoming file is reported, not installed (the CLI write itself then
    // adds its operational ignore rule), and no alias to it is copied.
    let report = import(&fork_d, &bare, &k4, "canonical", "fork-d");
    assert!(
        !fs::read_to_string(fork_d.join("trace/aliases.yaml"))
            .unwrap()
            .contains("\"original\":\".gitignore\"")
    );
    assert!(
        report["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == ".gitignore" && item["kind"] == "opaque_file")
    );
    assert_eq!(resolved(&fork_d, "canonical:C02"), "C02");
}

#[test]
fn self_key_round_trip_keeps_a_retired_native_claim() {
    // Canonical retired its own C02 by an audited rename to C03; a fork with
    // its own C02 absorbs canonical and returns.
    let owner = TempDir::new().unwrap();
    let path = |name: &str| -> PathBuf { owner.path().join(name) };
    let (seed_dir, canonical, fork_b) = (path("seed"), path("canonical"), path("fork-b"));
    seed(&seed_dir, true);
    snapshot(&seed_dir, &canonical);
    let after = "## C03: canonical claim\n- **Statement**: renamed\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";
    let before = after.replace("C03:", "C02:");
    put(
        &canonical,
        "logic/claims.md",
        format!("{CLAIMS}\n{after}").as_bytes(),
    );
    let mutation = serde_json::json!({"action":"rename","from":"logic/claims.md:C02","to":"logic/claims.md:C03","from_selector":{"document":"logic/claims.md","heading":[],"entry":"C02"},"to_selector":{"document":"logic/claims.md","heading":[],"entry":"C03"},"before":before,"after":after,"session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":[]});
    let session = serde_json::json!({
        "session": {"id": "2026-10-01_001", "date": "2026-10-01", "started": "2026-10-01T10:00:00Z", "last_turn": "2026-10-01T11:00:00Z", "turn_count": 1, "summary": "rename"},
        "events_logged": [], "ai_actions": [], "claims_touched": [],
        "logic_revisions": [{"turn":1,"entry":mutation["from_selector"],"field":"entry","before":before,"after":after,"signal":"user-directive","provenance":"user"}],
        "key_context": [], "open_threads": [], "ai_suggestions_pending": []
    });
    put(
        &canonical,
        "trace/sessions/2026-10-01_001.yaml",
        ara_core::write::source::render_yaml(&session, 0, "\n").as_bytes(),
    );
    put(
        &canonical,
        "trace/sessions/session_index.yaml",
        b"sessions:\n  - {id: '2026-10-01_001', date: '2026-10-01', summary: rename, turn_count: 1, events_count: 0, claims_touched: [], open_threads: 0}\n",
    );
    put(
        &canonical,
        "trace/logic_mutations.yaml",
        format!("mutations:\n  - {mutation}\n").as_bytes(),
    );
    assert_eq!(resolved(&canonical, "C02"), "C03");
    snapshot(&seed_dir, &fork_b);
    let text = fs::read_to_string(fork_b.join("logic/claims.md")).unwrap()
        + "\n## C02: fork B finding\n- **Statement**: B says y\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";
    put(&fork_b, "logic/claims.md", text.as_bytes());
    let k1 = path("K1");
    snapshot(&canonical, &k1);
    import(&fork_b, &seed_dir, &k1, "canonical", "fork-b");
    let live = resolved(&fork_b, "canonical:C03");
    assert_eq!(resolved(&fork_b, "canonical:C02"), live);
    let b2 = path("B2");
    snapshot(&fork_b, &b2);
    let mutations = fs::read(canonical.join("trace/logic_mutations.yaml")).unwrap();
    let report = import(&canonical, &seed_dir, &b2, "fork-b", "canonical");
    assert_eq!(report["unresolved_count"], 0);
    assert_eq!(
        headings(&canonical),
        [
            "## C01: shared",
            "## C03: canonical claim",
            "## C04: fork B finding"
        ]
    );
    assert_eq!(
        fs::read(canonical.join("trace/logic_mutations.yaml")).unwrap(),
        mutations
    );
    assert_eq!(resolved(&canonical, "C02"), "C03");
    assert_eq!(resolved(&canonical, &format!("fork-b:{live}")), "C03");
}
