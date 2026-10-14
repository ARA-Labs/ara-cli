//! Plan 22 part C: partial multi-address `show`, the containing section of
//! whole-document `find` hits, and bare entry-ID heading selectors.
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: Root\n";
const CLAIMS: &str = "# Claims\n\n## C01: Mechanism\n- **Statement**: Known.\n- **Status**: hypothesis\n\n## C02: Latency\n- **Statement**: Flat.\n- **Status**: hypothesis\n";
/// Line 1 lies before any heading; the other drift lines lie in nested
/// sections.
const PROBLEM: &str = "Intro mentions drift.\n\n# Problem Specification\n\nTop drift text.\n\n## Observations\n\n### O1: Drift appears\nWe saw drift here.\n\n### O2: Other\nNothing.\n\n## Gaps\n\n### G1: Missing baseline\nMore drift.\n";
/// A second `G1` heading, an em-dash `E05` heading, and headings that
/// spell native IDs (`C01`, `N01`).
const NOTES: &str = "# Notes\n\n## G1: Duplicate gap\nx\n\n## E05 — Dashed\ny\n\n## C01: Not the claim\nz\n\n## N01: Not the node\nw\n";

fn put(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}
fn artifact() -> TempDir {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "trace/exploration_tree.yaml", TREE);
    put(dir.path(), "logic/claims.md", CLAIMS);
    put(dir.path(), "logic/problem.md", PROBLEM);
    put(dir.path(), "logic/solution/notes.md", NOTES);
    dir
}
fn run(root: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(root).args(args);
    command.output().unwrap()
}
fn stdout(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}
/// A successful read's stdout.
fn ok(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    stdout(&output)
}
fn ok_json(root: &Path, args: &[&str]) -> Value {
    let mut args = args.to_vec();
    args.push("--json");
    serde_json::from_str(&ok(root, &args)).unwrap()
}
/// The JSON error of a read that fails as a whole.
fn rejected(root: &Path, args: &[&str]) -> Value {
    let mut args = args.to_vec();
    args.push("--json");
    let output = run(root, &args);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"].clone()
}
fn show<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut all = vec!["show"];
    all.extend(args);
    all
}

#[test]
fn all_good_multi_address_reads_are_their_single_reads_in_order() {
    let dir = artifact();
    let root = dir.path();
    let addresses = ["C02", "N01", "logic/problem.md#O1", "C01"];
    let joined = ok(root, &show(&addresses));
    let singles: String = addresses.iter().map(|a| ok(root, &show(&[a]))).collect();
    assert_eq!(joined, singles);
    let value = ok_json(root, &show(&addresses));
    assert!(value.get("errors").is_none(), "{value}");
    let entries: Vec<Value> = addresses
        .iter()
        .map(|a| ok_json(root, &show(&[a]))["entries"][0].clone())
        .collect();
    assert_eq!(value["entries"], Value::Array(entries));
}

#[test]
fn a_failed_address_prints_its_error_block_in_request_order_and_exits_1() {
    let dir = artifact();
    let root = dir.path();
    for bad in [
        "H01 [heuristic] logic/solution/heuristics.md",
        "G1",
        "rubric/x.md",
    ] {
        let alone = run(root, &show(&[bad]));
        assert_eq!(alone.status.code(), Some(1));
        assert!(
            alone.stdout.is_empty(),
            "a single failure keeps today's error"
        );
        let output = run(root, &show(&["C01", bad, "N01"]));
        assert_eq!(output.status.code(), Some(1), "{bad}");
        let expected = format!(
            "{}== {bad} [error]\n{}{}",
            ok(root, &show(&["C01"])),
            stderr(&alone),
            ok(root, &show(&["N01"])),
        );
        assert_eq!(stdout(&output), expected);
    }
}

#[test]
fn json_partial_reads_list_each_failed_address_with_its_single_error() {
    let dir = artifact();
    let root = dir.path();
    let output = run(root, &show(&["BOGUS", "C01", "G1", "--json"]));
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["entries"],
        ok_json(root, &show(&["C01"]))["entries"],
        "resolved entries keep their rows"
    );
    let errors = value["errors"].as_array().unwrap();
    let summary: Vec<(u64, &str, &str)> = errors
        .iter()
        .map(|failure| {
            (
                failure["position"].as_u64().unwrap(),
                failure["address"].as_str().unwrap(),
                failure["error"]["code"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [(0, "BOGUS", "unknown_id"), (2, "G1", "ambiguous_heading")]
    );
    for failure in errors {
        let address = failure["address"].as_str().unwrap();
        assert_eq!(failure["error"], rejected(root, &show(&[address])));
        assert!(
            !failure["error"]["details"]["candidates"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn every_address_failing_still_reports_each_one() {
    let dir = artifact();
    let output = run(dir.path(), &show(&["X1", "Y2"]));
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    let headers: Vec<&str> = text.lines().filter(|l| l.starts_with("== ")).collect();
    assert_eq!(headers, ["== X1 [error]", "== Y2 [error]"]);
}

#[test]
fn budgets_cover_error_blocks_and_page_a_single_resolved_selection() {
    let dir = artifact();
    let root = dir.path();
    let full = run(root, &show(&["logic/problem.md", "BOGUS"]));
    let size = full.stdout.len();
    // One resolved selection pages, and the page leaves room for the error.
    let budget = (size - 40).to_string();
    let paged = run(
        root,
        &show(&["logic/problem.md", "BOGUS", "--max-bytes", &budget]),
    );
    assert_eq!(paged.status.code(), Some(1));
    let text = stdout(&paged);
    assert!(text.len() <= size - 40, "{} > {budget}", text.len());
    assert!(text.contains("; truncated; next: --lines "), "{text}");
    assert!(text.contains("== BOGUS [error]\n"), "{text}");
    // Several resolved selections still fit together or reject before output.
    let together = run(root, &show(&["C01", "C02", "BOGUS"])).stdout.len();
    let budget = (together - 1).to_string();
    let over = run(
        root,
        &show(&["C01", "C02", "BOGUS", "--max-bytes", &budget]),
    );
    assert_eq!(over.status.code(), Some(1));
    assert!(over.stdout.is_empty());
    let message = stderr(&over);
    assert!(
        message.starts_with("error [output_limit_too_small]"),
        "{message}"
    );
    assert!(
        message.contains(&format!("--max-bytes {together}")),
        "{message}"
    );
}

/// Each `in:` address with the line it precedes, from brief `find` text.
fn sections_of(text: &str) -> Vec<(String, u64)> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let address = line.strip_prefix("  in: ")?;
            let next = lines[index + 1].trim_start();
            let number = next.split([':', '-']).next()?.parse().ok()?;
            Some((address.to_owned(), number))
        })
        .collect()
}

#[test]
fn whole_document_hits_name_the_innermost_section_of_each_line() {
    let dir = artifact();
    let root = dir.path();
    for context in [None, Some("1")] {
        let mut args = vec!["find", "drift", "--type", "source_document"];
        args.extend(context.map(|n| ["--context", n]).into_iter().flatten());
        let text = ok(root, &args);
        let found = sections_of(&text);
        let expected = [
            ("logic/problem.md#h/Problem%20Specification", 5),
            (
                "logic/problem.md#h/Problem%20Specification/Observations/O1%3A%20Drift%20appears",
                9,
            ),
            (
                "logic/problem.md#h/Problem%20Specification/Gaps/G1%3A%20Missing%20baseline",
                18,
            ),
        ];
        let expected: Vec<(String, u64)> =
            expected.iter().map(|(a, n)| (a.to_string(), *n)).collect();
        assert_eq!(found, expected, "{text}");
        // Line 1 lies before any heading: it prints before the first `in:`.
        let first = text.find("  in: ").unwrap();
        assert!(
            text[..first].contains("  1: Intro mentions drift."),
            "{text}"
        );
    }
}

#[test]
fn printed_sections_round_trip_through_show_and_hold_their_lines() {
    let dir = artifact();
    let root = dir.path();
    let value = ok_json(root, &["find", "drift", "--type", "source_document"]);
    let matches = value["results"][0]["matches"].as_array().unwrap();
    assert!(
        matches[0].get("in").is_none(),
        "line 1 is outside any heading"
    );
    for hit in &matches[1..] {
        let address = hit["in"].as_str().unwrap();
        let section = &ok_json(root, &show(&[address]))["entries"][0];
        let heading = section["heading_path"].as_array().unwrap();
        let line = hit["text"].as_str().unwrap();
        let own_heading = format!(
            "{} {}",
            "#".repeat(heading.len()),
            heading.last().unwrap().as_str().unwrap()
        );
        assert!(
            line == own_heading || section["content"].as_str().unwrap().contains(line),
            "{address} does not hold `{line}`"
        );
    }
}

#[test]
fn bare_entry_ids_select_the_one_heading_they_start() {
    let dir = artifact();
    let root = dir.path();
    for (id, heading) in [("O2", "O2: Other"), ("E05", "E05 — Dashed")] {
        let row = &ok_json(root, &show(&[id]))["entries"][0];
        assert_eq!(row["kind"], "source_document");
        assert_eq!(
            row["heading_path"].as_array().unwrap().last().unwrap(),
            heading
        );
    }
    // A prefix of a longer ID is not that ID.
    assert_eq!(rejected(root, &show(&["O"]))["code"], "unknown_id");
    put(
        root,
        "logic/solution/more.md",
        "# More\n\n## O21: Longer\nq\n",
    );
    assert_eq!(
        ok_json(root, &show(&["O2"]))["entries"][0]["heading_path"][2],
        "O2: Other"
    );
}

#[test]
fn several_headings_with_one_id_are_ambiguous_with_readable_candidates() {
    let dir = artifact();
    let root = dir.path();
    let error = rejected(root, &show(&["G1"]));
    assert_eq!(error["code"], "ambiguous_heading");
    let candidates: Vec<&str> = error["details"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        let row = &ok_json(root, &show(&[candidate]))["entries"][0];
        let last = row["heading_path"].as_array().unwrap().last().unwrap();
        assert!(last.as_str().unwrap().starts_with("G1: "));
    }
}

#[test]
fn native_ids_keep_their_meaning_over_headings_that_spell_them() {
    let dir = artifact();
    let root = dir.path();
    let claim = &ok_json(root, &show(&["C01"]))["entries"][0];
    assert_eq!(
        (&claim["kind"], &claim["id"]),
        (&"claim".into(), &"C01".into())
    );
    let node = &ok_json(root, &show(&["N01"]))["entries"][0];
    assert_eq!(
        (&node["kind"], &node["id"]),
        (&"question".into(), &"N01".into())
    );
}
