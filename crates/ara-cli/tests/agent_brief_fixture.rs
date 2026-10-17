//! Brief text over the vendored agent-cli fixture: every printed address is
//! readable, projections keep list text, sessions fit the default budget, and
//! empty results are visible.
use assert_cmd::Command;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ara-core/tests/fixtures/agent-cli")
}
fn ara() -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(fixture());
    command
}
fn stdout(args: &[&str]) -> String {
    let output = ara().args(args).assert().success().get_output().clone();
    String::from_utf8(output.stdout).unwrap()
}
/// The first tab-separated field of each line.
fn leading(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split('\t').next().unwrap().trim().to_owned())
        .collect()
}

#[test]
fn every_printed_address_is_accepted_by_show() {
    let mut addresses = BTreeSet::new();
    let documents = stdout(&["ls"]);
    let mut lines: Vec<&str> = documents.lines().collect();
    lines.pop(); // the direct-file note
    for document in leading(&lines.join("\n")) {
        addresses.insert(document.clone());
        let listed = stdout(&["ls", &document]);
        if listed.trim() != "no entries" {
            addresses.extend(leading(&listed));
        }
    }
    addresses.extend(leading(&stdout(&["ls", "--unfinished"])));
    for query in [
        "semaphore",
        "rubric heuristics",
        "claims evidence",
        "session",
    ] {
        let found = stdout(&["find", query, "--limit", "20"]);
        addresses.extend(
            found
                .lines()
                .filter(|line| !line.starts_with(' '))
                .map(|line| line.split(' ').next().unwrap().to_owned()),
        );
    }
    assert!(addresses.contains("logic/solution/heuristics.md"));
    assert!(addresses.len() > 300, "{}", addresses.len());
    let addresses: Vec<&str> = addresses.iter().map(String::as_str).collect();
    for chunk in addresses.chunks(64) {
        let output = ara()
            .arg("show")
            .args(chunk)
            .arg("--json")
            .output()
            .unwrap();
        if !output.status.success() {
            let failed: Vec<&&str> = chunk
                .iter()
                .filter(|address| {
                    !ara()
                        .args(["show", address, "--json"])
                        .output()
                        .unwrap()
                        .status
                        .success()
                })
                .collect();
            panic!("show rejects printed addresses: {failed:?}");
        }
        let shown: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(shown["entries"].as_array().unwrap().len(), chunk.len());
    }
}

#[test]
fn projections_keep_multiline_list_items() {
    let shown = stdout(&["show", "N01"]);
    let notes = &shown[shown.find("evidence_notes:\n").unwrap()..];
    let mut lines = notes.lines().skip(1);
    assert_eq!(lines.next(), Some("  - |"));
    assert!(
        lines
            .next()
            .unwrap()
            .starts_with("    User requested two papers")
    );
}

#[test]
fn session_projections_fit_the_default_budget_without_the_raw_body() {
    let source =
        std::fs::read_to_string(fixture().join("trace/sessions/2026-04-24_001.yaml")).unwrap();
    let shown = stdout(&["show", "2026-04-24_001"]);
    assert!(!shown.lines().any(|line| line.starts_with("body:")));
    assert!(shown.contains("--document trace/sessions/2026-04-24_001.yaml --source"));
    // JSON keeps the full session, body included.
    let output = ara()
        .args(["show", "2026-04-24_001", "--full", "--json"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["entries"][0]["body"], source.as_str());
}

#[test]
fn empty_brief_results_say_so_and_json_stays_empty() {
    assert_eq!(stdout(&["find", "zzzqqq"]).trim(), "no results");
    assert_eq!(
        stdout(&["ls", "--type", "claim", "--status", "nope"]).trim(),
        "no entries"
    );
    assert_eq!(
        stdout(&["ls", "trace/pm_reasoning_log.yaml"]).trim(),
        "no entries"
    );
    let output = ara().args(["find", "zzzqqq", "--json"]).output().unwrap();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["results"], serde_json::json!([]));
}
