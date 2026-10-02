//! CLI integration tests for `ara check`'s per-rule `.ara-check.toml` (#40).

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// A `dead_end` with `reason:` fires `ARA002` (fixable) and `ARA206` (warning).
const DEAD_END_REASON: &str = "tree:\n  - id: N01\n    type: dead_end\n    reason: it diverged\n";
/// An unknown node field fires `ARA201` (warning) only.
const UNKNOWN_FIELD: &str = "tree:\n  - id: N01\n    type: question\n    title: q\n    bogus: 1\n";
/// A duplicate node id fires `ARA105` (error) only.
const DUPLICATE_ID: &str =
    "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n";

fn ara() -> Command {
    Command::cargo_bin("ara").expect("binary builds")
}

/// Builds a temp ARA artifact, optionally with a `.ara-check.toml` in it.
fn artifact(tree_yaml: &str, config: Option<&str>) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("trace")).unwrap();
    std::fs::write(dir.path().join("trace/exploration_tree.yaml"), tree_yaml).unwrap();
    if let Some(config) = config {
        std::fs::write(dir.path().join(".ara-check.toml"), config).unwrap();
    }
    dir
}

fn check_json(dir: &Path, extra: &[&str]) -> (serde_json::Value, Option<i32>) {
    let output = ara()
        .arg("check")
        .arg(dir)
        .arg("--json")
        .args(extra)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).expect("valid JSON");
    (json, output.status.code())
}

fn rules(entries: &serde_json::Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["rule"].as_str().unwrap().to_string())
        .collect()
}

/// An empty config and `--no-config` both reproduce the no-config output
/// byte-for-byte (human and JSON, apart from JSON's `config` key).
#[test]
fn empty_config_and_no_config_match_builtin_behavior() {
    let bare = artifact(DEAD_END_REASON, None);
    let baseline = ara().arg("check").arg(bare.path()).output().unwrap();
    assert_eq!(baseline.status.code(), Some(1));

    let with_ignore = artifact(DEAD_END_REASON, Some("ignore = [\"ARA\"]\n"));
    let no_config = ara()
        .arg("check")
        .arg(with_ignore.path())
        .arg("--no-config")
        .output()
        .unwrap();
    assert_eq!(no_config.status.code(), Some(1));
    let normalize = |out: &[u8], dir: &Path| {
        String::from_utf8_lossy(out).replace(&dir.display().to_string(), "<dir>")
    };
    assert_eq!(
        normalize(&no_config.stdout, with_ignore.path()),
        normalize(&baseline.stdout, bare.path())
    );

    let empty = artifact(DEAD_END_REASON, Some(""));
    let out = ara().arg("check").arg(empty.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        normalize(&out.stdout, empty.path()),
        normalize(&baseline.stdout, bare.path())
    );

    // No config ⇒ no `config` key and no lint `severity` key in JSON.
    let (json, _) = check_json(bare.path(), &[]);
    assert!(json.get("config").is_none());
    assert!(json["lint"][0].get("severity").is_none());
}

/// Ignored rules disappear from human and JSON output and no longer affect the
/// exit code; the JSON names the config file used.
#[test]
fn ignore_removes_findings_from_output_and_exit_code() {
    let dir = artifact(DEAD_END_REASON, Some("ignore = [\"ARA002\", \"ARA206\"]\n"));
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .success()
        .stdout(predicate::str::contains("ARA002").not())
        .stdout(predicate::str::contains("ARA206").not())
        .stdout(predicate::str::contains(
            "PASS — 0 error(s), 0 warning(s), 0 fixable issue(s)",
        ));

    let (json, code) = check_json(dir.path(), &["--strict"]);
    assert_eq!(code, Some(0));
    assert!(json["lint"].as_array().unwrap().is_empty());
    assert!(json["validate"]["warnings"].as_array().unwrap().is_empty());
    assert!(json["summary"]["passed"].as_bool().unwrap());
    assert!(
        json["config"]
            .as_str()
            .unwrap()
            .ends_with(".ara-check.toml"),
        "{json}"
    );

    // An ignored rule is not fixed either: `--fix` leaves the source untouched.
    let tree = dir.path().join("trace/exploration_tree.yaml");
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .success();
    assert_eq!(std::fs::read_to_string(tree).unwrap(), DEAD_END_REASON);
}

/// A code prefix ignores the whole block (`ARA2` = every warning).
#[test]
fn prefix_ignore_and_select() {
    let dir = artifact(UNKNOWN_FIELD, Some("ignore = [\"ARA2\"]\n"));
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .success()
        .stdout(predicate::str::contains("ARA201").not());

    // `select = ["ARA1"]` keeps only structural errors: the fixable ARA002 and
    // the ARA206 warning are both dropped.
    let dir = artifact(DEAD_END_REASON, Some("select = [\"ARA1\"]\n"));
    let (json, code) = check_json(dir.path(), &["--strict"]);
    assert_eq!(code, Some(0));
    assert!(json["lint"].as_array().unwrap().is_empty());
    assert!(json["validate"]["warnings"].as_array().unwrap().is_empty());

    // A more specific select overrides a broader ignore.
    let dir = artifact(
        UNKNOWN_FIELD,
        Some("select = [\"ARA\", \"ARA201\"]\nignore = [\"ARA2\"]\n"),
    );
    let (json, code) = check_json(dir.path(), &["--strict"]);
    assert_eq!(code, Some(1));
    assert_eq!(rules(&json["validate"]["warnings"]), ["ARA201"]);
}

/// Promoting a warning to error fails the run without `--strict` and moves the
/// finding into `validate.errors`.
#[test]
fn severity_promotion_affects_exit_code() {
    let dir = artifact(UNKNOWN_FIELD, Some("[severity]\nARA201 = \"error\"\n"));
    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("ARA201 error: nodes[N01]"))
        .stdout(predicate::str::contains("FAIL — 1 error(s), 0 warning(s)"));

    let (json, code) = check_json(dir.path(), &[]);
    assert_eq!(code, Some(1));
    assert_eq!(rules(&json["validate"]["errors"]), ["ARA201"]);
    assert_eq!(json["validate"]["errors"][0]["severity"], "error");
    assert!(json["validate"]["warnings"].as_array().unwrap().is_empty());
    assert!(!json["summary"]["passed"].as_bool().unwrap());
}

/// Demoting an error to a warning passes normally but still fails `--strict`.
#[test]
fn severity_demotion_affects_exit_code() {
    let dir = artifact(DUPLICATE_ID, Some("[severity]\nARA1 = \"warning\"\n"));
    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("ARA105 warning: "));
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .failure()
        .code(1);

    let (json, code) = check_json(dir.path(), &[]);
    assert_eq!(code, Some(0));
    assert_eq!(rules(&json["validate"]["warnings"]), ["ARA105"]);
    assert_eq!(json["validate"]["warnings"][0]["severity"], "warning");
}

/// A format-lint rule demoted to `warning` is still fixable but no longer fails
/// the run on its own; JSON then carries its `severity`.
#[test]
fn lint_severity_demotion() {
    let dir = artifact(
        DEAD_END_REASON,
        Some("ignore = [\"ARA206\"]\n[severity]\nARA002 = \"warning\"\n"),
    );
    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("ARA002 warning [fixable]: "));
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .failure()
        .code(1);

    let (json, code) = check_json(dir.path(), &[]);
    assert_eq!(code, Some(0));
    assert_eq!(json["lint"][0]["severity"], "warning");
    assert_eq!(json["summary"]["fixable"], 1);
}

/// An `unfixable` rule is reported without the `[fixable]` marker, counted as an
/// error, and left in place by `--fix`.
#[test]
fn unfixable_rule_is_reported_but_not_fixed() {
    let dir = artifact(DEAD_END_REASON, Some("unfixable = [\"ARA002\"]\n"));
    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains(
            "ARA002: trace/exploration_tree.yaml",
        ))
        .stdout(predicate::str::contains("[fixable]").not())
        .stdout(predicate::str::contains(
            "FAIL — 1 error(s), 1 warning(s), 0 fixable issue(s)",
        ));

    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("fixed").not())
        .stdout(predicate::str::contains("skipped").not());
    let tree = std::fs::read_to_string(dir.path().join("trace/exploration_tree.yaml")).unwrap();
    assert_eq!(tree, DEAD_END_REASON);

    let (json, _) = check_json(dir.path(), &[]);
    assert_eq!(json["lint"][0]["fixable"], false);
    assert!(json["lint"][0]["fix"].is_null());
}

/// Unknown rule codes, malformed selectors, and unknown keys are hard errors
/// (exit 2) that name the config file.
#[test]
fn invalid_config_is_a_clean_error() {
    for (config, needle) in [
        ("ignore = [\"ARA999\"]\n", "unknown rule code `ARA999`"),
        ("select = [\"E501\"]\n", "invalid rule selector `E501`"),
        ("ignored = [\"ARA212\"]\n", "ignored"),
        ("[severity]\nARA212 = \"fatal\"\n", "fatal"),
        ("ignore = \"ARA212\"\n", "ignore"),
    ] {
        let dir = artifact(UNKNOWN_FIELD, Some(config));
        ara()
            .arg("check")
            .arg(dir.path())
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(".ara-check.toml"))
            .stderr(predicate::str::contains(needle));
    }
}

/// `--config` loads an explicit file (outside the artifact); a missing one is
/// exit 2; `--config` and `--no-config` conflict.
#[test]
fn explicit_config_flag() {
    let dir = artifact(UNKNOWN_FIELD, None);
    let cfg_dir = TempDir::new().unwrap();
    let cfg = cfg_dir.path().join("custom.toml");
    std::fs::write(&cfg, "ignore = [\"ARA201\"]\n").unwrap();

    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--strict")
        .arg("--config")
        .arg(&cfg)
        .assert()
        .success();

    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--config")
        .arg(cfg_dir.path().join("missing.toml"))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot read config"));

    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--config")
        .arg(&cfg)
        .arg("--no-config")
        .assert()
        .code(2);
}

/// Discovery walks up from the ARA dir to the git repository root.
#[test]
fn config_discovered_at_git_root() {
    let repo = TempDir::new().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    std::fs::write(
        repo.path().join(".ara-check.toml"),
        "ignore = [\"ARA201\"]\n",
    )
    .unwrap();
    let ara_dir = repo.path().join("papers/my-ara");
    std::fs::create_dir_all(ara_dir.join("trace")).unwrap();
    std::fs::write(ara_dir.join("trace/exploration_tree.yaml"), UNKNOWN_FIELD).unwrap();

    ara()
        .arg("check")
        .arg(&ara_dir)
        .arg("--strict")
        .assert()
        .success()
        .stdout(predicate::str::contains("ARA201").not());

    // A config in the ARA dir takes precedence over the repo-root one.
    std::fs::write(ara_dir.join(".ara-check.toml"), "").unwrap();
    ara()
        .arg("check")
        .arg(&ara_dir)
        .arg("--strict")
        .assert()
        .failure()
        .stdout(predicate::str::contains("ARA201"));
}

/// `ara validate` never reads `.ara-check.toml`.
#[test]
fn validate_ignores_check_config() {
    let dir = artifact(DUPLICATE_ID, Some("ignore = [\"ARA\"]\n"));
    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("duplicate node id"));
}

#[cfg(unix)]
#[test]
fn unreadable_discovered_config_never_falls_back_or_fixes() {
    for target in ["missing.toml", ".ara-check.toml"] {
        let repo = TempDir::new().unwrap();
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        std::fs::write(repo.path().join(".ara-check.toml"), "").unwrap();
        let dir = repo.path().join("artifact");
        std::fs::create_dir_all(dir.join("trace")).unwrap();
        let tree = dir.join("trace/exploration_tree.yaml");
        std::fs::write(&tree, DEAD_END_REASON).unwrap();
        let config = dir.join(".ara-check.toml");
        std::os::unix::fs::symlink(target, &config).unwrap();

        ara()
            .arg("check")
            .arg(&dir)
            .arg("--fix")
            .assert()
            .code(2)
            .stderr(predicate::str::contains(config.display().to_string()));
        assert_eq!(std::fs::read_to_string(&tree).unwrap(), DEAD_END_REASON);
    }
}
