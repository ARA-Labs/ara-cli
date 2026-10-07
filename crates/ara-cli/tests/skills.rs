//! Layout and command-drift checks for the CLI agent skills under `skills/`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn skills_root() -> PathBuf {
    // ara-cli/tests -> ara-cli -> crates -> repo root, then into skills.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills")
}

fn skill_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(skills_root())
        .expect("skills/ exists")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(markdown_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }
    files
}

/// `ara <path...> --help` output; panics when the command path does not exist.
fn help_text(path: &[String]) -> String {
    let output = Command::cargo_bin("ara")
        .expect("binary builds")
        .args(path)
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`ara {} --help` failed",
        path.join(" ")
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Subcommands listed under `Commands:` in a help text.
fn help_subcommands(help: &str) -> BTreeSet<String> {
    help.lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

/// Long options listed under `Options:` in a help text, without the leading `--`. The section
/// runs from the `Options:` header to the next unindented line.
fn help_long_flags(help: &str) -> BTreeSet<String> {
    let mut in_options = false;
    help.lines()
        .filter(|line| {
            if !line.is_empty() && !line.starts_with(' ') {
                in_options = line.starts_with("Options:");
                return false;
            }
            in_options
        })
        .filter_map(|line| {
            let option = line.trim_start();
            let option = option.split_once(", ").map_or(option, |(short, long)| {
                if short.starts_with('-') && !short.starts_with("--") {
                    long
                } else {
                    option
                }
            });
            let name = option.strip_prefix("--")?.split([' ', '=']).next()?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

/// Subcommands listed by `ara --help`.
fn binary_subcommands() -> BTreeSet<String> {
    help_subcommands(&help_text(&[]))
}

/// Code text of a Markdown file: fenced block lines plus inline code spans. An inline span
/// that wraps onto the next line is joined with a space. A span still open at a blank line, a
/// fence or the end of the file is an error, so a stray backtick cannot hide later commands.
fn code_snippets(text: &str) -> Result<Vec<String>, String> {
    let mut snippets = Vec::new();
    let mut fenced = false;
    let mut open_span: Option<String> = None;
    let mut opened_at = 0;
    let unclosed = |line: usize| {
        Err(format!(
            "inline code span opened on line {line} never closes"
        ))
    };
    for (index, line) in text.lines().enumerate() {
        let is_fence = line.trim_start().starts_with("```");
        if open_span.is_some() && !fenced && (is_fence || line.trim().is_empty()) {
            return unclosed(opened_at);
        }
        if is_fence {
            fenced = !fenced;
            continue;
        }
        if fenced {
            snippets.push(line.to_owned());
            continue;
        }
        if let Some(span) = open_span.as_mut() {
            span.push(' ');
        }
        let mut parts = line.split('`').peekable();
        while let Some(part) = parts.next() {
            if let Some(span) = open_span.as_mut() {
                span.push_str(part);
            }
            if parts.peek().is_none() {
                break;
            }
            // A backtick closes the open span or opens a new one.
            match open_span.take() {
                Some(span) => snippets.push(span.trim().to_owned()),
                None => {
                    open_span = Some(String::new());
                    opened_at = index + 1;
                }
            }
        }
    }
    match open_span {
        Some(_) => unclosed(opened_at),
        None => Ok(snippets),
    }
}

/// The subcommand of a snippet that is an `ara ...` command line, skipping `-C <dir>`.
/// Prose that merely mentions ara (comments, numbered steps) does not start with it.
fn ara_subcommand(snippet: &str) -> Option<String> {
    let words: Vec<&str> = snippet.split_whitespace().collect();
    if words.first() != Some(&"ara") {
        return None;
    }
    let mut i = 1;
    while i < words.len() && words[i].starts_with('-') {
        i += if words[i] == "-C" { 2 } else { 1 };
    }
    words
        .get(i)
        .filter(|sub| sub.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .map(|sub| (*sub).to_owned())
}

/// Words of an `ara ...` command line after `ara` and its global `-C <dir>`.
fn ara_command_words(snippet: &str) -> Option<Vec<&str>> {
    let words: Vec<&str> = snippet.split_whitespace().collect();
    if words.first() != Some(&"ara") {
        return None;
    }
    let mut i = 1;
    while i < words.len() && words[i] == "-C" {
        i += 2;
    }
    Some(words.get(i..).unwrap_or_default().to_vec())
}

/// Long flags written in a command line (`--lines 1:5` and `--heading=X` both give the name).
fn long_flags(words: &[&str]) -> Vec<String> {
    words
        .iter()
        .filter_map(|word| word.strip_prefix("--"))
        .filter_map(|flag| flag.split('=').next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_skill_is_named_after_its_directory_and_licensed() {
    let dirs = skill_dirs();
    assert!(!dirs.is_empty(), "no skills found");
    for dir in dirs {
        let dir_name = dir.file_name().unwrap().to_str().unwrap();
        let skill_md = std::fs::read_to_string(dir.join("SKILL.md"))
            .unwrap_or_else(|_| panic!("{dir_name}: missing SKILL.md"));
        let frontmatter = skill_md
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---"))
            .map(|(front, _)| front)
            .unwrap_or_else(|| panic!("{dir_name}: SKILL.md has no frontmatter"));
        let name = frontmatter
            .lines()
            .find_map(|line| line.strip_prefix("name:"))
            .map(str::trim)
            .unwrap_or_else(|| panic!("{dir_name}: SKILL.md has no `name:`"));
        assert_eq!(name, dir_name, "skill name must match its directory");
        assert!(
            dir_name.ends_with("-cli"),
            "{dir_name}: CLI skills end in -cli"
        );
        assert!(dir.join("LICENSE").is_file(), "{dir_name}: missing LICENSE");
    }
}

#[test]
fn cli_access_copies_are_identical() {
    let copies: Vec<(PathBuf, Vec<u8>)> = skill_dirs()
        .into_iter()
        .map(|dir| dir.join("references/cli-access.md"))
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    assert!(copies.len() > 1, "expected several cli-access.md copies");
    for (path, bytes) in &copies[1..] {
        assert!(
            *bytes == copies[0].1,
            "{} differs from {}",
            path.display(),
            copies[0].0.display()
        );
    }
}

#[test]
fn skills_only_name_existing_subcommands() {
    let known = binary_subcommands();
    assert!(
        known.contains("show"),
        "could not parse `ara --help`: {known:?}"
    );
    let mut unknown = BTreeSet::new();
    for dir in skill_dirs() {
        for file in markdown_files(&dir) {
            let text = std::fs::read_to_string(&file).unwrap();
            let rel = file.strip_prefix(skills_root()).unwrap().display();
            let snippets = code_snippets(&text).unwrap_or_else(|err| panic!("{rel}: {err}"));
            for sub in snippets.iter().filter_map(|s| ara_subcommand(s)) {
                if !known.contains(&sub) {
                    unknown.insert(format!("{rel}: ara {sub}"));
                }
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "skills name unknown subcommands: {unknown:#?}"
    );
}

#[test]
fn skills_only_use_existing_long_flags() {
    let top_help = help_text(&[]);
    let global_flags = help_long_flags(&top_help);
    let mut helps: BTreeMap<Vec<String>, String> = BTreeMap::new();
    helps.insert(Vec::new(), top_help);
    let mut unknown = BTreeSet::new();
    let mut checked = 0;
    for dir in skill_dirs() {
        for file in markdown_files(&dir) {
            let text = std::fs::read_to_string(&file).unwrap();
            let rel = file.strip_prefix(skills_root()).unwrap().display();
            let snippets = code_snippets(&text).unwrap_or_else(|err| panic!("{rel}: {err}"));
            for snippet in snippets {
                let Some(words) = ara_command_words(&snippet) else {
                    continue;
                };
                // Walk nested subcommands (`merge resolve`, `claim add`) as far as help lists them.
                let mut path: Vec<String> = Vec::new();
                for word in &words {
                    let parent = helps
                        .entry(path.clone())
                        .or_insert_with(|| help_text(&path));
                    if !help_subcommands(parent).contains(*word) {
                        break;
                    }
                    path.push((*word).to_owned());
                }
                if path.is_empty() {
                    continue;
                }
                let help = helps
                    .entry(path.clone())
                    .or_insert_with(|| help_text(&path));
                let known = help_long_flags(help);
                for flag in long_flags(&words) {
                    checked += 1;
                    if !known.contains(&flag) && !global_flags.contains(&flag) {
                        unknown.insert(format!("{rel}: ara {} --{flag}", path.join(" ")));
                    }
                }
            }
        }
    }
    assert!(checked > 0, "no long flags found in skill commands");
    assert!(
        unknown.is_empty(),
        "skills use long flags their subcommand lacks: {unknown:#?}"
    );
}

#[test]
fn flag_extraction_reads_command_words() {
    assert_eq!(
        ara_command_words("ara -C <artifact> show 'a#C04' --lines 1:5"),
        Some(vec!["show", "'a#C04'", "--lines", "1:5"])
    );
    assert_eq!(ara_command_words("1. Read ara output"), None);
    assert_eq!(
        long_flags(&["show", "--heading=-x", "--max-bytes", "9", "--", "-C"]),
        ["heading", "max-bytes"]
    );
    let help = "Arguments:\n  --not-an-option\nOptions:\n  -C <DIRECTORY>  Select\n      --context <CONTEXT>  Lines\n  -h, --help  Print help\n";
    assert_eq!(
        help_long_flags(help),
        BTreeSet::from(["context".to_owned(), "help".to_owned()])
    );
}

#[test]
fn subcommand_extraction_reads_only_command_lines() {
    assert_eq!(
        ara_subcommand("ara -C <artifact> show --document PAPER.md").as_deref(),
        Some("show")
    );
    assert_eq!(ara_subcommand("ara --version"), None);
    assert_eq!(
        ara_subcommand("1. Read knowledge with ara full source shows"),
        None
    );
    assert_eq!(
        code_snippets("run `ara find x` or ara prose").unwrap(),
        ["ara find x"]
    );
    assert_eq!(
        code_snippets("use `ara merge --as a\n--base b` here `x`\n\nnext").unwrap(),
        ["ara merge --as a --base b", "x"]
    );
    assert!(code_snippets("a `stray\n\n`ara find x`").is_err());
    assert!(code_snippets("a `stray\n```sh\nara find x\n```").is_err());
    assert!(code_snippets("ends `open").is_err());
}

/// The shared access page every CLI skill carries (all copies are identical).
fn cli_access() -> String {
    std::fs::read_to_string(skills_root().join("research-manager-cli/references/cli-access.md"))
        .unwrap()
}

/// Split a documented command line into words, honouring single quotes.
fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                word.get_or_insert_with(String::new);
            }
            c if c.is_whitespace() && !quoted => words.extend(word.take()),
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    assert!(!quoted, "unclosed quote in `{line}`");
    words.extend(word);
    words
}

fn ara_in(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(root);
    command
}

/// Run a command line that `cli-access.md` documents verbatim against `root`.
fn documented(root: &Path, line: &str) -> std::process::Output {
    assert!(
        cli_access().lines().any(|doc| doc == line),
        "cli-access.md no longer documents `{line}`"
    );
    let words = shell_words(line);
    assert_eq!(words[..3], ["ara", "-C", "<artifact>"], "{line}");
    ara_in(root).args(&words[3..]).output().unwrap()
}

fn stdout(output: &std::process::Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// The `error.code` of a failed `--json` command; errors are normally on stderr.
fn error_code(output: &std::process::Output) -> String {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let text = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value: serde_json::Value = serde_json::from_slice(text).unwrap();
    value["error"]["code"].as_str().unwrap().to_owned()
}

fn write_file(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// Dry-run one JSONL batch.
fn dry_run(root: &Path, operations: &[serde_json::Value]) -> std::process::Output {
    let text: String = operations.iter().map(|op| format!("{op}\n")).collect();
    ara_in(root)
        .args(["apply", "-", "--dry-run", "--json", "--no-duplicate-check"])
        .write_stdin(text)
        .output()
        .unwrap()
}

/// Runs the access page's two digest recipes and checks that each one guards
/// exactly its own operations: the printed heading-body digest for body and
/// document replacement, the computed entry-span digest for rename/remove.
#[test]
fn cli_access_digest_recipes_match_each_guard_scope() {
    use serde_json::json;
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    write_file(
        root,
        "PAPER.md",
        "---\ntitle: Guard scopes\n---\n# Guard scopes\n",
    );
    write_file(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Q\n    provenance: user\n",
    );
    write_file(
        root,
        "logic/concepts.md",
        "# Concepts\n\n## Old term\n- **Definition**: Caller concept\n\n## Other term\n- **Definition**: Kept\n",
    );

    // Step 1: the heading's document line from its own find hit.
    let hits = stdout(&documented(
        root,
        "ara -C <artifact> find 'Old term' --type concept",
    ));
    assert!(hits.contains("\n  3: ## Old term\n"), "{hits}");
    // Step 2: the heading body, its printed digest and its line count.
    let body_read = stdout(&documented(
        root,
        "ara -C <artifact> show --document logic/concepts.md --heading 'Old term' --source --lines 1:",
    ));
    let body_digest = body_read
        .split_whitespace()
        .find_map(|word| word.strip_prefix("source_digest="))
        .unwrap()
        .to_owned();
    assert!(body_read.contains("scope=heading_body"), "{body_read}");
    assert!(body_read.contains("\nlines: 1-2 of 2\n"), "{body_read}");
    // Step 3: the window H:H+N is the heading line plus that body.
    let window: serde_json::Value = serde_json::from_str(&stdout(&documented(
        root,
        "ara -C <artifact> show --document logic/concepts.md --source --lines 3:5 --full --json",
    )))
    .unwrap();
    let content = window["entries"][0]["content"].as_str().unwrap();
    assert_eq!(content, "## Old term\n- **Definition**: Caller concept\n\n");
    let window_digest = window["entries"][0]["digest"].as_str().unwrap().to_owned();
    // Step 4: the entry-span digest is the hash of that content.
    let span_digest = ara_core::write::source::digest(content.as_bytes());
    assert_ne!(span_digest, body_digest);

    let scratch = tempfile::TempDir::new().unwrap();
    let request = scratch.path().join("session.jsonl");
    std::fs::write(&request, format!("{}\n", json!({"op":"session.start","id":"$session","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Guard scopes"}))).unwrap();
    let scratch = tempfile::TempDir::new().unwrap();
    let request = scratch.path().join("session.jsonl");
    std::fs::write(&request, format!("{}\n", json!({"op":"session.start","id":"$session","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Guard scopes"}))).unwrap();
    let session: serde_json::Value = serde_json::from_slice(
        &ara_in(root)
            .arg("apply")
            .arg(&request)
            .arg("--json")
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap();
    let session = session["bindings"]["$session"].as_str().unwrap();
    let log = json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01"});
    let target = json!({"document":"logic/concepts.md","heading":["Old term"]});
    let audit = |mut op: serde_json::Value| {
        op["session"] = json!(session);
        op["turn"] = json!(1);
        op["signal"] = json!("user-directive");
        op["provenance"] = json!("user");
        op
    };
    let rename = |expected: &str| {
        audit(json!({"op":"entry.rename","target":target,"name":"New term","expected":expected}))
    };
    let remove =
        |expected: &str| audit(json!({"op":"entry.remove","target":target,"expected":expected}));
    let replace = |expected: &str| json!({"op":"document.replace","document":"logic/concepts.md","heading":["Old term"],"expected":expected,"content":"- **Definition**: Revised\n\n"});
    let revise = |expected: &str| {
        audit(
            json!({"op":"logic.revise","target":target,"set":{"Body":"- **Definition**: Revised\n\n"},"expected":expected}),
        )
    };

    // Structural edits accept only the entry-span digest.
    for structural in [rename(&span_digest), remove(&span_digest)] {
        let output = dry_run(root, &[log.clone(), structural]);
        assert!(output.status.success(), "{output:?}");
    }
    for wrong in [&body_digest, &window_digest] {
        for structural in [rename(wrong), remove(wrong)] {
            assert_eq!(
                error_code(&dry_run(root, &[log.clone(), structural])),
                "write.digest_conflict"
            );
        }
    }
    // Body replacements accept only the printed heading-body digest.
    let output = dry_run(root, &[replace(&body_digest)]);
    assert!(output.status.success(), "{output:?}");
    let output = dry_run(root, &[log.clone(), revise(&body_digest)]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        error_code(&dry_run(root, &[replace(&span_digest)])),
        "write.digest_conflict"
    );
    assert_eq!(
        error_code(&dry_run(root, &[log.clone(), revise(&span_digest)])),
        "write.digest_conflict"
    );
}

/// `find` reports whole-document lines; the access page reads around a hit
/// with a document window, not an address window, and projections reject
/// line bounds.
#[test]
fn cli_access_reads_find_hits_by_document_lines() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ara-core/tests/fixtures/agent-cli");
    let hits = stdout(
        &ara_in(&root)
            .args(["find", "lossless", "--type", "claim"])
            .output()
            .unwrap(),
    );
    assert!(
        hits.contains("C04 [claim] logic/claims.md\n  30: ## C04:"),
        "{hits}"
    );
    // The documented document window around that hit holds the hit line.
    let window = stdout(&documented(
        &root,
        "ara -C <artifact> show --document logic/claims.md --source --lines 26:40",
    ));
    assert!(window.contains("\n## C04: Universal Ingestor"), "{window}");
    assert!(window.contains("lines: 26-40 of"), "{window}");

    let show = |args: &[&str]| ara_in(&root).args(args).arg("--json").output().unwrap();
    // Address windows count body lines: the hit's line 30 is past C04's 8.
    assert_eq!(
        error_code(&show(&["show", "logic/claims.md#C04", "--lines", "30:30"])),
        "line_out_of_range"
    );
    // Projections take no line bounds; the source document they name does.
    assert_eq!(
        error_code(&show(&["show", "trace:N01", "--lines", "1:5"])),
        "lines_unavailable"
    );
    let output = show(&[
        "show",
        "--document",
        "trace/exploration_tree.yaml",
        "--source",
        "--lines",
        "1:5",
    ]);
    assert!(output.status.success(), "{output:?}");
    let access = cli_access();
    for phrase in [
        "show --document <source> --source --lines A:B",
        "`line_out_of_range`",
        "`lines_unavailable`",
    ] {
        assert!(access.contains(phrase), "cli-access.md lost `{phrase}`");
    }
}

#[test]
fn shell_words_honours_single_quotes() {
    assert_eq!(
        shell_words("ara -C <artifact> show 'logic/claims.md#C04' --heading 'Old term'"),
        [
            "ara",
            "-C",
            "<artifact>",
            "show",
            "logic/claims.md#C04",
            "--heading",
            "Old term"
        ]
    );
}

fn published_jsonl(section: &str) -> Vec<serde_json::Value> {
    let access = cli_access();
    let start = access.find(section).expect("published task exists");
    let text = &access[start..];
    let block = text
        .split_once("```jsonl\n")
        .expect("published JSONL example")
        .1;
    block
        .split_once("\n```")
        .unwrap()
        .0
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn commit_example(
    root: &Path,
    request: &Path,
    operations: &[serde_json::Value],
) -> serde_json::Value {
    std::fs::write(
        request,
        operations
            .iter()
            .map(|op| format!("{op}\n"))
            .collect::<String>(),
    )
    .unwrap();
    serde_json::from_slice(
        &ara_in(root)
            .arg("apply")
            .arg(request)
            .arg("--json")
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap()
}

fn source_yaml(root: &Path, path: &str) -> serde_json::Value {
    let output = stdout(
        &ara_in(root)
            .args(["show", path, "--source", "--full", "--json"])
            .output()
            .unwrap(),
    );
    let response: serde_json::Value = serde_json::from_str(&output).unwrap();
    ara_core::write::positions::YamlDocument::parse(
        response["entries"][0]["content"].as_str().unwrap(),
    )
    .unwrap()
    .root
    .to_json()
    .unwrap()
}

#[test]
fn skill_task_links_cover_every_shared_subsection() {
    let sections = [
        "reading-orient-search-read-cite",
        "initialize-or-extend-an-artifact",
        "inspect-ancestry-citations-and-imported-identities",
        "review-unfinished-work",
        "record-a-research-turn",
        "stage-or-crystallize-an-observation",
        "create-claims-and-heuristics",
        "edit-current-knowledge",
        "rename-merge-or-split-knowledge-entries",
        "record-annotations-and-confirmed-user-reactions",
        "integrate-another-artifact",
    ];
    let access = cli_access();
    let pages: String = [
        "research-manager-cli/SKILL.md",
        "compiler-cli/SKILL.md",
        "research-foresight-cli/SKILL.md",
        "research-foresight-cli/references/RETRIEVE.md",
    ]
    .iter()
    .map(|path| std::fs::read_to_string(skills_root().join(path)).unwrap())
    .collect();
    for section in sections {
        assert!(
            pages.contains(&format!("cli-access.md#{section}")),
            "unlinked task {section}"
        );
        let heading = access
            .lines()
            .filter_map(|line| {
                line.strip_prefix("## ")
                    .or_else(|| line.strip_prefix("### "))
            })
            .any(|heading| {
                heading
                    .to_lowercase()
                    .replace([',', ':'], "")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join("-")
                    == section
            });
        assert!(heading, "missing task anchor {section}");
    }
}

#[test]
fn skill_research_turn_and_consolidated_reads_run_on_matching_binary() {
    use serde_json::json;
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("artifact");
    let request = dir.path().join("turn.jsonl");
    commit_example(
        &root,
        &request,
        &[
            json!({"op":"artifact.init","profile":"research-manager","paper":"---\ntitle: Skill runtime\n---\n# Skill runtime\n"}),
        ],
    );
    let turn = published_jsonl("### Record a research turn");
    let report = commit_example(&root, &request, &turn);
    assert_eq!(report["format"], "ara.apply/v1");
    assert_eq!(report["bindings"]["$question"], "N01");
    let session = report["operations"][0]["id"].as_str().unwrap();
    let turn_number = report["operations"][0]["turn"].as_u64().unwrap();
    assert_eq!(turn_number, 1);
    let session_path = format!("trace/sessions/{session}.yaml");
    let record = source_yaml(&root, &session_path);
    assert_eq!(record["session"]["turn_count"], turn_number);
    assert_eq!(record["session"]["summary"], turn[0]["summary"]);
    assert_eq!(
        record["events_logged"][0]["id"],
        report["bindings"]["$question"]
    );
    assert_eq!(record["events_logged"][0]["turn"], turn_number);
    assert_eq!(record["events_logged"][0]["type"], "question");
    assert_eq!(record["events_logged"][0]["routing"], "direct");
    let reasoning = source_yaml(&root, "trace/pm_reasoning_log.yaml");
    let authored_reasoning = reasoning["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry.get("notes").is_some())
        .unwrap();
    assert_eq!(
        authored_reasoning["turn"],
        format!("{session}#{turn_number}")
    );
    assert_eq!(authored_reasoning["notes"], turn[2]["record"]["notes"]);
    let index = source_yaml(&root, "trace/sessions/session_index.yaml");
    assert_eq!(index["sessions"][0]["id"], session);
    assert_eq!(index["sessions"][0]["turn_count"], turn_number);
    assert_eq!(index["sessions"][0]["events_count"], 1);

    let authored = published_jsonl("### Stage or crystallize an observation");
    let created = commit_example(&root, &request, &authored);
    for name in ["$observation", "$promoted", "$claim", "$heuristic"] {
        assert!(created["bindings"][name].is_string(), "{created}");
    }
    let observations = source_yaml(&root, "staging/observations.yaml");
    let observation = &observations["observations"][0];
    assert_eq!(observation["id"], created["bindings"]["$observation"]);
    assert_eq!(observation["content"], authored[1]["content"]);
    assert_eq!(observation["context"], authored[1]["context"]);
    assert_eq!(observation["provenance"], authored[1]["provenance"]);
    assert_eq!(observation["potential_type"], authored[1]["potential_type"]);
    assert_eq!(observation["bound_to"], authored[1]["bound_to"]);
    assert_eq!(observation["promoted"], true);
    assert_eq!(
        observation["promoted_to"],
        created["operations"][2]["target"]
    );
    assert_eq!(observation["crystallized_via"], authored[2]["signal"]);
    let record = source_yaml(&root, &session_path);
    assert_eq!(record["session"]["turn_count"], 2);
    for (event, binding, routing, provenance) in [
        (1, "$observation", "staged", &authored[1]["provenance"]),
        (
            2,
            "$promoted",
            "crystallized",
            &authored[2]["fields"]["Provenance"],
        ),
        (3, "$claim", "direct", &authored[3]["fields"]["Provenance"]),
        (
            4,
            "$heuristic",
            "direct",
            &authored[4]["fields"]["Provenance"],
        ),
    ] {
        assert_eq!(
            record["events_logged"][event]["id"],
            created["bindings"][binding]
        );
        assert_eq!(record["events_logged"][event]["turn"], 2);
        assert_eq!(record["events_logged"][event]["routing"], routing);
        assert_eq!(&record["events_logged"][event]["provenance"], provenance);
    }
    let index = source_yaml(&root, "trace/sessions/session_index.yaml");
    assert_eq!(index["sessions"][0]["turn_count"], 2);
    assert_eq!(index["sessions"][0]["events_count"], 5);
    for (path, expected) in [
        ("logic/claims.md", "Caller source statement"),
        ("logic/solution/heuristics.md", "Only the declared setup"),
    ] {
        assert!(
            stdout(
                &ara_in(&root)
                    .args(["show", path, "--source", "--full"])
                    .output()
                    .unwrap()
            )
            .contains(expected)
        );
    }
    let path: serde_json::Value = serde_json::from_str(&stdout(&documented(
        &root,
        "ara -C <artifact> show N01 --with path,parents,depends_on --json",
    )))
    .unwrap();
    assert_eq!(path["entries"][0]["relations"]["path"][0]["id"], "N01");
    let unfinished: serde_json::Value = serde_json::from_slice(
        &ara_in(&root)
            .args(["ls", "--unfinished", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap();
    assert!(
        unfinished["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "N01")
    );
    let refs: serde_json::Value = serde_json::from_str(&stdout(&documented(
        &root,
        "ara -C <artifact> show trace/exploration_tree.yaml --with refs --json",
    )))
    .unwrap();
    assert!(refs["entries"][0]["relations"]["refs"]["structured"].is_array());

    let compiler = dir.path().join("compiler");
    let initialization = published_jsonl("### Initialize or extend an artifact");
    commit_example(&compiler, &request, &initialization);
    let paper = stdout(
        &ara_in(&compiler)
            .args(["show", "PAPER.md", "--source", "--full"])
            .output()
            .unwrap(),
    );
    assert!(paper.contains("Skill compiler example"));
    let problem: serde_json::Value = serde_json::from_slice(
        &ara_in(&compiler)
            .args(["show", "logic/problem.md", "--source", "--full", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap();
    assert_eq!(
        problem["entries"][0]["content"],
        initialization[0]["documents"]["logic/problem.md"]
    );
}
