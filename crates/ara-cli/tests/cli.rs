//! CLI integration tests for `ara validate`.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn ara() -> Command {
    Command::cargo_bin("ara").expect("binary builds")
}

fn official(name: &str) -> PathBuf {
    // ara-cli/tests -> ara-cli -> crates -> repo root, then into ara-core fixtures.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ara-core/tests/fixtures/official")
        .join(name)
}

/// Builds a temp ARA artifact with the given tree YAML and optional claims.
fn artifact(tree_yaml: &str, claims_md: Option<&str>) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("trace")).unwrap();
    std::fs::write(dir.path().join("trace/exploration_tree.yaml"), tree_yaml).unwrap();
    if let Some(claims) = claims_md {
        std::fs::create_dir_all(dir.path().join("logic")).unwrap();
        std::fs::write(dir.path().join("logic/claims.md"), claims).unwrap();
    }
    dir
}

#[test]
fn validate_official_exits_zero() {
    ara()
        .arg("validate")
        .arg(official("minimal-artifact"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));

    ara()
        .arg("validate")
        .arg(official("resnet-ara-example"))
        .assert()
        .success();
}

#[test]
fn validate_broken_exits_nonzero() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
        None,
    );
    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("duplicate node id"));
}

#[test]
fn json_output_is_valid_json() {
    let output = ara()
        .arg("validate")
        .arg(official("minimal-artifact"))
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(parsed.get("errors").is_some());
    assert!(parsed.get("warnings").is_some());
}

#[test]
fn validate_broken_json_output_is_byte_stable() {
    let dir = artifact(
        "tree:\n  - id: N02\n    type: experiment\n    evidence: [C02, C01]\n  - id: N02\n    type: insight\n",
        None,
    );

    let output = ara()
        .arg("validate")
        .arg(dir.path())
        .arg("--json")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();

    let expected = concat!(
        "{\n",
        "  \"errors\": [\n",
        "    {\n",
        "      \"severity\": \"error\",\n",
        "      \"path\": \"nodes[N02]\",\n",
        "      \"message\": \"duplicate node id\"\n",
        "    }\n",
        "  ],\n",
        "  \"warnings\": [\n",
        "    {\n",
        "      \"severity\": \"warning\",\n",
        "      \"path\": \"nodes[N02].evidence[0]\",\n",
        "      \"message\": \"claim reference `C02` unresolved (no claims.md provided)\"\n",
        "    },\n",
        "    {\n",
        "      \"severity\": \"warning\",\n",
        "      \"path\": \"nodes[N02].evidence[1]\",\n",
        "      \"message\": \"claim reference `C01` unresolved (no claims.md provided)\"\n",
        "    }\n",
        "  ]\n",
        "}\n",
    );
    assert_eq!(output, expected.as_bytes());
}

#[test]
fn strict_promotes_warnings_to_failure() {
    // Unknown field -> warning, no error. Exit 0 normally, non-zero with --strict.
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n    title: q\n    bogus_field: 1\n",
        None,
    );

    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unknown field"));

    ara()
        .arg("validate")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .failure();
}

#[test]
fn missing_dir_is_clean_error_not_panic() {
    ara()
        .arg("validate")
        .arg("/no/such/ara/dir")
        .assert()
        .failure()
        .stdout(predicate::str::contains("cannot read"));
}

#[test]
fn missing_tree_file_is_clean_error() {
    let dir = TempDir::new().unwrap(); // empty, no trace/exploration_tree.yaml
    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("cannot read"));
}

// ── Layout command tests ──────────────────────────────────────────────────

#[test]
fn layout_json_produces_valid_positioned_manifest() {
    let output = ara()
        .arg("layout")
        .arg(official("minimal-artifact"))
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    // Has nodes with pos
    let nodes = manifest["nodes"].as_array().unwrap();
    assert!(!nodes.is_empty());
    for node in nodes {
        assert!(node.get("pos").is_some(), "node missing pos: {node}");
        let pos = &node["pos"];
        assert!(pos["x"].as_f64().unwrap().is_finite());
        assert!(pos["y"].as_f64().unwrap().is_finite());
    }
    // Has bounds
    let bounds = &manifest["bounds"];
    assert!(bounds["width"].as_f64().unwrap() > 0.0);
    assert!(bounds["height"].as_f64().unwrap() > 0.0);
}

#[test]
fn layout_missing_dir_exits_nonzero() {
    ara()
        .arg("layout")
        .arg("/no/such/ara/dir")
        .arg("--json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("layout skipped"));
}

#[test]
fn layout_parse_error_skips_layout() {
    let cycle_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cycle-dir");
    ara()
        .arg("layout")
        .arg(&cycle_dir)
        .arg("--json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("layout skipped"));
}

#[test]
fn validate_layout_flag_shows_counts_and_bounds() {
    ara()
        .arg("validate")
        .arg(official("minimal-artifact"))
        .arg("--layout")
        .assert()
        .success()
        .stdout(predicate::str::contains("node(s)"))
        .stdout(predicate::str::contains("bounds"));
}

#[test]
fn validate_layout_on_error_matches_validate() {
    let cycle_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cycle-dir");
    ara()
        .arg("validate")
        .arg(&cycle_dir)
        .arg("--layout")
        .assert()
        .failure()
        .stdout(predicate::str::contains("cycle"));
}

// ── Check command tests ───────────────────────────────────────────────────

/// A clean official artifact passes `ara check` with exit 0.
#[test]
fn check_clean_official_exits_zero() {
    ara()
        .arg("check")
        .arg(official("minimal-artifact"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

/// A fixable format issue (a `reason:` on a `dead_end` node) is reported with its
/// rule id and `[fixable]` marker, and exits 1 without `--fix`.
#[test]
fn check_fixable_without_fix_exits_one() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: dead_end\n    reason: it diverged\n",
        None,
    );
    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("ARA002"))
        .stdout(predicate::str::contains("[fixable]"));
}

/// `--fix` applies the safe fix in place; the change persists so a follow-up
/// `ara check` (no fix) on the same dir also passes.
#[test]
fn check_fix_applies_and_persists() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: dead_end\n    reason: it diverged\n",
        None,
    );
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .success()
        .stdout(predicate::str::contains("fixed ARA002"));

    // The fix reached disk: a plain re-check now passes.
    ara().arg("check").arg(dir.path()).assert().success();

    let fixed = std::fs::read_to_string(dir.path().join("trace/exploration_tree.yaml")).unwrap();
    assert!(fixed.contains("why_failed: it diverged"), "got: {fixed}");
}

/// Running `--fix` twice is idempotent: the second run applies nothing and exits 0.
#[test]
fn check_fix_is_idempotent() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: dead_end\n    reason: it diverged\n",
        None,
    );
    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .success();

    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .success()
        .stdout(predicate::str::contains("applied 0 fix(es)"));
}

#[test]
fn check_fix_recovers_multiple_claims_to_clean_exit() {
    let tree = "\
tree:
  - id: N01
    type: experiment
    evidence: [C01]
  - id: N02
    type: experiment
    evidence: [C02]
";
    let claims = "\
## C01 - First recovered claim
- **Statement**: first

## C02 — Second recovered claim
- **Statement**: second
";
    let dir = artifact(tree, Some(claims));
    let tree_path = dir.path().join("trace/exploration_tree.yaml");
    let claims_path = dir.path().join("logic/claims.md");
    let original_tree = std::fs::read(&tree_path).unwrap();

    let before_output = ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let before_stdout = String::from_utf8(before_output).unwrap();
    assert_eq!(
        before_stdout
            .matches("evidence references unknown claim")
            .count(),
        2,
        "got: {before_stdout}"
    );

    let first_output = ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_stdout = String::from_utf8(first_output).unwrap();
    let fixed_tree = std::fs::read(&tree_path).unwrap();
    assert_eq!(fixed_tree, original_tree);
    assert_eq!(
        first_stdout
            .matches("fixed ARA004 in logic/claims.md: rewrote dash claim-header separator to `: `")
            .count(),
        2,
        "got: {first_stdout}"
    );
    assert!(
        first_stdout.contains(
            "PASS — applied 2 fix(es); 0 error(s), 0 warning(s), 0 fixable issue(s) remaining"
        ),
        "got: {first_stdout}"
    );
    assert!(
        !first_stdout.contains("unknown claim"),
        "post-fix diagnostics still contain a caused error: {first_stdout}"
    );

    let fixed_claims = std::fs::read(&claims_path).unwrap();
    assert_eq!(
        fixed_claims,
        b"## C01: First recovered claim\n- **Statement**: first\n\n## C02: Second recovered claim\n- **Statement**: second\n"
    );

    let second_output = ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second_stdout = String::from_utf8(second_output).unwrap();
    assert!(
        second_stdout.contains(
            "PASS — applied 0 fix(es); 0 error(s), 0 warning(s), 0 fixable issue(s) remaining"
        ),
        "got: {second_stdout}"
    );
    assert!(
        !second_stdout.contains("fixed ARA004"),
        "second run reported another fix: {second_stdout}"
    );
    assert_eq!(std::fs::read(&tree_path).unwrap(), fixed_tree);
    assert_eq!(std::fs::read(&claims_path).unwrap(), fixed_claims);
}

#[test]
fn check_fix_applies_alias_while_unrelated_error_remains() {
    let tree = "\
tree:
  - id: N01
    type: dead_end
    reason: it diverged
  - id: N01
    type: question
";
    let dir = artifact(tree, None);
    let tree_path = dir.path().join("trace/exploration_tree.yaml");

    let first_output = ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let first_stdout = String::from_utf8(first_output).unwrap();
    assert!(
        first_stdout.contains(
            "fixed ARA002 in trace/exploration_tree.yaml: renamed `reason:` to `why_failed:` on a dead_end node"
        ),
        "got: {first_stdout}"
    );
    assert!(
        first_stdout.contains("error: nodes[N01]: duplicate node id"),
        "got: {first_stdout}"
    );
    assert!(
        first_stdout.contains(
            "FAIL — applied 1 fix(es); 1 error(s), 0 warning(s), 0 fixable issue(s) remaining"
        ),
        "got: {first_stdout}"
    );

    let fixed_tree = std::fs::read(&tree_path).unwrap();
    assert_eq!(
        fixed_tree,
        b"tree:\n  - id: N01\n    type: dead_end\n    why_failed: it diverged\n  - id: N01\n    type: question\n"
    );

    let second_output = ara()
        .arg("check")
        .arg(dir.path())
        .arg("--fix")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let second_stdout = String::from_utf8(second_output).unwrap();
    assert!(
        second_stdout.contains("error: nodes[N01]: duplicate node id"),
        "got: {second_stdout}"
    );
    assert!(
        second_stdout.contains(
            "FAIL — applied 0 fix(es); 1 error(s), 0 warning(s), 0 fixable issue(s) remaining"
        ),
        "got: {second_stdout}"
    );
    assert!(
        !second_stdout.contains("fixed ARA002"),
        "second run reported another fix: {second_stdout}"
    );
    assert_eq!(std::fs::read(&tree_path).unwrap(), fixed_tree);
}

/// A real validate error (duplicate node id) exits 1 and surfaces the error.
#[test]
fn check_validate_error_exits_one() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
        None,
    );
    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("duplicate node id"));
}

/// An artifact that parses with only a warning passes normally but fails under
/// `--strict`.
#[test]
fn check_strict_promotes_warning_to_failure() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n    title: q\n    bogus_field: 1\n",
        None,
    );

    ara()
        .arg("check")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unknown field"));

    ara()
        .arg("check")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .failure();
}

/// `ara check --json` emits a parseable composed report.
#[test]
fn check_json_output_is_valid_json() {
    let output = ara()
        .arg("check")
        .arg(official("minimal-artifact"))
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(parsed.get("validate").is_some());
    assert!(parsed.get("lint").is_some());
    assert!(parsed.get("summary").is_some());
    assert!(parsed["summary"]["passed"].as_bool().unwrap());
}

/// `--json` must still honor the exit contract: a fixable issue exits 1 while
/// emitting valid JSON (regression: the no-fix JSON path used to always exit 0).
#[test]
fn check_json_fixable_exits_one_with_valid_json() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: dead_end\n    reason: it diverged\n",
        None,
    );
    let output = ara()
        .arg("check")
        .arg(dir.path())
        .arg("--json")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(parsed["summary"]["fixable"].as_u64().unwrap(), 1);
    assert!(!parsed["summary"]["passed"].as_bool().unwrap());
}

/// `--json` on a real validate error (duplicate node id) also exits 1 with valid
/// JSON.
#[test]
fn check_json_validate_error_exits_one_with_valid_json() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
        None,
    );
    let output = ara()
        .arg("check")
        .arg(dir.path())
        .arg("--json")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(parsed["summary"]["errors"].as_u64().unwrap() >= 1);
    assert!(!parsed["summary"]["passed"].as_bool().unwrap());
}

/// A non-existent target maps to the internal-failure exit code 2.
#[test]
fn check_missing_dir_exits_two() {
    ara().arg("check").arg("/no/such/ara/dir").assert().code(2);
}
