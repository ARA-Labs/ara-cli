#![cfg(feature = "native")]

use ara_core::write::{ArtifactSnapshot, Fields, WorkingArtifact, WriteOperation, node};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tempfile::TempDir;

const TREE: &str = "trace/exploration_tree.yaml";

const BATCH_TIME: &str = "2026-10-01T12:00:00Z";
fn working(source: &str) -> (TempDir, WorkingArtifact) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("trace")).unwrap();
    std::fs::write(
        directory.path().join("PAPER.md"),
        "---\ntitle: Writer fixture\n---\n# Writer fixture\n",
    )
    .unwrap();
    std::fs::write(directory.path().join(TREE), source).unwrap();
    let snapshot = ArtifactSnapshot::load(directory.path()).unwrap();
    let mut working = WorkingArtifact::new(snapshot);
    // Planner-level tests stand in for the writer's one locked clock read.
    working.batch_time = Some(BATCH_TIME.into());
    (directory, working)
}

fn add(parent: &str) -> WriteOperation {
    WriteOperation::NodeAdd {
        id: None,
        kind: "experiment".into(),
        parent: parent.into(),
        title: "Boundary 检查".into(),
        fields: Fields::from([("result".into(), json!("Observed boundary"))]),
        depends_on: Vec::new(),
    }
}

fn value(working: &WorkingArtifact, id: &str, field: &str) -> Value {
    let document = working.yaml(TREE).unwrap();
    fn search<'a>(
        node: &'a ara_core::write::positions::YamlNode,
        id: &str,
    ) -> Option<&'a ara_core::write::positions::YamlNode> {
        if node
            .get("id")
            .unwrap()
            .is_some_and(|value| value.scalar() == Some(id))
        {
            return Some(node);
        }
        node.get("children")
            .unwrap()
            .and_then(|children| children.sequence().ok())
            .and_then(|children| children.iter().find_map(|child| search(child, id)))
    }
    let root = &document.root;
    let found = if let Some(tree) = root.get("tree").unwrap() {
        tree.sequence()
            .unwrap()
            .iter()
            .find_map(|node| search(node, id))
            .unwrap()
    } else {
        search(root.get("root").unwrap().unwrap(), id).unwrap()
    };
    found.get(field).unwrap().unwrap().to_json().unwrap()
}

#[test]
fn nested_source_ids_allocate_after_real_maximum_not_block_scalar_text() {
    let source = "# keep header\nmetadata: {nested: [keep, 7]}\ntree:\n  - id: N01\n    type: question\n    description: |\n      id: N99999\n      你好\n    children:\n      - id: N17\n        type: decision\n        status: legacy-wrong-kind\n        unknown: {opaque: [1, 2]}\n";
    let (_directory, mut working) = working(source);
    let result = node::plan(&mut working, &add("N17")).unwrap();
    assert_eq!(result.id.as_deref(), Some("N18"));
    assert!(working.text(TREE).unwrap().starts_with(source));
    assert_eq!(value(&working, "N18", "result"), json!("Observed boundary"));
    assert_eq!(value(&working, "N17", "status"), json!("legacy-wrong-kind"));
    node::validate_references(&working).unwrap();
}

#[test]
fn root_add_preserves_existing_tree_and_metadata_bytes() {
    let source =
        "metadata: {keep: 'café'}\ntree:\n  - id: N03\n    type: question\n    title: Existing\n";
    let (_directory, mut working) = working(source);
    let result = node::plan(&mut working, &add("root")).unwrap();
    assert_eq!(result.id.as_deref(), Some("N04"));
    assert!(working.text(TREE).unwrap().starts_with(source));
    assert_eq!(node::node_ids(&working).unwrap(), ["N03", "N04"]);
}

#[test]
fn empty_missing_and_null_collections_create_exact_node_values() {
    for source in ["tree: []\n", "tree:\n", "metadata: retained\n", "root:\n"] {
        let (_directory, mut working) = working(source);
        let result = node::plan(&mut working, &add("root")).unwrap();
        assert_eq!(result.id.as_deref(), Some("N01"), "{source}");
        assert_eq!(value(&working, "N01", "title"), json!("Boundary 检查"));
        assert_eq!(value(&working, "N01", "type"), json!("experiment"));
        if source.contains("metadata") {
            assert!(
                working
                    .text(TREE)
                    .unwrap()
                    .starts_with("metadata: retained\n")
            );
        }
        if source.starts_with("root") {
            assert!(
                working
                    .yaml(TREE)
                    .unwrap()
                    .root
                    .get("tree")
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn missing_empty_and_null_children_support_single_root_dialect() {
    for tail in ["", "  children: []\n", "  children:\n"] {
        let source = format!("root:\n  id: N01\n  type: question\n  title: 原文\n{tail}");
        let (_directory, mut working) = working(&source);
        let result = node::plan(&mut working, &add("N01")).unwrap();
        assert_eq!(result.id.as_deref(), Some("N02"));
        assert!(
            working
                .text(TREE)
                .unwrap()
                .starts_with("root:\n  id: N01\n  type: question\n  title: 原文\n")
        );
        assert_eq!(value(&working, "N02", "result"), json!("Observed boundary"));
        assert!(
            working
                .yaml(TREE)
                .unwrap()
                .root
                .get("tree")
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn exact_multiline_utf8_crlf_payload_and_existing_source_are_preserved() {
    let source = "metadata: café\r\ntree:\r\n  - id: N01\r\n    type: question\r\n    description: |-\r\n      Keep α\r\n      id: N999\r\n    unknown: {data: [a, b]} # preserve comment\r\n";
    for supplied in [
        "你好\nquote: \"x\"\n\n",
        "α\r\nβ\r\n",
        "line\nno trailing newline",
    ] {
        let (_directory, mut working) = working(source);
        let mut operation = add("N01");
        if let WriteOperation::NodeAdd { fields, .. } = &mut operation {
            fields.insert("result".into(), json!(supplied));
            fields.insert("timestamp".into(), json!("2026-10-01T10:00"));
            fields.insert("provenance".into(), json!("ai-executed"));
        }
        node::plan(&mut working, &operation).unwrap();
        assert!(working.text(TREE).unwrap().starts_with(source));
        assert_eq!(value(&working, "N02", "result"), json!(supplied));
        assert_eq!(
            value(&working, "N02", "timestamp"),
            json!("2026-10-01T10:00")
        );
        assert_eq!(value(&working, "N02", "provenance"), json!("ai-executed"));
        assert!(
            !working
                .text(TREE)
                .unwrap()
                .replace("\r\n", "")
                .contains('\n')
        );
    }
}

#[test]
fn explicit_replay_id_preserves_spelling_and_future_allocation_uses_maximum() {
    let (_directory, mut working) = working("tree: []\n");
    let mut requested = add("root");
    if let WriteOperation::NodeAdd { id, .. } = &mut requested {
        *id = Some("N0007".into());
    }
    let result = node::plan(&mut working, &requested).unwrap();
    assert_eq!(result.id.as_deref(), Some("N0007"));
    assert_eq!(value(&working, "N0007", "id"), json!("N0007"));
    let next = node::plan(&mut working, &add("root")).unwrap();
    assert_eq!(next.id.as_deref(), Some("N08"));
    let before = working.text(TREE).unwrap().to_owned();
    let error = node::plan(&mut working, &requested).unwrap_err();
    assert_eq!(error.exit_code(), 1);
    assert_eq!(working.text(TREE).unwrap(), before);
}

#[test]
fn malformed_ambiguous_duplicate_and_overflow_ids_reject_without_source_change() {
    for source in [
        "tree:\n  - id: N1\n    type: question\n  - id: N01\n    type: question\n",
        "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: question\n",
        "tree:\n  - id: N01\n    id: N02\n    type: question\n",
        "tree:\n  - id: N18446744073709551615\n    type: question\n",
        "tree:\n  - id: N18446744073709551616\n    type: question\n",
        "tree:\n  - id: not-a-node\n    type: question\n",
    ] {
        let (directory, mut working) = working(source);
        let error = node::plan(&mut working, &add("root")).unwrap_err();
        assert_eq!(error.exit_code(), 1, "{source}");
        assert_eq!(working.text(TREE).unwrap(), source);
        assert_eq!(
            std::fs::read_to_string(directory.path().join(TREE)).unwrap(),
            source
        );
    }
}

#[test]
fn unsupported_target_alias_anchor_and_tag_leave_original_bytes_identical() {
    for source in [
        "tree:\n  - &node\n    id: N01\n    type: question\n",
        "prototype: &node {id: N01, type: question}\ntree:\n  - *node\n",
        "tree:\n  - !node\n    id: N01\n    type: question\n",
        "tree:\n  - id: !id N01\n    type: question\n",
    ] {
        let (directory, mut working) = working(source);
        let error = node::plan(&mut working, &add("N01")).unwrap_err();
        assert_eq!(error.exit_code(), 1, "{source}");
        assert_eq!(working.text(TREE).unwrap(), source);
        assert_eq!(
            std::fs::read_to_string(directory.path().join(TREE)).unwrap(),
            source
        );
    }
}

#[test]
fn unknown_parent_and_second_single_root_reject_without_rewriting_dialect() {
    for (source, parent) in [
        ("tree:\n  - id: N01\n    type: question\n", "N99"),
        ("root:\n  id: N01\n  type: question\n", "root"),
        ("tree: []\nroot:\n  id: N01\n  type: question\n", "root"),
    ] {
        let (_directory, mut working) = working(source);
        assert_eq!(
            node::plan(&mut working, &add(parent))
                .unwrap_err()
                .exit_code(),
            1
        );
        assert_eq!(working.text(TREE).unwrap(), source);
    }
}

#[test]
fn field_registry_rejects_unknown_wrong_kind_wrong_types_and_artifact_object_keys() {
    for (key, payload) in [
        ("unknown", json!("x")),
        ("choice", json!("x")),
        ("result", json!(["not scalar"])),
        ("source_refs", json!({"invalid": "mapping"})),
        ("thinking", json!({"not": "scalar"})),
        (
            "artifacts",
            json!([{"name": "run", "pointer": "src/run.py", "what": "result", "extra": true}]),
        ),
        (
            "artifacts",
            json!([{"name": "run", "pointer": "src/run.py"}]),
        ),
        ("title", json!("inconsistent")),
        ("parent", json!("O01")),
    ] {
        let source = "tree: []\n";
        let (_directory, mut working) = working(source);
        let mut operation = add("root");
        if let WriteOperation::NodeAdd { fields, .. } = &mut operation {
            fields.insert(key.into(), payload);
        }
        let error = node::plan(&mut working, &operation).unwrap_err();
        assert_eq!(error.exit_code(), 1, "{key}");
        assert_eq!(
            error.field.as_deref(),
            Some(format!("fields.{key}").as_str())
        );
        assert_eq!(working.text(TREE).unwrap(), source);
    }
}

#[test]
fn authored_artifacts_and_typed_node_specific_fields_round_trip() {
    let (_directory, mut working) = working("tree: []\n");
    let operation = WriteOperation::NodeAdd {
        id: Some("N09".into()),
        kind: "decision".into(),
        parent: "root".into(),
        title: "Use bounded method".into(),
        fields: Fields::from([
            ("choice".into(), json!("bounded")),
            ("alternatives".into(), json!(["unbounded", "manual"])),
            (
                "evidence".into(),
                json!(["evidence/table.md", "caller statement"]),
            ),
            ("status".into(), json!("unresolved")),
            ("thinking".into(), json!("Tradeoffs\nretained")),
            ("concepts".into(), json!(["Boundary"])),
            ("source_refs".into(), json!(["paper:section2"])),
            (
                "artifacts".into(),
                json!([{"name":"runner","pointer":"src/run.py","what":"executes boundary"}]),
            ),
        ]),
        depends_on: Vec::new(),
    };
    node::plan(&mut working, &operation).unwrap();
    if let WriteOperation::NodeAdd { fields, .. } = operation {
        for (field, supplied) in fields {
            assert_eq!(value(&working, "N09", &field), supplied, "{field}");
        }
    }
}

const THREE: &str = "tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: experiment\n  - id: N03\n    type: question\n";

#[test]
fn repeated_edge_is_no_op_and_new_edge_preserves_source_entries() {
    let (_directory, mut working) = working(THREE);
    let operation = WriteOperation::EdgeAdd {
        node: "N02".into(),
        depends_on: "N03".into(),
    };
    let first = node::plan(&mut working, &operation).unwrap();
    assert!(!first.no_op);
    assert_eq!(value(&working, "N02", "also_depends_on"), json!(["N03"]));
    let before = working.text(TREE).unwrap().to_owned();
    let repeated = node::plan(&mut working, &operation).unwrap();
    assert!(repeated.no_op);
    assert_eq!(working.text(TREE).unwrap(), before);
    node::validate_references(&working).unwrap();
}

#[test]
fn dependency_rejects_self_ancestor_cycle_and_final_dangling_reference() {
    for (node_id, dependency) in [("N01", "N01"), ("N02", "N01")] {
        let (_directory, mut working) = working(THREE);
        let operation = WriteOperation::EdgeAdd {
            node: node_id.into(),
            depends_on: dependency.into(),
        };
        assert_eq!(
            node::plan(&mut working, &operation)
                .unwrap_err()
                .exit_code(),
            1
        );
        assert_eq!(working.text(TREE).unwrap(), THREE);
    }
    let (_directory, mut working) = working(THREE);
    node::plan(
        &mut working,
        &WriteOperation::EdgeAdd {
            node: "N02".into(),
            depends_on: "N03".into(),
        },
    )
    .unwrap();
    let before = working.text(TREE).unwrap().to_owned();
    let error = node::plan(
        &mut working,
        &WriteOperation::EdgeAdd {
            node: "N03".into(),
            depends_on: "N01".into(),
        },
    )
    .unwrap_err();
    assert!(error.message.contains("cycle"));
    assert_eq!(working.text(TREE).unwrap(), before);
    node::plan(
        &mut working,
        &WriteOperation::EdgeAdd {
            node: "N02".into(),
            depends_on: "N99".into(),
        },
    )
    .unwrap();
    assert!(
        node::validate_references(&working)
            .unwrap_err()
            .message
            .contains("unknown node")
    );
}

#[test]
fn concrete_forward_dependencies_resolve_at_final_batch_boundary() {
    let (_directory, mut working) = working("tree: []\n");
    let mut first = add("root");
    if let WriteOperation::NodeAdd { id, depends_on, .. } = &mut first {
        *id = Some("N01".into());
        depends_on.push("N05".into());
    }
    node::plan(&mut working, &first).unwrap();
    let mut later = add("root");
    if let WriteOperation::NodeAdd { id, .. } = &mut later {
        *id = Some("N05".into());
    }
    node::plan(&mut working, &later).unwrap();
    node::validate_references(&working).unwrap();
    assert_eq!(value(&working, "N01", "also_depends_on"), json!(["N05"]));
}

#[test]
fn same_as_uses_timestamps_and_rejects_duplicates_reciprocal_cycles_and_unknown_chronology() {
    // IDs and source traversal order deliberately disagree with chronology.
    let source = "tree:\n  - id: N01\n    type: question\n    timestamp: \"2026-10-03T10:00\"\n    unknown: {nested: [1, 2]} # opaque\n  - id: N99\n    type: question\n    timestamp: \"2026-10-01T10:00\"\n";
    let (_directory, mut candidate) = working(source);
    node::plan(
        &mut candidate,
        &WriteOperation::NodeLinkSameAs {
            node: "N01".into(),
            same_as: "N99".into(),
        },
    )
    .unwrap();
    node::validate_references(&candidate).unwrap();
    assert_eq!(value(&candidate, "N01", "same_as"), json!(["N99"]));
    assert!(
        candidate
            .text(TREE)
            .unwrap()
            .contains("unknown: {nested: [1, 2]} # opaque")
    );
    assert!(
        node::plan(
            &mut candidate,
            &WriteOperation::NodeLinkSameAs {
                node: "N01".into(),
                same_as: "N99".into()
            }
        )
        .is_err()
    );
    node::plan(
        &mut candidate,
        &WriteOperation::NodeLinkSameAs {
            node: "N99".into(),
            same_as: "N01".into(),
        },
    )
    .unwrap();
    assert!(node::validate_references(&candidate).is_err());
    assert!(!candidate.text(TREE).unwrap().contains("also_depends_on"));

    let legacy = "tree:\n  - id: N01\n    type: question\n    same_as: [N99]\n  - id: N99\n    type: question\n";
    let (_directory, mut candidate) = working(legacy);
    node::plan(&mut candidate, &add("root")).unwrap();
    node::validate_references(&candidate).unwrap();
    node::plan(
        &mut candidate,
        &WriteOperation::NodeLinkSameAs {
            node: "N99".into(),
            same_as: "N01".into(),
        },
    )
    .unwrap();
    assert!(node::validate_references(&candidate).is_err());
}

#[test]
fn new_nested_node_cannot_author_redundant_dependency() {
    let (_directory, mut working) = working(THREE);
    let mut operation = add("N02");
    if let WriteOperation::NodeAdd { depends_on, .. } = &mut operation {
        depends_on.push("N01".into());
    }
    assert!(
        node::plan(&mut working, &operation)
            .unwrap_err()
            .message
            .contains("redundant")
    );
    assert_eq!(working.text(TREE).unwrap(), THREE);
}

#[test]
fn requested_numeric_equivalent_id_cannot_replay_over_existing_spelling() {
    let source = "tree:\n  - id: N1\n    type: question\n";
    let (_directory, mut working) = working(source);
    let mut operation = add("root");
    if let WriteOperation::NodeAdd { id, .. } = &mut operation {
        *id = Some("N01".into());
    }
    assert_eq!(
        node::plan(&mut working, &operation)
            .unwrap_err()
            .exit_code(),
        1
    );
    assert_eq!(working.text(TREE).unwrap(), source);
}

#[test]
fn nonempty_flow_children_append_without_normalizing_existing_nodes() {
    let source =
        "tree:\n  - id: N01\n    type: question\n    children: [{id: N02, type: experiment}]\n";
    let (_directory, mut working) = working(source);
    node::plan(&mut working, &add("N01")).unwrap();
    let after = working.text(TREE).unwrap();
    assert!(after.contains("[{id: N02, type: experiment}, "));
    let document = working.yaml(TREE).unwrap().root.to_json().unwrap();
    assert_eq!(document["tree"][0]["children"].as_array().unwrap().len(), 2);
    assert_eq!(
        document["tree"][0]["children"][0],
        json!({"id":"N02","type":"experiment"})
    );
}

#[test]
fn same_as_rejects_self_and_final_dangling_targets() {
    let (_directory, mut working) = working(THREE);
    let self_link = WriteOperation::NodeLinkSameAs {
        node: "N02".into(),
        same_as: "N02".into(),
    };
    assert_eq!(
        node::plan(&mut working, &self_link)
            .unwrap_err()
            .exit_code(),
        1
    );
    assert_eq!(working.text(TREE).unwrap(), THREE);
    let dangling = WriteOperation::NodeLinkSameAs {
        node: "N02".into(),
        same_as: "N99".into(),
    };
    node::plan(&mut working, &dangling).unwrap();
    assert!(
        node::validate_references(&working)
            .unwrap_err()
            .message
            .contains("unknown node")
    );
}

#[test]
fn conflict_comments_preserve_values_and_ignore_old_block_scalar_fake_references() {
    let source = "tree:\n  - id: N01\n    type: question\n    description: |\n      # CONFLICT: see N999; old quoted text\n    unknown: {keep: true}\n  - id: N02\n    type: experiment\n";
    let (_directory, mut working) = working(source);
    let comment = "Caller-reported contradiction\n你好 \"quoted\" -- context\r\n\n";
    node::annotate(&mut working, "N01", "conflict", &["N02".into()], comment).unwrap();
    let text = working.text(TREE).unwrap();
    let at = source.find("  - id: N01").unwrap();
    let inserted = text.len() - source.len();
    assert_eq!(format!("{}{}", &text[..at], &text[at + inserted..]), source);
    let annotation = &text[at..at + inserted];
    assert!(annotation.starts_with("  # CONFLICT: see N02\n"));
    let payload = annotation
        .lines()
        .find_map(|line| line.strip_prefix("  # ARA annotation: "))
        .unwrap();
    let payload: Value = serde_json::from_str(payload).unwrap();
    assert_eq!(
        payload,
        json!({"kind": "conflict", "references": ["N02"], "comment": comment})
    );
    assert_eq!(
        value(&working, "N01", "description"),
        json!("# CONFLICT: see N999; old quoted text\n")
    );
    node::validate_references(&working).unwrap();
}

#[test]
fn conflict_annotation_references_are_checked_at_final_boundary() {
    let (_directory, mut working) = working(THREE);
    node::annotate(
        &mut working,
        "N02",
        "conflict",
        &["N09".into()],
        "Concrete forward reference",
    )
    .unwrap();
    assert!(
        node::validate_references(&working)
            .unwrap_err()
            .message
            .contains("unknown entry")
    );
    let mut operation = add("root");
    if let WriteOperation::NodeAdd { id, .. } = &mut operation {
        *id = Some("N09".into());
    }
    node::plan(&mut working, &operation).unwrap();
    node::validate_references(&working).unwrap();
    assert_eq!(value(&working, "N09", "title"), json!("Boundary 检查"));
}

#[test]
fn root_level_explicit_parent_contributes_ancestry_and_cycle_edges() {
    let source = "tree:\n  - id: N01\n    type: question\n  - id: N02\n    type: experiment\n    parent: N01\n    unknown: {keep: resumed-branch}\n  - id: N03\n    type: question\n";
    let (_directory, mut working) = working(source);
    let ancestor = WriteOperation::EdgeAdd {
        node: "N02".into(),
        depends_on: "N01".into(),
    };
    assert!(
        node::plan(&mut working, &ancestor)
            .unwrap_err()
            .message
            .contains("redundant")
    );
    assert_eq!(working.text(TREE).unwrap(), source);
    node::plan(
        &mut working,
        &WriteOperation::EdgeAdd {
            node: "N02".into(),
            depends_on: "N03".into(),
        },
    )
    .unwrap();
    let before = working.text(TREE).unwrap().to_owned();
    let cycle = WriteOperation::EdgeAdd {
        node: "N03".into(),
        depends_on: "N01".into(),
    };
    assert!(
        node::plan(&mut working, &cycle)
            .unwrap_err()
            .message
            .contains("cycle")
    );
    assert_eq!(working.text(TREE).unwrap(), before);
    assert!(before.contains("    parent: N01\n    unknown: {keep: resumed-branch}\n"));
}

#[test]
fn unknown_conflicting_and_cyclic_source_parent_pointers_reject_safe_mutations() {
    for source in [
        "tree:\n  - id: N01\n    type: question\n    parent: N99\n",
        "tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: experiment\n        parent: N03\n  - id: N03\n    type: question\n",
        "tree:\n  - id: N01\n    type: question\n    parent: N02\n  - id: N02\n    type: question\n    parent: N01\n",
        "tree:\n  - id: N01\n    type: question\n    parent: N01\n",
    ] {
        let (_directory, mut working) = working(source);
        let error = node::plan(&mut working, &add("N01")).unwrap_err();
        assert_eq!(error.exit_code(), 1);
        assert_eq!(error.field.as_deref(), Some("parent"));
        assert_eq!(working.text(TREE).unwrap(), source);
    }
}

#[test]
fn authored_parent_metadata_must_match_the_operation_parent() {
    for (operation_parent, explicit_parent) in [("N02", "N01"), ("root", "N01")] {
        let (_directory, mut working) = working(THREE);
        let mut operation = add(operation_parent);
        if let WriteOperation::NodeAdd { fields, .. } = &mut operation {
            fields.insert("parent".into(), json!(explicit_parent));
        }
        let error = node::plan(&mut working, &operation).unwrap_err();
        assert_eq!(error.field.as_deref(), Some("fields.parent"));
        assert_eq!(working.text(TREE).unwrap(), THREE);
    }
}

#[test]
fn matching_nested_parent_pointer_is_preserved_and_creation_metadata_agrees() {
    let source = "tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: experiment\n        parent: N01\n";
    let (_directory, mut working) = working(source);
    let mut operation = add("N02");
    if let WriteOperation::NodeAdd { fields, .. } = &mut operation {
        fields.insert("parent".into(), json!("N02"));
    }
    let result = node::plan(&mut working, &operation).unwrap();
    assert_eq!(result.id.as_deref(), Some("N03"));
    assert!(working.text(TREE).unwrap().starts_with(source));
    assert_eq!(value(&working, "N02", "parent"), json!("N01"));
    assert_eq!(value(&working, "N03", "parent"), json!("N02"));
    node::validate_references(&working).unwrap();
}

#[test]
fn same_as_generic_deltas_cannot_bypass_chronology_duplicate_or_cycle_guards() {
    let source = "tree:\n  - id: N01\n    type: question\n    timestamp: \"2026-10-03T10:00\"\n  - id: N99\n    type: question\n    timestamp: \"2026-10-01T10:00\"\n    same_as: [N01]\n";
    let (_directory, mut candidate) = working(source);
    candidate
        .replace_yaml_field(
            TREE,
            &[
                "tree".into(),
                ara_core::write::positions::PathPart::Index(0),
            ],
            "same_as",
            &json!(["N99"]),
        )
        .unwrap();
    let error = node::validate_references(&candidate).unwrap_err();
    assert_eq!(error.field.as_deref(), Some("same_as"));
    assert!(error.message.contains("cycle"));
    for targets in [json!(["N99", "N99"]), json!(["N01"]), json!(["N999"])] {
        let (_directory, mut candidate) = working(
            "tree:\n  - id: N01\n    type: question\n    timestamp: \"2026-10-03T10:00\"\n  - id: N99\n    type: question\n    timestamp: \"2026-10-01T10:00\"\n",
        );
        candidate
            .replace_yaml_field(
                TREE,
                &[
                    "tree".into(),
                    ara_core::write::positions::PathPart::Index(0),
                ],
                "same_as",
                &targets,
            )
            .unwrap();
        assert!(node::validate_references(&candidate).is_err());
    }
    for timestamps in [
        ("2026-10-01T10:00Z", "2026-10-01T12:00+02:00"),
        ("2026-10-01T09:00Z", "2026-10-01T10:00Z"),
    ] {
        let source = format!(
            "tree:\n  - id: N01\n    type: question\n    timestamp: {:?}\n  - id: N99\n    type: question\n    timestamp: {:?}\n",
            timestamps.0, timestamps.1
        );
        let (_directory, mut candidate) = working(&source);
        node::plan(
            &mut candidate,
            &WriteOperation::NodeLinkSameAs {
                node: "N01".into(),
                same_as: "N99".into(),
            },
        )
        .unwrap();
        assert!(node::validate_references(&candidate).is_err());
    }
}

#[test]
fn same_as_uses_real_add_operation_order_without_inventing_historical_order() {
    let (_directory, mut candidate) = working("tree: []\n");
    for id in ["N99", "N01"] {
        let mut operation = add("root");
        if let WriteOperation::NodeAdd { id: requested, .. } = &mut operation {
            *requested = Some(id.into());
        }
        node::plan(&mut candidate, &operation).unwrap();
    }
    node::plan(
        &mut candidate,
        &WriteOperation::NodeLinkSameAs {
            node: "N01".into(),
            same_as: "N99".into(),
        },
    )
    .unwrap();
    node::validate_references(&candidate).unwrap();
    let (_directory, mut candidate) =
        working("tree:\n  - id: N99\n    type: question\n  - id: N01\n    type: question\n");
    node::plan(
        &mut candidate,
        &WriteOperation::NodeLinkSameAs {
            node: "N01".into(),
            same_as: "N99".into(),
        },
    )
    .unwrap();
    assert!(node::validate_references(&candidate).is_err());
}

#[test]
fn new_nodes_require_source_kind_payloads_and_dead_ends_remain_leaves() {
    let payloads = [
        ("question", json!({"description":"Synthetic question"})),
        (
            "decision",
            json!({"choice":"Synthetic choice","alternatives":["Synthetic alternative"]}),
        ),
        ("experiment", json!({"result":"Synthetic result"})),
        (
            "dead_end",
            json!({"hypothesis":"Synthetic hypothesis","failure_mode":"Synthetic failure","lesson":"Synthetic lesson"}),
        ),
        (
            "pivot",
            json!({"from":"Synthetic prior direction","to":"Synthetic next direction","trigger":"Synthetic trigger"}),
        ),
    ];
    for (kind, payload) in payloads {
        for key in payload.as_object().unwrap().keys() {
            let (_directory, mut candidate) = working("tree: []\n");
            let mut fields: Fields = serde_json::from_value(payload.clone()).unwrap();
            fields.remove(key);
            assert!(
                node::plan(
                    &mut candidate,
                    &WriteOperation::NodeAdd {
                        id: None,
                        kind: kind.into(),
                        parent: "root".into(),
                        title: "Synthetic fixture".into(),
                        fields,
                        depends_on: vec![]
                    }
                )
                .is_err()
            );
            assert_eq!(candidate.text(TREE).unwrap(), "tree: []\n");
        }
        let (_directory, mut candidate) = working("tree: []\n");
        node::plan(
            &mut candidate,
            &WriteOperation::NodeAdd {
                id: None,
                kind: kind.into(),
                parent: "root".into(),
                title: "Synthetic fixture".into(),
                fields: serde_json::from_value(payload).unwrap(),
                depends_on: vec![],
            },
        )
        .unwrap();
        node::validate_references(&candidate).unwrap();
        if kind == "dead_end" {
            assert!(node::plan(&mut candidate, &add("N01")).is_err());
        }
    }
    let source = "tree:\n  - id: N01\n    type: insight\n  - id: N02\n    type: experiment\n";
    let (_directory, mut candidate) = working(source);
    node::plan(&mut candidate, &add("root")).unwrap();
    node::validate_references(&candidate).unwrap();
    assert!(candidate.text(TREE).unwrap().starts_with(source));
    let mut operation = add("root");
    if let WriteOperation::NodeAdd { kind, .. } = &mut operation {
        *kind = "insight".into();
    }
    assert!(node::plan(&mut candidate, &operation).is_err());
}

#[test]
fn native_status_and_scalar_or_list_source_refs_roundtrip_without_narrowing() {
    for status in [
        "completed",
        "failed",
        "pending",
        "in-progress",
        "resolved",
        "unresolved",
    ] {
        for sources in [
            json!("paper:section 2, exact source"),
            json!(["paper:section 2", "src/run.py"]),
        ] {
            let (_directory, mut candidate) = working("tree: []\n");
            let mut operation = add("root");
            if let WriteOperation::NodeAdd { fields, .. } = &mut operation {
                fields.insert("status".into(), json!(status));
                fields.insert("source_refs".into(), sources.clone());
            }
            node::plan(&mut candidate, &operation).unwrap();
            assert_eq!(value(&candidate, "N01", "status"), json!(status));
            assert_eq!(value(&candidate, "N01", "source_refs"), sources);
        }
    }
}

#[test]
fn new_concept_refs_resolve_final_exact_names_and_old_unresolved_refs_stay_readable() {
    let source = "tree:\n  - id: N01\n    type: question\n    concepts: [Historical missing]\n";
    let (_directory, mut candidate) = working(source);
    let mut operation = add("root");
    if let WriteOperation::NodeAdd { fields, .. } = &mut operation {
        fields.insert("concepts".into(), json!(["Exact concept"]));
    }
    node::plan(&mut candidate, &operation).unwrap();
    assert!(node::validate_references(&candidate).is_err());
    candidate
        .create(
            "logic/concepts.md",
            "# Concepts\n\n## Exact concept\n- **Definition**: Synthetic definition\n",
        )
        .unwrap();
    node::validate_references(&candidate).unwrap();
    candidate
        .replace_yaml_field(
            TREE,
            &[
                "tree".into(),
                ara_core::write::positions::PathPart::Index(1),
            ],
            "concepts",
            &json!(["exact concept"]),
        )
        .unwrap();
    assert!(node::validate_references(&candidate).is_err());
    assert!(
        candidate
            .text(TREE)
            .unwrap()
            .contains("concepts: [Historical missing]")
    );
}

#[test]
fn captured_import_history_keeps_unverifiable_links_readable_but_generic_edits_revoke_it() {
    let source = "tree:\n  - id: N01\n    type: question\n  - id: N02\n    type: question\n    same_as: [N01]\n";
    let (_directory, mut candidate) = working("tree: []\n");
    let files = std::collections::BTreeMap::from([(TREE, STANDARD.encode(source.as_bytes()))]);
    let ledger = json!({"format":"ara.merge-log/v1","records":[
        {"kind":"enrollment","source_key":"history-fixture","label":"historical","time":"2026-10-01T10:00"},
        {"kind":"revision","source_key":"history-fixture","fingerprint":"synthetic-captured-revision","base":"synthetic-base","predecessor":null,"time":"2026-10-01T10:00","git":null,"files":files,"mappings":[]}
    ]});
    candidate
        .create(
            "trace/merge_log.yaml",
            &ara_core::write::source::render_yaml(&ledger, 0, "\n"),
        )
        .unwrap();
    candidate
        .stage_replace(
            TREE,
            source.as_bytes(),
            "compose exact lossless YAML merge spans",
        )
        .unwrap();
    node::validate_references(&candidate).unwrap();
    // Import-only provenance cannot exempt an ordinary generic relation edit.
    candidate
        .replace_yaml_field(
            TREE,
            &[
                "tree".into(),
                ara_core::write::positions::PathPart::Index(0),
            ],
            "same_as",
            &json!(["N02"]),
        )
        .unwrap();
    assert!(node::validate_references(&candidate).is_err());
    assert!(ara_core::write::batch::parse_batch(br#"{"op":"node.link_same_as","node":"N02","same_as":"N01","reason":"compose exact lossless YAML merge spans"}"#).is_err());
}

#[test]
fn equal_timestamp_instants_need_real_creation_order_evidence() {
    let (_directory, mut candidate) = working("tree: []\n");
    for (id, timestamp) in [
        ("N99", "2026-10-01T10:00Z"),
        ("N01", "2026-10-01T12:00+02:00"),
    ] {
        let mut operation = add("root");
        if let WriteOperation::NodeAdd {
            id: requested,
            fields,
            ..
        } = &mut operation
        {
            *requested = Some(id.into());
            fields.insert("timestamp".into(), json!(timestamp));
        }
        node::plan(&mut candidate, &operation).unwrap();
    }
    node::plan(
        &mut candidate,
        &WriteOperation::NodeLinkSameAs {
            node: "N01".into(),
            same_as: "N99".into(),
        },
    )
    .unwrap();
    node::validate_references(&candidate).unwrap();
}

#[test]
fn add_to_empty_children_inside_flow_parent_keeps_complete_neighbor_source() {
    let root = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("trace")).unwrap();
    let original = r#"{"tree":[{"id":"N01","type":"question","description":"Parent","children":[],"opaque":{"keep":"雪"}}]}"#;
    std::fs::write(root.path().join("trace/exploration_tree.yaml"), original).unwrap();
    let operation: ara_core::write::WriteOperation = serde_json::from_value(serde_json::json!({"op":"node.add","type":"question","parent":"N01","title":"Child","fields":{"description":"Full caller text\n"}})).unwrap();
    ara_core::write::execute(
        root.path(),
        &[operation],
        ara_core::write::ApplyMode::Commit,
    )
    .unwrap();
    let source = std::fs::read_to_string(root.path().join("trace/exploration_tree.yaml")).unwrap();
    assert!(source.contains(r#""opaque":{"keep":"雪"}"#));
    let value = ara_core::write::positions::YamlDocument::parse(&source)
        .unwrap()
        .root
        .to_json()
        .unwrap();
    assert_eq!(
        value["tree"][0]["children"][0]["description"],
        "Full caller text\n"
    );
}

#[test]
fn node_lookup_cache_tracks_staged_tree_changes_and_explicit_invalidation() {
    let (_directory, mut candidate) = working("tree: [{id: N01, type: question}]\n");
    assert_eq!(
        node::node_kind(&candidate, "N01").unwrap().as_deref(),
        Some("question")
    );
    node::plan(&mut candidate, &add("N01")).unwrap();
    assert_eq!(
        node::node_kind(&candidate, "N02").unwrap().as_deref(),
        Some("experiment")
    );
    candidate
        .files
        .insert(TREE.into(), b"tree: [{id: N03, type: decision}]\n".to_vec());
    candidate.invalidate_path(TREE);
    assert_eq!(node::node_kind(&candidate, "N01").unwrap(), None);
    assert_eq!(
        node::node_kind(&candidate, "N03").unwrap().as_deref(),
        Some("decision")
    );
}

#[test]
fn node_lookup_rechecks_direct_candidate_and_preimage_mutations_without_digest_updates() {
    let (_directory, mut candidate) = working("tree: [{id: N01, type: question}]\n");
    assert_eq!(
        node::node_kind(&candidate, "N01").unwrap().as_deref(),
        Some("question")
    );
    candidate.base.files.get_mut(TREE).unwrap().bytes =
        b"tree: [{id: N02, type: decision}]\n".to_vec();
    assert_eq!(node::node_kind(&candidate, "N01").unwrap(), None);
    assert_eq!(
        node::node_kind(&candidate, "N02").unwrap().as_deref(),
        Some("decision")
    );
    candidate.files.insert(
        TREE.into(),
        b"tree: [{id: N03, type: experiment}]\n".to_vec(),
    );
    assert_eq!(node::node_kind(&candidate, "N02").unwrap(), None);
    assert_eq!(
        node::node_kind(&candidate, "N03").unwrap().as_deref(),
        Some("experiment")
    );
    *candidate.files.get_mut(TREE).unwrap() =
        b"tree: [{id: N03, type: experiment}, {id: N03, type: decision}]\n".to_vec();
    assert!(
        node::node_kind(&candidate, "N03")
            .unwrap_err()
            .message
            .contains("duplicate source node ID")
    );
    candidate.deleted_paths.insert(TREE.into());
    assert_eq!(node::node_kind(&candidate, "N03").unwrap(), None);
}

#[test]
fn cached_reference_validation_rechecks_candidate_and_preimage_bytes() {
    let original =
        "tree: [{id: N01, type: question}, {id: N02, type: decision, also_depends_on: [N99]}]\n";
    let (_directory, mut candidate) = working(original);
    candidate
        .stage_replace(
            TREE,
            format!("# keep history\n{original}").as_bytes(),
            "replace document",
        )
        .unwrap();
    node::validate_references(&candidate).unwrap();
    // N99 was historical only in the old preimage. An unchanged stored digest
    // cannot authorize that exemption after the public preimage bytes change.
    candidate.base.files.get_mut(TREE).unwrap().bytes =
        original.replace("[N99]", "[N98]").into_bytes();
    let error = node::validate_references(&candidate).unwrap_err();
    assert_eq!(error.code, "write.node");
    assert!(error.message.contains("N99"));
    // Restoring the preimage restores only the historical exemption, not
    // permission for a different dangling reference in directly edited bytes.
    candidate.base.files.get_mut(TREE).unwrap().bytes = original.as_bytes().to_vec();
    node::validate_references(&candidate).unwrap();
    candidate.files.insert(
        TREE.into(),
        format!("# keep history\n{}", original.replace("[N99]", "[N97]")).into_bytes(),
    );
    let error = node::validate_references(&candidate).unwrap_err();
    assert_eq!(error.code, "write.node");
    assert!(error.message.contains("N97"));
}
