#![cfg(feature = "native")]

use ara_core::{
    merge::{self, MergeOptions, MergePlan},
    write::{
        ArtifactSnapshot, WorkingArtifact,
        source::{FileSnapshot, digest},
    },
};
use std::{collections::BTreeMap, path::PathBuf};

const CLAIMS: &str = "logic/claims.md";
const CLAIM: &str = "## C01: Base title\n- **Statement**: base statement\n- **Conditions**: base conditions\n- **Status**: hypothesis\n- **Provenance**: user\n- **Falsification**: counterexample\n\nBase residual paragraph.\n";

fn snapshot(input: &[(&str, &str)]) -> ArtifactSnapshot {
    let mut files = BTreeMap::new();
    for (path, text) in [
        ("trace/exploration_tree.yaml", "tree: []\n"),
        (
            "PAPER.md",
            "---\ntitle: Shared artifact\nara_version: '2.0'\n---\n# Shared artifact\n",
        ),
        (CLAIMS, "# Claims\n\n"),
    ]
    .into_iter()
    .chain(input.iter().copied())
    {
        files.insert(
            path.into(),
            FileSnapshot {
                bytes: text.as_bytes().to_vec(),
                existed: true,
                permissions: None,
                digest: digest(text.as_bytes()),
            },
        );
    }
    ArtifactSnapshot {
        root: PathBuf::from("/pure-merge-fixture"),
        files,
        identity_paths: Default::default(),
    }
}
fn options() -> MergeOptions {
    MergeOptions {
        source_key: "fork-bob".into(),
        label: "bob".into(),
        time: "2026-10-01T10:00".into(),
        git: None,
        predecessor: None,
    }
}
fn plan(base: &ArtifactSnapshot, ours: &ArtifactSnapshot, theirs: &ArtifactSnapshot) -> MergePlan {
    merge::plan_merge(base, ours, theirs, &options())
        .unwrap_or_else(|error| panic!("{error}; evidence={:?}", error.evidence))
}
fn captured(working: &WorkingArtifact) -> ArtifactSnapshot {
    let mut snapshot = working.base.clone();
    for path in &working.deleted_paths {
        snapshot.files.remove(path);
    }
    for (path, bytes) in &working.files {
        snapshot.files.insert(
            path.clone(),
            FileSnapshot {
                bytes: bytes.clone(),
                existed: true,
                permissions: None,
                digest: digest(bytes),
            },
        );
    }
    snapshot
}
fn claims(body: &str) -> String {
    format!("# Claims\n\n{body}")
}

#[test]
fn disjoint_title_known_fields_and_residual_preserve_untouched_ours_bytes() {
    let original = claims(CLAIM).replace('\n', "\r\n");
    let ours = original
        .replace("Base title", "Our title  ")
        .replace("base conditions", "our conditions  ");
    let theirs = original
        .replace("base statement", "their statement")
        .replace(
            "Base residual paragraph.",
            "Their residual paragraph.\r\n\r\nWith a complete second paragraph.",
        );
    let merged = plan(
        &snapshot(&[(CLAIMS, &original)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    let expected = ours.replace("base statement", "their statement").replace(
        "Base residual paragraph.",
        "Their residual paragraph.\r\n\r\nWith a complete second paragraph.",
    );
    assert_eq!(merged.working.text(CLAIMS).unwrap(), expected);
    assert!(merged.report.conflicts.is_empty());
}

#[test]
fn equal_semantic_concurrent_scalar_and_list_edits_keep_our_lexical_form() {
    let other = "## C02: Dependency\n- **Statement**: premise\n";
    let base = claims(&format!("{CLAIM}- **Dependencies**: []\n\n{other}"));
    let ours = base
        .replace("- **Status**: hypothesis", "* **Status**: supported  ")
        .replace("- **Dependencies**: []", "* **Dependencies**: [ 'C02' ]  ");
    let theirs = base
        .replace("hypothesis", "supported")
        .replace("- **Dependencies**: []", "- **Dependencies**: [\"C02\"]");
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    assert_eq!(merged.working.text(CLAIMS).unwrap(), ours);
    assert!(merged.report.conflicts.is_empty());
    assert!(!merged.working.files.contains_key(CLAIMS));
}

#[test]
fn unknown_extension_fields_keep_complete_multiline_values_and_duplicate_occurrences() {
    let unknown = "- **X-proof**:\n  - nested: {opaque: [one, two]}\n  - note: exact  spaces\n- **X-proof**: independent second occurrence\n";
    let base = claims(&format!("{CLAIM}{unknown}"));
    let ours = base.replace("one, two", "one, ours");
    let theirs = base
        .replace("one, two", "one, theirs")
        .replace(
            "independent second occurrence",
            "incoming second occurrence",
        )
        .replace("base conditions", "incoming conditions");
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    let expected = ours
        .replace(
            "independent second occurrence",
            "incoming second occurrence",
        )
        .replace("base conditions", "incoming conditions");
    assert_eq!(merged.working.text(CLAIMS).unwrap(), expected);
    let conflict = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.field == "extension:X-proof")
        .unwrap();
    assert_eq!(
        conflict.base.bytes,
        b"- **X-proof**:\n  - nested: {opaque: [one, two]}\n  - note: exact  spaces\n"
    );
    assert_eq!(
        conflict.ours.bytes,
        b"- **X-proof**:\n  - nested: {opaque: [one, ours]}\n  - note: exact  spaces\n"
    );
    assert_eq!(
        conflict.theirs.bytes,
        b"- **X-proof**:\n  - nested: {opaque: [one, theirs]}\n  - note: exact  spaces\n"
    );
}

#[test]
fn every_supported_layer_has_native_inventory_and_only_canonical_ids_are_numeric() {
    let paper = "---\ntitle: Shared artifact\nknowledge_paths: [appendix/derivation.md]\n---\n# Shared artifact\n";
    let base = snapshot(&[
        ("PAPER.md", paper),
        ("logic/solution/heuristics.md", "# Heuristics\n"),
        ("logic/experiments.md", "# Experiments\n"),
        ("logic/concepts.md", "# Concepts\n"),
        ("logic/related_work.md", "# Related work\n"),
        ("logic/problem.md", "# Problem\n"),
        ("logic/solution/method.md", "# Method\n"),
        ("rubric/requirements.md", "# Requirements\n"),
        ("appendix/derivation.md", "# Derivation\n"),
    ]);
    let mut theirs = base.clone();
    for (path, text) in [
        (
            CLAIMS,
            "# Claims\n\n## C88: Incoming claim\n- **Statement**: mechanism\n",
        ),
        (
            "logic/solution/heuristics.md",
            "# Heuristics\n\n## H77: Incoming heuristic\n- **Rationale**: reason\n",
        ),
        (
            "logic/experiments.md",
            "# Experiments\n\n## E66: Experiment plan\n- **Question**: test this\n",
        ),
        (
            "logic/concepts.md",
            "# Concepts\n\n## Native term\n- **Definition**: precise\n",
        ),
        (
            "logic/related_work.md",
            "# Related work\n\n## RWalpha: Native citation\n- **Title**: Full title\n- **Relation**: builds on this\n",
        ),
        (
            "logic/problem.md",
            "# Problem\n\n## Observations\n\n### O1: Problem observation\n- **Statement**: observed\n",
        ),
        (
            "logic/solution/method.md",
            "# Method\n\n## Architecture\nMechanism body.\n",
        ),
        (
            "rubric/requirements.md",
            "# Requirements\n\n## Acceptance\nBounded requirements.\n",
        ),
        (
            "appendix/derivation.md",
            "# Derivation\n\n## Proof\nComplete proof body.\n",
        ),
    ] {
        theirs.files.insert(
            path.into(),
            FileSnapshot {
                bytes: text.as_bytes().to_vec(),
                existed: true,
                permissions: None,
                digest: digest(text.as_bytes()),
            },
        );
    }
    let merged = plan(&base, &base, &theirs);
    assert!(merged.report.conflicts.is_empty());
    for (id, expected) in [("C88", "C01"), ("H77", "H01"), ("E66", "E01")] {
        assert_eq!(
            merged
                .report
                .imports
                .iter()
                .find(|mapping| mapping.original == id)
                .unwrap()
                .target,
            expected
        );
    }
    for address in [
        "logic/concepts.md#Native term",
        "RWalpha",
        "logic/problem.md#Problem/Observations/O1: Problem observation",
        "logic/solution/method.md",
        "logic/solution/method.md#Method/Architecture",
        "rubric/requirements.md#Requirements/Acceptance",
        "appendix/derivation.md#Derivation/Proof",
    ] {
        let mapping = merged
            .report
            .imports
            .iter()
            .find(|mapping| mapping.original == address)
            .unwrap();
        assert_eq!(mapping.target, address);
    }
    assert!(
        merged
            .working
            .text("logic/problem.md")
            .unwrap()
            .contains("### O1: Problem observation")
    );
    assert!(
        merged
            .working
            .text("logic/related_work.md")
            .unwrap()
            .contains("## RWalpha: Native citation\n- **Title**: Full title")
    );
}

#[test]
fn canonical_claim_deletion_rejects_import_without_mutation() {
    let original = claims(CLAIM);
    let base = snapshot(&[(CLAIMS, &original)]);
    let removed = snapshot(&[(CLAIMS, "# Claims\n\n")]);
    let error = merge::plan_merge(&base, &base, &removed, &options())
        .err()
        .unwrap();
    assert_eq!(error.code, "write.claim_retention");
    assert_eq!(base.files[CLAIMS].bytes, original.as_bytes());
}

#[test]
fn eligible_heuristic_deletion_accepts_agreement_and_unchanged_peer_in_both_directions() {
    let path = "logic/solution/heuristics.md";
    let original = "# Heuristics\n\n## H01: Retired technique\n- **Rationale**: Prior behavior.\n";
    let base = snapshot(&[(path, original)]);
    let removed = snapshot(&[(path, "# Heuristics\n\n")]);
    for (ours, theirs) in [(&base, &removed), (&removed, &base), (&removed, &removed)] {
        let merged = plan(&base, ours, theirs);
        assert_eq!(merged.working.text(path).unwrap(), "# Heuristics\n\n");
        assert!(merged.report.conflicts.is_empty());
    }
}

#[test]
fn delete_edit_retains_complete_candidates_and_preserves_our_absence() {
    let original = claims(CLAIM);
    let edited = original.replace("base statement", "their edited statement");
    let removed = snapshot(&[(CLAIMS, "# Claims\n\n")]);
    let merged = plan(
        &snapshot(&[(CLAIMS, &original)]),
        &removed,
        &snapshot(&[(CLAIMS, &edited)]),
    );
    assert_eq!(merged.working.text(CLAIMS).unwrap(), "# Claims\n\n");
    let conflict = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.selector == "C01")
        .unwrap();
    assert_eq!(conflict.kind, "delete_edit");
    assert!(conflict.base.present);
    assert!(!conflict.ours.present);
    assert_eq!(conflict.base.bytes, CLAIM.as_bytes());
    assert_eq!(
        conflict.theirs.bytes,
        CLAIM
            .replace("base statement", "their edited statement")
            .as_bytes()
    );
}

#[test]
fn delete_edit_keeps_our_entry_and_still_merges_other_entry_fields() {
    let second =
        "## C02: Second\n- **Statement**: second statement\n- **Conditions**: second conditions\n";
    let base = claims(&format!("{CLAIM}\n{second}"));
    let ours = base.replace("base statement", "our edited statement");
    let theirs = claims(&second.replace("second conditions", "their second conditions"));
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    assert_eq!(
        merged.working.text(CLAIMS).unwrap(),
        ours.replace("second conditions", "their second conditions")
    );
    let conflict = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.selector == "C01")
        .unwrap();
    assert!(!conflict.theirs.present);
    assert_eq!(
        conflict.ours.bytes,
        format!(
            "{}\n",
            CLAIM.replace("base statement", "our edited statement")
        )
        .as_bytes()
    );
}

#[test]
fn residual_prose_is_one_opaque_bounded_value_not_a_disjoint_line_merge() {
    let base = claims(&CLAIM.replace(
        "Base residual paragraph.",
        "First paragraph.\n\nSecond paragraph.",
    ));
    let ours = base.replace("First paragraph.", "Our first paragraph.");
    let theirs = base.replace("Second paragraph.", "Their second paragraph.");
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    assert_eq!(merged.working.text(CLAIMS).unwrap(), ours);
    let conflict = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.field.starts_with("$body:"))
        .unwrap();
    assert_eq!(
        conflict.base.bytes,
        b"First paragraph.\n\nSecond paragraph.\n"
    );
    assert_eq!(
        conflict.ours.bytes,
        b"Our first paragraph.\n\nSecond paragraph.\n"
    );
    assert_eq!(
        conflict.theirs.bytes,
        b"First paragraph.\n\nTheir second paragraph.\n"
    );
}

#[test]
fn imported_references_rewrite_only_incoming_fields_and_numeric_entries_even_when_quoted() {
    let base = claims(CLAIM);
    let ours_extra = "## C05: Our unrelated claim\n- **Statement**: our C05 stays exactly\n";
    let ours = format!(
        "{}\n{ours_extra}",
        base.replace("base conditions", "our C05 conditions")
    );
    let incoming_extra =
        "## C05: Their new claim\n- **Statement**: depends on C05\n- **Proof**: [\"C05\"]\n";
    let theirs = format!(
        "{}\n{incoming_extra}",
        base.replace("base statement", "incoming C05 statement")
    );
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    let text = merged.working.text(CLAIMS).unwrap();
    assert!(merged.report.conflicts.is_empty());
    assert!(text.contains("- **Statement**: incoming C06 statement\n"));
    assert!(text.contains("- **Conditions**: our C05 conditions\n"));
    assert!(text.contains(ours_extra));
    assert!(text.contains(
        "## C06: Their new claim\n- **Statement**: depends on C06\n- **Proof**: [\"C06\"]\n"
    ));
    assert_eq!(
        merged
            .report
            .imports
            .iter()
            .find(|mapping| mapping.original == "C05")
            .unwrap()
            .target,
        "C06"
    );
}

#[test]
fn independent_concept_and_recipe_collisions_are_full_identity_conflicts() {
    let base = snapshot(&[("logic/concepts.md", "# Concepts\n")]);
    let ours_concept = "# Concepts\n\n## Native term\n- **Definition**: our meaning\n";
    let theirs_concept = "# Concepts\n\n## Native term\n- **Definition**: their meaning\n";
    let ours_recipe = "# Recipe\n\nOur complete recipe.\n";
    let theirs_recipe = "# Recipe\n\nTheir complete recipe.\n";
    let ours = snapshot(&[
        ("logic/concepts.md", ours_concept),
        ("logic/solution/recipe.md", ours_recipe),
    ]);
    let theirs = snapshot(&[
        ("logic/concepts.md", theirs_concept),
        ("logic/solution/recipe.md", theirs_recipe),
    ]);
    let merged = plan(&base, &ours, &theirs);
    assert_eq!(
        merged.working.text("logic/concepts.md").unwrap(),
        ours_concept
    );
    assert_eq!(
        merged.working.text("logic/solution/recipe.md").unwrap(),
        ours_recipe
    );
    let concept = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.selector == "logic/concepts.md#Native term")
        .unwrap();
    assert_eq!(concept.kind, "identity");
    assert_eq!(
        concept.theirs.bytes,
        b"## Native term\n- **Definition**: their meaning\n"
    );
    let recipe = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.path == "logic/solution/recipe.md")
        .unwrap();
    assert_eq!(recipe.kind, "identity");
    assert_eq!(recipe.theirs.bytes, theirs_recipe.as_bytes());
}

#[test]
fn paper_frontmatter_and_bounded_headings_merge_without_touching_comments_or_extensions() {
    let base = "---\ntitle: Base title\n# untouched metadata comment\nabstract: Base abstract\nx-private: {complete: [opaque, extension]}\n---\n# Paper\n\n## Summary\nBase summary.\n\n## Method\nBase method.\n";
    let ours = base
        .replace("Base title", "Our title")
        .replace("Base method.", "Our method.  ");
    let theirs = base
        .replace("Base abstract", "Their abstract")
        .replace("Base summary.", "Their summary.");
    let merged = plan(
        &snapshot(&[("PAPER.md", base)]),
        &snapshot(&[("PAPER.md", &ours)]),
        &snapshot(&[("PAPER.md", &theirs)]),
    );
    assert!(merged.report.conflicts.is_empty());
    assert_eq!(
        merged.working.text("PAPER.md").unwrap(),
        ours.replace("Base abstract", "Their abstract")
            .replace("Base summary.", "Their summary.")
    );
}

#[test]
fn whole_mutable_document_deletion_has_real_absence_but_root_paper_is_not_deleted() {
    let base = snapshot(&[("logic/solution/old.md", "# Old method\n\nOld body.\n")]);
    let mut theirs = base.clone();
    theirs.files.remove("logic/solution/old.md");
    let merged = plan(&base, &base, &theirs);
    assert!(!merged.working.exists("logic/solution/old.md"));
    assert!(
        merged
            .working
            .deleted_paths
            .contains("logic/solution/old.md")
    );
    assert!(merged.report.conflicts.is_empty());
    let mut no_paper = base.clone();
    no_paper.files.remove("PAPER.md");
    let protected = plan(&base, &base, &no_paper);
    assert_eq!(
        protected.working.bytes("PAPER.md").unwrap(),
        base.files["PAPER.md"].bytes
    );
    assert!(
        protected
            .report
            .conflicts
            .iter()
            .any(|conflict| conflict.path == "PAPER.md"
                && conflict.kind == "unsupported_document_deletion")
    );
}

#[test]
fn field_resolution_rejects_stale_current_fingerprint_before_any_history_write() {
    let base = claims(CLAIM);
    let ours = base.replace("base statement", "our conflicting statement");
    let theirs = base.replace("base statement", "their conflicting statement");
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    let conflict = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.field == "statement")
        .unwrap();
    let mut later = captured(&merged.working);
    let changed = ours.replace("our conflicting statement", "later local edit");
    later.files.insert(
        CLAIMS.into(),
        FileSnapshot {
            bytes: changed.as_bytes().to_vec(),
            existed: true,
            permissions: None,
            digest: digest(changed.as_bytes()),
        },
    );
    let error = merge::plan_resolution(
        &later,
        &conflict.id,
        "theirs",
        "2026-10-01_001",
        1,
        "user-directive",
        "user",
    )
    .unwrap_err();
    assert_eq!(error.code, "merge.stale_conflict");
    assert_eq!(later.files[CLAIMS].bytes, changed.as_bytes());
}

#[test]
fn unsupported_knowledge_extension_and_protected_bodies_retain_full_review_evidence() {
    let base = snapshot(&[
        ("logic/extension.md", "Opaque base.\n"),
        ("src/model.rs", "base code\n"),
        ("evidence/proof.md", "base proof\n"),
    ]);
    let ours = base.clone();
    let mut theirs = base.clone();
    for (path, text) in [
        (
            "logic/extension.md",
            "Opaque incoming.\nWith its whole body.\n",
        ),
        ("src/model.rs", "incoming code\n"),
        ("evidence/proof.md", "incoming proof\n"),
    ] {
        theirs.files.insert(
            path.into(),
            FileSnapshot {
                bytes: text.as_bytes().to_vec(),
                existed: true,
                permissions: None,
                digest: digest(text.as_bytes()),
            },
        );
    }
    let merged = plan(&base, &ours, &theirs);
    for path in ["logic/extension.md", "src/model.rs", "evidence/proof.md"] {
        assert!(!merged.working.files.contains_key(path));
        assert_eq!(merged.working.bytes(path).unwrap(), base.files[path].bytes);
        let conflict = merged
            .report
            .conflicts
            .iter()
            .find(|conflict| conflict.path == path)
            .unwrap();
        assert_eq!(conflict.theirs.bytes, theirs.files[path].bytes);
    }
}

#[test]
fn absent_empty_and_literal_null_extension_values_are_distinct_conflict_evidence() {
    let base = claims(CLAIM);
    let ours = format!("{base}- **X-field**:\n");
    let theirs = format!("{base}- **X-field**: null\n");
    let merged = plan(
        &snapshot(&[(CLAIMS, &base)]),
        &snapshot(&[(CLAIMS, &ours)]),
        &snapshot(&[(CLAIMS, &theirs)]),
    );
    assert_eq!(merged.working.text(CLAIMS).unwrap(), ours);
    let conflict = merged
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.field == "extension:X-field")
        .unwrap();
    assert!(!conflict.base.present);
    assert!(conflict.ours.present);
    assert!(conflict.theirs.present);
    assert_eq!(conflict.ours.bytes, b"- **X-field**:\n");
    assert_eq!(conflict.theirs.bytes, b"- **X-field**: null\n");
}

#[test]
fn deletion_cannot_leave_surviving_known_dependencies_dangling() {
    let first = "## C01: Dependent\n- **Statement**: uses premise\n- **Dependencies**: [C02]\n\n";
    let second = "## C02: Premise\n- **Statement**: premise\n";
    let original = claims(&format!("{first}{second}"));
    let base = snapshot(&[(CLAIMS, &original)]);
    let removed = snapshot(&[(CLAIMS, &claims(first))]);
    let error = match merge::plan_merge(&base, &base, &removed, &options()) {
        Ok(_) => panic!("a surviving C01 dependency cannot target deleted C02"),
        Err(error) => error,
    };
    assert!(error.code.contains("reference"));
    assert_eq!(base.files[CLAIMS].bytes, original.as_bytes());
    assert!(
        error
            .evidence
            .iter()
            .any(|conflict| conflict.base.bytes == original.as_bytes())
    );
}

#[test]
fn complete_new_document_import_relocates_numeric_headings_and_structured_fields() {
    let base = snapshot(&[(CLAIMS, &claims(CLAIM))]);
    let ours = base.clone();
    let theirs = snapshot(&[
        (CLAIMS, &claims(CLAIM)),
        (
            "logic/solution/heuristics.md",
            "# Heuristics\n\n## H50: Incoming heuristic\n- **Rationale**: use C01\n- **Code ref**: [\"logic/claims.md:C01\"]\n",
        ),
    ]);
    let merged = plan(&base, &ours, &theirs);
    assert!(merged.report.conflicts.is_empty());
    assert_eq!(
        merged.working.text("logic/solution/heuristics.md").unwrap(),
        "# Heuristics\n\n## H01: Incoming heuristic\n- **Rationale**: use C01\n- **Code ref**: [\"logic/claims.md:C01\"]\n"
    );
}

#[test]
fn advancing_source_uses_mapped_effective_ancestor_and_keeps_destination_identity() {
    let base_text = claims(CLAIM);
    let base = snapshot(&[(CLAIMS, &base_text)]);
    let ours = snapshot(&[(
        CLAIMS,
        &format!("{base_text}\n## C05: Our claim\n- **Statement**: our unrelated statement\n"),
    )]);
    let source_text =
        format!("{base_text}\n## C05: Source claim\n- **Statement**: original source statement\n");
    let source = snapshot(&[(CLAIMS, &source_text)]);
    let first = plan(&base, &ours, &source);
    let destination = captured(&first.working);
    let advanced = snapshot(&[(
        CLAIMS,
        &source_text.replace("original source statement", "advanced source statement"),
    )]);
    let second = plan(&source, &destination, &advanced);
    assert!(second.report.conflicts.is_empty());
    let text = second.working.text(CLAIMS).unwrap();
    assert!(text.contains("## C05: Our claim\n- **Statement**: our unrelated statement\n"));
    assert!(text.contains("## C06: Source claim\n- **Statement**: advanced source statement\n"));
    assert_eq!(
        second
            .report
            .imports
            .iter()
            .find(|mapping| mapping.original == "C05")
            .unwrap()
            .target,
        "C06"
    );
}

#[test]
fn imported_numeric_subtree_relocates_native_descendant_identity_but_not_examples() {
    let base_text = claims(CLAIM);
    let base = snapshot(&[(CLAIMS, &base_text)]);
    let ours = snapshot(&[(
        CLAIMS,
        &format!("{base_text}\n## C05: Our claim\n- **Statement**: our meaning\n"),
    )]);
    let incoming = format!(
        "{base_text}\n## C05: Source claim\n- **Statement**: source meaning\n\n### Details\nIncoming C05 reference.\n\n```text\nC05 is an example, not a relocated pointer.\n```\n\"historical C05\"\n"
    );
    let theirs = snapshot(&[(CLAIMS, &incoming)]);
    let merged = plan(&base, &ours, &theirs);
    assert!(merged.report.conflicts.is_empty());
    let native = merged
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == "logic/claims.md#Claims/C05/Details")
        .unwrap();
    assert_eq!(native.target, "logic/claims.md#Claims/C06/Details");
    let text = merged.working.text(CLAIMS).unwrap();
    assert!(text.contains("## C06: Source claim"));
    assert!(text.contains("### Details\nIncoming C06 reference.\n\n```text\nC05 is an example, not a relocated pointer.\n```\n\"historical C05\"\n"));
}

#[test]
fn incoming_registration_precedes_creation_even_when_registered_path_sorts_before_paper() {
    let base = snapshot(&[]);
    let paper = std::str::from_utf8(&base.files["PAPER.md"].bytes)
        .unwrap()
        .replace(
            "ara_version: '2.0'\n",
            "ara_version: '2.0'\nknowledge_paths: [Appendix/proof.md]\n",
        );
    let incoming = "# Proof\n\n## Derivation\nComplete registered proof.\n";
    let theirs = snapshot(&[("PAPER.md", &paper), ("Appendix/proof.md", incoming)]);
    let merged = plan(&base, &base, &theirs);
    assert!(merged.report.conflicts.is_empty());
    assert_eq!(merged.working.text("PAPER.md").unwrap(), paper);
    assert_eq!(merged.working.text("Appendix/proof.md").unwrap(), incoming);
}

#[test]
fn concurrent_alias_and_native_proof_targets_coalesce_preserving_ours_lexical_bytes() {
    let aliases = "format: ara.aliases/v1\naliases:\n  - source_key: old-fork\n    label: legacy\n    original: C02\n    target: C02\n    revision: fixture\n";
    let text = claims(&format!(
        "{CLAIM}- **Proof**: []\n\n## C02: Existing premise\n- **Statement**: premise\n"
    ));
    let ours = text.replace("- **Proof**: []", "* **Proof**: [ 'legacy:C02' ]  ");
    let theirs = text.replace("- **Proof**: []", "- **Proof**: [C02]");
    let base = snapshot(&[(CLAIMS, &text), ("trace/aliases.yaml", aliases)]);
    let merged = plan(
        &base,
        &snapshot(&[(CLAIMS, &ours), ("trace/aliases.yaml", aliases)]),
        &snapshot(&[(CLAIMS, &theirs), ("trace/aliases.yaml", aliases)]),
    );
    assert!(merged.report.conflicts.is_empty());
    assert_eq!(merged.working.text(CLAIMS).unwrap(), ours);
    assert!(!merged.working.files.contains_key(CLAIMS));
}

fn install_local_mutation_audit(snapshot: &mut ArtifactSnapshot, row: serde_json::Value) {
    let value = serde_json::json!({"session":{"id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00Z","last_turn":"2026-10-01T10:00Z","turn_count":1,"summary":"audited rename"},"events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[{"turn":1,"entry":row["from_selector"],"field":"entry","before":row["before"],"after":row["after"],"signal":row["signal"],"provenance":row["provenance"]}],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]});
    for (path,bytes) in [
        ("trace/logic_mutations.yaml",format!("mutations:\n  - {row}\n").into_bytes()),
        ("trace/sessions/2026-10-01_001.yaml",ara_core::write::source::render_yaml(&value,0,"\n").into_bytes()),
        ("trace/sessions/session_index.yaml",b"sessions:\n  - {id: 2026-10-01_001, date: '2026-10-01', summary: audited rename, turn_count: 1, events_count: 0, claims_touched: [], open_threads: 0}\n".to_vec()),
    ] {snapshot.files.insert(path.into(),FileSnapshot{digest:digest(&bytes),bytes,existed:true,permissions:None});}
}
#[test]
fn advancing_source_follows_local_rename_without_changing_persisted_import_mapping() {
    let base_text = claims(CLAIM);
    let base = snapshot(&[(CLAIMS, &base_text)]);
    let ours = snapshot(&[(
        CLAIMS,
        &format!("{base_text}\n## C05: Our unrelated claim\n- **Statement**: our meaning\n"),
    )]);
    let source_text =
        format!("{base_text}\n## C05: Source claim\n- **Statement**: original source statement\n");
    let source = snapshot(&[(CLAIMS, &source_text)]);
    let first = plan(&base, &ours, &source);
    let mut locally_renamed = captured(&first.working);
    let old_entry = "## C06: Source claim\n- **Statement**: original source statement\n";
    let new_entry = old_entry.replace("C06:", "C10:");
    let renamed_text = first
        .working
        .text(CLAIMS)
        .unwrap()
        .replace(old_entry, &new_entry);
    locally_renamed.files.insert(
        CLAIMS.into(),
        FileSnapshot {
            bytes: renamed_text.as_bytes().to_vec(),
            existed: true,
            permissions: None,
            digest: digest(renamed_text.as_bytes()),
        },
    );
    let from_selector = serde_json::json!({"document":CLAIMS,"heading":[],"entry":"C06"});
    let to_selector = serde_json::json!({"document":CLAIMS,"heading":[],"entry":"C10"});
    install_local_mutation_audit(
        &mut locally_renamed,
        serde_json::json!({"action":"rename","from":"logic/claims.md:C06","to":"logic/claims.md:C10","from_selector":from_selector,"to_selector":to_selector,"before":old_entry,"after":new_entry,"session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":["trace/aliases.yaml"]}),
    );
    let advanced = snapshot(&[(
        CLAIMS,
        &source_text.replace("original source statement", "advanced source statement"),
    )]);
    let second = plan(&source, &locally_renamed, &advanced);
    assert!(second.report.conflicts.is_empty());
    let text = second.working.text(CLAIMS).unwrap();
    assert!(text.contains("## C10: Source claim\n- **Statement**: advanced source statement\n"));
    assert!(!text.contains("## C06:"));
    assert!(text.contains("## C05: Our unrelated claim\n- **Statement**: our meaning\n"));
    assert_eq!(
        second
            .report
            .imports
            .iter()
            .find(|mapping| mapping.original == "C05")
            .unwrap()
            .target,
        "C06"
    );
    assert_eq!(
        merge::resolve(&captured(&second.working), "bob:C05").unwrap(),
        "C10"
    );
}

#[test]
fn exact_native_mutation_selectors_treat_slash_as_literal_heading_text() {
    let method = "logic/solution/method.md";
    let base_text = "# Method\n\n## Other\n\n### Name\nUnrelated nested name stays exact.\n";
    let base = snapshot(&[(method, base_text)]);
    let source_text = format!("{base_text}\n## Group\n\n### Old/Name\nOriginal source body.\n");
    let source = snapshot(&[(method, &source_text)]);
    let first = plan(&base, &base, &source);
    let mut renamed = captured(&first.working);
    let before = "### Old/Name\nOriginal source body.\n";
    let after = before.replace("Old/Name", "New/Name");
    let renamed_text = first.working.text(method).unwrap().replace(before, &after);
    renamed.files.insert(
        method.into(),
        FileSnapshot {
            bytes: renamed_text.as_bytes().to_vec(),
            existed: true,
            permissions: None,
            digest: digest(renamed_text.as_bytes()),
        },
    );
    let from_selector =
        serde_json::json!({"document":method,"heading":["Method","Group","Old/Name"],"entry":null});
    let to_selector =
        serde_json::json!({"document":method,"heading":["Method","Group","New/Name"],"entry":null});
    install_local_mutation_audit(
        &mut renamed,
        serde_json::json!({"action":"rename","from":"logic/solution/method.md:Method/Group/Old/Name","to":"logic/solution/method.md:Method/Group/New/Name","from_selector":from_selector,"to_selector":to_selector,"before":before,"after":after,"session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":["trace/aliases.yaml"]}),
    );
    let advanced = snapshot(&[(
        method,
        &source_text.replace("Original source body.", "Advanced source body."),
    )]);
    let second = plan(&source, &renamed, &advanced);
    assert!(second.report.conflicts.is_empty());
    let text = second.working.text(method).unwrap();
    assert!(text.contains("### New/Name\nAdvanced source body.\n"));
    assert!(text.contains("### Name\nUnrelated nested name stays exact.\n"));
    assert!(!text.contains("### Old/Name"));
    let current = captured(&second.working);
    assert_eq!(
        merge::resolve(
            &current,
            "bob:logic/solution/method.md#Method/Group/Old/Name"
        )
        .unwrap(),
        "logic/solution/method.md#Method/Group/New/Name"
    );
    let original_selector = ara_core::write::EntrySelector::Document {
        document: method.into(),
        heading: vec!["Method".into(), "Group".into(), "Old/Name".into()],
        entry: None,
    };
    assert_eq!(
        merge::resolve_selector(&current, &original_selector).unwrap(),
        ara_core::write::EntrySelector::Document {
            document: method.into(),
            heading: vec!["Method".into(), "Group".into(), "New/Name".into()],
            entry: None
        }
    );
}

#[test]
fn malformed_mutable_markdown_reports_complete_bytes_of_its_own_document() {
    let path = "logic/solution/heuristics.md";
    let original = "# Heuristics\n";
    let invalid = "# Heuristics\n\n## H50: Invalid source\n- **Rationale**: first exact value\n- **Rationale**: second exact value\n";
    let base = snapshot(&[(path, original)]);
    let theirs = snapshot(&[(path, invalid)]);
    let error = match merge::plan_merge(&base, &base, &theirs, &options()) {
        Ok(_) => panic!("duplicate known Markdown fields must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code, "merge.markdown_duplicate_field");
    assert_eq!(error.field.as_deref(), Some(path));
    let evidence = error
        .evidence
        .iter()
        .find(|conflict| conflict.path == path)
        .unwrap();
    assert_eq!(evidence.base.bytes, original.as_bytes());
    assert_eq!(evidence.ours.bytes, original.as_bytes());
    assert_eq!(evidence.theirs.bytes, invalid.as_bytes());
}

#[test]
fn complete_qualified_descendant_proof_follows_the_actual_relocated_inventory() {
    let base = snapshot(&[(CLAIMS, "# Claims\n")]);
    let ours = snapshot(&[(
        CLAIMS,
        "# Claims\n\n## C05: Destination\n- **Statement**: No Details descendant here\n",
    )]);
    let source_text = "# Claims\n\n## C05: Source\n- **Statement**: Imported finding\n- **Proof**: [logic/claims.md#Claims/C05/Details, logic/claims.md#Claims/C05/Literal/Detail]\n\n### Details\nExact source details.\n\n### Literal/Detail\nExact literal slash body.\n\n\"logic/claims.md#Claims/C05/Details\"\nhttps://example.invalid/logic/claims.md#Claims/C05/Details\n";
    let source = snapshot(&[(CLAIMS, source_text)]);
    let merged = plan(&base, &ours, &source);
    assert!(merged.report.conflicts.is_empty());
    let result = merged.working.text(CLAIMS).unwrap();
    assert!(result.contains("- **Proof**: [logic/claims.md#Claims/C06/Details, logic/claims.md#Claims/C06/Literal/Detail]"));
    assert!(result.contains("\"logic/claims.md#Claims/C05/Details\""));
    assert!(result.contains("https://example.invalid/logic/claims.md#Claims/C05/Details"));
    let mapping = merged
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == "logic/claims.md#Claims/C05/Details")
        .unwrap();
    assert_eq!(mapping.target, "logic/claims.md#Claims/C06/Details");
    let destination = captured(&merged.working);
    assert_eq!(
        merge::resolve_local(&destination, "logic/claims.md#Claims/C06/Details").unwrap(),
        mapping.target
    );
    let selected = merge::resolve_selector(
        &destination,
        &ara_core::write::EntrySelector::Document {
            document: CLAIMS.into(),
            heading: vec!["Claims".into(), "C06: Source".into(), "Details".into()],
            entry: None,
        },
    )
    .unwrap();
    let entry = ara_core::write::logic::resolve(&merged.working, &selected).unwrap();
    assert!(result[entry.range].starts_with("### Details\nExact source details."));
    let literal = ara_core::write::EntrySelector::Document {
        document: CLAIMS.into(),
        heading: vec![
            "Claims".into(),
            "C06: Source".into(),
            "Literal/Detail".into(),
        ],
        entry: None,
    };
    assert_eq!(
        merge::resolve_selector(&destination, &literal).unwrap(),
        literal
    );
    assert_eq!(
        merge::resolve_local(&destination, "logic/claims.md#Claims/C06/Literal/Detail").unwrap(),
        "logic/claims.md#Claims/C06/Literal/Detail"
    );
    let split = ara_core::write::EntrySelector::Document {
        document: CLAIMS.into(),
        heading: vec![
            "Claims".into(),
            "C06: Source".into(),
            "Literal".into(),
            "Detail".into(),
        ],
        entry: None,
    };
    assert!(merge::resolve_selector(&destination, &split).is_err());
    let replay = plan(&base, &destination, &source);
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        merge::fingerprint(&captured(&replay.working)),
        merge::fingerprint(&destination)
    );
}
