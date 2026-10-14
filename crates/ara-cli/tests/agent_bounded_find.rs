//! Plan 22 part D: bounded `find` output. A per-hit line cap, a byte budget
//! that drops whole hits from the end, a document filter and an
//! every-term filter. Each property is checked over a grid of queries and
//! bounds on one generated artifact.
use assert_cmd::Command;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use tempfile::TempDir;

const TERMS: [&str; 4] = ["alpha", "beta", "gamma", "delta"];
const QUERIES: [&str; 6] = [
    "alpha",
    "alpha beta",
    "beta gamma delta",
    "alpha beta gamma delta",
    "gamma",
    "delta alpha",
];
/// A budget no response here reaches: the 0.1.31 unbounded output.
const UNBOUNDED: [&str; 4] = ["--lines-per-hit", "0", "--max-bytes", "100000000"];

fn put(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// Term combinations from a fixed linear congruential sequence, so every
/// document mixes the terms differently but the artifact is reproducible.
fn words(seed: &mut u64, count: usize) -> String {
    (0..count)
        .map(|_| {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            match (*seed >> 33) % 6 {
                index @ 0..=3 => TERMS[index as usize],
                4 => "filler",
                _ => "words",
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn artifact() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let mut seed = 7;
    put(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Root alpha\n",
    );
    let mut claims = String::from("# Claims\n\n");
    for id in 1..=8 {
        claims.push_str(&format!(
            "## C{id:02}: Claim {}\n- **Statement**: {}\n- **Status**: hypothesis\n- **Tags**: {}\n\n",
            words(&mut seed, 2),
            words(&mut seed, 6),
            words(&mut seed, 3),
        ));
    }
    put(root, "logic/claims.md", &claims);
    // Long whole documents: many matched lines per hit, under headings.
    for name in ["problem", "solution/notes", "solution/design"] {
        let mut text = format!("Intro {}.\n\n# {name}\n\n", words(&mut seed, 3));
        for section in 1..=4 {
            text.push_str(&format!("## Part {section}\n\n"));
            for _ in 0..6 {
                text.push_str(&format!("{}\n", words(&mut seed, 5)));
            }
            text.push('\n');
        }
        put(root, &format!("logic/{name}.md"), &text);
    }
    dir
}

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(root).args(args);
    command.output().unwrap()
}
fn ok(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn ok_json(root: &Path, args: &[&str]) -> Value {
    let mut args = args.to_vec();
    args.push("--json");
    serde_json::from_str(&ok(root, &args)).unwrap()
}
fn find<'a>(query: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["find", query, "--limit", "40"];
    args.extend(extra);
    args
}

/// One printed hit: its header and the lines under it.
struct Hit {
    header: String,
    body: Vec<String>,
}
/// Hits of brief `find` text, and the closing budget note if any.
fn hits(text: &str) -> (Vec<Hit>, Option<String>) {
    let mut hits: Vec<Hit> = Vec::new();
    let mut note = None;
    for line in text.lines() {
        if line.starts_with("  ") {
            hits.last_mut().unwrap().body.push(line.to_owned());
        } else if line.ends_with("narrow with --type, --document or more specific terms") {
            note = Some(line.to_owned());
        } else {
            hits.push(Hit {
                header: line.to_owned(),
                body: Vec::new(),
            });
        }
    }
    (hits, note)
}
/// The line number of a matched (`N:`) line.
fn matched(line: &str) -> Option<u64> {
    let (number, _) = line.trim_start().split_once(": ")?;
    number.parse().ok()
}
fn headers(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.header.as_str()).collect()
}
/// `(kind, source, id or key)` of each JSON result, in order.
fn identities(value: &Value) -> Vec<(String, String, String)> {
    value["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or("").to_owned();
            let id = row["id"].as_str().or(row["key"].as_str()).unwrap_or("");
            (text("kind"), text("source"), id.to_owned())
        })
        .collect()
}

#[test]
fn the_line_cap_keeps_the_first_matched_lines_of_each_hit_in_order() {
    let dir = artifact();
    let root = dir.path();
    for query in QUERIES {
        for context in [None, Some("2")] {
            let with = |extra: &[&str]| {
                let mut extra = extra.to_vec();
                extra.extend(context.map(|n| ["--context", n]).into_iter().flatten());
                ok(root, &find(query, &extra))
            };
            let (all, _) = hits(&with(&UNBOUNDED));
            for cap in [1usize, 2, 3, 5] {
                let cap_text = cap.to_string();
                let (capped, note) = hits(&with(&[
                    "--lines-per-hit",
                    &cap_text,
                    "--max-bytes",
                    "100000000",
                ]));
                assert_eq!(headers(&capped), headers(&all), "{query} cap {cap}");
                assert!(note.is_none());
                for (hit, full) in capped.iter().zip(&all) {
                    let kept: Vec<u64> = hit.body.iter().filter_map(|l| matched(l)).collect();
                    let every: Vec<u64> = full.body.iter().filter_map(|l| matched(l)).collect();
                    let expected = &every[..every.len().min(cap)];
                    assert_eq!(kept, expected, "{query} cap {cap} {}", hit.header);
                    let more = hit
                        .body
                        .iter()
                        .find_map(|line| line.strip_prefix("  +"))
                        .map(|rest| rest.split(' ').next().unwrap().parse::<usize>().unwrap());
                    let cut = every.len() - kept.len();
                    assert_eq!(more, (cut > 0).then_some(cut), "{}", hit.header);
                    // An `in:` line always heads a kept matched line.
                    for (index, line) in hit.body.iter().enumerate() {
                        if line.starts_with("  in: ") {
                            assert!(matched(&hit.body[index + 1]).is_some(), "{line}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn brief_text_defaults_to_three_lines_and_json_to_twenty() {
    let dir = artifact();
    let root = dir.path();
    let query = "alpha beta gamma delta";
    let explicit = ok(root, &find(query, &["--lines-per-hit", "3"]));
    assert_eq!(ok(root, &find(query, &[])), explicit);
    let json = ok_json(root, &find(query, &[]));
    assert_eq!(
        json,
        ok_json(root, &find(query, &["--lines-per-hit", "20"]))
    );
    let all = ok_json(root, &find(query, &["--lines-per-hit", "0"]));
    let rows = all["results"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|row| row["matches"].as_array().unwrap().len() > 20)
    );
    for row in rows {
        assert_eq!(
            row["matches"].as_array().unwrap().len() as u64,
            row["match_count"].as_u64().unwrap()
        );
    }
    assert!(json.get("truncation").is_none());
}

#[test]
fn output_within_every_limit_is_unchanged() {
    let dir = artifact();
    let root = dir.path();
    // Claim hits carry at most three matched lines, so neither bound acts.
    for query in QUERIES {
        for extra in [
            &["--type", "claim"][..],
            &["--type", "claim", "--context", "1"],
        ] {
            let unbounded: Vec<&str> = extra.iter().chain(&UNBOUNDED).copied().collect();
            assert_eq!(
                ok(root, &find(query, extra)),
                ok(root, &find(query, &unbounded)),
                "{query} {extra:?}"
            );
        }
    }
}

#[test]
fn the_budget_drops_whole_hits_from_the_end_and_counts_them() {
    let dir = artifact();
    let root = dir.path();
    let (mut dropped, mut cut_first) = (0, 0);
    for query in QUERIES {
        let full = ok(root, &find(query, &["--max-bytes", "100000000"]));
        let (all, _) = hits(&full);
        for budget in (200..full.len()).step_by(97) {
            let budget_text = budget.to_string();
            let output = run(root, &find(query, &["--max-bytes", &budget_text]));
            if output.status.code() != Some(0) {
                let error: Value = serde_json::from_slice(
                    &run(root, &find(query, &["--max-bytes", &budget_text, "--json"])).stderr,
                )
                .unwrap();
                assert_eq!(error["error"]["code"], "output_limit_too_small");
                continue;
            }
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.len() <= budget, "{query}: {} > {budget}", text.len());
            let (kept, note) = hits(&text);
            let note = note.expect("a cut response says so");
            let all_headers = headers(&all);
            assert_eq!(headers(&kept), all_headers[..kept.len()], "{query}");
            let omitted = all.len() - kept.len();
            dropped += usize::from(omitted > 0);
            cut_first += usize::from(note.starts_with("first hit cut"));
            if omitted > 0 {
                let plural = if omitted == 1 { "" } else { "s" };
                assert!(
                    note.contains(&format!("{omitted} more hit{plural};")),
                    "{note}"
                );
            }
            // Every hit but a cut first one prints whole.
            let whole = if note.starts_with("first hit cut") {
                1
            } else {
                0
            };
            for (hit, original) in kept.iter().zip(&all).skip(whole) {
                assert_eq!(hit.body, original.body, "{}", hit.header);
            }
        }
    }
    assert!(dropped > 10 && cut_first > 0, "{dropped} {cut_first}");
}

#[test]
fn a_first_hit_over_the_budget_is_cut_at_a_matched_line() {
    let dir = artifact();
    let root = dir.path();
    let args = |budget: &'static str| {
        find(
            "alpha beta gamma delta",
            &[
                "--type",
                "source_document",
                "--lines-per-hit",
                "0",
                "--context",
                "1",
                "--max-bytes",
                budget,
            ][..],
        )
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    };
    let strings = args("400");
    let refs: Vec<&str> = strings.iter().map(String::as_str).collect();
    let text = ok(root, &refs);
    assert!(text.len() <= 400, "{text}");
    let (kept, note) = hits(&text);
    assert_eq!(kept.len(), 1);
    let note = note.unwrap();
    // `logic/problem.md` is the one whole-document hit.
    assert_eq!(
        note,
        "first hit cut to fit the byte budget; narrow with --type, --document or more specific terms"
    );
    assert!(kept[0].body.iter().any(|line| line.starts_with("  +")));
    // Too small for one matched line: rejected with the budget it needs.
    let strings = args("60");
    let mut refs: Vec<&str> = strings.iter().map(String::as_str).collect();
    refs.push("--json");
    let output = run(root, &refs);
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["error"]["code"], "output_limit_too_small");
    let required = error["error"]["details"]["required"]
        .as_u64()
        .unwrap()
        .to_string();
    refs.pop();
    *refs.last_mut().unwrap() = &required;
    let text = ok(root, &refs);
    assert!(text.len() <= required.parse().unwrap());
}

#[test]
fn an_explicit_json_budget_drops_whole_hits_and_reports_them() {
    let dir = artifact();
    let root = dir.path();
    let full = ok_json(root, &find("alpha beta", &[]));
    let budget = serde_json::to_string(&full).unwrap().len() / 2;
    let budget_text = budget.to_string();
    let text = ok(
        root,
        &find("alpha beta", &["--max-bytes", &budget_text, "--json"]),
    );
    assert!(text.len() <= budget);
    let cut: Value = serde_json::from_str(&text).unwrap();
    let kept = identities(&cut);
    assert_eq!(kept, identities(&full)[..kept.len()]);
    assert_eq!(
        cut["truncation"]["omitted_hits"].as_u64().unwrap() as usize,
        identities(&full).len() - kept.len()
    );
}

#[test]
fn a_document_filter_keeps_exactly_that_documents_hits_in_order() {
    let dir = artifact();
    let root = dir.path();
    for query in QUERIES {
        let all = identities(&ok_json(root, &find(query, &[])));
        let sources: BTreeSet<String> = all.iter().map(|(_, source, _)| source.clone()).collect();
        for source in &sources {
            let only = identities(&ok_json(root, &find(query, &["--document", source])));
            let expected: Vec<_> = all.iter().filter(|hit| &hit.1 == source).cloned().collect();
            assert_eq!(only, expected, "{query} --document {source}");
        }
        let two: Vec<&String> = sources.iter().take(2).collect();
        let both = identities(&ok_json(
            root,
            &find(query, &["--document", two[0], "--document", two[1]]),
        ));
        let expected: Vec<_> = all
            .iter()
            .filter(|hit| two.contains(&&hit.1))
            .cloned()
            .collect();
        assert_eq!(both, expected, "{query}");
    }
}

#[test]
fn documents_outside_the_boundary_fail_as_show_does() {
    let dir = artifact();
    let root = dir.path();
    put(root, "rubric/requirements.md", "alpha\n");
    put(root, "evidence/README.md", "alpha\n");
    put(root, "src/run.py", "alpha\n");
    let code = |args: &[&str]| {
        let output = run(root, args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let value: Value = serde_json::from_slice(&output.stderr).unwrap();
        value["error"]["code"].as_str().unwrap().to_owned()
    };
    for path in [
        "rubric/requirements.md",
        "evidence/README.md",
        "src/run.py",
        "logic/missing.md",
    ] {
        assert_eq!(
            code(&["find", "alpha", "--document", path, "--json"]),
            code(&["show", "--document", path, "--json"]),
            "{path}"
        );
    }
    assert_eq!(
        code(&["find", "alpha", "--document", "src/run.py", "--json"]),
        "invalid_document"
    );
}

#[test]
fn every_term_keeps_only_hits_whose_lines_hold_all_terms() {
    let dir = artifact();
    let root = dir.path();
    for query in QUERIES {
        let terms: BTreeSet<&str> = query.split(' ').collect();
        let all = identities(&ok_json(root, &find(query, &[])));
        let value = ok_json(root, &find(query, &["--all", "--lines-per-hit", "0"]));
        let every = identities(&value);
        let order: Vec<_> = all
            .iter()
            .filter(|hit| every.contains(hit))
            .cloned()
            .collect();
        assert_eq!(every, order, "{query}: a subset in the same order");
        for row in value["results"].as_array().unwrap() {
            let text: String = row["matches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| line["text"].as_str().unwrap().to_lowercase() + " ")
                .collect();
            let found: BTreeSet<&str> = text.split(|c: char| !c.is_alphanumeric()).collect();
            assert!(terms.is_subset(&found), "{query}: {row}");
        }
        if terms.len() == 1 {
            assert_eq!(every, all, "one term: --all changes nothing");
        }
    }
    let and = identities(&ok_json(root, &find("alpha beta gamma delta", &["--all"])));
    let or = identities(&ok_json(root, &find("alpha beta gamma delta", &[])));
    assert!(and.len() < or.len(), "the fixture separates AND from OR");
}
