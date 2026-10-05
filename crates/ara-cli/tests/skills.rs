//! Layout and command-drift checks for the CLI agent skills under `skills/`.

use std::collections::BTreeSet;
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

/// Subcommands listed by `ara --help`.
fn binary_subcommands() -> BTreeSet<String> {
    let output = Command::cargo_bin("ara")
        .expect("binary builds")
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    help.lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

/// Code text of a Markdown file: fenced block lines plus inline code spans.
fn code_snippets(text: &str) -> Vec<String> {
    let mut snippets = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            snippets.push(line.to_owned());
        } else {
            snippets.extend(line.split('`').skip(1).step_by(2).map(str::to_owned));
        }
    }
    snippets
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
            for sub in code_snippets(&text)
                .iter()
                .filter_map(|s| ara_subcommand(s))
            {
                if !known.contains(&sub) {
                    let rel = file.strip_prefix(skills_root()).unwrap().display();
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
        code_snippets("run `ara find x` or ara prose"),
        ["ara find x"]
    );
}
