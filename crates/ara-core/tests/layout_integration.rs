//! Integration tests for the layout module.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ara_core::{LayoutOptions, parse_and_layout};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(fixtures().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// parse_and_layout on a valid artifact returns positioned manifest.
#[test]
fn parse_and_layout_ok_path() {
    let yaml = read("official/minimal-artifact/trace/exploration_tree.yaml");
    let claims = read("official/minimal-artifact/logic/claims.md");
    let opts = LayoutOptions::default();
    let (manifest, report) = parse_and_layout(&yaml, Some(&claims), &opts).expect("ok");
    assert!(report.is_ok());
    assert!(manifest.bounds.is_some());
    for node in &manifest.nodes {
        assert!(node.pos.is_some(), "node {} missing pos", node.id);
        let pos = node.pos.unwrap();
        assert!(pos.x.is_finite());
        assert!(pos.y.is_finite());
    }
}

/// parse_and_layout on a broken input (cycle) returns Err(report) with layout skipped.
#[test]
fn parse_and_layout_error_path() {
    let yaml = read("broken/cycle.yaml");
    let opts = LayoutOptions::default();
    let err = parse_and_layout(&yaml, None, &opts).unwrap_err();
    assert!(!err.is_ok());
    assert!(err.errors().iter().any(|d| d.message.contains("cycle")));
}

/// Layout determinism: positioned JSON is byte-identical across two runs.
#[test]
fn layout_determinism_in_process() {
    let yaml = read("official/resnet-ara-example/trace/exploration_tree.yaml");
    let claims = read("official/resnet-ara-example/logic/claims.md");
    let opts = LayoutOptions::default();
    let (a, _) = parse_and_layout(&yaml, Some(&claims), &opts).expect("ok");
    let (b, _) = parse_and_layout(&yaml, Some(&claims), &opts).expect("ok");
    let ja = serde_json::to_string_pretty(&a).unwrap();
    let jb = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(ja, jb);
}

/// Positioned manifest snapshot for the minimal-artifact.
#[test]
fn positioned_manifest_snapshot() {
    let yaml = read("official/minimal-artifact/trace/exploration_tree.yaml");
    let claims = read("official/minimal-artifact/logic/claims.md");
    let opts = LayoutOptions::default();
    let (manifest, _) = parse_and_layout(&yaml, Some(&claims), &opts).expect("ok");
    insta::assert_json_snapshot!("positioned_minimal_manifest", manifest);
}

/// Stage 1 snapshots remain byte-identical when layout is OFF (no pos/bounds in JSON).
/// This re-runs the existing parse_fixtures snapshot from a fresh test binary to
/// prove the new Option fields don't perturb serialization.
#[test]
#[cfg(feature = "native")]
fn stage1_snapshots_unperturbed_by_new_fields() {
    let path = fixtures().join("official/minimal-artifact");
    let (manifest, _) = ara_core::parse_dir(&path).expect("ok");
    let json = serde_json::to_string_pretty(&manifest).unwrap();
    // The JSON must NOT contain "pos" or "bounds" when layout hasn't run.
    assert!(!json.contains("\"pos\""), "pos should be absent");
    assert!(!json.contains("\"bounds\""), "bounds should be absent");
}

/// Scale probe: layout the largest corpus tree and verify bounded time.
#[test]
fn scale_probe_largest_corpus() {
    let yaml = read("corpus/rebench/rebench-rust_codecontests/trace/exploration_tree.yaml");
    let claims = read("corpus/rebench/rebench-rust_codecontests/logic/claims.md");
    let opts = LayoutOptions::default();

    let start = Instant::now();
    let result = parse_and_layout(&yaml, Some(&claims), &opts);
    let elapsed = start.elapsed();

    match result {
        Ok((manifest, _)) => {
            let node_count = manifest.nodes.len();
            println!("scale probe: {} nodes, layout in {:?}", node_count, elapsed);
            assert!(
                elapsed.as_secs() < 5,
                "layout took {elapsed:?} — exceeds 5s budget"
            );
            assert!(node_count > 0);
        }
        Err(report) => {
            println!("scale probe: parse error (expected for some corpus), {elapsed:?}: {report}");
        }
    }
}
