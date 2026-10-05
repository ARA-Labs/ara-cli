//! Brief default text for agent reads: addresses, source spans and line
//! numbers, digest and write-guard correspondence, diagnostics and errors.
//! Assertions parse the output; they do not snapshot wording or layout.
use assert_cmd::Command;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use tempfile::TempDir;

const CLAIMS: &str = "# Claims\n\nPreamble about THROUGHPUT budgets.\n\n## C01: Mechanism\n- **Statement**: Throughput doubles under load.\n- **Status**: hypothesis\n- **Proof**: Measured throughput twice.\n\n## C02 \u{2014} Dash title\n- **Statement**: Latency stays flat.\n- **Status**: hypothesis\n";
const PROBLEM: &str = "# Problem\n\nIntro line.\n\n## Notes\nfirst notes\n\n## Notes\nsecond notes\n\n## Gap\nA gap remains.";
const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: Root question\n    children:\n      - id: N02\n        type: experiment\n        title: Load test\n        evidence: [C01]\n        result: |-\n          throughput measured\n          second line\n";

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
    dir
}
fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(root);
    command
}
/// Default (brief) output: stdout and stderr of a successful read.
fn brief(root: &Path, args: &[&str]) -> (String, String) {
    let output = ara(root).args(args).assert().success().get_output().clone();
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}
fn json(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}
fn digest(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}
/// The `source_digest=` value and the text after `selector:` on the one
/// metadata line of a single-block `show`, and the body that follows it.
fn show_block(stdout: &str) -> (String, String, String) {
    let (head, body) = stdout
        .split_once("\nsource_digest=")
        .map(|(head, rest)| (head, rest.split_once('\n').unwrap()))
        .unwrap();
    assert!(head.starts_with("== "), "{stdout}");
    let (meta, body) = body;
    let digest = meta.split_whitespace().next().unwrap().to_owned();
    let selector = meta.split_once("selector: ").unwrap().1.to_owned();
    (digest, selector, body.to_owned())
}
fn apply_dry_run(root: &Path, operation: &Value) -> std::process::Output {
    ara(root)
        .args(["apply", "-", "--dry-run", "--json", "--no-duplicate-check"])
        .write_stdin(format!("{operation}\n"))
        .output()
        .unwrap()
}

#[test]
fn entry_show_prints_the_native_section_under_its_write_guard_digest() {
    let dir = artifact();
    let root = dir.path();
    let (stdout, _) = brief(root, &["show", "C01"]);
    let (shown_digest, selector, body) = show_block(&stdout);
    let start = CLAIMS.find("- **Statement**: Throughput").unwrap();
    let end = CLAIMS.find("## C02").unwrap();
    assert_eq!(body, &CLAIMS[start..end]);
    assert_eq!(shown_digest, digest(&CLAIMS[start..end]));
    let read = json(
        root,
        &[
            "show",
            "--document",
            "logic/claims.md",
            "--heading",
            "C01",
            "--source",
        ],
    );
    assert_eq!(read["entries"][0]["digest"], shown_digest.as_str());
    // The header cites the short native form, and it reads the same section.
    let cited = stdout.lines().next().unwrap().split(' ').nth(1).unwrap();
    assert_eq!(cited, "logic/claims.md#C01");
    let (again, _) = brief(root, &["show", cited]);
    assert_eq!(again, stdout);
    assert!(!stdout.contains("#h/"), "{stdout}");
    assert_eq!(
        selector,
        "--document logic/claims.md --heading Claims --heading 'C01: Mechanism'"
    );
    // The digest guards exactly the selection the selector names.
    let replace = json!({"op":"document.replace","document":"logic/claims.md","heading":["Claims","C01: Mechanism"],"expected":shown_digest,"content":"- **Statement**: New.\n- **Status**: hypothesis\n\n"});
    assert!(apply_dry_run(root, &replace).status.success());
    let mut stale = replace.clone();
    stale["expected"] = json!(digest("other"));
    let output = apply_dry_run(root, &stale);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("write.digest_conflict"));
    // JSON keeps the entry projection and gains no display data.
    let projected = json(root, &["show", "C01"]);
    assert_eq!(projected["entries"][0]["kind"], "claim");
    assert!(projected["entries"][0].get("display").is_none());
    assert!(projected["entries"][0].get("digest").is_none());
}

#[test]
fn document_and_source_reads_print_exact_bytes_and_whole_document_digests() {
    let dir = artifact();
    let root = dir.path();
    for args in [
        vec!["show", "logic/problem.md"],
        vec!["show", "--document", "logic/problem.md", "--source"],
    ] {
        let (stdout, _) = brief(root, &args);
        let (shown, selector, body) = show_block(&stdout);
        assert_eq!(shown, digest(PROBLEM));
        assert_eq!(selector, "--document logic/problem.md");
        // The source has no final newline; the display adds exactly one.
        assert_eq!(body, format!("{PROBLEM}\n"));
    }
    // A repeated heading vector keeps its source digest but names no selector.
    let (stdout, _) = brief(
        root,
        &["show", "logic/problem.md#h/Problem/Notes;occurrence=2"],
    );
    let (shown, selector, body) = show_block(&stdout);
    assert_eq!(body, "second notes\n\n");
    assert_eq!(shown, digest("second notes\n\n"));
    assert!(!selector.contains("--document"), "{selector}");
}

#[test]
fn projections_carry_no_source_digest_and_name_the_exact_source_read() {
    let dir = artifact();
    let (stdout, _) = brief(dir.path(), &["show", "N02"]);
    assert!(stdout.starts_with("== N02 "), "{stdout}");
    assert!(!stdout.contains("sha256:") && !stdout.contains("source_digest"));
    assert!(stdout.contains("throughput measured\n"));
    assert!(stdout.contains("--document trace/exploration_tree.yaml --source"));
}

/// One displayed source line: number, `:` (hit) or `-` (context), text.
type Line = (usize, char, String);
/// `(address, lines)` per brief `find` result.
fn find_results(stdout: &str) -> Vec<(String, Vec<Line>)> {
    let mut results: Vec<(String, Vec<Line>)> = Vec::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("  ") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                continue;
            }
            let marker = rest[digits.len()..].chars().next().unwrap();
            let text = rest[digits.len() + 1..].strip_prefix(' ').unwrap_or("");
            results
                .last_mut()
                .unwrap()
                .1
                .push((digits.parse().unwrap(), marker, text.to_owned()));
        } else {
            results.push((line.split(' ').next().unwrap().to_owned(), Vec::new()));
        }
    }
    results
}
fn source_line(root: &Path, path: &str, line: usize) -> String {
    std::fs::read_to_string(root.join(path))
        .unwrap()
        .lines()
        .nth(line - 1)
        .unwrap()
        .to_owned()
}

#[test]
fn find_reports_actual_source_lines_in_bm25_order() {
    let dir = artifact();
    let root = dir.path();
    let found = json(root, &["find", "THROUGHPUT"]);
    let results = found["results"].as_array().unwrap();
    // Existing fields keep their meaning; hit locations are additive.
    for result in results {
        for key in ["kind", "source", "score", "excerpt"] {
            assert!(result.get(key).is_some(), "{result}");
        }
        assert!(result.get("context").is_none());
        let source = result["source"].as_str().unwrap();
        for hit in result["matches"].as_array().unwrap() {
            let line = hit["line"].as_u64().unwrap() as usize;
            assert_eq!(hit["text"], source_line(root, source, line).as_str());
            assert!(
                hit["text"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains("throughput")
            );
        }
        assert_eq!(result["line"], result["matches"][0]["line"]);
    }
    let claim = results.iter().find(|r| r["id"] == "C01").unwrap();
    assert_eq!(claim["matches"].as_array().unwrap().len(), 2);
    assert_eq!(claim["line"], 6);
    let preamble = results
        .iter()
        .find(|r| r["key"] == "logic/claims.md")
        .unwrap();
    assert_eq!(preamble["line"], 3);
    let node = results.iter().find(|r| r["id"] == "N02").unwrap();
    assert_eq!(node["line"], 11);

    let (stdout, stderr) = brief(root, &["find", "THROUGHPUT"]);
    assert!(stderr.is_empty(), "{stderr}");
    let parsed = find_results(&stdout);
    let order: Vec<String> = results
        .iter()
        .map(|r| r["id"].as_str().or(r["key"].as_str()).unwrap().to_owned())
        .collect();
    assert_eq!(
        parsed.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        order
    );
    for ((_, lines), result) in parsed.iter().zip(results) {
        let source = result["source"].as_str().unwrap();
        assert!(!lines.is_empty());
        for (line, marker, text) in lines {
            assert_eq!(*marker, ':');
            assert_eq!(*text, source_line(root, source, *line));
        }
    }
}

#[test]
fn find_context_merges_ranges_stays_in_the_entry_and_zero_adds_nothing() {
    let dir = artifact();
    let root = dir.path();
    // C01 hits lines 6 and 8; one line of context merges them into 5..=9,
    // clipped to the section (lines 5..=9).
    let found = json(
        root,
        &["find", "throughput", "--type", "claim", "--context", "1"],
    );
    let context = &found["results"][0]["context"];
    assert_eq!(context.as_array().unwrap().len(), 1);
    assert_eq!(
        (context[0]["start"].clone(), context[0]["end"].clone()),
        (json!(5), json!(9))
    );
    let found = json(
        root,
        &["find", "throughput", "--type", "claim", "--context", "9"],
    );
    assert_eq!(found["results"][0]["context"][0]["start"], 5);
    assert_eq!(found["results"][0]["context"][0]["end"], 9);
    // `--context` is long-only; a trailing global `-C` still selects the root.
    let output = Command::cargo_bin("ara")
        .unwrap()
        .env_remove("ARA_DIR")
        .args([
            "find",
            "throughput",
            "--type",
            "claim",
            "--context",
            "1",
            "-C",
        ])
        .arg(root)
        .assert()
        .success()
        .get_output()
        .clone();
    let parsed = find_results(&String::from_utf8(output.stdout).unwrap());
    let lines: Vec<(usize, char)> = parsed[0].1.iter().map(|(n, m, _)| (*n, *m)).collect();
    assert_eq!(lines, [(5, '-'), (6, ':'), (7, '-'), (8, ':'), (9, '-')]);
    for (line, _, text) in &parsed[0].1 {
        assert_eq!(*text, source_line(root, "logic/claims.md", *line));
    }
    let (stdout, _) = brief(
        root,
        &["find", "throughput", "--type", "claim", "--context", "0"],
    );
    let parsed = find_results(&stdout);
    assert!(parsed[0].1.iter().all(|(_, marker, _)| *marker == ':'));
    assert_eq!(parsed[0].1.len(), 2);
}

#[test]
fn ls_lists_documents_entries_and_heading_addresses() {
    let dir = artifact();
    let root = dir.path();
    let (stdout, _) = brief(root, &["ls"]);
    let claims = stdout
        .lines()
        .find(|l| l.starts_with("logic/claims.md\t"))
        .unwrap();
    assert!(claims.contains("claim=2"), "{claims}");
    let tree = stdout
        .lines()
        .find(|l| l.starts_with("trace/exploration_tree.yaml\t"))
        .unwrap();
    assert!(
        tree.contains("experiment=1") && tree.contains("question=1"),
        "{tree}"
    );
    let note = stdout.lines().last().unwrap();
    for root in ["rubric/", "evidence/", "src/"] {
        assert!(note.contains(root), "{note}");
    }
    // JSON `ls` keeps listing entries.
    let listed = json(root, &["ls"]);
    assert!(listed.get("documents").is_none());
    assert_eq!(listed["entries"].as_array().unwrap().len(), 7);

    let (stdout, _) = brief(root, &["ls", "logic/claims.md"]);
    let ids: Vec<&str> = stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(ids, ["C01", "C02"]);
    let (stdout, _) = brief(root, &["ls", "--type", "claim"]);
    let ids: Vec<&str> = stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(ids, ["C01", "C02"]);

    let (stdout, _) = brief(root, &["ls", "logic/problem.md"]);
    let addresses: Vec<&str> = stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(addresses.len(), 4);
    let listed = json(root, &["ls", "logic/problem.md"]);
    for (address, row) in addresses.iter().zip(listed["entries"].as_array().unwrap()) {
        assert_eq!(row["address"], *address);
        let shown = json(root, &["show", address]);
        assert_eq!(shown["entries"][0]["heading_path"], row["heading_path"]);
    }
    let output = ara(root)
        .args(["ls", "rubric/requirements.md"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("file_access: rubric/ evidence/ src/")
    );
}

#[test]
fn diagnostics_print_once_on_stderr_and_json_keeps_them_structured() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ara-core/tests/fixtures/agent-cli");
    let value = json(&fixture, &["show", "C04"]);
    let warnings = value["diagnostics"]["warnings"].as_array().unwrap();
    for args in [
        vec!["show", "C04"],
        vec!["find", "semaphore"],
        vec!["ls"],
        vec!["open"],
    ] {
        let (_, stderr) = brief(&fixture, &args);
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert!(
            stderr.contains(&format!("{} warnings", warnings.len())),
            "{stderr}"
        );
        for warning in warnings {
            let code = warning["code"].as_str().unwrap();
            assert_eq!(stderr.matches(code).count(), 1, "{code}: {stderr}");
        }
    }
}

#[test]
fn text_errors_show_candidates_hints_and_blocking_codes() {
    let dir = artifact();
    let root = dir.path();
    let error = ara(root)
        .args(["show", "C03", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let error: Value = serde_json::from_slice(&error.stderr).unwrap();
    let output = ara(root)
        .args(["show", "C03"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(output.stdout.is_empty());
    let first = error["error"]["details"]["candidates"][0].as_str().unwrap();
    assert!(text.lines().any(|line| line.trim() == first), "{text}");

    // A refusing error names the same blocking codes in text as in JSON.
    put(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: A\n  - id: N01\n    type: question\n    title: B\n",
    );
    let error = ara(root)
        .args(["ls", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let error: Value = serde_json::from_slice(&error.stderr).unwrap();
    let blocking = error["error"]["details"]["blocking"].as_array().unwrap();
    assert!(!blocking.is_empty());
    let output = ara(root).arg("ls").assert().code(1).get_output().clone();
    let text = String::from_utf8(output.stderr).unwrap();
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("blocking:"))
        .unwrap();
    for code in blocking {
        assert!(line.contains(code.as_str().unwrap()), "{line}");
    }
}

#[test]
fn status_reports_counts_only_when_complete_and_always_the_codes() {
    let dir = artifact();
    let root = dir.path();
    let (stdout, _) = brief(root, &["status"]);
    let counts = stdout.lines().find(|l| l.starts_with("counts\t")).unwrap();
    assert!(counts.contains("claim=2"), "{counts}");
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("next_ids\t") && l.contains("C=C03"))
    );
    // A dangling evidence claim is an error: counts and next IDs withheld.
    put(
        root,
        "trace/exploration_tree.yaml",
        &TREE.replace("evidence: [C01]", "evidence: [C09]"),
    );
    let output = ara(root)
        .arg("status")
        .assert()
        .code(1)
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let errors = stdout
        .lines()
        .find(|l| l.starts_with("diagnostics\t"))
        .unwrap();
    assert!(errors.contains("ARA107"), "{stdout}");
    let counts = stdout.lines().find(|l| l.starts_with("counts\t")).unwrap();
    assert!(!counts.contains('='), "{counts}");
    assert!(
        output.stderr.is_empty(),
        "status prints its codes once, on stdout"
    );
    let status = ara(root)
        .args(["status", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(status.get("display").is_none());
    assert_eq!(status["diagnostics"]["errors"], 1);
}

#[test]
fn path_refs_and_open_lead_with_addresses() {
    let dir = artifact();
    let root = dir.path();
    let (stdout, _) = brief(root, &["path", "N02"]);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines[0].starts_with("N01\t") && lines[1].starts_with("  N02\t"),
        "{stdout}"
    );
    let (stdout, _) = brief(root, &["refs", "C01"]);
    let row = stdout.lines().find(|l| l.starts_with("N02\t")).unwrap();
    let fields: Vec<&str> = row.split('\t').collect();
    assert_eq!(fields[1], "evidence");
    let (path, line) = fields[2].split_once(':').unwrap();
    assert_eq!(path, "trace/exploration_tree.yaml");
    assert!(source_line(root, path, line.parse().unwrap()).contains("C01"));
    let (stdout, _) = brief(root, &["open"]);
    let ids: Vec<&str> = stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert!(ids.contains(&"C01") && ids.contains(&"C02"), "{stdout}");
}

fn stderr_of(root: &Path, args: &[&str], code: i32) -> String {
    let output = ara(root)
        .args(args)
        .assert()
        .code(code)
        .get_output()
        .clone();
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn fields_keep_their_row_meaning_without_json() {
    let dir = artifact();
    let root = dir.path();
    let (stdout, _) = brief(root, &["ls", "--fields", "id"]);
    let ids: Vec<&str> = stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(ids[..3], ["N01", "N02", "C01"]);
    let (stdout, _) = brief(root, &["show", "C01", "--fields", "id"]);
    let row: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        row,
        json!({"id":"C01","kind":"claim","source":"logic/claims.md"})
    );
    let (stdout, _) = brief(root, &["find", "throughput", "--fields", "id"]);
    assert!(!stdout.contains("excerpt:"), "{stdout}");
    let ids: Vec<&str> = stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    let found = json(root, &["find", "throughput", "--fields", "id"]);
    let expected: Vec<&str> = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().or(r["key"].as_str()).unwrap())
        .collect();
    assert_eq!(ids, expected);
}

#[test]
fn read_only_documents_name_no_write_selector() {
    let dir = artifact();
    let root = dir.path();
    put(root, "PAPER.md", "---\ntitle: T\n---\n# Paper\n");
    for args in [
        vec!["show", "PAPER.md"],
        vec!["show", "trace/exploration_tree.yaml"],
        vec!["show", "--document", "PAPER.md", "--source"],
    ] {
        let (stdout, _) = brief(root, &args);
        let (shown, selector, _) = show_block(&stdout);
        assert!(shown.starts_with("sha256:"));
        assert!(!selector.contains("--document"), "{selector}");
    }
    // A replaceable document keeps its selector in a source read too.
    let (stdout, _) = brief(
        root,
        &[
            "show",
            "--document",
            "logic/claims.md",
            "--heading",
            "C01",
            "--source",
        ],
    );
    assert!(
        show_block(&stdout)
            .1
            .starts_with("--document logic/claims.md")
    );
}

#[test]
fn refusals_print_counts_and_codes_not_the_report() {
    let dir = artifact();
    let root = dir.path();
    put(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: A\n  - id: N01\n    type: question\n    title: B\n",
    );
    let error = ara(root)
        .args(["ls", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let error: Value = serde_json::from_slice(&error.stderr).unwrap();
    let message = error["error"]["message"].as_str().unwrap();
    let text = stderr_of(root, &["ls"], 1);
    let first = text.lines().next().unwrap();
    assert!(first.contains("1 errors (ARA105)"), "{first}");
    // No line of the JSON message's full report appears in the text.
    for line in message.lines().filter(|line| line.contains("N01")) {
        assert!(!text.contains(line), "{text}");
    }
}

#[test]
fn candidates_cite_entry_sections_by_native_id_and_round_trip() {
    let dir = artifact();
    let root = dir.path();
    let output = ara(root)
        .args(["show", "logic/claims.md#C1", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    let candidates: Vec<&str> = error["error"]["details"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(candidates[0], "logic/claims.md#C01");
    assert!(candidates.contains(&"logic/claims.md#C02"));
    for candidate in &candidates {
        json(root, &["show", candidate]);
    }
    let text = stderr_of(root, &["show", "logic/claims.md#C1"], 1);
    assert!(
        text.lines()
            .any(|line| line.trim() == "logic/claims.md#C01"),
        "{text}"
    );
}

#[test]
fn status_names_why_a_next_id_is_unavailable() {
    let dir = artifact();
    let root = dir.path();
    put(root, "trace/logic_mutations.yaml", "mutations: []\n");
    put(
        root,
        "trace/merge_log.yaml",
        "format: other/v9\nrecords: []\n",
    );
    let status = json(root, &["status"]);
    let reason = status["next_id_errors"]["C"]["code"].as_str().unwrap();
    let (stdout, _) = brief(root, &["status"]);
    let next = stdout
        .lines()
        .find(|l| l.starts_with("next_ids\t"))
        .unwrap();
    assert!(next.contains("C=unavailable"), "{next}");
    let why = stdout.lines().find(|l| l.contains(reason)).unwrap();
    assert!(why.contains('C'), "{why}");
}

#[test]
fn find_on_crlf_sources_reports_lines_without_carriage_returns() {
    let dir = artifact();
    let root = dir.path();
    put(root, "logic/claims.md", &CLAIMS.replace('\n', "\r\n"));
    let found = json(
        root,
        &["find", "throughput", "--type", "claim", "--context", "1"],
    );
    let result = &found["results"][0];
    assert_eq!(result["id"], "C01");
    assert_eq!(
        result["matches"],
        json!([
            {"line": 6, "text": "- **Statement**: Throughput doubles under load."},
            {"line": 8, "text": "- **Proof**: Measured throughput twice."},
        ])
    );
    assert_eq!(
        (
            result["context"][0]["start"].clone(),
            result["context"][0]["end"].clone()
        ),
        (json!(5), json!(9))
    );
    let (stdout, _) = brief(root, &["find", "throughput", "--type", "claim"]);
    assert!(!stdout.contains('\r'));
    for (line, _, text) in &find_results(&stdout)[0].1 {
        assert_eq!(*text, source_line(root, "logic/claims.md", *line));
    }
}

#[test]
fn recovered_stray_fences_name_no_heading_selector() {
    let dir = artifact();
    let root = dir.path();
    let claims = CLAIMS.replace("Preamble about THROUGHPUT budgets.\n\n", "");
    put(root, "logic/claims.md", &format!("---\n\n{claims}"));
    let (stdout, stderr) = brief(root, &["show", "C01"]);
    assert!(stderr.contains("ARA228"), "{stderr}");
    let (shown, selector, body) = show_block(&stdout);
    assert_eq!(shown, digest(&body));
    assert!(!selector.contains("--document"), "{selector}");
    // The premise: the writer rejects that heading selection.
    let replace = json!({"op":"document.replace","document":"logic/claims.md","heading":["Claims","C01: Mechanism"],"expected":shown,"content":"x\n"});
    let output = apply_dry_run(root, &replace);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("write.frontmatter"));
    // A whole-document replace that removes the fence is accepted.
    let (stdout, _) = brief(root, &["show", "logic/claims.md"]);
    assert_eq!(show_block(&stdout).1, "--document logic/claims.md");
}

#[test]
fn selectors_quote_values_that_start_with_a_dash() {
    let dir = artifact();
    let root = dir.path();
    put(root, "logic/problem.md", "# -Problem\n\n## -Gap\nbody\n");
    let (stdout, _) = brief(root, &["show", "logic/problem.md#h/-Problem/-Gap"]);
    let (shown, selector, _) = show_block(&stdout);
    assert_eq!(
        selector,
        "--document logic/problem.md --heading=-Problem --heading=-Gap"
    );
    let output = ara(root)
        .args([
            "show",
            "--document",
            "logic/problem.md",
            "--heading=-Problem",
            "--heading=-Gap",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let read: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(read["entries"][0]["digest"], shown.as_str());
}

#[test]
fn session_projections_render_nested_array_items() {
    let dir = artifact();
    let root = dir.path();
    put(
        root,
        "trace/sessions/session_example.yaml",
        "session:\n  id: session_example\n  date: '2026-10-01'\n  summary: Nested lists\nevents_logged:\n  - [alpha, beta]\n  - - gamma\n    - [delta, 'eps, ilon']\n    - key: zeta\n",
    );
    let full = json(root, &["show", "session_example", "--full"]);
    assert_eq!(
        full["entries"][0]["events_logged"],
        json!([["alpha", "beta"], ["gamma", ["delta", "eps, ilon"], {"key": "zeta"}]])
    );
    for args in [
        vec!["show", "session_example"],
        vec!["show", "session_example", "--full"],
    ] {
        let (stdout, _) = brief(root, &args);
        for value in ["alpha", "beta", "gamma", "delta", "eps, ilon", "key: zeta"] {
            assert!(stdout.contains(value), "{value} missing: {stdout}");
        }
    }
}
