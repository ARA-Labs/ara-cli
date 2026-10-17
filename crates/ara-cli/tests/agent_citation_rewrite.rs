//! Plan 19 C1: citation repair for rename, claim merge, redirecting removal
//! and claim split, exercised through the real binary. Assertions read the
//! artifact back through the native field and YAML parsers, never raw text.
#[path = "support/citations.rs"]
mod support;
use support::*;

#[test]
fn retained_claim_merge_repairs_citers_and_keeps_history_exact() {
    let dir = fixture(true);
    let root = dir.path();
    let fields_before = logic_fields(root);
    let tree_before = fs::read(root.join("trace/exploration_tree.yaml")).unwrap();
    let observations_before = fs::read(root.join("staging/observations.yaml")).unwrap();
    let preview = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Merge C02 into C01"}),
            merge_op(),
        ],
        true,
    );
    assert_eq!(
        preview["operations"][1]["rewritten_references"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        logic_fields(root),
        fields_before,
        "dry run persists nothing"
    );

    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Merge C02 into C01"}),
            merge_op(),
        ],
        false,
    );
    // Typed tokens move; quotes, prose and delimiters stay.
    let c04 = json!({"id":"C04"});
    assert_eq!(
        field(root, c04.clone(), "Proof"),
        "\"Table 2\" in E01 and logic/claims.md:C01 (quoted), plus C01."
    );
    assert_eq!(
        field(root, c04.clone(), "Sources"),
        "[\"paper §2\", \"C01\"]"
    );
    assert_eq!(field(root, c04.clone(), "Dependencies"), "[C01, C03]");
    assert_eq!(field(root, json!({"id":"C05"}), "Merged into"), "C01");
    assert_eq!(
        field(
            root,
            json!({"document":"logic/experiments.md","heading":["Experiments","E01: Ablation"]}),
            "Sources"
        ),
        "C01"
    );
    // Unknown fields, untyped fields and prose are listed, untouched.
    assert_eq!(
        field(root, c04.clone(), "Statement"),
        "Depends on the source; see C02."
    );
    assert_eq!(field(root, c04, "Custom note"), "C02 stays here");
    let skipped = &report["operations"][1]["skipped_references"];
    let reasons: BTreeSet<&str> = skipped
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons,
        BTreeSet::from(["prose", "untyped_field", "unknown_field"]),
        "{skipped}"
    );
    assert!(
        skipped
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["line"].as_u64().unwrap() > 0)
    );
    // The source is retained with its merge relation.
    assert_eq!(field(root, json!({"id":"C02"}), "Status"), "withdrawn");
    assert_eq!(field(root, json!({"id":"C02"}), "Merged into"), "C01");
    // Immutable trace evidence and bound_to are byte-exact.
    assert_eq!(
        fs::read(root.join("trace/exploration_tree.yaml")).unwrap(),
        tree_before
    );
    assert_eq!(
        fs::read(root.join("staging/observations.yaml")).unwrap(),
        observations_before
    );
    let record = session_of(root, &report);
    assert_eq!(
        touches(&record),
        [
            ("C02".to_owned(), "merged".to_owned()),
            ("C04".to_owned(), "revised".to_owned()),
            ("C05".to_owned(), "revised".to_owned()),
        ]
    );
    assert_audits_match(root, &fields_before, &record, &[]);
    ara(root).arg("check").arg(root).assert().success();
}

#[test]
fn canonical_rename_rewrites_typed_citations_and_authenticates_history() {
    let dir = fixture(false);
    let root = dir.path();
    let fields_before = logic_fields(root);
    let tree_before = fs::read(root.join("trace/exploration_tree.yaml")).unwrap();
    let expected = entry_digest(root, json!({"id":"C02"}));
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename C02"}),
            json!({"op":"entry.rename","target":{"id":"C02"},"name":"C07","expected":expected,"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Dependencies"),
        "[C07, C03]"
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Proof"),
        "\"Table 2\" in E01 and logic/claims.md:C07 (quoted), plus C07."
    );
    assert_eq!(field(root, json!({"id":"C05"}), "Merged into"), "C07");
    assert_eq!(field(root, json!({"id":"C07"}), "Dependencies"), "[C03]");
    // Historical tree citations resolve through the appended mapping.
    assert_eq!(
        fs::read(root.join("trace/exploration_tree.yaml")).unwrap(),
        tree_before
    );
    let mutations = yaml(root, "trace/logic_mutations.yaml")["mutations"].clone();
    assert_eq!(mutations.as_array().unwrap().len(), 1);
    assert_eq!(mutations[0]["from"], "logic/claims.md:C02");
    assert_eq!(mutations[0]["to"], "logic/claims.md:C07");
    let record = session_of(root, &report);
    assert_eq!(
        touches(&record),
        [
            ("C04".to_owned(), "revised".to_owned()),
            ("C05".to_owned(), "revised".to_owned()),
        ]
    );
    assert_audits_match(
        root,
        &fields_before,
        &record,
        &[(
            &["Claims", "C02: Source claim"],
            &["Claims", "C07: Source claim"],
        )],
    );
    assert_eq!(
        report["operations"][1]["rewritten_references"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    // A citer of the retired alias C02 resolves through the authenticated
    // redirect to C07, so a second restructure repairs it too, and the first
    // mapping row stays exact.
    apply(
        root,
        &[
            json!({"op":"claim.add","id":"C10","title":"Alias citer","fields":{"Statement":"S","Conditions":"C","Status":"hypothesis","Provenance":"user","Falsification":"F","Dependencies":["C02"]}}),
        ],
        false,
    );
    let first = mutations[0].clone();
    let expected = entry_digest(root, json!({"id":"C07"}));
    apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename C07"}),
            json!({"op":"entry.rename","target":{"id":"C07"},"name":"C09","expected":expected,"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    let mutations = yaml(root, "trace/logic_mutations.yaml")["mutations"].clone();
    assert_eq!(mutations[0], first, "existing historical spans are exact");
    assert_eq!(field(root, json!({"id":"C05"}), "Merged into"), "C09");
    assert_eq!(field(root, json!({"id":"C10"}), "Dependencies"), "[C09]");
    ara(root).arg("check").arg(root).assert().success();
}

#[test]
fn rename_refuses_unresolved_mentions_with_locations_and_writes_nothing() {
    let dir = fixture(true);
    let root = dir.path();
    let before = artifact_bytes(root);
    let expected = entry_digest(root, json!({"id":"C02"}));
    let text = format!(
        "\n{}\n\n{}\n",
        json!({"op":"session.log","summary":"Rename C02"}),
        json!({"op":"entry.rename","target":{"id":"C02"},"name":"C07","expected":expected,"signal":"user-directive","provenance":"user","rewrite_references":true}),
    );
    let error = apply_failure(root, &text);
    assert_eq!(error["code"], "write.dangling_reference");
    assert_eq!(error["line"], 4, "physical JSONL line");
    assert_eq!(error["details"]["field"], "rewrite_references");
    let locations = error["details"]["locations"].as_array().unwrap();
    let found: BTreeSet<(String, Option<String>, String)> = locations
        .iter()
        .map(|row| {
            (
                row["document"].as_str().unwrap().to_owned(),
                row["field"].as_str().map(str::to_owned),
                row["reason"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        found,
        BTreeSet::from([
            ("logic/claims.md".into(), None, "prose".into()),
            (
                "logic/claims.md".into(),
                Some("Statement".into()),
                "untyped_field".into()
            ),
            (
                "logic/claims.md".into(),
                Some("Custom note".into()),
                "unknown_field".into()
            ),
            (
                "logic/experiments.md".into(),
                Some("Verifies".into()),
                "untyped_field".into()
            ),
        ])
    );
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn heading_renames_keep_delimiters_and_duplicate_leaves_apart() {
    let dir = fixture(false);
    let root = dir.path();
    let fields_before = logic_fields(root);
    let target = json!({"document":"logic/concepts.md","heading":["Group A","Term"]});
    let expected = entry_digest(root, target.clone());
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename a duplicate leaf"}),
            json!({"op":"entry.rename","target":target,"name":"Word","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    let group_b = json!({"document":"logic/concepts.md","heading":["Group B","Term"]});
    assert_eq!(
        field(root, group_b.clone(), "Related"),
        "Group A/Word, A/B #1"
    );
    assert_eq!(field(root, group_b, "Definition"), "B term.");
    assert_eq!(
        field(
            root,
            json!({"document":"logic/concepts.md","heading":["A/B #1"]}),
            "Related"
        ),
        "logic/concepts.md#Group A/Word"
    );
    // A qualified token that resolves to the other `Term` leaf is a citation
    // of a different entry: it is neither rewritten nor a blocking mention.
    assert_eq!(
        field(
            root,
            json!({"document":"logic/concepts.md","heading":["Lone"]}),
            "Sources"
        ),
        "logic/concepts.md#Group B/Term"
    );
    let record = session_of(root, &report);
    assert_audits_match(
        root,
        &fields_before,
        &record,
        &[(&["Group A", "Term"], &["Group A", "Word"])],
    );

    // A heading containing `/` and `#` is renamed as one literal segment.
    let fields_before = logic_fields(root);
    let target = json!({"document":"logic/concepts.md","heading":["A/B #1"]});
    let expected = entry_digest(root, target.clone());
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Rename a delimiter heading"}),
            json!({"op":"entry.rename","target":target,"name":"C/D #2","expected":expected,"signal":"terminology-drift","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    assert_eq!(
        field(
            root,
            json!({"document":"logic/concepts.md","heading":["Group B","Term"]}),
            "Related"
        ),
        "Group A/Word, C/D #2"
    );
    assert_eq!(
        field(
            root,
            json!({"document":"logic/concepts.md","heading":["Lone"]}),
            "Related"
        ),
        "C/D #2"
    );
    assert_audits_match(
        root,
        &fields_before,
        &session_of(root, &report),
        &[(&["A/B #1"], &["C/D #2"])],
    );
    ara(root).arg("check").arg(root).assert().success();
}

#[test]
fn ambiguous_history_refuses_the_restructure() {
    let dir = fixture(false);
    let root = dir.path();
    // `logic/concepts.md#Term` names both leaves; renaming one would
    // silently re-point this immutable citation to the other.
    write_file(
        root,
        "trace/exploration_tree.yaml",
        tree(", \"logic/concepts.md#Term\""),
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
    assert_eq!(error["code"], "write.history_unresolved");
    assert_eq!(error["line"], 2);
    assert_eq!(
        error["details"]["locations"][0],
        json!({"source":"trace/exploration_tree.yaml","field":"source_refs","literal":"logic/concepts.md#Term"})
    );
    assert_eq!(artifact_bytes(root), before);

    // A removal without a replacement cannot resolve historical citations.
    let expected = entry_digest(root, json!({"id":"H02"}));
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Remove"}),
            json!({"op":"entry.remove","target":{"id":"H02"},"expected":expected,"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ]),
    );
    assert_eq!(error["code"], "write.redirect_required");
    assert_eq!(error["details"]["field"], "redirect");
    assert_eq!(artifact_bytes(root), before);
}

#[test]
fn eligible_non_claim_removal_redirects_typed_citers() {
    let dir = fixture(false);
    let root = dir.path();
    let fields_before = logic_fields(root);
    let expected = entry_digest(root, json!({"id":"H02"}));
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Fold H02 into H01"}),
            json!({"op":"entry.remove","target":{"id":"H02"},"expected":expected,"redirect":{"id":"H01"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
        ],
        false,
    );
    assert_eq!(
        field(root, json!({"id":"C03"}), "Sources"),
        "H01 heuristic notes"
    );
    let mutations = yaml(root, "trace/logic_mutations.yaml")["mutations"].clone();
    assert_eq!(mutations[0]["action"], "remove");
    assert_eq!(mutations[0]["to"], "logic/solution/heuristics.md:H01");
    let record = session_of(root, &report);
    assert_eq!(touches(&record), [("C03".to_owned(), "revised".to_owned())]);
    let mut before = fields_before;
    before.retain(|key, _| !key.1.iter().any(|part| part.starts_with("H02")));
    assert_audits_match(root, &before, &record, &[]);
    ara(root).arg("check").arg(root).assert().success();
}

fn split_rows() -> Vec<Value> {
    vec![
        json!({"target":{"id":"C04"},"field":"Dependencies","before":"[C02, C03]","after":"[C02, C08, C03]"}),
        json!({"target":{"id":"C04"},"field":"Proof","before":"\"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C02.","after":"\"Table 2\" in E01 and logic/claims.md:C02 (quoted), plus C02."}),
        json!({"target":{"id":"C04"},"field":"Sources","before":"[\"paper §2\", \"C02\"]","after":"[\"paper §2\", \"C08\"]"}),
        json!({"target":{"id":"C05"},"field":"Merged into","before":"C02","after":"C08"}),
        json!({"target":{"document":"logic/experiments.md","heading":["Experiments","E01: Ablation"]},"field":"Sources","before":"C02","after":"C02"}),
    ]
}

fn split_batch(rows: Vec<Value>, extra: Value) -> Vec<Value> {
    let mut revise = json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Statement":"Narrowed source."},"signal":"empirical-resolution","provenance":"user","action":"split","split_into":[{"id":"C08"}],"references":rows});
    for (key, value) in extra.as_object().unwrap() {
        revise[key] = value.clone();
    }
    vec![
        json!({"op":"session.log","summary":"Split C02"}),
        json!({"op":"claim.add","id":"C08","title":"Spin-off","fields":{"Statement":"Spun off.","Conditions":"C","Status":"hypothesis","Provenance":"user","Falsification":"F"}}),
        revise,
    ]
}

#[test]
fn mapped_split_applies_every_classified_row_and_marks_the_audit() {
    let dir = fixture(false);
    let root = dir.path();
    let report = apply(root, &split_batch(split_rows(), json!({})), false);
    assert_eq!(
        field(root, json!({"id":"C04"}), "Dependencies"),
        "[C02, C08, C03]"
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Sources"),
        "[\"paper §2\", \"C08\"]"
    );
    assert_eq!(field(root, json!({"id":"C05"}), "Merged into"), "C08");
    assert_eq!(
        field(root, json!({"id":"C02"}), "Statement"),
        "Narrowed source."
    );
    let record = session_of(root, &report);
    assert_eq!(
        touches(&record),
        [
            ("C08".to_owned(), "created".to_owned()),
            ("C02".to_owned(), "split".to_owned()),
            ("C04".to_owned(), "revised".to_owned()),
            ("C05".to_owned(), "revised".to_owned()),
        ]
    );
    let primary: Vec<&Value> = record["logic_revisions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["entry"] == json!({"id":"C02"}))
        .collect();
    assert_eq!(primary.len(), 1);
    assert_eq!(primary[0]["action"], "split");
    assert_eq!(primary[0]["split_into"], json!([{"id":"C08"}]));
    // Unchanged retaining rows add no audit.
    assert_eq!(
        report["operations"][2]["rewritten_references"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    ara(root).arg("check").arg(root).assert().success();
}

#[test]
fn split_validation_fails_closed_at_the_physical_line() {
    let dir = fixture(false);
    let root = dir.path();
    let before = artifact_bytes(root);
    let failure = |operations: Vec<Value>| {
        let text = format!("\n{}", jsonl(&operations));
        let error = apply_failure(root, &text);
        assert_eq!(artifact_bytes(root), before);
        error
    };
    // Every current citing field needs a row.
    let mut rows = split_rows();
    rows.pop();
    let error = failure(split_batch(rows, json!({})));
    assert_eq!(error["code"], "write.split_unclassified");
    assert_eq!(error["line"], 4);
    assert_eq!(
        error["details"]["locations"][0]["document"],
        "logic/experiments.md"
    );
    // action and split_into only together.
    let error = failure(split_batch(split_rows(), json!({"split_into":[]})));
    assert_eq!(error["code"], "write.split");
    assert_eq!(error["details"]["field"], "action");
    // Distinct existing claims other than the primary.
    for (split_into, at) in [
        (json!([{"id":"C02"}]), "split_into[0]"),
        (json!([{"id":"C08"},{"id":"C08"}]), "split_into[1]"),
        (json!([{"id":"C99"}]), "split_into[0]"),
    ] {
        let error = failure(split_batch(split_rows(), json!({"split_into":split_into})));
        assert_eq!(error["code"], "write.split_destination", "{error}");
        assert_eq!(error["details"]["field"], at, "{error}");
    }
    // Destinations are only the primary or a declared spin-off.
    let mut rows = split_rows();
    rows[2]["after"] = json!("[\"paper §2\", \"C01\"]");
    let error = failure(split_batch(rows, json!({})));
    assert_eq!(error["code"], "write.split_destination");
    assert_eq!(error["details"]["field"], "references[2].after");
    // A scalar citation cannot fan out.
    let mut rows = split_rows();
    rows[3]["after"] = json!("C02, C08");
    let error = failure(split_batch(rows, json!({})));
    assert_eq!(error["code"], "write.reference_scalar");
    // Exact before source.
    let mut rows = split_rows();
    rows[0]["before"] = json!("[C02,C03]");
    let error = failure(split_batch(rows, json!({})));
    assert_eq!(error["code"], "write.reference_before");
    // A split never chooses destinations itself.
    let error = failure(split_batch(
        split_rows(),
        json!({"rewrite_references":true}),
    ));
    assert_eq!(error["code"], "write.reference_mode");
}

#[test]
fn modes_cycles_and_digests_fail_closed_without_writes() {
    let dir = fixture(false);
    let root = dir.path();
    let before = artifact_bytes(root);
    let expected = entry_digest(root, json!({"id":"C02"}));
    let cases = [
        (
            json!({"op":"entry.rename","target":{"id":"C02"},"name":"C07","expected":expected,"signal":"user-directive","provenance":"user","rewrite_references":true,"references":[{"target":{"id":"C04"},"field":"Dependencies","before":"[C02, C03]","after":"[\"C07\",\"C03\"]"}]}),
            "write.reference_mode",
        ),
        (
            json!({"op":"entry.rename","target":{"id":"C02"},"name":"C07","expected":"sha256:00","signal":"user-directive","provenance":"user","rewrite_references":true}),
            "write.digest_conflict",
        ),
        (
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true,"expected":"sha256:00"}),
            "write.digest_conflict",
        ),
        // C05 is already merged into C02: merging C02 into C05 would cycle.
        (
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C05"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
            "write.redirect_cycle",
        ),
        // The survivor cites the source; repairing it would self-cite.
        (
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C04"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
            "write.reference_rewrite",
        ),
        (
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"supported"},"signal":"user-directive","provenance":"user","rewrite_references":true}),
            "write.merge_shape",
        ),
        (
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true,"references":[{"target":{"id":"C04"},"field":"Dependencies","before":"[C02, C03]","after":"[C01, C03]"}]}),
            "write.reference_mode",
        ),
        (
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true,"unknown":true}),
            "",
        ),
    ];
    for (operation, code) in cases {
        let error = apply_failure(
            root,
            &jsonl(&[
                json!({"op":"session.log","summary":"Restructure"}),
                operation,
            ]),
        );
        if !code.is_empty() {
            assert_eq!(error["code"], code, "{error}");
        }
        assert_eq!(error["line"], 2, "{error}");
        assert_eq!(artifact_bytes(root), before);
    }
}

#[test]
fn explicit_merge_rows_and_later_failures_are_atomic() {
    let dir = fixture(false);
    let root = dir.path();
    let before = artifact_bytes(root);
    // A failure after reference planning leaves every source, index,
    // history file and directory unchanged.
    let error = apply_failure(
        root,
        &jsonl(&[
            json!({"op":"session.log","summary":"Merge then fail"}),
            merge_op(),
            json!({"op":"logic.revise","target":{"id":"C99"},"set":{"Status":"supported"},"signal":"user-directive","provenance":"user"}),
        ]),
    );
    assert_eq!(error["line"], 3);
    assert_eq!(artifact_bytes(root), before);
    assert!(!root.join("trace/sessions").exists());
    assert!(!root.join("trace/logic_mutations.yaml").exists());

    // Explicit rows instead of generation: only the listed citer moves.
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Explicit merge"}),
            json!({"op":"logic.revise","target":{"id":"C02"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","references":[{"target":{"id":"C04"},"field":"Dependencies","before":"[C02, C03]","after":"[C01, C03]"}]}),
        ],
        false,
    );
    assert_eq!(
        field(root, json!({"id":"C04"}), "Dependencies"),
        "[C01, C03]"
    );
    assert_eq!(field(root, json!({"id":"C05"}), "Merged into"), "C02");
    let record = session_of(root, &report);
    assert_eq!(
        touches(&record),
        [
            ("C02".to_owned(), "merged".to_owned()),
            ("C04".to_owned(), "revised".to_owned()),
        ]
    );
}

#[test]
fn caller_judgment_replaces_the_derived_merge_row() {
    let dir = fixture(false);
    let root = dir.path();
    let report = apply(
        root,
        &[
            json!({"op":"session.log","summary":"Merge","claims_touched":[{"id":"C02","action":"withdrawn"}]}),
            merge_op(),
        ],
        false,
    );
    let record = session_of(root, &report);
    assert_eq!(
        touches(&record),
        [
            ("C02".to_owned(), "withdrawn".to_owned()),
            ("C04".to_owned(), "revised".to_owned()),
            ("C05".to_owned(), "revised".to_owned()),
        ]
    );
    // `refuted` contradicts the merge's explicit Status change.
    let dir = fixture(false);
    let error = apply_failure(
        dir.path(),
        &jsonl(&[
            json!({"op":"session.log","summary":"Merge","claims_touched":[{"id":"C02","action":"refuted"}]}),
            merge_op(),
        ]),
    );
    assert_eq!(error["code"], "write.claim_touch_conflict");
}
