//! PR 115: ordered restructure selectors and fail-closed citation repairs.
#[path = "support/citations.rs"]
mod support;
use support::*;

fn binding_fixture() -> TempDir {
    let dir = fixture(false);
    write_file(
        dir.path(),
        "logic/claims.md",
        format!(
            "{}\n## C16: Allocation boundary\n- **Statement**: Existing boundary.\n- **Status**: hypothesis\n",
            claims(false)
        ),
    );
    dir
}

fn add_claim(id: &str, title: &str, proof: Option<&str>) -> Value {
    let mut fields = json!({"Statement":title,"Conditions":"C","Status":"hypothesis","Provenance":"user","Falsification":"F"});
    if let Some(proof) = proof {
        fields["Proof"] = json!(proof);
    }
    json!({"op":"claim.add","id":id,"title":title,"fields":fields})
}

fn split_op(destinations: Value, rows: Value) -> Value {
    json!({"op":"logic.revise","target":{"id":"C17"},"set":{"Statement":"Narrowed primary."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":destinations,"references":rows})
}

fn split_prefix() -> Vec<Value> {
    vec![
        json!({"op":"session.log","summary":"Split with ordered creation bindings"}),
        add_claim("C17", "Primary", None),
    ]
}

#[test]
fn split_into_resolves_an_earlier_allocated_claim_binding() {
    let dir = binding_fixture();
    let root = dir.path();
    let before = artifact_bytes(root);
    let mut operations = split_prefix();
    operations.push(add_claim("$spin", "Spin-off", None));
    operations.push(split_op(json!([{"id":"$spin"}]), json!([])));
    let preview = apply(root, &operations, true);
    assert_eq!(artifact_bytes(root), before, "bound dry run writes nothing");
    assert_eq!(preview["operations"][2]["id"], "C18");
    let report = apply(root, &operations, false);
    assert_eq!(report["operations"][2]["id"], "C18");
    assert_eq!(
        field(root, json!({"id":"C17"}), "Statement"),
        "Narrowed primary."
    );
    assert_eq!(field(root, json!({"id":"C18"}), "Statement"), "Spin-off");
    let record = session_of(root, &report);
    let rows = record["logic_revisions"].as_array().unwrap();
    let split = rows.iter().find(|row| row["action"] == "split").unwrap();
    assert_eq!(split["split_into"], json!([{"id":"C18"}]));
}

#[test]
fn split_reference_target_resolves_an_earlier_creation_binding() {
    let dir = binding_fixture();
    let root = dir.path();
    let mut operations = split_prefix();
    operations.push(add_claim("C18", "Spin-off", None));
    operations.push(add_claim("$citer", "Bound citer", Some("C17")));
    operations.push(split_op(
        json!([{"id":"C18"}]),
        json!([{"target":{"id":"$citer"},"field":"Proof","before":"C17","after":"C18"}]),
    ));
    let report = apply(root, &operations, false);
    assert_eq!(report["operations"][3]["id"], "C19");
    assert_eq!(field(root, json!({"id":"C19"}), "Proof"), "C18");
    let record = session_of(root, &report);
    assert!(
        record["logic_revisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| {
                row["entry"] == json!({"id":"C19"})
                    && row["field"] == "Proof"
                    && row["before"] == "C17"
                    && row["after"] == "C18"
            })
    );
}

#[test]
fn split_into_unknown_forward_and_wrong_kind_bindings_have_indexed_diagnostics() {
    for (kind, expected_code) in [
        ("unknown", "write.binding_unknown"),
        ("forward", "write.binding_unknown"),
        ("wrong-kind", "write.split_destination"),
    ] {
        let dir = binding_fixture();
        let root = dir.path();
        let before = artifact_bytes(root);
        let mut operations = split_prefix();
        operations.push(add_claim("C18", "Valid spin-off", None));
        if kind == "wrong-kind" {
            operations.push(json!({"op":"heuristic.add","id":"$bad","title":"Not a claim","fields":{"Rationale":"Different namespace.","Sensitivity":"low","Code ref":"src/a.py"}}));
        }
        let revise_line = operations.len() + 2;
        operations.push(split_op(json!([{"id":"C18"},{"id":"$bad"}]), json!([])));
        if kind == "forward" {
            operations.push(add_claim("$bad", "Too late", None));
        }
        let error = apply_failure(root, &format!("\n{}", jsonl(&operations)));
        assert_eq!(error["code"], expected_code, "{kind}: {error}");
        assert_eq!(error["line"], revise_line, "{kind}: {error}");
        assert_eq!(
            error["details"]["field"], "split_into[1]",
            "{kind}: {error}"
        );
        assert_eq!(artifact_bytes(root), before, "{kind} must be atomic");
    }
}

#[test]
fn split_reference_unknown_forward_and_wrong_kind_bindings_have_indexed_diagnostics() {
    for (kind, expected_code) in [
        ("unknown", "write.binding_unknown"),
        ("forward", "write.binding_unknown"),
        ("wrong-kind", "write.reference"),
    ] {
        let dir = binding_fixture();
        let root = dir.path();
        let before = artifact_bytes(root);
        let mut operations = split_prefix();
        operations.push(add_claim("C18", "Spin-off", None));
        operations.push(add_claim("C19", "Valid citer", Some("C17")));
        if kind == "wrong-kind" {
            operations.push(json!({"op":"heuristic.add","id":"$bad","title":"Not a claim citer","fields":{"Rationale":"Different namespace.","Sensitivity":"low","Code ref":"src/a.py"}}));
        }
        let revise_line = operations.len() + 2;
        operations.push(split_op(
            json!([{"id":"C18"}]),
            json!([
                {"target":{"id":"C19"},"field":"Proof","before":"C17","after":"C18"},
                {"target":{"id":"$bad"},"field":"Proof","before":"C17","after":"C18"}
            ]),
        ));
        if kind == "forward" {
            operations.push(add_claim("$bad", "Too late", Some("C17")));
        }
        let error = apply_failure(root, &format!("\n{}", jsonl(&operations)));
        assert_eq!(error["code"], expected_code, "{kind}: {error}");
        assert_eq!(error["line"], revise_line, "{kind}: {error}");
        let at = if kind == "wrong-kind" {
            "references[1].field"
        } else {
            "references[1].target"
        };
        assert_eq!(error["details"]["field"], at, "{kind}: {error}");
        assert_eq!(artifact_bytes(root), before, "{kind} must be atomic");
    }
}

#[test]
fn split_reference_after_remains_literal_even_when_its_binding_exists() {
    let dir = binding_fixture();
    let root = dir.path();
    let before = artifact_bytes(root);
    let mut operations = split_prefix();
    operations.push(add_claim("$spin", "Spin-off", None));
    operations.push(add_claim("C19", "Citer", Some("C17")));
    // Concrete selectors isolate after's literal contract from selector substitution.
    operations.push(split_op(
        json!([{"id":"C18"}]),
        json!([{"target":{"id":"C19"},"field":"Proof","before":"C17","after":"$spin"}]),
    ));
    let error = apply_failure(root, &jsonl(&operations));
    assert_eq!(error["code"], "write.reference_mapping", "{error}");
    assert_eq!(error["details"]["field"], "references[0].after");
    assert_eq!(artifact_bytes(root), before);
}

fn self_citation_fixture() -> TempDir {
    let dir = fixture(false);
    write_file(
        dir.path(),
        "logic/claims.md",
        format!(
            "{}\n## C17: Survivor\n- **Statement**: Survivor.\n- **Status**: hypothesis\n- **Proof**: C18\n\n## C18: Source\n- **Statement**: Source.\n- **Status**: hypothesis\n\n## C19: Spin-off\n- **Statement**: Spin-off.\n- **Status**: hypothesis\n- **Proof**: C18\n",
            claims(false)
        ),
    );
    dir
}

fn merge_self_op(automatic: bool) -> Value {
    let mut op = json!({"op":"logic.revise","target":{"id":"C18"},"set":{"Status":"withdrawn","Merged into":"C17"},"signal":"user-directive","provenance":"user"});
    if automatic {
        op["rewrite_references"] = json!(true);
    } else {
        op["references"] =
            json!([{"target":{"id":"C17"},"field":"Proof","before":"C18","after":"C17"}]);
    }
    op
}

#[test]
fn explicit_merge_repair_refuses_self_citation_like_automatic_repair() {
    for automatic in [true, false] {
        let dir = self_citation_fixture();
        let root = dir.path();
        let before = artifact_bytes(root);
        let error = apply_failure(
            root,
            &jsonl(&[
                json!({"op":"session.log","summary":"Must not introduce a self citation"}),
                merge_self_op(automatic),
            ]),
        );
        assert_eq!(
            error["code"], "write.reference_rewrite",
            "automatic={automatic}: {error}"
        );
        assert_eq!(error["line"], 2);
        assert!(
            error["details"]["locations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|location| {
                    location["document"] == "logic/claims.md" && location["field"] == "Proof"
                }),
            "{error}"
        );
        assert_eq!(artifact_bytes(root), before);
    }
}

#[test]
fn explicit_split_repair_refuses_a_spin_off_citing_itself() {
    let dir = self_citation_fixture();
    let root = dir.path();
    let before = artifact_bytes(root);
    let operation = json!({"op":"logic.revise","target":{"id":"C18"},"set":{"Statement":"Narrowed source."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C19"}],"references":[
        {"target":{"id":"C17"},"field":"Proof","before":"C18","after":"C18"},
        {"target":{"id":"C19"},"field":"Proof","before":"C18","after":"C19"}
    ]});
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Split must not introduce a self citation"}),
            operation,
        ]),
    );
    assert_eq!(error["code"], "write.reference_rewrite", "{error}");
    assert_eq!(error["line"], 2);
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn explicit_split_repair_refuses_normalized_and_expanded_self_citations() {
    for (before_proof, after_proof) in [
        ("C18", "logic/claims.md#C19"),
        ("[\"C18\"]", "[\"C18\", \"logic/claims.md#C19\"]"),
    ] {
        let dir = self_citation_fixture();
        let root = dir.path();
        let claims_path = root.join("logic/claims.md");
        let text = std::fs::read_to_string(&claims_path).unwrap();
        write_file(
            root,
            "logic/claims.md",
            text.replace("- **Proof**: C18", &format!("- **Proof**: {before_proof}")),
        );
        let before = artifact_bytes(root);
        let error = apply_failure(
            root,
            &jsonl(&[
                json!({"op":"session.log","summary":"Reject normalized self citation repair"}),
                json!({"op":"logic.revise","target":{"id":"C18"},"set":{"Statement":"Narrowed source."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C19"}],"references":[
                    {"target":{"id":"C17"},"field":"Proof","before":before_proof,"after":before_proof},
                    {"target":{"id":"C19"},"field":"Proof","before":before_proof,"after":after_proof}
                ]}),
            ]),
        );
        assert_eq!(error["code"], "write.reference_rewrite", "{error}");
        assert_eq!(error["details"]["field"], "references[1].after");
        assert_eq!(error["line"], 2);
        assert_eq!(artifact_bytes(root), before);
    }
}

#[test]
fn explicit_split_repair_keeps_existing_self_citations_and_primary_noops() {
    let dir = self_citation_fixture();
    let root = dir.path();
    let claims_path = root.join("logic/claims.md");
    let text = std::fs::read_to_string(&claims_path).unwrap();
    write_file(
        root,
        "logic/claims.md",
        text.replace(
            "- **Proof**: C18",
            "- **Proof**: [\"C18\", \"logic/claims.md#C19\"]",
        ),
    );
    let proof = "[\"C18\", \"logic/claims.md#C19\"]";
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Keep preexisting citations while classifying split"}),
            json!({"op":"logic.revise","target":{"id":"C18"},"set":{"Statement":"Narrowed source."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C19"}],"references":[
                {"target":{"id":"C17"},"field":"Proof","before":proof,"after":proof},
                {"target":{"id":"C19"},"field":"Proof","before":proof,"after":proof}
            ]}),
        ],
        false,
    );
    assert_eq!(field(root, json!({"id":"C17"}), "Proof"), proof);
    assert_eq!(field(root, json!({"id":"C19"}), "Proof"), proof);
}

#[test]
fn deliberate_content_revision_before_merge_is_not_a_citation_repair() {
    for automatic in [true, false] {
        let dir = self_citation_fixture();
        let root = dir.path();
        let mut merge = merge_self_op(automatic);
        if !automatic {
            merge["references"] = json!([]);
        }
        let report = apply(
            root,
            &[
                json!({"op":"session.log","summary":"Revise evidence deliberately, then merge"}),
                json!({"op":"logic.revise","target":{"id":"C17"},"set":{"Proof":"Independent evidence."},"signal":"user-directive","provenance":"user"}),
                merge,
            ],
            false,
        );
        assert_eq!(
            field(root, json!({"id":"C17"}), "Proof"),
            "Independent evidence."
        );
        assert_eq!(field(root, json!({"id":"C18"}), "Merged into"), "C17");
        let record = session_of(root, &report);
        assert!(
            record["logic_revisions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| {
                    row["entry"] == json!({"id":"C17"})
                        && row["field"] == "Proof"
                        && row["before"] == "C18"
                        && row["after"] == "Independent evidence."
                })
        );
    }
}

#[test]
fn deliberate_content_revision_before_split_keeps_its_own_audit() {
    let dir = self_citation_fixture();
    let root = dir.path();
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Revise spin-off evidence deliberately, then split"}),
            json!({"op":"logic.revise","target":{"id":"C19"},"set":{"Proof":"Independent evidence."},"signal":"user-directive","provenance":"user"}),
            json!({"op":"logic.revise","target":{"id":"C18"},"set":{"Statement":"Narrowed source."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C19"}],"references":[{"target":{"id":"C17"},"field":"Proof","before":"C18","after":"C18"}]}),
        ],
        false,
    );
    assert_eq!(
        field(root, json!({"id":"C19"}), "Proof"),
        "Independent evidence."
    );
    assert_eq!(field(root, json!({"id":"C17"}), "Proof"), "C18");
    let record = session_of(root, &report);
    assert!(
        record["logic_revisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| {
                row["entry"] == json!({"id":"C19"})
                    && row["field"] == "Proof"
                    && row["before"] == "C18"
                    && row["after"] == "Independent evidence."
            })
    );
}

#[test]
fn explicit_merge_repair_cannot_smuggle_a_deliberate_content_revision() {
    let dir = self_citation_fixture();
    let root = dir.path();
    let before = artifact_bytes(root);
    let mut operation = merge_self_op(false);
    operation["references"][0]["after"] = json!("Independent evidence.");
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Content changes need a separate revision"}),
            operation,
        ]),
    );
    assert_eq!(error["code"], "write.reference_mapping", "{error}");
    assert_eq!(error["details"]["field"], "references[0].after");
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn split_automatic_repair_still_refuses_to_choose_a_destination() {
    let dir = self_citation_fixture();
    let root = dir.path();
    let before = artifact_bytes(root);
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"A split requires explicit classification"}),
            json!({"op":"logic.revise","target":{"id":"C18"},"set":{"Statement":"Narrowed source."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C19"}],"rewrite_references":true}),
        ]),
    );
    assert_eq!(error["code"], "write.reference_mode", "{error}");
    assert_eq!(error["details"]["field"], "rewrite_references");
    assert_eq!(artifact_bytes(root), before);
}
