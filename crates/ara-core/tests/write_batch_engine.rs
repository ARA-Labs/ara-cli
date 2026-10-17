#![cfg(feature = "native")]
use ara_core::write::{self, ApplyMode};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "ara-writer-batch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("trace")).unwrap();
        fs::write(root.join("trace/exploration_tree.yaml"),"# user's tree\nmeta: {unknown: [one, two]}\ntree:\n  - id: N01\n    type: question\n    title: Existing\n    unknown: {nested: retained}\n").unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn dryrun_predicts_complete_commit_without_creating_operational_paths() {
    let fixture = Fixture::new();
    let before = fs::read(fixture.0.join("trace/exploration_tree.yaml")).unwrap();
    let operations=write::batch::parse_batch(br#"{"op":"node.add","id":"$parent","type":"question","parent":"root","title":"Boundary","fields":{"description":"Literal @input $missing\n\nid: N999\n"}}
{"op":"node.add","id":"$child","type":"experiment","parent":"$parent","title":"Check","fields":{"result":"observed\n\ncomplete\n","provenance":"ai-executed"}}
{"op":"edge.add","node":"$child","depends_on":"N01"}
"#).unwrap();
    let dry = write::execute(&fixture.0, &operations, ApplyMode::DryRun).unwrap();
    assert!(!dry.committed);
    assert!(dry.dry_run);
    assert_eq!(dry.bindings["$parent"], "N02");
    assert_eq!(dry.bindings["$child"], "N03");
    assert_eq!(
        fs::read(fixture.0.join("trace/exploration_tree.yaml")).unwrap(),
        before
    );
    assert!(!fixture.0.join(".ara").exists());
    assert!(!fixture.0.join(".gitignore").exists());
    let committed = write::execute(&fixture.0, &operations, ApplyMode::Commit).unwrap();
    assert!(committed.committed);
    assert_eq!(committed.bindings, dry.bindings);
    assert_eq!(committed.changed_paths, dry.changed_paths);
    let source = fs::read_to_string(fixture.0.join("trace/exploration_tree.yaml")).unwrap();
    assert!(source.contains("# user's tree\nmeta: {unknown: [one, two]}\n"));
    assert!(source.contains("unknown: {nested: retained}"));
    let (manifest, report) = ara_core::parse_dir(&fixture.0).unwrap();
    assert!(report.is_ok());
    assert_eq!(manifest.nodes.len(), 3);
    let parent = manifest
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "N02")
        .unwrap();
    assert_eq!(
        parent.description.as_deref(),
        Some("Literal @input $missing\n\nid: N999\n")
    );
    assert!(fixture.0.join(".ara/lock").is_file());
}
#[test]
fn late_semantic_failure_does_not_publish_any_candidate_or_binding() {
    let fixture = Fixture::new();
    let before = fs::read(fixture.0.join("trace/exploration_tree.yaml")).unwrap();
    let operations=write::batch::parse_batch(br#"{"op":"node.add","id":"$first","type":"question","parent":"root","title":"Tentative","fields":{"description":"Synthetic question for tentative"}}
{"op":"node.add","type":"question","parent":"N999","title":"Rejected","fields":{"description":"Synthetic question for rejected"}}
"#).unwrap();
    let error = write::execute(&fixture.0, &operations, ApplyMode::Commit).unwrap_err();
    assert_eq!(error.exit_code(), 1);
    assert_eq!(error.line, Some(2));
    assert_eq!(
        fs::read(fixture.0.join("trace/exploration_tree.yaml")).unwrap(),
        before
    );
    assert!(!fixture.0.join(".gitignore").exists());
}
#[test]
fn bindings_enforce_kind_and_never_forward_resolve() {
    for payload in [
        r#"{"op":"observation.stage","id":"$observation","content":"Raw","potential_type":"unknown","provenance":"user","timestamp":"2026-10-01T10:00","bound_to":["N01"]}
{"op":"node.add","type":"question","parent":"$observation","title":"Wrong namespace","fields":{"description":"Synthetic question for wrong namespace"}}"#,
        r#"{"op":"node.add","type":"question","parent":"$future","title":"Forward","fields":{"description":"Synthetic question for forward"}}
{"op":"node.add","id":"$future","type":"question","parent":"root","title":"Too late","fields":{"description":"Synthetic question for too late"}}"#,
    ] {
        let fixture = Fixture::new();
        let operations = write::batch::parse_batch(payload.as_bytes()).unwrap();
        let before = fs::read(fixture.0.join("trace/exploration_tree.yaml")).unwrap();
        let error = write::execute(&fixture.0, &operations, ApplyMode::Commit).unwrap_err();
        assert!(matches!(
            error.code.as_str(),
            "write.binding_kind" | "write.binding_unknown"
        ));
        assert_eq!(
            fs::read(fixture.0.join("trace/exploration_tree.yaml")).unwrap(),
            before
        );
        assert!(!fixture.0.join("staging/observations.yaml").exists());
    }
}
#[test]
fn concrete_forward_references_resolve_at_final_candidate() {
    let fixture = Fixture::new();
    let operations=write::batch::parse_batch(br#"{"op":"node.add","id":"N02","type":"experiment","parent":"root","title":"Depends on later record","fields":{"result":"Observed"},"depends_on":["N03"]}
{"op":"node.add","id":"N03","type":"question","parent":"root","title":"Later source","fields":{"description":"Synthetic question for later source"}}
"#).unwrap();
    write::execute(&fixture.0, &operations, ApplyMode::Commit).unwrap();
    let (manifest, report) = ara_core::parse_dir(&fixture.0).unwrap();
    assert!(report.is_ok());
    assert!(
        manifest
            .links
            .iter()
            .any(|link| link.from.as_str() == "N02" && link.to.as_str() == "N03")
    );
}
#[test]
fn recursive_duplicate_key_error_retains_physical_line_and_field() {
    let error=write::batch::parse_batch(b"\n\n{\"op\":\"node.add\",\"type\":\"question\",\"parent\":\"root\",\"title\":\"Q\",\"fields\":{\"description\":\"a\",\"description\":\"b\"}}\n").unwrap_err();
    assert_eq!(error.line, Some(3));
    assert_eq!(error.field.as_deref(), Some("fields.description"));
}
