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
