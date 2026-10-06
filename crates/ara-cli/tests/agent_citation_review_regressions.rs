//! Behavioral regressions for PR 115 review comments 4191480480 and 4191480509.
#[path = "support/citations.rs"]
mod support;
use support::*;

fn json_list_fixture() -> TempDir {
    let dir = fixture(false);
    write_file(
        dir.path(),
        "logic/claims.md",
        concat!(
            "# Claims\n\n## C01: Survivor\n- **Statement**: Survivor.\n- **Status**: supported\n\n## C09: Source\n- **Statement**: Source.\n- **Status**: hypothesis\n\n## C04: Citer\n- **Statement**: Citer.\n- **Status**: hypothesis\n- **Sources**: ",
            r#"["`C09`", "'C09'", "C09", "escaped \"C09\""]"#,
            "\n",
        ),
    );
    dir
}

#[test]
fn refs_protects_inner_quotes_and_code_in_native_json_lists() {
    let dir = json_list_fixture();
    let report = read(dir.path(), &["refs", "C09"]);
    let rows: Vec<_> = report["structured"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"] == "C04" && row["field"] == "Sources")
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "only the unprotected sibling is a citation: {rows:?}"
    );
    assert_eq!(rows[0]["literal"], "C09");
}

#[test]
fn automatic_merge_repairs_only_unprotected_native_json_list_items() {
    let dir = json_list_fixture();
    let root = dir.path();
    let before = field(root, json!({"id":"C04"}), "Sources");
    let items: Vec<String> = serde_json::from_str(&before).unwrap();
    assert_eq!(items, ["`C09`", "'C09'", "C09", "escaped \"C09\""]);
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Merge quoted-list source"}),
            json!({"op":"logic.revise","target":{"id":"C09"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    let after = field(root, json!({"id":"C04"}), "Sources");
    assert_eq!(after, before.replacen("\"C09\"", "\"C01\"", 1));
    let report = read(root, &["refs", "C01"]);
    let rows: Vec<_> = report["structured"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"] == "C04" && row["field"] == "Sources")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["literal"], "C01");
}

#[test]
fn native_json_backtick_delimiter_runs_protect_code_during_refs_and_repair() {
    let dir = json_list_fixture();
    let root = dir.path();
    let before = r#"["``C09``", "```C09```", "``C09` example``", "C09"]"#;
    let claims = fs::read_to_string(root.join("logic/claims.md")).unwrap();
    write_file(
        root,
        "logic/claims.md",
        claims.replace(r#"["`C09`", "'C09'", "C09", "escaped \"C09\""]"#, before),
    );
    let report = read(root, &["refs", "C09"]);
    let rows: Vec<_> = report["structured"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"] == "C04" && row["field"] == "Sources")
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "code-span contents are not citations: {rows:?}"
    );
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Merge around code delimiter runs"}),
            json!({"op":"logic.revise","target":{"id":"C09"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Sources"),
        before.replacen("\"C09\"", "\"C01\"", 1)
    );
}

#[test]
fn historical_single_segment_heading_selector_does_not_match_a_joined_nested_path() {
    let dir = fixture(false);
    let root = dir.path();
    write_file(
        root,
        "logic/concepts.md",
        "## Group A\n\n### Term\n- **Definition**: Nested term.\n\n## Group A/Term\n- **Definition**: Literal slash term.\n",
    );
    write_file(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Literal selectors\n    provenance: user\n    description: Distinguish heading segments\n",
    );
    let literal = json!({"document":"logic/concepts.md","heading":["Group A/Term"]});
    let first = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Revise literal slash heading"}),
            json!({"op":"logic.revise","target":literal,"set":{"Definition":"Revised literal slash term."},"signal":"user-directive","provenance":"user"}),
        ],
        false,
    );
    let old_record = session_of(root, &first);
    let old_revision = old_record["logic_revisions"][0].clone();
    assert_eq!(old_revision["entry"]["heading"], json!(["Group A/Term"]));
    let nested = json!({"document":"logic/concepts.md","heading":["Group A","Term"]});
    let expected = entry_digest(root, nested.clone());
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename nested heading only"}),
            json!({"op":"entry.rename","target":nested,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    assert_eq!(
        field(root, literal, "Definition"),
        "Revised literal slash term."
    );
    assert_eq!(
        field(
            root,
            json!({"document":"logic/concepts.md","heading":["Group A","Word"]}),
            "Definition"
        ),
        "Nested term."
    );
    let session = first["operations"][0]["id"].as_str().unwrap();
    let record = yaml(root, &format!("trace/sessions/{session}.yaml"));
    assert!(
        record["logic_revisions"]
            .as_array()
            .unwrap()
            .contains(&old_revision)
    );
}

#[test]
fn historical_scalar_joined_heading_locator_keeps_its_existing_semantics() {
    let dir = fixture(false);
    let root = dir.path();
    write_file(
        root,
        "logic/concepts.md",
        "## Group A\n\n### Term\n- **Definition**: Nested term.\n",
    );
    let tree = "tree:\n  - id: N01\n    type: question\n    title: Scalar path\n    provenance: user\n    description: Preserve scalar addressing\n    source_refs: [\"logic/concepts.md#Group A/Term\"]\n    concepts: [\"Group A/Term\"]\n";
    write_file(root, "trace/exploration_tree.yaml", tree);
    let nested = json!({"document":"logic/concepts.md","heading":["Group A","Term"]});
    let expected = entry_digest(root, nested.clone());
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename scalar-referenced heading"}),
            json!({"op":"entry.rename","target":nested,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    let checked = report["operations"][1]["historical_citations"]
        .as_array()
        .unwrap();
    assert!(
        checked
            .iter()
            .any(|row| row["literal"] == "logic/concepts.md#Group A/Term")
    );
    assert!(checked.iter().any(|row| row["literal"] == "Group A/Term"));
    assert_eq!(
        fs::read_to_string(root.join("trace/exploration_tree.yaml")).unwrap(),
        tree
    );
    read(root, &["show", "logic/concepts.md#Group A/Term"]);
}
