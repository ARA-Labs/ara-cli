//! Plan 19 C1: the read side (`refs`, `show`) and the writer's citation
//! inventory agree, history stays resolvable after restructures, and the
//! remaining refusals are reached through the real binary.
#[path = "support/citations.rs"]
mod support;
use support::*;

const ALIASES: &str = "format: ara.aliases/v1\naliases:\n  - {\"source_key\":\"s1\",\"label\":\"s1\",\"original\":\"C09\",\"target\":\"C02\",\"revision\":\"r1\"}\n";
const BOUND: &str = "observations:\n  - id: O01\n    timestamp: \"2026-10-01T10:00:00Z\"\n    provenance: user\n    content: \"C02 looks fragile\"\n    potential_type: claim\n    promoted: false\n    bound_to: [N01, C02]\n";
const MIXED: &str = "\n## C06: Mixed citer\n- **Statement**: Mixed.\n- **Status**: hypothesis\n- **Depends on**: C02\n- **Proof**: \"C02\" quoted, <!-- C02 --> commented, and C02 cited\n- **Sources**: logic/claims.md#C02: Source claim\n";

/// History that cites C02: tree evidence, observation `bound_to`, a portable
/// alias target and an earlier session turn.
fn history_fixture() -> TempDir {
    let dir = fixture(false);
    let root = dir.path();
    write_file(root, "staging/observations.yaml", BOUND);
    write_file(root, "trace/aliases.yaml", ALIASES);
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Earlier turn","timestamp":"2026-10-01T09:00:00Z"}),
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Statement":"Source, restated."},"signal":"user-directive","provenance":"user"}),
        ],
        false,
    );
    dir
}

fn structured(root: &Path, id: &str) -> Vec<Value> {
    read(root, &["show", id, "--with", "refs"])["entries"][0]["relations"]["refs"]["structured"]
        .as_array()
        .unwrap()
        .clone()
}

fn owner_of(row: &Value) -> String {
    match &row["entry"] {
        Value::Object(selector) if selector.contains_key("id") => {
            selector["id"].as_str().unwrap().to_owned()
        }
        selector => {
            let heading = selector["heading"].as_array().unwrap();
            let last = heading.last().unwrap().as_str().unwrap();
            last.split([':', ' ']).next().unwrap().to_owned()
        }
    }
}

#[test]
fn refs_and_the_rewrite_inventory_agree_on_every_markdown_citation_kind() {
    let dir = fixture(false);
    let root = dir.path();
    let claims = fs::read_to_string(root.join("logic/claims.md")).unwrap();
    write_file(root, "logic/claims.md", format!("{claims}{MIXED}"));
    let rows = structured(root, "C02");
    let listed: BTreeSet<(String, String, String)> = rows
        .iter()
        .filter(|row| row["source"].as_str().unwrap().ends_with(".md"))
        .map(|row| {
            (
                row["source"].as_str().unwrap().to_owned(),
                row["id"].as_str().unwrap().to_owned(),
                row["field"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Merge"}),
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ],
        true,
    );
    let operation = &report["operations"][1];
    let mut inventory: BTreeSet<(String, String, String)> = operation["rewritten_references"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["document"].as_str().unwrap().to_owned(),
                owner_of(row),
                row["field"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let skipped = operation["skipped_references"].as_array().unwrap();
    for row in skipped {
        if row["reason"] == "read_only_field" {
            let heading = row["heading"].as_array().unwrap();
            let last = heading.last().unwrap().as_str().unwrap();
            inventory.insert((
                row["document"].as_str().unwrap().to_owned(),
                last.split(':').next().unwrap().to_owned(),
                row["field"].as_str().unwrap().to_owned(),
            ));
        }
    }
    assert_eq!(listed, inventory);
    for expected in [
        ("logic/claims.md", "C04", "Proof"),
        ("logic/claims.md", "C04", "Sources"),
        ("logic/claims.md", "C05", "Merged into"),
        ("logic/claims.md", "C06", "Depends on"),
        ("logic/claims.md", "C06", "Sources"),
        ("logic/experiments.md", "E01", "Sources"),
    ] {
        let key = (expected.0.into(), expected.1.into(), expected.2.into());
        assert!(listed.contains(&key), "{expected:?} in {listed:?}");
    }
    // Quoted and commented tokens are neither listed nor rewritten.
    let c06_proof: Vec<&Value> = rows
        .iter()
        .filter(|row| row["id"] == "C06" && row["field"] == "Proof")
        .collect();
    assert_eq!(c06_proof.len(), 1);
    let protected = skipped
        .iter()
        .filter(|row| row["reason"] == "protected" && row["field"] == "Proof")
        .count();
    assert_eq!(protected, 2, "{skipped:?}");
    let proof = operation["rewritten_references"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| owner_of(row) == "C06" && row["field"] == "Proof")
        .unwrap();
    assert_eq!(
        proof["after"],
        "\"C02\" quoted, <!-- C02 --> commented, and C01 cited"
    );
}

#[test]
fn history_citations_agree_with_refs_and_stay_resolvable_after_rename() {
    let dir = history_fixture();
    let root = dir.path();
    let history: BTreeSet<(String, String)> = structured(root, "C02")
        .iter()
        .filter(|row| !row["source"].as_str().unwrap().ends_with(".md"))
        .map(|row| {
            (
                row["source"].as_str().unwrap().to_owned(),
                row["field"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let expected = entry_digest(root, json!({"id":"C02"}));
    let rename = [
        json!({"op":"session.log","summary":"Rename C02","timestamp":"2026-10-02T09:00:00Z"}),
        json!({"op":"entry.rename","target":{"id":"C02"},"name":"C07","expected":expected,"signal":"user-directive","provenance":"user","rewrite_references":true}),
    ];
    let preview = apply(root, &rename, true);
    let checked: BTreeSet<(String, String)> = preview["operations"][1]["historical_citations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["source"].as_str().unwrap().to_owned(),
                row["field"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(history, checked);
    for source in [
        "trace/exploration_tree.yaml",
        "staging/observations.yaml",
        "trace/aliases.yaml",
    ] {
        assert!(
            checked.iter().any(|(path, _)| path == source),
            "{checked:?}"
        );
    }
    let immutable: Vec<(&str, Vec<u8>)> = [
        "trace/exploration_tree.yaml",
        "staging/observations.yaml",
        "trace/aliases.yaml",
    ]
    .into_iter()
    .map(|path| (path, fs::read(root.join(path)).unwrap()))
    .collect();
    let earlier = yaml(root, "trace/sessions/2026-10-01_001.yaml");
    apply(root, &rename, false);
    for (path, bytes) in immutable {
        assert_eq!(fs::read(root.join(path)).unwrap(), bytes, "{path}");
    }
    assert_eq!(
        yaml(root, "trace/sessions/2026-10-01_001.yaml"),
        earlier,
        "earlier session rows are exact"
    );
    // The read side resolves every historical citation to the renamed claim.
    let after: BTreeSet<(String, String)> = structured(root, "C07")
        .iter()
        .filter(|row| !row["source"].as_str().unwrap().ends_with(".md"))
        .map(|row| {
            (
                row["source"].as_str().unwrap().to_owned(),
                row["field"].as_str().unwrap().to_owned(),
            )
        })
        .filter(|(source, _)| source != "trace/logic_mutations.yaml")
        .filter(|(source, _)| !source.starts_with("trace/sessions/2026-10-02"))
        .collect();
    assert_eq!(after, history);
    assert_eq!(
        read(root, &["show", "O01"])["entries"][0]["bound_to"],
        json!(["N01", "C02"])
    );
    assert_eq!(read(root, &["show", "C02"])["entries"][0]["id"], "C07");
    read(root, &["show", "N01"]);
    ara(root).arg("check").arg(root).assert().success();
}

fn duplicate_leaf_fixture() -> TempDir {
    let dir = fixture(false);
    write_file(
        dir.path(),
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Boundary mechanism\n    provenance: user\n    description: Root question\n    concepts: [\"Group A/Term\"]\n    source_refs: [\"logic/concepts.md#Group A/Term\", \"logic/concepts.md#Group B/Term\"]\n",
    );
    dir
}

/// Every historical spelling of a retired heading, including the mapping's
/// own `from`, reads the renamed section.
fn assert_retired_spellings(root: &Path, spellings: &[&str], address: &str) {
    for spelling in spellings {
        let shown = read(root, &["show", spelling]);
        assert_eq!(
            shown["entries"][0]["address"], address,
            "{spelling}: {shown}"
        );
    }
}

fn assert_duplicate_leaf_reads(root: &Path) {
    assert_retired_spellings(
        root,
        &[
            "logic/concepts.md#Group A/Term",
            "logic/concepts.md:Group A/Term",
            "logic/concepts.md#h/Group%20A/Term",
        ],
        "logic/concepts.md#h/Group%20A/Word",
    );
    let node = read(root, &["show", "N01"]);
    assert_eq!(node["entries"][0]["id"], "N01");
    let refs =
        read(root, &["show", "N01", "--with", "refs"])["entries"][0]["relations"]["refs"].clone();
    assert_eq!(refs["target"], "N01");
    let other = read(root, &["show", "logic/concepts.md#Group B/Term"]);
    let content = other["entries"][0]["content"].as_str().unwrap();
    assert!(
        content.contains("B term.") && content.contains("Group A/Word"),
        "{other}"
    );
    let moved = read(
        root,
        &[
            "show",
            "--document",
            "logic/concepts.md",
            "--heading",
            "Group A",
            "--heading",
            "Word",
        ],
    );
    assert!(moved["entries"][0]["digest"].is_string(), "{moved}");
    ara(root).arg("check").arg(root).assert().success();
}

#[test]
fn duplicate_leaf_rename_keeps_reads_working_on_both_paths() {
    let target = json!({"document":"logic/concepts.md","heading":["Group A","Term"]});
    // Generated repairs.
    let dir = duplicate_leaf_fixture();
    let root = dir.path();
    let expected = entry_digest(root, target.clone());
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename"}),
            json!({"op":"entry.rename","target":target,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    let checked = report["operations"][1]["historical_citations"]
        .as_array()
        .unwrap();
    assert_eq!(checked.len(), 2, "{checked:?}");
    assert_duplicate_leaf_reads(root);

    // Hand-written rows take the same read path. That path keeps the
    // textual guard, which still counts `Term` inside the Group B locator.
    let dir = duplicate_leaf_fixture();
    let root = dir.path();
    let concepts = fs::read_to_string(root.join("logic/concepts.md")).unwrap();
    write_file(
        root,
        "logic/concepts.md",
        concepts.replace("- **Sources**: logic/concepts.md#Group B/Term\n", ""),
    );
    let expected = entry_digest(root, target.clone());
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename"}),
            json!({"op":"entry.rename","target":target,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","references":[
                {"target":{"document":"logic/concepts.md","heading":["Group B","Term"]},"field":"Related","before":"Group A/Term, A/B #1","after":"Group A/Word, A/B #1"},
                {"target":{"document":"logic/concepts.md","heading":["A/B #1"]},"field":"Related","before":"logic/concepts.md#Group A/Term","after":"logic/concepts.md#Group A/Word"}
            ]}),
        ],
        false,
    );
    assert_duplicate_leaf_reads(root);
}

#[test]
fn ambiguous_field_mentions_block_a_rename_with_their_location() {
    let dir = fixture(false);
    let root = dir.path();
    let concepts = fs::read_to_string(root.join("logic/concepts.md")).unwrap();
    write_file(
        root,
        "logic/concepts.md",
        concepts.replace(
            "- **Definition**: Lone concept.\n",
            "- **Definition**: Lone concept.\n- **Claims affected**: logic/concepts.md#Term\n",
        ),
    );
    let before = artifact_bytes(root);
    let target = json!({"document":"logic/concepts.md","heading":["Group A","Term"]});
    let expected = entry_digest(root, target.clone());
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Rename"}),
            json!({"op":"entry.rename","target":target,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ]),
    );
    assert_eq!(error["code"], "write.dangling_reference");
    let locations = error["details"]["locations"].as_array().unwrap();
    assert!(
        locations.iter().any(|row| row["reason"] == "ambiguous"
            && row["heading"] == json!(["Lone"])
            && row["field"] == "Claims affected"),
        "{locations:?}"
    );
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn final_validation_refuses_history_broken_later_in_the_batch() {
    let dir = fixture(false);
    let root = dir.path();
    write_file(
        root,
        "trace/exploration_tree.yaml",
        tree(", \"logic/concepts.md#Lone\""),
    );
    let concepts = fs::read_to_string(root.join("logic/concepts.md")).unwrap();
    write_file(
        root,
        "logic/concepts.md",
        concepts.replace("Lone concept.", "Single concept."),
    );
    let before = artifact_bytes(root);
    let lone = json!({"document":"logic/concepts.md","heading":["Lone"]});
    let expected = entry_digest(root, lone.clone());
    // The concepts document exactly as the rename leaves it; a later
    // whole-document replacement then drops the renamed section, which no
    // plan-time guard sees but the recorded history check does.
    let text = fs::read_to_string(root.join("logic/concepts.md")).unwrap();
    let kept = &text[..text.find("## Lone").unwrap()];
    let renamed = format!(
        "{kept}## Solo\n- **Definition**: Single concept.\n- **Related**: A/B #1\n- **Sources**: logic/concepts.md#Group B/Term\n- **Last revised**:\n  2026-10-01 (2026-10-01_001#1)\n"
    );
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Rename then drop","timestamp":"2026-10-01T09:00:00Z"}),
            json!({"op":"entry.rename","target":lone,"name":"Solo","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
            json!({"op":"document.replace","document":"logic/concepts.md","expected":source::digest(renamed.as_bytes()),"content":kept}),
        ]),
    );
    assert_eq!(error["code"], "write.history_unresolved", "{error}");
    assert_eq!(
        error["line"], 2,
        "the restructure that recorded the citation"
    );
    assert_eq!(
        error["details"]["locations"][0]["literal"],
        "logic/concepts.md#Lone"
    );
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn a_rewrite_that_closes_a_dependency_cycle_is_refused() {
    let dir = fixture(false);
    let root = dir.path();
    let before = artifact_bytes(root);
    // C01 depends on C04, which depends on the source C02; repairing C04 to
    // the survivor C01 would make C01 and C04 depend on each other.
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Cycle"}),
            json!({"op":"logic.revise","target":{"id":"C01"},"set":{"Dependencies":["C04"]},"signal":"user-directive","provenance":"user"}),
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ]),
    );
    assert_eq!(error["code"], "write.reference");
    assert!(
        error["message"].as_str().unwrap().contains("cycle"),
        "{error}"
    );
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn split_takes_the_shown_body_digest_and_list_fan_out() {
    let dir = fixture(false);
    let root = dir.path();
    let rows = |sources: &str| {
        vec![
            json!({"target":{"id":"C04"},"field":"Dependencies","before":"[C02, C03]","after":"[C02, C03]"}),
            json!({"target":{"id":"C04"},"field":"Proof","before":"\"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C02.","after":"\"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C08."}),
            json!({"target":{"id":"C04"},"field":"Sources","before":"[\"paper §2\", \"C02\"]","after":sources}),
            json!({"target":{"id":"C05"},"field":"Merged into","before":"C02","after":"C02"}),
            json!({"target":{"document":"logic/experiments.md","heading":["Experiments","E01: Ablation"]},"field":"Sources","before":"C02","after":"C02"}),
        ]
    };
    let batch = |expected: &str, sources: &str| {
        vec![
            json!({"op":"session.log","summary":"Split C02"}),
            json!({"op":"claim.add","id":"C08","title":"Spin-off","fields":{"Statement":"Spun off.","Conditions":"C","Status":"hypothesis","Provenance":"user","Falsification":"F"}}),
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Statement":"Narrowed."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C08"}],"references":rows(sources),"expected":expected}),
        ]
    };
    let before = artifact_bytes(root);
    let error = apply_failure(root, &jsonl(&batch("sha256:00", "[\"paper §2\", \"C08\"]")));
    assert_eq!(error["code"], "write.digest_conflict");
    assert_eq!(error["details"]["field"], "expected");
    assert_eq!(error["line"], 3);
    // `expected` is the heading-body digest, the same one `show` prints.
    let digest = body_digest(root, json!({"id":"C02"}));
    let shown = read(
        root,
        &[
            "show",
            "--document",
            "logic/claims.md",
            "--heading",
            "Claims",
            "--heading",
            "C02: Source claim",
        ],
    );
    assert_eq!(shown["entries"][0]["digest"], digest, "{shown}");
    // Prose edits beside the mapping are refused, and a non-list field
    // cannot fan out.
    let mut prose = batch(&digest, "[\"paper §2\", \"C08\"]");
    prose[2]["references"][1]["after"] =
        json!("\"Table 3\" in E01 and logic/claims.md:C02 (quoted), plus C08.");
    let error = apply_failure(root, &jsonl(&prose));
    assert_eq!(error["code"], "write.reference_mapping");
    assert_eq!(error["details"]["field"], "references[1].after");
    let mut fan = batch(&digest, "[\"paper §2\", \"C08\"]");
    fan[2]["references"][1]["after"] =
        json!("\"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C02, C08.");
    let error = apply_failure(root, &jsonl(&fan));
    assert_eq!(error["code"], "write.reference_mapping");
    let mut dropped = batch(&digest, "[\"C08\"]");
    dropped[2]["references"][2]["after"] = json!("[\"C08\"]");
    let error = apply_failure(root, &jsonl(&dropped));
    assert_eq!(error["code"], "write.reference_mapping");
    assert_eq!(artifact_bytes(root), before);
    // A JSON list may fan one citation out to several destinations.
    apply(
        root,
        &batch(&digest, "[\"paper §2\", \"C02\", \"C08\"]"),
        false,
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Sources"),
        "[\"paper §2\", \"C02\", \"C08\"]"
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Proof"),
        "\"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C08."
    );
}

#[test]
fn unknown_operation_fields_fail_with_the_batch_decoder_code() {
    let dir = fixture(false);
    let root = dir.path();
    let before = artifact_bytes(root);
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Merge"}),
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true,"unknown":true}),
        ]),
    );
    assert_eq!(error["code"], "write.batch_operation");
    assert_eq!(error["line"], 2);
    assert_eq!(error["details"]["field"], "unknown");
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn nested_rename_in_a_standard_concepts_document_keeps_old_spellings_readable() {
    let dir = fixture(false);
    let root = dir.path();
    write_file(
        root,
        "logic/concepts.md",
        "# Concepts\n\n## Group A\n\n### Term\n- **Definition**: A term.\n\n## Lone\n- **Definition**: Lone concept.\n- **Related**: Concepts/Group A/Term\n",
    );
    write_file(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Boundary mechanism\n    provenance: user\n    description: Root question\n    source_refs: [\"logic/concepts.md#Concepts/Group A/Term\"]\n",
    );
    let target = json!({"document":"logic/concepts.md","heading":["Concepts","Group A","Term"]});
    let expected = entry_digest(root, target.clone());
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename"}),
            json!({"op":"entry.rename","target":target,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    assert_eq!(
        report["operations"][1]["historical_citations"][0]["literal"],
        "logic/concepts.md#Concepts/Group A/Term"
    );
    assert_eq!(
        field(
            root,
            json!({"document":"logic/concepts.md","heading":["Concepts","Lone"]}),
            "Related"
        ),
        "Concepts/Group A/Word"
    );
    assert_retired_spellings(
        root,
        &[
            "logic/concepts.md#Concepts/Group A/Term",
            "logic/concepts.md#Group A/Term",
            "logic/concepts.md:Concepts/Group A/Term",
            "logic/concepts.md#h/Concepts/Group%20A/Term",
        ],
        "logic/concepts.md#h/Concepts/Group%20A/Word",
    );
    read(root, &["show", "N01"]);
    read(root, &["show", "N01", "--with", "refs"]);
    ara(root).arg("check").arg(root).assert().success();
}

#[test]
fn multi_target_refs_keep_each_exact_inventory_and_source_span() {
    let dir = fixture(true);
    let root = dir.path();
    let claims = fs::read_to_string(root.join("logic/claims.md")).unwrap();
    write_file(root, "logic/claims.md", format!("{claims}{MIXED}"));
    let selected = ["C02", "H02", "C03", "C02"];
    let combined = read(
        root,
        &["show", "C02", "H02", "C03", "C02", "--with", "refs"],
    );
    assert_eq!(
        combined["entries"].as_array().unwrap().len(),
        selected.len()
    );
    for (index, target) in selected.iter().enumerate() {
        assert_eq!(combined["entries"][index]["id"], *target);
        let single = read(root, &["show", target, "--with", "refs"]);
        let refs = &combined["entries"][index]["relations"]["refs"];
        assert_eq!(*refs, single["entries"][0]["relations"]["refs"], "{target}");
        for citation in refs["structured"].as_array().unwrap() {
            let source = citation["source"].as_str().unwrap();
            if !source.ends_with(".md") {
                continue;
            }
            let text = fs::read_to_string(root.join(source)).unwrap();
            let start = citation["range"]["start"].as_u64().unwrap() as usize;
            let end = citation["range"]["end"].as_u64().unwrap() as usize;
            assert_eq!(
                &text[start..end],
                citation["literal"].as_str().unwrap(),
                "{citation}"
            );
        }
    }
    let rows = combined["entries"][0]["relations"]["refs"]["structured"]
        .as_array()
        .unwrap();
    assert!(rows.iter().any(|row| row["id"] == "C04"
        && row["field"] == "Dependencies"
        && row["literal"] == "C02"));
    let mixed_proof = rows
        .iter()
        .filter(|row| row["id"] == "C06" && row["field"] == "Proof")
        .collect::<Vec<_>>();
    assert_eq!(mixed_proof.len(), 1, "{mixed_proof:?}");
    assert_eq!(mixed_proof[0]["literal"], "C02");
}

#[test]
fn ordinary_reads_do_not_touch_requested_only_citation_history() {
    let dir = fixture(false);
    let root = dir.path();
    write_file(
        root,
        "trace/aliases.yaml",
        "format: ara.aliases/v1\naliases: [\n",
    );
    assert_eq!(read(root, &["show", "C02"])["entries"][0]["id"], "C02");
    assert!(
        read(root, &["ls"])["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "C02")
    );
    ara(root)
        .args(["show", "C02", "C03", "--with", "refs", "--json"])
        .assert()
        .code(1)
        .stdout("");
    assert_eq!(
        fs::read_to_string(root.join("trace/aliases.yaml")).unwrap(),
        "format: ara.aliases/v1\naliases: [\n"
    );
    assert!(!root.join(".ara").exists());
}
