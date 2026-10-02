//! Generated real nesting, not parent-pointer flattening. Compact flow notation
//! avoids hundreds of MiB of indentation in the permanent 10,000-depth test.
use ara_core::{LinkKind, RuleCode, parse_sources};
use std::fmt::Write;

fn nested(depth: usize, leaf: &str) -> String {
    let mut source = String::from("tree: [\n");
    for id in (1..=depth).rev() {
        writeln!(source, "{{id: N{id}, type: question, children: [").unwrap();
    }
    source.push_str(leaf);
    source.push_str(&"]}\n".repeat(depth));
    source.push_str("]\n");
    source
}

#[test]
fn ten_thousand_nested_nodes_preserve_preorder_and_parent_links_on_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let source = nested(10_000, "");
            let (manifest, report) = parse_sources(&source, None).expect("true nested tree parses");
            assert!(report.warnings().is_empty(), "{report}");
            assert_eq!(manifest.nodes.len(), 10_000);
            assert_eq!(manifest.links.len(), 9_999);
            for (index, node) in manifest.nodes.iter().enumerate() {
                assert_eq!(node.id.as_str(), format!("N{}", 10_000 - index));
            }
            for (index, link) in manifest.links.iter().enumerate() {
                assert_eq!(link.kind, LinkKind::Child);
                assert_eq!(link.from.as_str(), format!("N{}", 10_000 - index));
                assert_eq!(link.to.as_str(), format!("N{}", 9_999 - index));
            }
            let refused = parse_sources(&nested(10_001, ""), None).unwrap_err();
            assert_eq!(
                refused
                    .errors()
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>(),
                [RuleCode::MalformedTree]
            );
            let fields = ara_core::source_node_fields(&source, &["N10000", "N1"]).unwrap();
            assert_eq!(
                fields["N10000"]["id"],
                ara_core::SourceValue::String("N10000".into())
            );
            assert_eq!(
                fields["N1"]["id"],
                ara_core::SourceValue::String("N1".into())
            );
            assert!(ara_core::source_node_fields(&nested(10_001, ""), &["N1"]).is_err());
            // Both normalization and destruction execute on the deliberately small stack.
            drop(manifest);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn invalid_deepest_child_reports_original_node_location_without_stack_unwind_recursion() {
    let source = nested(9_999, "{id: N0, type: question, children: invalid}\n");
    let report = parse_sources(&source, None).unwrap_err();
    assert!(
        report.errors().iter().any(
            |d| d.code == RuleCode::MalformedTree && d.message.contains("line 10001, column 1")
        ),
        "{report}"
    );
}

#[test]
fn deepest_unknown_field_remains_a_node_specific_warning() {
    let source = nested(9_999, "{id: N0, type: question, unrecognized: value}\n");
    let (_, report) = parse_sources(&source, None).unwrap();
    let warning = report
        .warnings()
        .iter()
        .find(|d| d.code == RuleCode::UnknownNodeField)
        .unwrap();
    assert_eq!(warning.path, "nodes[N0]");
    assert!(warning.message.contains("unrecognized"));
}

#[test]
fn generic_metadata_nesting_is_not_unbounded_by_the_tree_budget() {
    let metadata = format!("{}x{}", "[".repeat(65), "]".repeat(65));
    let source = format!("tree: [{{id: N1, type: question, concepts: {metadata}}}]");
    let report = parse_sources(&source, None).unwrap_err();
    assert_eq!(
        report
            .errors()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [RuleCode::MalformedTree]
    );
}

#[test]
fn block_tree_deeper_than_the_old_budget_preserves_decoded_text() {
    let mut source = String::from("tree:\n");
    for id in 1..=100 {
        let indent = " ".repeat(id * 4 - 2);
        writeln!(source, "{indent}- id: N{id}\n{indent}  type: question\n{indent}  description: |2-\n{indent}    café 你好\n{indent}  children:").unwrap();
    }
    let (manifest, _) = parse_sources(&source, None).unwrap();
    assert_eq!(manifest.nodes.len(), 100);
    assert!(
        manifest
            .nodes
            .iter()
            .all(|node| node.description.as_deref() == Some("café 你好"))
    );
}

#[cfg(feature = "native")]
#[test]
fn native_load_keeps_exact_deep_source_bytes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("trace")).unwrap();
    let source = nested(10_000, "").replace("\n", "\r\n");
    std::fs::write(dir.path().join("trace/exploration_tree.yaml"), &source).unwrap();
    let loaded = ara_core::parse_dir_detailed(dir.path());
    assert!(loaded.report.is_ok(), "{}", loaded.report);
    assert_eq!(loaded.manifest.unwrap().nodes.len(), 10_000);
    assert_eq!(loaded.sources["trace/exploration_tree.yaml"], source);
}

#[test]
fn selected_root_and_deepest_node_fields_omit_real_children_without_losing_unknowns() {
    let source = nested(10_000, "").replacen("tree: [\n", "root: ", 1);
    let source = source.strip_suffix("]\n").unwrap().replace(
        "id: N1, type: question, children:",
        "id: N1, type: question, opaque: \"café 你好\", children:",
    );
    let fields = ara_core::source_node_fields(&source, &["N10000", " N1 "]).unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(
        fields["N1"]["opaque"],
        ara_core::SourceValue::String("café 你好".into())
    );
    for (identity, node) in fields {
        assert_eq!(node["id"], ara_core::SourceValue::String(identity));
        assert_eq!(
            node["type"],
            ara_core::SourceValue::String("question".into())
        );
        assert!(!node.contains_key("children"));
    }
}

#[test]
fn source_projection_preserves_opaque_tags_aliases_merges_and_utf8_crlf_values() {
    use ara_core::SourceValue;
    let source = "root:\r\n  id: N01\r\n  type: question\r\n  opaque: &meta !opaque {flag: true, count: 7, label: \"café\"}\r\n  alias: *meta\r\n  merged: {<<: *meta, count: 9}\r\n  text: !opaque \"你好\"\r\n  description: |2-\r\n    café 你好\r\n  children: []\r\n";
    let projected = ara_core::source_node_fields(source, &["N01"]).unwrap();
    let fields = &projected["N01"];
    let SourceValue::Mapping(opaque) = &fields["opaque"] else {
        panic!("opaque mapping lost")
    };
    assert_eq!(opaque["flag"], SourceValue::Bool(true));
    assert_eq!(opaque["count"], SourceValue::Integer(7));
    assert_eq!(opaque["label"], SourceValue::String("café".into()));
    assert_eq!(fields["alias"], fields["opaque"]);
    let SourceValue::Mapping(merged) = &fields["merged"] else {
        panic!("merged mapping lost")
    };
    assert_eq!(merged["count"], SourceValue::Integer(9));
    assert_eq!(merged["label"], SourceValue::String("café".into()));
    assert_eq!(fields["text"], SourceValue::String("你好".into()));
    assert_eq!(
        fields["description"],
        SourceValue::String("café 你好".into())
    );
    assert!(!fields.contains_key("children"));
    #[cfg(feature = "native")]
    {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("trace")).unwrap();
        std::fs::write(dir.path().join("trace/exploration_tree.yaml"), source).unwrap();
        let loaded = ara_core::parse_dir_detailed(dir.path());
        assert!(loaded.manifest.is_some(), "{}", loaded.report);
        let from_loaded =
            ara_core::source_node_fields(&loaded.sources["trace/exploration_tree.yaml"], &["N01"])
                .unwrap();
        assert_eq!(from_loaded, projected);
    }
}

#[test]
fn source_projection_preserves_unsigned_and_signed_integer_boundaries() {
    use ara_core::SourceValue;
    let source = format!(
        "root: {{id: N1, type: question, positive: {}, negative: {}}}",
        u64::MAX,
        i64::MIN
    );
    let fields = ara_core::source_node_fields(&source, &["N1"]).unwrap();
    assert_eq!(fields["N1"]["positive"], SourceValue::Unsigned(u64::MAX));
    assert_eq!(fields["N1"]["negative"], SourceValue::Integer(i64::MIN));
    assert_eq!(
        serde_json::to_value(&fields["N1"]).unwrap()["positive"].as_u64(),
        Some(u64::MAX)
    );
}

#[test]
fn source_projection_rejects_ambiguous_missing_and_overdeep_metadata_without_partial_fields() {
    let metadata = format!("{}x{}", "[".repeat(65), "]".repeat(65));
    let overdeep = format!("root: {{id: N1, opaque: {metadata}}}");
    for (source, requested) in [
        ("tree: [{id: N1}, {id: N1}]", "N1"),
        ("tree: [{id: N1}]", "N2"),
        (overdeep.as_str(), "N1"),
    ] {
        assert!(ara_core::source_node_fields(source, &[requested]).is_err());
    }
}
