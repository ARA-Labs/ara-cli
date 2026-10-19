//! Bounded and resumable `show` reads: `--lines A:B` windows, the response
//! byte budget, line-boundary pagination and the limit errors. Assertions
//! check bounds, ranges, digests, error codes and exact reassembly; they do
//! not snapshot wording or layout.
use assert_cmd::Command;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use tempfile::TempDir;

const CLAIMS: &str = "# Claims\n\n## C01: Mechanism\n- **Statement**: Throughput doubles.\n- **Status**: hypothesis\n\n## C02: Latency\n- **Statement**: Latency stays flat.\n- **Status**: hypothesis\n";
const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: Root question\n    description: |-\n      a long description line\n      another line\n";

fn put(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}
/// An artifact whose `logic/problem.md` is `problem`.
fn artifact(problem: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "trace/exploration_tree.yaml", TREE);
    put(dir.path(), "logic/claims.md", CLAIMS);
    put(dir.path(), "logic/problem.md", problem);
    dir
}
fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(root);
    command
}
fn run(root: &Path, args: &[&str]) -> std::process::Output {
    ara(root).args(args).output().unwrap()
}
/// Stdout of a successful brief read.
fn text(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert!(output.status.success(), "{args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}
/// Stdout bytes and parsed value of a successful `--json` read.
fn json(root: &Path, args: &[&str]) -> (usize, Value) {
    let output = ara(root).args(args).arg("--json").output().unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    (
        output.stdout.len(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}
/// The JSON error of a rejected `--json` read, which wrote no stdout.
fn rejected(root: &Path, args: &[&str], exit: i32) -> Value {
    let output = ara(root).args(args).arg("--json").output().unwrap();
    assert_eq!(output.status.code(), Some(exit), "{args:?}: {output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"].clone()
}
/// The same rejection in brief text: no stdout, a coded error on stderr.
fn rejected_text(root: &Path, args: &[&str], code: &str) -> String {
    let output = run(root, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with(&format!("error [{code}]")), "{stderr}");
    stderr
}
/// The budget a brief `output_limit_too_small` error names.
fn required(stderr: &str) -> usize {
    let (_, rest) = stderr.split_once("needs at least ").unwrap();
    rest.split_once(' ').unwrap().0.parse().unwrap()
}
fn digest(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}
/// One-based inclusive lines of `text`, each with its terminator.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// A parsed single-block brief page.
#[derive(Debug)]
struct Page {
    digest: String,
    body: String,
    /// `(start, end, total)`; `None` when the block printed no range.
    range: Option<(usize, usize, usize)>,
    next: Option<String>,
}
/// Parse a single-block page: the header up to the `source_digest=` line,
/// the body, and the final `lines:` range line when there is one.
fn page(stdout: &str) -> Page {
    assert!(stdout.starts_with("== "), "{stdout}");
    let (_, rest) = stdout.split_once("\nsource_digest=").unwrap();
    let (meta, mut body) = rest.split_once('\n').unwrap();
    let digest = meta.split_whitespace().next().unwrap().to_owned();
    let (mut range, mut next) = (None, None);
    let trailer = body
        .trim_end_matches('\n')
        .rfind('\n')
        .map_or(0, |at| at + 1);
    if let Some(line) = body[trailer..].strip_prefix("lines: ") {
        body = &body[..trailer];
        let mut parts = line.trim_end().split("; ");
        let shown = parts.next().unwrap();
        let (span, total) = shown.split_once(" of ").unwrap();
        let total = total.parse().unwrap();
        range = Some(match span {
            "none" => (0, 0, total),
            span => {
                let (start, end) = span.split_once('-').unwrap();
                (start.parse().unwrap(), end.parse().unwrap(), total)
            }
        });
        for part in parts {
            if let Some(window) = part.strip_prefix("next: --lines ") {
                next = Some(window.to_owned());
            }
        }
    }
    Page {
        digest: digest.trim_start_matches("source_digest=").to_owned(),
        body: body.to_owned(),
        range,
        next,
    }
}

/// A document with CRLF lines, multibyte code points (2-, 3- and 4-byte)
/// and no final newline.
fn mixed_document(count: usize) -> String {
    let mut text = String::from("# Problem\r\n\r\n");
    for line in 0..count {
        let tail = ["é", "€", "😀", "plain"][line % 4];
        let ending = if line % 3 == 0 { "\r\n" } else { "\n" };
        text.push_str(&format!(
            "line {line} {}{ending}",
            tail.repeat(line % 7 + 1)
        ));
    }
    text.push_str("last line without newline 😀");
    text
}

/// Follow `next` from `first` until a page is not truncated, asserting each
/// stdout fits `budget` and each truncated page is maximal; returns the pages.
fn follow(
    root: &Path,
    source: &str,
    base: &[&str],
    first: Option<&str>,
    budget: usize,
) -> Vec<Page> {
    let budget_text = budget.to_string();
    let mut window = first.map(str::to_owned);
    let mut pages = Vec::new();
    loop {
        let mut args = base.to_vec();
        args.extend(["--max-bytes", &budget_text]);
        if let Some(window) = &window {
            args.extend(["--lines", window]);
        }
        let stdout = text(root, &args);
        assert!(stdout.len() <= budget, "{} > {budget}", stdout.len());
        let page = page(&stdout);
        if page.next.is_some() {
            assert!(grown(&stdout, source, &page).len() > budget, "{page:?}");
        }
        window = page.next.clone();
        pages.push(page);
        if window.is_none() {
            return pages;
        }
        assert!(pages.len() < 10_000, "no progress");
    }
}

/// The exact stdout of a truncated page's read had the page held one more
/// line: the same header, the extra line, and its range line.
fn grown(stdout: &str, source: &str, page: &Page) -> String {
    let (start, end, total) = page.range.unwrap();
    let next = page.next.as_deref().unwrap();
    let upper = next.split_once(':').unwrap().1;
    let last = upper.parse().map_or(total, |upper: usize| upper.min(total));
    let trailer = stdout.trim_end_matches('\n').rfind('\n').unwrap() + 1;
    let extra = lines(source)[end];
    let mut text = format!("{}{extra}", &stdout[..trailer]);
    if !extra.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("lines: {start}-{} of {total}", end + 1));
    if end + 1 < last {
        text.push_str(&format!("; truncated; next: --lines {}:{upper}", end + 2));
    }
    text.push('\n');
    text
}

#[test]
fn line_and_budget_arguments_reject_before_any_output() {
    let dir = artifact("# Problem\n\nbody\n");
    let root = dir.path();
    let base = ["show", "--document", "logic/problem.md"];
    for spec in [
        "0:2",
        "1:0",
        "-1:2",
        "3:2",
        "a:2",
        "1:b",
        ":",
        "",
        "2",
        "1:2:3",
        "+1:2",
        " 1:2",
        "1:99999999999999999999999",
    ] {
        let mut args = base.to_vec();
        args.extend(["--lines", spec]);
        assert_eq!(rejected(root, &args, 2)["code"], "invalid_lines", "{spec}");
    }
    for budget in ["0", "-5", "abc", "+5", "1e3", "", "99999999999999999999999"] {
        let mut args = base.to_vec();
        args.extend(["--max-bytes", budget]);
        assert_eq!(
            rejected(root, &args, 2)["code"],
            "invalid_max_bytes",
            "{budget}"
        );
    }
    // `--fields` keeps its row semantics and cannot combine with bounds.
    for bound in [["--lines", "1:2"], ["--max-bytes", "100"]] {
        let mut args = base.to_vec();
        args.extend(bound);
        args.extend(["--fields", "content"]);
        assert_eq!(rejected(root, &args, 2)["code"], "argument_error");
    }
    // Argument errors reject before the artifact or the source is read.
    let empty = TempDir::new().unwrap();
    let args = ["show", "--document", "logic/problem.md", "--source"];
    let mut bad = args.to_vec();
    bad.extend(["--lines", "0:1"]);
    assert_eq!(rejected(empty.path(), &bad, 2)["code"], "invalid_lines");
}

#[test]
fn line_windows_are_inclusive_clamped_and_keep_the_full_digest() {
    let source = "# Problem\r\n\r\nalpha é\nbeta 😀\r\ngamma";
    let dir = artifact(source);
    let root = dir.path();
    let all = lines(source);
    assert_eq!(all.len(), 5);
    let read = |spec: &str| {
        json(
            root,
            &["show", "--document", "logic/problem.md", "--lines", spec],
        )
        .1["entries"][0]
            .clone()
    };
    for (spec, start, end) in [
        ("2:4", 2, 4),
        ("3:", 3, 5),
        (":2", 1, 2),
        ("4:999", 4, 5),
        ("5:5", 5, 5),
        ("1:", 1, 5),
    ] {
        let row = read(spec);
        assert_eq!(row["content"], all[start - 1..end].concat(), "{spec}");
        assert_eq!(row["digest"], digest(source), "{spec}");
        assert_eq!(row["display"]["lines"]["start"], start, "{spec}");
        assert_eq!(row["display"]["lines"]["end"], end, "{spec}");
        assert_eq!(row["display"]["lines"]["total"], 5, "{spec}");
        assert_eq!(row["display"]["truncated"], false, "{spec}");
        assert!(row["display"]["next"].is_null(), "{spec}");
        assert_eq!(row["display"]["scope"], "whole_document");
        assert_eq!(row["display"]["selector"]["document"], "logic/problem.md");
    }
    // An explicit start beyond EOF rejects; an end beyond EOF does not.
    let error = rejected(
        root,
        &["show", "--document", "logic/problem.md", "--lines", "6:"],
        1,
    );
    assert_eq!(error["code"], "line_out_of_range");
    assert_eq!(error["details"]["total"], 5);
    // A final newline belongs to its line: two lines, not three.
    let dir = artifact("# P\nx\n");
    let row = json(
        dir.path(),
        &["show", "--document", "logic/problem.md", "--lines", "2:"],
    )
    .1["entries"][0]
        .clone();
    assert_eq!(row["content"], "x\n");
    assert_eq!(row["display"]["lines"]["total"], 2);
    let error = rejected(
        dir.path(),
        &["show", "--document", "logic/problem.md", "--lines", "3:"],
        1,
    );
    assert_eq!(error["code"], "line_out_of_range");
}

#[test]
fn empty_selections_succeed_unless_the_start_is_explicitly_beyond_eof() {
    let dir = artifact("");
    let root = dir.path();
    let base = ["show", "--document", "logic/problem.md", "--source"];
    let mut args = base.to_vec();
    args.extend(["--lines", ":5"]);
    let row = json(root, &args).1["entries"][0].clone();
    assert_eq!(row["content"], "");
    assert_eq!(row["digest"], digest(""));
    assert_eq!(row["display"]["lines"]["total"], 0);
    assert_eq!(row["display"]["truncated"], false);
    let page = page(&text(root, &args));
    assert_eq!((page.body.as_str(), page.range), ("", Some((0, 0, 0))));
    let mut args = base.to_vec();
    args.extend(["--lines", "1:"]);
    assert_eq!(rejected(root, &args, 1)["code"], "line_out_of_range");
    // An empty selection with no window reads as before.
    assert_eq!(page_of(root, &base).body, "");
}
fn page_of(root: &Path, args: &[&str]) -> Page {
    page(&text(root, args))
}

#[test]
fn text_pages_reassemble_the_selection_exactly_without_gaps_or_overlap() {
    let source = mixed_document(400);
    let dir = artifact(&source);
    let root = dir.path();
    let all = lines(&source);
    for base in [
        vec!["show", "logic/problem.md"],
        vec!["show", "--document", "logic/problem.md", "--source"],
    ] {
        let pages = follow(root, &source, &base, None, 1024);
        assert!(pages.len() > 5, "{}", pages.len());
        let mut expected_start = 1;
        let mut body = String::new();
        for page in &pages {
            // Every page carries the full selection's digest, never a page hash.
            assert_eq!(page.digest, digest(&source));
            let (start, end, total) = page.range.unwrap();
            assert_eq!((start, total), (expected_start, all.len()));
            assert!(end >= start, "a page makes progress");
            expected_start = end + 1;
            body.push_str(&page.body);
        }
        assert_eq!(expected_start, all.len() + 1);
        // The source lacks a final newline; the last page's display adds one.
        assert_eq!(body, format!("{source}\n"));
    }
}

#[test]
fn a_window_paginates_within_its_original_upper_bound() {
    let source = mixed_document(300);
    let dir = artifact(&source);
    let root = dir.path();
    let all = lines(&source);
    let base = [
        "show",
        "--document",
        "logic/problem.md",
        "--source",
        "--full",
    ];
    let pages = follow(root, &source, &base, Some("50:250"), 900);
    assert!(pages.len() > 2);
    for page in &pages[..pages.len() - 1] {
        assert!(page.next.as_deref().unwrap().ends_with(":250"), "{page:?}");
    }
    let body: String = pages.iter().map(|page| page.body.as_str()).collect();
    assert_eq!(body, all[49..250].concat());
    assert_eq!(pages.last().unwrap().range.unwrap().1, 250);
    // An open upper bound stays open.
    let pages = follow(root, &source, &base, Some("200:"), 900);
    assert!(pages.len() > 1);
    assert!(pages[0].next.as_deref().unwrap().ends_with(':'));
    let body: String = pages.iter().map(|page| page.body.as_str()).collect();
    assert_eq!(body, format!("{}\n", all[199..].concat()));
}

#[test]
fn json_pages_are_complete_envelopes_that_reassemble_exactly() {
    let source = mixed_document(300);
    let dir = artifact(&source);
    let root = dir.path();
    let all = lines(&source);
    let budget = 2000;
    let mut window = "1:".to_owned();
    let mut content = String::new();
    let mut expected_start = 1;
    loop {
        let (size, value) = json(
            root,
            &[
                "show",
                "--document",
                "logic/problem.md",
                "--source",
                "--max-bytes",
                &budget.to_string(),
                "--lines",
                &window,
            ],
        );
        assert!(size <= budget, "{size}");
        let row = &value["entries"][0];
        assert_eq!(row["digest"], digest(&source));
        let shown = &row["display"]["lines"];
        assert_eq!(shown["start"], expected_start);
        assert_eq!(shown["total"], all.len());
        expected_start = shown["end"].as_u64().unwrap() as usize + 1;
        content.push_str(row["content"].as_str().unwrap());
        match row["display"]["next"].as_str() {
            Some(next) => {
                assert_eq!(row["display"]["truncated"], true);
                window = next.to_owned();
            }
            None => {
                assert_eq!(row["display"]["truncated"], false);
                break;
            }
        }
    }
    assert_eq!(content, source);
    assert_eq!(expected_start, all.len() + 1);
}

#[test]
fn brief_show_defaults_to_16_kib_but_json_and_source_full_stay_unbounded() {
    let source = mixed_document(2000);
    assert!(source.len() > 3 * 16 * 1024);
    let dir = artifact(&source);
    let root = dir.path();
    for args in [
        vec!["show", "logic/problem.md"],
        vec!["show", "--document", "logic/problem.md"],
        vec!["show", "--document", "logic/problem.md", "--source"],
    ] {
        let stdout = text(root, &args);
        assert!(stdout.len() <= 16 * 1024, "{}", stdout.len());
        let page = page(&stdout);
        assert!(page.next.is_some(), "{args:?}");
        assert_eq!(page.digest, digest(&source));
    }
    // Existing JSON and explicit `--source --full` reads are unchanged.
    let (_, value) = json(
        root,
        &[
            "show",
            "--document",
            "logic/problem.md",
            "--source",
            "--full",
        ],
    );
    assert_eq!(value["entries"][0]["content"], source);
    assert!(value["entries"][0].get("display").is_none());
    let (_, value) = json(root, &["show", "--document", "logic/problem.md"]);
    assert!(value["entries"][0].get("display").is_none());
    let full = page_of(
        root,
        &[
            "show",
            "--document",
            "logic/problem.md",
            "--source",
            "--full",
        ],
    );
    assert_eq!(full.body, format!("{source}\n"));
    assert!(full.range.is_none());
    // A read that fits prints no range metadata.
    let small = artifact("# Problem\n\nshort\n");
    let page = page_of(small.path(), &["show", "logic/problem.md"]);
    assert_eq!(page.body, "# Problem\n\nshort\n");
    assert!(page.range.is_none());
}

#[test]
fn an_oversized_line_returns_the_minimum_budget_instead_of_an_empty_page() {
    let huge = "x".repeat(1 << 20);
    let source = format!("# Problem\nshort\n{huge}\ntail\n");
    let dir = artifact(&source);
    let root = dir.path();
    let base = ["show", "--document", "logic/problem.md", "--source"];
    // The first page stops before the megabyte line.
    let first = page_of(root, &base);
    assert_eq!(first.body, "# Problem\nshort\n");
    assert_eq!(first.next.as_deref(), Some("3:"));
    let mut args = base.to_vec();
    args.extend(["--lines", "3:"]);
    let stderr = rejected_text(root, &args, "output_limit_too_small");
    let required = required(&stderr);
    assert!(required > huge.len(), "{required}");
    assert!(
        stderr.contains(&format!("--max-bytes {required}")),
        "{stderr}"
    );
    // JSON has no default budget; an explicit one reports the same fields.
    let mut explicit = args.clone();
    explicit.extend(["--max-bytes", "16384"]);
    let error = rejected(root, &explicit, 1);
    assert_eq!(error["code"], "output_limit_too_small");
    assert_eq!(error["details"]["line"], 3);
    assert_eq!(error["details"]["max_bytes"], 16 * 1024);
    assert!(error["details"]["required"].as_u64().unwrap() as usize > huge.len());
    // The named minimum makes progress; one byte less does not.
    let required_text = required.to_string();
    let mut fits = args.clone();
    fits.extend(["--max-bytes", &required_text]);
    let stdout = text(root, &fits);
    assert!(stdout.len() <= required);
    // The cheapest page that includes line 3: here the whole remainder,
    // whose range line is shorter than a truncated one with `next`.
    let page = page(&stdout);
    assert!(page.body.starts_with(&format!("{huge}\n")));
    assert_eq!(page.range.unwrap().0, 3);
    let less = (required - 1).to_string();
    let mut short = args.clone();
    short.extend(["--max-bytes", &less]);
    rejected_text(root, &short, "output_limit_too_small");
    // A one-line megabyte document with no final newline.
    let dir = artifact(&huge);
    let stderr = rejected_text(
        dir.path(),
        &["show", "logic/problem.md"],
        "output_limit_too_small",
    );
    assert!(stderr.contains("Line 1 "), "{stderr}");
}

#[test]
fn tiny_budgets_count_metadata_and_never_loop_without_progress() {
    let dir = artifact("# Problem\n\nbody\n");
    let root = dir.path();
    let stderr = rejected_text(
        root,
        &["show", "logic/problem.md", "--max-bytes", "1"],
        "output_limit_too_small",
    );
    let required = required(&stderr);
    // The minimum covers the header and digest line, not only the body.
    assert!(required > 100, "{required}");
    let budget = required.to_string();
    let stdout = text(root, &["show", "logic/problem.md", "--max-bytes", &budget]);
    assert!(stdout.len() <= required);
    assert!(!page(&stdout).body.is_empty(), "the minimum makes progress");
}

#[test]
fn multi_address_reads_fit_together_or_reject_before_output() {
    let dir = artifact("# Problem\n\nbody\n");
    let root = dir.path();
    let both = text(root, &["show", "C01", "C02"]);
    assert_eq!(both.matches("\n== ").count() + 1, 2);
    // Windows apply to each selection; order is kept.
    let (_, value) = json(root, &["show", "C02", "C01", "--lines", "1:1"]);
    let rows = value["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["content"], "- **Statement**: Latency stays flat.\n");
    assert_eq!(rows[1]["content"], "- **Statement**: Throughput doubles.\n");
    assert_eq!(rows[0]["display"]["lines"]["total"], 2);
    // Overlapping requested selections stay separate items.
    let (_, value) = json(
        root,
        &["show", "C01", "logic/claims.md#C01", "--lines", ":1"],
    );
    assert_eq!(value["entries"].as_array().unwrap().len(), 2);
    // Too large together: an error naming the aggregate budget, no stdout.
    let budget = (both.len() - 1).to_string();
    let stderr = rejected_text(
        root,
        &["show", "C01", "C02", "--max-bytes", &budget],
        "output_limit_too_small",
    );
    assert_eq!(required(&stderr), both.len());
    assert!(stderr.contains("separately"), "{stderr}");
    let budget = both.len().to_string();
    assert_eq!(
        text(root, &["show", "C01", "C02", "--max-bytes", &budget]),
        both
    );
    // One selection out of range rejects the whole read.
    let error = rejected(root, &["show", "C01", "C02", "--lines", "3:"], 1);
    assert_eq!(error["code"], "line_out_of_range");
    // Errors name a selection by its cited address, not its `#h/` form.
    assert_eq!(error["id"], "logic/claims.md#C02");
    let stderr = rejected_text(root, &["show", "C01", "--lines", "9:"], "line_out_of_range");
    assert!(
        stderr.contains("`logic/claims.md#C01`") && !stderr.contains("#h/"),
        "{stderr}"
    );
    let error = rejected(
        root,
        &["show", "C01", "--lines", "1:", "--max-bytes", "10"],
        1,
    );
    assert_eq!(error["id"], "logic/claims.md#C01");
    let error = rejected(root, &["show", "C01", "C02", "--max-bytes", "10"], 1);
    assert!(error.get("id").is_none());
}

#[test]
fn projections_reject_line_windows_and_oversize_with_an_exact_source_read() {
    let dir = artifact("# Problem\n");
    let root = dir.path();
    let error = rejected(root, &["show", "N01", "--lines", "1:2"], 1);
    assert_eq!(error["code"], "lines_unavailable");
    let hint = error["details"]["hint"].as_str().unwrap();
    assert!(
        hint.contains("--document trace/exploration_tree.yaml --source"),
        "{hint}"
    );
    rejected_text(
        root,
        &["show", "N01", "--lines", "1:2"],
        "lines_unavailable",
    );
    let error = rejected(root, &["show", "N01", "--max-bytes", "40"], 1);
    assert_eq!(error["code"], "output_limit_too_small");
    assert!(error["details"].get("line").is_none());
    let hint = error["details"]["hint"].as_str().unwrap();
    assert!(hint.contains("--source"), "{hint}");
    // An entry with a native section takes line windows in JSON too.
    let (_, value) = json(root, &["show", "C01", "--lines", "2:"]);
    let row = &value["entries"][0];
    assert_eq!(row["kind"], "source_document");
    assert_eq!(row["content"], "- **Status**: hypothesis\n\n");
    assert_eq!(row["display"]["scope"], "heading_body");
    assert_eq!(
        row["display"]["selector"]["heading"],
        serde_json::json!(["Claims", "C01: Mechanism"])
    );
}

#[test]
fn a_changed_source_changes_the_digest_between_pages() {
    let source = mixed_document(300);
    let dir = artifact(&source);
    let root = dir.path();
    let base = ["show", "logic/problem.md", "--max-bytes", "1024"];
    let first = page_of(root, &base);
    let next = first.next.clone().unwrap();
    let mut changed = source.clone();
    changed.insert_str(source.find("line 0").unwrap(), "inserted\n");
    put(root, "logic/problem.md", &changed);
    let mut args = base.to_vec();
    args.extend(["--lines", &next]);
    let second = page_of(root, &args);
    assert_eq!(first.digest, digest(&source));
    assert_eq!(second.digest, digest(&changed));
    assert_ne!(first.digest, second.digest, "the caller must restart");
}

#[test]
fn json_budgets_are_explicit_and_never_cut_an_envelope() {
    let source = mixed_document(50);
    let dir = artifact(&source);
    let root = dir.path();
    let (size, value) = json(root, &["show", "C01", "C02", "--max-bytes", "100000"]);
    assert!(size <= 100_000);
    // An explicit budget keeps entry projections in JSON.
    assert_eq!(value["entries"][0]["kind"], "claim");
    let error = rejected(root, &["show", "C01", "C02", "--max-bytes", "50"], 1);
    assert_eq!(error["code"], "output_limit_too_small");
    assert_eq!(error["details"]["required"], size);
    // A bounded source read carries display metadata even when it fits.
    let (size, value) = json(
        root,
        &[
            "show",
            "--document",
            "logic/problem.md",
            "--max-bytes",
            "100000",
        ],
    );
    assert!(size <= 100_000);
    let row = &value["entries"][0];
    assert_eq!(row["content"], source);
    assert_eq!(row["display"]["truncated"], false);
    assert_eq!(row["display"]["lines"]["total"], lines(&source).len());
}

#[test]
fn every_native_page_keeps_complete_relations_and_multi_selection_never_pages() {
    let dir = artifact("# Problem\n");
    let root = dir.path();
    let body = (0..100)
        .map(|index| format!("native source line {index:03} {}\n", "x".repeat(80)))
        .collect::<String>();
    put(
        root,
        "logic/claims.md",
        &format!(
            "# Claims\n\n## C01: Large native selection\n- **Statement**: A bounded source keeps its citations.\n{body}\n## C02: Citing selection\n- **Statement**: The source is cited.\n- **Dependencies**: [C01]\n",
        ),
    );
    let selection = ["show", "C01", "--with", "refs", "--lines", "1:"];
    let (_, complete) = json(root, &selection);
    let relations = complete["entries"][0]["relations"].clone();
    assert_eq!(relations["refs"]["structured"][0]["id"], "C02");
    let error = rejected(root, &[&selection[..], &["--max-bytes", "1"]].concat(), 1);
    assert_eq!(error["code"], "output_limit_too_small");
    let budget = (error["details"]["required"].as_u64().unwrap() + 512).to_string();
    let (bytes, first) = json(root, &[&selection[..], &["--max-bytes", &budget]].concat());
    assert!(bytes <= budget.parse::<usize>().unwrap());
    assert_eq!(first["entries"][0]["relations"], relations);
    let next = first["entries"][0]["display"]["next"].as_str().unwrap();
    let (_, second) = json(
        root,
        &[
            "show",
            "C01",
            "--with",
            "refs",
            "--lines",
            next,
            "--max-bytes",
            &budget,
        ],
    );
    assert_eq!(second["entries"][0]["relations"], relations);
    assert_ne!(
        first["entries"][0]["content"],
        second["entries"][0]["content"]
    );
    let multi = rejected(
        root,
        &["show", "C01", "C02", "--with", "refs", "--max-bytes", "1"],
        1,
    );
    assert_eq!(multi["code"], "output_limit_too_small");
    assert!(multi["details"]["required"].as_u64().unwrap() > 1);
}
