//! Shared fixture and parser helpers for the plan 19 C1 citation tests.
#![allow(dead_code)]
pub use std::collections::{BTreeMap, BTreeSet};
pub use std::fs;
pub use std::path::{Path, PathBuf};

pub use ara_core::markdown;
pub use ara_core::write::{self, EntrySelector, WorkingArtifact, source};
pub use assert_cmd::Command;
pub use serde_json::{Value, json};
pub use tempfile::TempDir;

pub const PAPER: &str = "---\ntitle: Citation fixture\n---\n# Citation fixture\n";
pub const OBSERVATIONS: &str = "observations:\n  - id: O01\n    timestamp: \"2026-10-01T10:00:00Z\"\n    provenance: user\n    content: \"C02 looks fragile\"\n    potential_type: claim\n    promoted: false\n    bound_to: [N01]\n";
pub const HEURISTICS: &str = "# Heuristics\n\n## H01: Keep\n- **Rationale**: Primary.\n- **Sensitivity**: low\n- **Code ref**: src/a.py\n\n## H02: Fold\n- **Rationale**: Folded.\n- **Sensitivity**: low\n- **Code ref**: src/b.py\n";
pub const CONCEPTS: &str = "## Group A\n\n### Term\n- **Definition**: A term.\n\n## Group B\n\n### Term\n- **Definition**: B term.\n- **Related**: Group A/Term, A/B #1\n\n## A/B #1\n- **Definition**: Delimiters.\n- **Related**: logic/concepts.md#Group A/Term\n\n## Lone\n- **Definition**: Lone concept.\n- **Related**: A/B #1\n- **Sources**: logic/concepts.md#Group B/Term\n";

pub fn tree(extra: &str) -> String {
    format!(
        "tree:\n  - id: N01\n    type: question\n    title: Boundary mechanism\n    provenance: user\n    description: Root question\n    evidence: [\"C02\", \"logic/claims.md:C02\"]\n    source_refs: [\"logic/concepts.md#Group B/Term\"{extra}]\n"
    )
}

/// `mentions` adds prose, untyped and unknown-field mentions of C02.
pub fn claims(mentions: bool) -> String {
    format!(
        "# Claims\n\n## C01: Survivor claim\n- **Statement**: Survivor.\n- **Status**: supported\n- **Dependencies**: []\n\n## C02: Source claim\n- **Statement**: Source.\n- **Status**: hypothesis\n- **Dependencies**: [C03]\n\n## C03: Base claim\n- **Statement**: Base.\n- **Status**: supported\n- **Sources**: H02 heuristic notes\n- **Dependencies**: []\n\n## C04: Citer\n{}- **Statement**: {}\n- **Status**: hypothesis\n- **Proof**: \"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C02.\n- **Sources**: [\"paper §2\", \"C02\"]\n- **Dependencies**: [C02, C03]\n{}\n## C05: Previously merged\n- **Statement**: Old duplicate.\n- **Status**: withdrawn\n- **Merged into**: C02\n",
        if mentions {
            "Prose aside: C02 is related.\n"
        } else {
            ""
        },
        if mentions {
            "Depends on the source; see C02."
        } else {
            "Depends on the source."
        },
        if mentions {
            "- **Custom note**: C02 stays here\n"
        } else {
            ""
        },
    )
}

pub fn experiments(mentions: bool) -> String {
    format!(
        "# Experiments\n\n## E01: Ablation\n{}- **Sources**: C02\n",
        if mentions {
            "- **Verifies**: C02\n"
        } else {
            ""
        }
    )
}

pub fn write_file(root: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn fixture(mentions: bool) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_file(root, "PAPER.md", PAPER);
    write_file(root, "trace/exploration_tree.yaml", tree(""));
    write_file(root, "staging/observations.yaml", OBSERVATIONS);
    write_file(root, "logic/claims.md", claims(mentions));
    write_file(root, "logic/experiments.md", experiments(mentions));
    write_file(root, "logic/solution/heuristics.md", HEURISTICS);
    write_file(root, "logic/concepts.md", CONCEPTS);
    dir
}

pub fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .arg("-C")
        .arg(root)
        .env_remove("ARA_DIR")
        .env_remove("ARA_NO_DUPLICATE_CHECK");
    command
}

pub fn jsonl(operations: &[Value]) -> String {
    operations.iter().map(|op| format!("{op}\n")).collect()
}

pub fn apply(root: &Path, operations: &[Value], dry_run: bool) -> Value {
    let mut command = ara(root);
    command
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(jsonl(operations));
    if dry_run {
        command.arg("--dry-run");
    }
    let output = command.assert().success().get_output().clone();
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

/// A rejected batch: exit 1, nothing on stdout, the JSON error on stderr.
pub fn apply_failure(root: &Path, text: &str) -> Value {
    let output = ara(root)
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(text.to_owned())
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"].clone()
}

pub fn yaml(root: &Path, path: &str) -> Value {
    let text = fs::read_to_string(root.join(path)).unwrap();
    write::positions::YamlDocument::parse(&text)
        .unwrap()
        .root
        .to_json()
        .unwrap()
}

pub fn artifact_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, relative: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".ara" {
                continue;
            }
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(path.clone(), fs::read(root.join(path)).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, Path::new(""), &mut result);
    result
}

pub fn working(root: &Path) -> WorkingArtifact {
    WorkingArtifact::new(write::ArtifactSnapshot::load(root).unwrap())
}

pub fn selector(value: Value) -> EntrySelector {
    serde_json::from_value(value).unwrap()
}

/// The entry-span digest `entry.rename`/`entry.remove` guard.
pub fn entry_digest(root: &Path, target: Value) -> String {
    let working = working(root);
    let entry = write::logic::resolve(&working, &selector(target)).unwrap();
    source::digest(working.text(&entry.document).unwrap()[entry.range].as_bytes())
}

pub fn field(root: &Path, target: Value, name: &str) -> String {
    write::logic::field_value(&working(root), &selector(target), name).unwrap()
}

pub type FieldKey = (String, Vec<String>, String);

/// Every decoded field of the native logic documents, keyed by document,
/// full heading vector and label; `Last revised` is the writer's pointer.
pub fn logic_fields(root: &Path) -> BTreeMap<FieldKey, String> {
    let mut result = BTreeMap::new();
    for document in [
        "logic/claims.md",
        "logic/experiments.md",
        "logic/concepts.md",
        "logic/solution/heuristics.md",
    ] {
        let Ok(text) = fs::read_to_string(root.join(document)) else {
            continue;
        };
        let headings = markdown::headings(&text);
        for (index, heading) in headings.iter().enumerate() {
            let end = headings
                .get(index + 1)
                .map_or(heading.body_range.end, |next| next.range.start);
            for field in markdown::fields(&text, heading.body_range.start..end) {
                if field.name == "Last revised" {
                    continue;
                }
                result.insert(
                    (
                        document.to_owned(),
                        heading.path.iter().map(|part| part.to_string()).collect(),
                        field.name.to_owned(),
                    ),
                    markdown::decode_field(&field).into_owned(),
                );
            }
        }
    }
    result
}

/// The session record of a report's leading `session.log`, reduced to the
/// rows of that log's own turn.
pub fn session_of(root: &Path, report: &Value) -> Value {
    let session = report["operations"][0]["id"].as_str().unwrap();
    let turn = &report["operations"][0]["turn"];
    let mut record = yaml(root, &format!("trace/sessions/{session}.yaml"));
    for key in ["logic_revisions", "claims_touched", "events_logged"] {
        if let Some(rows) = record[key].as_array_mut() {
            rows.retain(|row| &row["turn"] == turn);
        }
    }
    record
}

/// The revision rows identify every actual field mutation: one row per
/// changed (entry, field), with the exact decoded before/after values.
pub fn assert_audits_match(
    root: &Path,
    before: &BTreeMap<FieldKey, String>,
    record: &Value,
    renamed: &[(&[&str], &[&str])],
) {
    let after = logic_fields(root);
    let rename = |key: &FieldKey| -> FieldKey {
        let mut key = key.clone();
        for (old, new) in renamed {
            if key.1.len() >= old.len() && key.1.iter().zip(old.iter()).all(|(a, b)| a == b) {
                let rest = key.1.split_off(old.len());
                key.1 = new
                    .iter()
                    .map(|part| part.to_string())
                    .chain(rest)
                    .collect();
            }
        }
        key
    };
    let mut changed = BTreeSet::new();
    for (key, value) in before {
        let moved = rename(key);
        match after.get(&moved) {
            Some(new) if new == value => {}
            new => {
                changed.insert((key.2.clone(), value.clone(), new.cloned()));
            }
        }
    }
    for (key, value) in &after {
        if !before.keys().any(|old| rename(old) == *key) {
            changed.insert((key.2.clone(), String::new(), Some(value.clone())));
        }
    }
    let rows: BTreeSet<(String, String, Option<String>)> = record["logic_revisions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["field"] != "entry")
        .map(|row| {
            (
                row["field"].as_str().unwrap().to_owned(),
                row["before"].as_str().unwrap_or_default().to_owned(),
                row["after"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(rows, changed, "revision rows identify every mutation");
}

pub fn touches(record: &Value) -> Vec<(String, String)> {
    record["claims_touched"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().to_owned(),
                row["action"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

pub fn merge_op() -> Value {
    json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true})
}

/// Run a read command and return its JSON report (exit 0, empty stderr).
pub fn read(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}

/// The heading-body digest `show` prints as `source_digest`.
pub fn body_digest(root: &Path, target: Value) -> String {
    let working = working(root);
    let entry = write::logic::resolve(&working, &selector(target)).unwrap();
    source::digest(working.text(&entry.document).unwrap()[entry.body].as_bytes())
}
