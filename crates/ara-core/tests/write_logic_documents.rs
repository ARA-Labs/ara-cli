#![cfg(feature = "native")]
use ara_core::{
    markdown,
    write::{self, ApplyMode, EntrySelector, Fields, WorkingArtifact, WriteOperation, source},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tempfile::TempDir;

fn make_working(documents: &[(&str, &str)]) -> (TempDir, WorkingArtifact) {
    let root = TempDir::new().unwrap();
    for (path, text) in documents {
        let destination = root.path().join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, text).unwrap();
    }
    let mut artifact = WorkingArtifact::new(write::ArtifactSnapshot::load(root.path()).unwrap());
    // Planner-level tests stand in for the writer's one locked clock read.
    artifact.batch_time = Some("2026-10-01T12:00:00Z".into());
    (root, artifact)
}
fn selector(id: &str) -> EntrySelector {
    EntrySelector::Id { id: id.into() }
}
fn fields(value: Value) -> Fields {
    serde_json::from_value(value).unwrap()
}
fn claim_fields(statement: &str) -> Fields {
    fields(
        json!({"Statement":statement,"Conditions":"Caller boundary","Status":"hypothesis","Provenance":"user","Falsification criteria":"Caller falsification","Proof":"E01 and complete prose"}),
    )
}
fn add_claim(working: &mut WorkingArtifact, id: &str, statement: &str) {
    write::plan_operation(
        working,
        &WriteOperation::ClaimAdd {
            id: Some(id.into()),
            title: "Caller title".into(),
            fields: claim_fields(statement),
        },
    )
    .unwrap();
}

#[test]
fn exact_multiline_blank_trailing_lines_fences_and_unknown_source_survive() {
    let unknown =
        "- **Private label**: opaque\n  nested: [one, two]\n\nUnrelated prose stays exact.\n";
    let original = format!("# Claims\n\n## C01: Existing\n- **Statement**: before\n{unknown}");
    let (_root, mut working) = make_working(&[("logic/claims.md", &original)]);
    let supplied =
        " snow 雪 = value\r\n\n## not a section\n```yaml\n- **Statement**: not a peer\n```\n\n";
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryEdit {
            target: selector("C01"),
            set: fields(json!({"Statement":supplied})),
        },
    )
    .unwrap();
    assert_eq!(
        write::logic::field_value(&working, &selector("C01"), "Statement").unwrap(),
        supplied
    );
    assert!(working.text("logic/claims.md").unwrap().ends_with(unknown));
    let before = working.text("logic/claims.md").unwrap().to_owned();
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Statement":supplied}))
            }
        )
        .unwrap()
        .no_op
    );
    assert_eq!(working.text("logic/claims.md").unwrap(), before);
    for payload in ["", "\n", "\n\n", "a\n", " a ", "a\r\nb\r\n"] {
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Statement":payload})),
            },
        )
        .unwrap();
        assert_eq!(
            write::logic::field_value(&working, &selector("C01"), "Statement").unwrap(),
            payload
        );
    }
}

#[test]
fn creation_requires_complete_schemas_and_canonical_alias_duplicates_reject() {
    let (_root, mut working) = make_working(&[]);
    let mut incomplete = claim_fields("Complete");
    incomplete.remove("Conditions");
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::ClaimAdd {
                id: None,
                title: "Missing boundary".into(),
                fields: incomplete
            }
        )
        .is_err()
    );
    assert!(!working.exists("logic/claims.md"));
    add_claim(&mut working, "C01", "Complete\n\nstatement\n");
    assert_eq!(
        write::logic::field_value(&working, &selector("C01"), "Falsification criteria").unwrap(),
        "Caller falsification"
    );
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Falsification":"one","Falsification criteria":"two"}))
            }
        )
        .is_err()
    );
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Last revised":"fake"}))
            }
        )
        .is_err()
    );
    let result=write::plan_operation(&mut working,&WriteOperation::HeuristicAdd{id:Some("H01".into()),title:"Technique".into(),fields:fields(json!({"Rationale":"Reason","Status":"active","Provenance":"user","Sensitivity":"unknown","Code ref":["src/run.rs"],"Sources":["caller DOI"],"Bounds":"caller limits"}))}).unwrap();
    assert_eq!(result.id.as_deref(), Some("H01"));
    assert_eq!(
        write::logic::field_value(&working, &selector("H01"), "Code ref").unwrap(),
        "[\"src/run.rs\"]"
    );
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("H01"),
                set: fields(json!({"Unknown":"value"}))
            }
        )
        .is_err()
    );
    for dependency in ["C0", "C000", "C-1", "H01", "C1x"] {
        assert!(
            write::plan_operation(
                &mut working,
                &WriteOperation::EntryEdit {
                    target: selector("C01"),
                    set: fields(json!({"Dependencies":[dependency]}))
                }
            )
            .is_err()
        );
    }
}

#[test]
fn native_nested_selectors_disambiguate_concepts_plans_and_related_work() {
    let concepts = "# Concepts\n\n## First\n### Shared\n- **Definition**: first\n## Second\n### Shared\n- **Definition**: second\n";
    let experiments = "# Plans\n\n## E01: Plan\n- **Setup**: original\n";
    let related = "# Related Work\n\n## RW01: Paper\n- **DOI**: old-doi\n";
    let (_root, mut working) = make_working(&[
        ("logic/concepts.md", concepts),
        ("logic/experiments.md", experiments),
        ("logic/related_work.md", related),
    ]);
    let ambiguous = EntrySelector::Document {
        document: "logic/concepts.md".into(),
        heading: vec!["Shared".into()],
        entry: None,
    };
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: ambiguous,
                set: fields(json!({"Definition":"wrong"}))
            }
        )
        .is_err()
    );
    let exact = EntrySelector::Document {
        document: "logic/concepts.md".into(),
        heading: vec!["Second".into(), "Shared".into()],
        entry: None,
    };
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryEdit {
            target: exact.clone(),
            set: fields(
                json!({"Definition":"new definition\n","Role":"Caller role","Appears in":["C01"]}),
            ),
        },
    )
    .unwrap();
    assert_eq!(
        write::logic::field_value(&working, &exact, "Definition").unwrap(),
        "new definition\n"
    );
    assert!(
        working
            .text("logic/concepts.md")
            .unwrap()
            .contains("### Shared\n- **Definition**: first\n")
    );
    for (document, id, key, value) in [
        ("logic/experiments.md", "E01", "Setup", "new setup"),
        ("logic/related_work.md", "RW01", "DOI", "new-doi"),
    ] {
        let target = EntrySelector::Document {
            document: document.into(),
            entry: Some(id.into()),
            heading: Vec::new(),
        };
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: target.clone(),
                set: Fields::from([(key.into(), json!(value))]),
            },
        )
        .unwrap();
        assert_eq!(
            write::logic::field_value(&working, &target, key).unwrap(),
            value
        );
    }
    let escape = EntrySelector::Document {
        document: "logic/../src/file.md".into(),
        heading: vec!["Shared".into()],
        entry: None,
    };
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: escape,
                set: fields(json!({"Definition":"no"}))
            }
        )
        .is_err()
    );
}

#[test]
fn duplicate_source_fields_and_unbounded_prose_edits_reject() {
    let (_root, mut working) = make_working(&[(
        "logic/claims.md",
        "## C01: duplicate\n- **Statement**: one\n- **statement**: two\n",
    )]);
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Statement":"three"}))
            }
        )
        .is_err()
    );
    let (_root, mut working) = make_working(&[(
        "logic/claims.md",
        "## C01: ambiguous\n- **Statement**: one\n\nUnrelated unindented prose.\n",
    )]);
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Statement":"three"}))
            }
        )
        .is_err()
    );
}

#[test]
fn digest_conflicts_and_bounded_document_replacements() {
    let original = "# Problem\n\n## Assumptions\nOld body.\n\n## Preserved\nKeep exact.\n";
    let (_root, mut working) = make_working(&[("logic/problem.md", original)]);
    let heading = vec!["Assumptions".into()];
    let body = write::documents::body_range(original, &heading).unwrap();
    let expected = source::digest(original[body].as_bytes());
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::DocumentReplace {
                document: "logic/problem.md".into(),
                heading: heading.clone(),
                expected: "sha256:wrong".into(),
                content: "New body.\n".into()
            }
        )
        .is_err()
    );
    assert_eq!(working.text("logic/problem.md").unwrap(), original);
    write::plan_operation(
        &mut working,
        &WriteOperation::DocumentReplace {
            document: "logic/problem.md".into(),
            heading,
            expected,
            content: "New body.\n".into(),
        },
    )
    .unwrap();
    assert_eq!(
        working.text("logic/problem.md").unwrap(),
        "# Problem\n\n## Assumptions\nNew body.\n## Preserved\nKeep exact.\n"
    );
    let text = working.text("logic/problem.md").unwrap().to_owned();
    write::plan_operation(
        &mut working,
        &WriteOperation::DocumentReplace {
            document: "logic/problem.md".into(),
            heading: vec![],
            expected: source::digest(text.as_bytes()),
            content: "Caller whole document\n".into(),
        },
    )
    .unwrap();
    assert_eq!(
        working.text("logic/problem.md").unwrap(),
        "Caller whole document\n"
    );
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::DocumentCreate {
                document: "evidence/README.md".into(),
                content: "not allowed".into()
            }
        )
        .is_err()
    );
}

#[test]
fn paper_metadata_unknowns_comments_and_untouched_bodies_survive() {
    let original = "---\ntitle: Original\n# unknown comment\nprivate_metadata: {nested: [1, 2], custom: kept}\nauthors: [Caller]\nabstract: |\n  Original abstract.\n---\n# Research\n\n## Layer Index\nOriginal index.\n\n## Body\nRetain exact.\n";
    let (_root, mut working) = make_working(&[("PAPER.md", original)]);
    let heading = vec!["Layer Index".into()];
    let body = write::documents::body_range(original, &heading).unwrap();
    write::plan_operation(
        &mut working,
        &WriteOperation::PaperEdit {
            frontmatter: fields(
                json!({"title":"New title","abstract":"Full\nabstract\n","claims_summary":["C01"]}),
            ),
            heading,
            expected: Some(source::digest(original[body].as_bytes())),
            content: Some("Caller index.\n".into()),
            audit: None,
        },
    )
    .unwrap();
    let text = working.text("PAPER.md").unwrap();
    assert!(text.contains(
        "# unknown comment\nprivate_metadata: {nested: [1, 2], custom: kept}\nauthors: [Caller]\n"
    ));
    assert!(text.ends_with("## Layer Index\nCaller index.\n## Body\nRetain exact.\n"));
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::PaperEdit {
                frontmatter: fields(json!({"private_metadata":"forbidden"})),
                heading: vec![],
                expected: None,
                content: None,
                audit: None
            }
        )
        .is_err()
    );
}

#[test]
fn initialization_has_exact_nine_seeds_and_never_overwrites() {
    let (root, mut working) = make_working(&[]);
    let paper = "# Caller research\n";
    write::plan_operation(
        &mut working,
        &WriteOperation::ArtifactInit {
            profile: "research-manager".into(),
            paper: Some(paper.into()),
            documents: BTreeMap::new(),
            missing_only: false,
        },
    )
    .unwrap();
    let expected = BTreeMap::from([
        ("PAPER.md", paper),
        ("trace/exploration_tree.yaml", "tree: []\n"),
        (write::sessions::INDEX, "sessions: []\n"),
        ("trace/pm_reasoning_log.yaml", "entries: []\n"),
        ("staging/observations.yaml", "observations: []\n"),
        ("logic/claims.md", "# Claims\n"),
        ("logic/problem.md", "# Problem\n"),
        ("logic/solution/heuristics.md", "# Heuristics\n"),
        ("evidence/README.md", "# Evidence Index\n"),
    ]);
    assert_eq!(
        working.paths(),
        expected
            .keys()
            .map(|key| (*key).to_owned())
            .collect::<Vec<_>>()
    );
    for (path, text) in expected {
        assert_eq!(working.text(path).unwrap(), text);
    }
    assert!(!root.path().join("PAPER.md").exists());
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::ArtifactInit {
                profile: "research-manager".into(),
                paper: Some("overwrite".into()),
                documents: BTreeMap::new(),
                missing_only: false
            }
        )
        .is_err()
    );
    assert_eq!(working.text("PAPER.md").unwrap(), paper);
    let (root, mut working) = make_working(&[]);
    std::fs::write(root.path().join("caller.txt"), "retain").unwrap();
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::ArtifactInit {
                profile: "research-manager".into(),
                paper: Some(paper.into()),
                documents: BTreeMap::new(),
                missing_only: false
            }
        )
        .is_err()
    );
    assert!(working.changed_paths().is_empty());
}

#[test]
fn compiler_init_requires_all_caller_knowledge_and_dry_run_creates_nothing() {
    let root = TempDir::new().unwrap();
    let documents = BTreeMap::from([
        ("PAPER.md".into(), "# Actual caller research\n".into()),
        (
            "logic/problem.md".into(),
            "# Problem\nCaller problem.\n".into(),
        ),
        ("logic/claims.md".into(), "# Claims\n".into()),
        (
            "logic/concepts.md".into(),
            "# Concepts\nCaller concepts.\n".into(),
        ),
        (
            "logic/experiments.md".into(),
            "# Experiments\nCaller plans.\n".into(),
        ),
        (
            "logic/related_work.md".into(),
            "# Related Work\nCaller citations.\n".into(),
        ),
        (
            "logic/solution/constraints.md".into(),
            "# Constraints\nCaller bounds.\n".into(),
        ),
    ]);
    let operation = WriteOperation::ArtifactInit {
        profile: "compiler".into(),
        paper: None,
        documents: documents.clone(),
        missing_only: false,
    };
    let report = write::execute(root.path(), &[operation], ApplyMode::DryRun).unwrap();
    assert!(!report.committed);
    assert!(!root.path().join(".ara").exists());
    assert!(!root.path().join("PAPER.md").exists());
    let mut working = WorkingArtifact::new(write::ArtifactSnapshot::load(root.path()).unwrap());
    let mut missing = documents.clone();
    missing.remove("logic/concepts.md");
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::ArtifactInit {
                profile: "compiler".into(),
                paper: None,
                documents: missing,
                missing_only: false
            }
        )
        .is_err()
    );
    assert!(working.changed_paths().is_empty());
    write::plan_operation(
        &mut working,
        &WriteOperation::ArtifactInit {
            profile: "compiler".into(),
            paper: None,
            documents: documents.clone(),
            missing_only: false,
        },
    )
    .unwrap();
    for (path, text) in documents {
        assert_eq!(working.text(&path).unwrap(), text);
    }
}

#[test]
fn revisions_record_exact_source_endpoints_and_revision_pointer() {
    let (_root, mut working) = make_working(&[]);
    add_claim(&mut working, "C01", "Before\n\nlong wording\n");
    write::plan_operation(
        &mut working,
        &WriteOperation::LogicRevise {
            target: selector("C01"),
            set: fields(json!({"Statement":"After\n\nnew wording\n"})),
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: "user-directive".into(),
            provenance: "user-revised".into(),
            note: Some("Caller requested narrower scope".into()),
            expected: None,
            rewrite_references: false,
            references: vec![],
            action: None,
            split_into: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        working.revisions[0].record["before"],
        "Before\n\nlong wording\n"
    );
    assert_eq!(
        working.revisions[0].record["after"],
        "After\n\nnew wording\n"
    );
    assert_eq!(
        write::logic::field_value(&working, &selector("C01"), "Last revised").unwrap(),
        "2026-10-01 (2026-10-01_001#1)"
    );
    assert_eq!(working.revisions[0].session, "2026-10-01_001");
    assert_eq!(working.revisions[0].turn, 1);
}

#[test]
fn concrete_forward_dependencies_resolve_final_and_cycles_or_missing_refs_reject() {
    let (_root, mut working) =
        make_working(&[("logic/experiments.md", "## E01: Caller experiment\n")]);
    let mut first = claim_fields("first");
    first.insert("Dependencies".into(), json!(["C02"]));
    write::plan_operation(
        &mut working,
        &WriteOperation::ClaimAdd {
            id: Some("C01".into()),
            title: "first".into(),
            fields: first,
        },
    )
    .unwrap();
    assert!(write::logic::validate_references(&working).is_err());
    add_claim(&mut working, "C02", "second");
    write::logic::validate_references(&working).unwrap();
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryEdit {
            target: selector("C02"),
            set: fields(json!({"Dependencies":["C01"]})),
        },
    )
    .unwrap();
    assert!(write::logic::validate_references(&working).is_err());
}

#[test]
fn structural_mutations_audit_prose_and_preserve_immutable_history() {
    let concepts = "# Concepts\n\n## Old term\n- **Definition**: Caller concept\n";
    let history = "entries:\n  - note: Old term was discussed\n";
    let (_root, mut working) = make_working(&[
        ("logic/concepts.md", concepts),
        ("trace/pm_reasoning_log.yaml", history),
    ]);
    let target = EntrySelector::Document {
        document: "logic/concepts.md".into(),
        heading: vec!["Old term".into()],
        entry: None,
    };
    let entry = write::logic::resolve(&working, &target).unwrap();
    let expected = source::digest(concepts[entry.range].as_bytes());
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryRename {
            target: target.clone(),
            name: "New term".into(),
            expected,
            references: vec![],
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: Some("user-directive".into()),
            provenance: Some("user-revised".into()),
            rewrite_references: false,
        },
    )
    .unwrap();
    assert_eq!(
        working.text("trace/pm_reasoning_log.yaml").unwrap(),
        history
    );
    let mappings = working.yaml("trace/logic_mutations.yaml").unwrap();
    let mapping = mappings
        .root
        .get("mutations")
        .unwrap()
        .unwrap()
        .sequence()
        .unwrap()[0]
        .to_json()
        .unwrap();
    assert_eq!(
        mapping["from_selector"],
        json!({"document":"logic/concepts.md","heading":["Concepts","Old term"],"entry":null})
    );
    assert_eq!(
        mapping["to_selector"],
        json!({"document":"logic/concepts.md","heading":["Concepts","New term"],"entry":null})
    );
    assert_eq!(
        mapping["historical_references"],
        json!(["trace/pm_reasoning_log.yaml"])
    );
    assert!(
        working.revisions[0].record["before"]
            .as_str()
            .unwrap()
            .contains("## Old term")
    );
    assert!(
        working.revisions[0].record["after"]
            .as_str()
            .unwrap()
            .contains("## New term")
    );
    let (_root, mut working) = make_working(&[
        ("logic/concepts.md", concepts),
        (
            "logic/problem.md",
            "# Problem\nOld term remains in prose.\n",
        ),
    ]);
    let entry = write::logic::resolve(&working, &target).unwrap();
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryRemove {
                target,
                expected: source::digest(concepts[entry.range].as_bytes()),
                references: vec![],
                session: Some("2026-10-01_001".into()),
                turn: Some(1),
                signal: Some("user-directive".into()),
                provenance: Some("user-revised".into()),
                redirect: None,
                rewrite_references: false,
            }
        )
        .is_err()
    );
    assert_eq!(working.text("logic/concepts.md").unwrap(), concepts);
}

#[test]
fn inline_taste_and_conflicts_are_typed_additive_records() {
    let (_root, mut working) = make_working(&[("logic/experiments.md", "## E01: Experiment\n")]);
    add_claim(&mut working, "C01", "Exact caller statement");
    add_claim(&mut working, "C02", "Other statement");
    let before = write::logic::field_value(&working, &selector("C01"), "Statement").unwrap();
    let taste = json!({"date":"2026-10-01","tag":"uncertain","object":"framing","comment":"Caller-confirmed reaction.\nSecond line."});
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryTasteAppend {
            target: selector("C01"),
            record: taste.clone(),
        },
    )
    .unwrap();
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryAnnotate {
            target: selector("C01"),
            kind: "conflict".into(),
            references: vec!["C02".into()],
            comment: "Unresolved contradiction\nCaller supplied.".into(),
        },
    )
    .unwrap();
    assert_eq!(
        write::logic::field_value(&working, &selector("C01"), "Statement").unwrap(),
        before
    );
    let entry = write::logic::resolve(&working, &selector("C01")).unwrap();
    let source_fields = markdown::fields(working.text("logic/claims.md").unwrap(), entry.body);
    let stored = source_fields.iter().find(|f| f.name == "Taste").unwrap();
    assert_eq!(
        markdown::decode_field(stored),
        "- [2026-10-01] `uncertain` on `framing` — Caller-confirmed reaction.\n  Second line."
    );
    assert!(
        working
            .text("logic/claims.md")
            .unwrap()
            .contains("<!-- CONFLICT: see C02 -->\n")
    );
    write::logic::validate_references(&working).unwrap();
    assert!(write::plan_operation(&mut working,&WriteOperation::EntryTasteAppend{target:selector("C01"),record:json!({"date":"2026-02-30","tag":"uncertain","object":"framing","comment":"bad calendar"})}).is_err());
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryTasteAppend {
                target: EntrySelector::Document {
                    document: "logic/experiments.md".into(),
                    entry: Some("E01".into()),
                    heading: vec![]
                },
                record: taste
            }
        )
        .is_err()
    );
}

#[test]
fn registered_knowledge_paths_are_bounded_documents_and_rubric_is_not_native() {
    let (_root, mut working) = make_working(&[("PAPER.md", "# Caller research\n")]);
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::DocumentCreate {
                document: "appendix/details.md".into(),
                content: "# Details\nCaller text.\n".into()
            }
        )
        .is_err()
    );
    write::plan_operation(
        &mut working,
        &WriteOperation::PaperEdit {
            frontmatter: fields(json!({"knowledge_paths":["appendix/details.md"]})),
            heading: vec![],
            expected: None,
            content: None,
            audit: None,
        },
    )
    .unwrap();
    let content = "# Details\n\n## Method\nCaller equation $x=y$.\n";
    write::plan_operation(
        &mut working,
        &WriteOperation::DocumentCreate {
            document: "appendix/details.md".into(),
            content: content.into(),
        },
    )
    .unwrap();
    write::plan_operation(
        &mut working,
        &WriteOperation::DocumentReplace {
            document: "appendix/details.md".into(),
            heading: vec![],
            expected: source::digest(content.as_bytes()),
            content: "# Details\nRevised caller body.\n".into(),
        },
    )
    .unwrap();
    assert_eq!(
        working.text("appendix/details.md").unwrap(),
        "# Details\nRevised caller body.\n"
    );
    let rubric = "# Requirements\n\n## R01: Requirement\n- **Rubric ID**: source-uuid\n- **Requirement**: original text\n";
    let error = write::plan_operation(
        &mut working,
        &WriteOperation::DocumentCreate {
            document: "rubric/requirements.md".into(),
            content: rubric.into(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "write.document");
    assert!(!working.exists("rubric/requirements.md"));
    for (target, code) in [
        (EntrySelector::Id { id: "R01".into() }, "write.namespace"),
        (
            EntrySelector::Document {
                document: "rubric/requirements.md".into(),
                entry: Some("R01".into()),
                heading: vec![],
            },
            "write.selector",
        ),
    ] {
        let error = write::plan_operation(
            &mut working,
            &WriteOperation::EntryEdit {
                target,
                set: fields(json!({"Requirement":"Exact caller requirement"})),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, code);
    }
    for paths in [
        json!(["src/code.md"]),
        json!(["evidence/result.md"]),
        json!(["rubric/requirements.md"]),
        json!(["appendix/details.md", "appendix/details.md"]),
        json!(["appendix/../secret.md"]),
    ] {
        let (_root, mut candidate) = make_working(&[("PAPER.md", "# Caller research\n")]);
        assert!(
            write::plan_operation(
                &mut candidate,
                &WriteOperation::PaperEdit {
                    frontmatter: fields(json!({"knowledge_paths":paths})),
                    heading: vec![],
                    expected: None,
                    content: None,
                    audit: None
                }
            )
            .is_err()
        );
    }
}

#[test]
fn structural_reference_edits_are_exact_and_typed_dependencies_migrate() {
    let (_root, mut working) = make_working(&[("logic/experiments.md", "## E01: Experiment\n")]);
    add_claim(&mut working, "C01", "Original identity");
    write::plan_operation(&mut working,&serde_json::from_value(json!({"op":"session.start","id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Synthetic rename audit"})).unwrap()).unwrap();
    write::plan_operation(
        &mut working,
        &serde_json::from_value(
            json!({"op":"session.log","session":"2026-10-01_001","timestamp":"2026-10-01T10:01"}),
        )
        .unwrap(),
    )
    .unwrap();
    let mut dependent = claim_fields("Dependent statement");
    dependent.insert("Dependencies".into(), json!(["C01"]));
    write::plan_operation(
        &mut working,
        &WriteOperation::ClaimAdd {
            id: Some("C02".into()),
            title: "Dependent".into(),
            fields: dependent,
        },
    )
    .unwrap();
    let entry = write::logic::resolve(&working, &selector("C01")).unwrap();
    let expected = source::digest(working.text("logic/claims.md").unwrap()[entry.range].as_bytes());
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryRename {
            target: selector("C01"),
            name: "C03".into(),
            expected,
            references: vec![write::ReferenceEdit {
                target: selector("C02"),
                field: "Dependencies".into(),
                // New claims write typed dependencies as `[C01]`; `before` is exact source.
                before: "[C01]".into(),
                after: "[\"C03\"]".into(),
            }],
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: Some("dependency-change".into()),
            provenance: Some("user-revised".into()),
            rewrite_references: false,
        },
    )
    .unwrap();
    assert_eq!(
        write::logic::field_value(&working, &selector("C02"), "Dependencies").unwrap(),
        "[\"C03\"]"
    );
    assert_eq!(working.revisions[0].record["before"], "[C01]");
    assert_eq!(working.revisions[0].record["after"], "[\"C03\"]");
    assert_eq!(working.revisions[0].record["entry"], json!({"id":"C02"}));
    let pending = std::mem::take(&mut working.revisions);
    for revision in pending {
        write::sessions::append_revision(
            &mut working,
            &revision.session,
            revision.turn,
            &revision.record,
        )
        .unwrap();
    }
    write::logic::validate_references(&working).unwrap();
}

#[test]
fn revision_batches_refuse_historical_turns_without_batch_ownership() {
    let (root, mut working) = make_working(&[
        ("trace/exploration_tree.yaml", "tree: []\n"),
        ("logic/experiments.md", "## E01: Experiment\n"),
    ]);
    add_claim(&mut working, "C01", "Before");
    std::fs::create_dir_all(root.path().join("logic")).unwrap();
    std::fs::write(
        root.path().join("logic/claims.md"),
        working.text("logic/claims.md").unwrap(),
    )
    .unwrap();
    let revise = WriteOperation::LogicRevise {
        target: selector("C01"),
        set: fields(json!({"Statement":"After"})),
        session: Some("2026-10-01_001".into()),
        turn: Some(1),
        signal: "user-directive".into(),
        provenance: "user-revised".into(),
        note: None,
        expected: None,
        rewrite_references: false,
        references: vec![],
        action: None,
        split_into: vec![],
    };
    assert!(write::execute(root.path(), &[revise], ApplyMode::Commit).is_err());
    assert!(
        std::fs::read_to_string(root.path().join("logic/claims.md"))
            .unwrap()
            .contains("- **Statement**: Before\n")
    );
}

#[test]
fn body_revision_requires_digest_and_preserves_exact_before_after_with_owned_turn() {
    let original =
        "# Architecture\n\n## Component\nOriginal caller prose.\n\n## Untouched\nKeep exact.\n";
    let (root, mut candidate) = make_working(&[
        ("trace/exploration_tree.yaml", "tree: []\n"),
        ("logic/solution/architecture.md", original),
    ]);
    let target = EntrySelector::Document {
        document: "logic/solution/architecture.md".into(),
        heading: vec!["Component".into()],
        entry: None,
    };
    let selected = write::logic::resolve(&candidate, &target).unwrap();
    let before = original[selected.body].to_owned();
    let operation = WriteOperation::LogicRevise {
        target: target.clone(),
        set: fields(
            json!({"Body":"Caller new component.\n\n### Algorithm\n```text\nx = y\n```\n"}),
        ),
        session: Some("2026-10-01_001".into()),
        turn: Some(1),
        signal: "artifact-commitment".into(),
        provenance: "ai-executed".into(),
        note: Some("Caller component revision".into()),
        expected: None,
        rewrite_references: false,
        references: vec![],
        action: None,
        split_into: vec![],
    };
    assert!(write::plan_operation(&mut candidate, &operation).is_err());
    assert_eq!(
        candidate.text("logic/solution/architecture.md").unwrap(),
        original
    );
    let mut operation = operation;
    if let WriteOperation::LogicRevise { expected, .. } = &mut operation {
        *expected = Some(source::digest(before.as_bytes()));
    }
    let operations = vec![
        WriteOperation::SessionStart {
            id: Some("2026-10-01_001".into()),
            date: Some("2026-10-01".into()),
            started: Some("2026-10-01T10:00".into()),
            summary: "Caller began research".into(),
        },
        operation,
        WriteOperation::SessionLog {
            session: Some("2026-10-01_001".into()),
            timestamp: Some("2026-10-01T10:05".into()),
            summary: Some("Caller revised architecture".into()),
            events: vec![],
            ai_actions: vec![],
            claims_touched: vec![],
            logic_revisions: vec![],
            key_context: vec![],
            open_threads: None,
            ai_suggestions_pending: None,
        },
    ];
    write::execute(root.path(), &operations, ApplyMode::Commit).unwrap();
    let updated =
        std::fs::read_to_string(root.path().join("logic/solution/architecture.md")).unwrap();
    assert!(updated.ends_with("## Untouched\nKeep exact.\n"));
    let range = write::documents::body_range(&updated, &["Component".into()]).unwrap();
    let session = source::YamlDocument::parse(
        &std::fs::read_to_string(root.path().join("trace/sessions/2026-10-01_001.yaml")).unwrap(),
    )
    .unwrap()
    .root
    .to_json()
    .unwrap();
    let record = &session["logic_revisions"][0];
    assert_eq!(record["before"], before);
    assert_eq!(record["after"], updated[range].to_owned());
    assert_eq!(record["field"], "Body");
    assert_eq!(record["turn"], 1);
    assert!(
        record["after"]
            .as_str()
            .unwrap()
            .contains("2026-10-01 (2026-10-01_001#1)")
    );
}

#[test]
fn missing_only_initialization_retains_real_sources_and_requires_caller_match() {
    let paper = "---\ntitle: Actual research\nprivate: preserved\n---\n# Actual research\n";
    let claims =
        "# Claims\n\n## C01: Real work\n- **Statement**: Preserve actual accumulated content.\n";
    let (root, mut candidate) = make_working(&[("PAPER.md", paper), ("logic/claims.md", claims)]);
    std::fs::write(root.path().join("caller.txt"), "Keep unrelated content").unwrap();
    write::plan_operation(
        &mut candidate,
        &WriteOperation::ArtifactInit {
            profile: "research-manager".into(),
            paper: Some(paper.into()),
            documents: BTreeMap::new(),
            missing_only: true,
        },
    )
    .unwrap();
    assert_eq!(candidate.text("logic/claims.md").unwrap(), claims);
    assert_eq!(candidate.text("PAPER.md").unwrap(), paper);
    assert_eq!(
        candidate.text("staging/observations.yaml").unwrap(),
        "observations: []\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("caller.txt")).unwrap(),
        "Keep unrelated content"
    );
    let mut conflicting = WorkingArtifact::new(write::ArtifactSnapshot::load(root.path()).unwrap());
    assert!(
        write::plan_operation(
            &mut conflicting,
            &WriteOperation::ArtifactInit {
                profile: "research-manager".into(),
                paper: Some("# Different caller paper\n".into()),
                documents: BTreeMap::new(),
                missing_only: true
            }
        )
        .is_err()
    );
    assert!(conflicting.changed_paths().is_empty());
}

#[test]
fn merged_claim_redirect_is_revision_owned_typed_and_cycle_checked() {
    let (_root, mut candidate) = make_working(&[("logic/experiments.md", "## E01: Experiment\n")]);
    add_claim(&mut candidate, "C01", "Older conclusion");
    add_claim(&mut candidate, "C02", "Newer conclusion");
    assert!(
        write::plan_operation(
            &mut candidate,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Merged into":"C02"}))
            }
        )
        .is_err()
    );
    write::plan_operation(
        &mut candidate,
        &WriteOperation::LogicRevise {
            target: selector("C01"),
            set: fields(json!({"Merged into":"C02","Status":"withdrawn"})),
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: "dependency-change".into(),
            provenance: "user-revised".into(),
            note: None,
            expected: None,
            rewrite_references: false,
            references: vec![],
            action: None,
            split_into: vec![],
        },
    )
    .unwrap();
    let record = candidate
        .revisions
        .iter()
        .find(|r| r.record["field"] == "Merged into")
        .unwrap();
    assert_eq!(record.record["before"], Value::Null);
    assert_eq!(record.record["after"], "C02");
    write::logic::validate_references(&candidate).unwrap();
    write::plan_operation(
        &mut candidate,
        &WriteOperation::LogicRevise {
            target: selector("C02"),
            set: fields(json!({"Merged into":"C01"})),
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: "dependency-change".into(),
            provenance: "user-revised".into(),
            note: None,
            expected: None,
            rewrite_references: false,
            references: vec![],
            action: None,
            split_into: vec![],
        },
    )
    .unwrap();
    assert!(write::logic::validate_references(&candidate).is_err());
}

#[test]
fn referenced_removal_requires_live_redirect_and_unreferenced_removal_archives_body() {
    let concepts = "# Concepts\n\n## Old term\n- **Definition**: Caller concept\n\n## Replacement\n- **Definition**: Caller replacement\n";
    let history = "entries:\n  - note: Old term was discussed\n";
    let (_root, mut candidate) = make_working(&[
        ("logic/concepts.md", concepts),
        ("trace/pm_reasoning_log.yaml", history),
    ]);
    let target = EntrySelector::Document {
        document: "logic/concepts.md".into(),
        heading: vec!["Old term".into()],
        entry: None,
    };
    let selected = write::logic::resolve(&candidate, &target).unwrap();
    let expected = source::digest(concepts[selected.range].as_bytes());
    let operation = WriteOperation::EntryRemove {
        target: target.clone(),
        expected: expected.clone(),
        references: vec![],
        session: Some("2026-10-01_001".into()),
        turn: Some(1),
        signal: Some("user-directive".into()),
        provenance: Some("user-revised".into()),
        redirect: None,
        rewrite_references: false,
    };
    assert!(write::plan_operation(&mut candidate, &operation).is_err());
    assert_eq!(candidate.text("logic/concepts.md").unwrap(), concepts);
    let mut operation = operation;
    if let WriteOperation::EntryRemove { redirect, .. } = &mut operation {
        *redirect = Some(EntrySelector::Document {
            document: "logic/concepts.md".into(),
            heading: vec!["Replacement".into()],
            entry: None,
        });
    }
    write::plan_operation(&mut candidate, &operation).unwrap();
    assert_eq!(
        candidate.text("trace/pm_reasoning_log.yaml").unwrap(),
        history
    );
    let records = candidate.yaml("trace/logic_mutations.yaml").unwrap();
    let row = records
        .root
        .get("mutations")
        .unwrap()
        .unwrap()
        .sequence()
        .unwrap()[0]
        .to_json()
        .unwrap();
    assert_eq!(
        row["to_selector"],
        json!({"document":"logic/concepts.md","heading":["Concepts","Replacement"],"entry":null})
    );
    assert!(row["before"].as_str().unwrap().contains("Caller concept"));
    let (_root, mut candidate) = make_working(&[("logic/concepts.md", concepts)]);
    write::plan_operation(
        &mut candidate,
        &WriteOperation::EntryRemove {
            target,
            expected,
            references: vec![],
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: Some("user-directive".into()),
            provenance: Some("user-revised".into()),
            redirect: None,
            rewrite_references: false,
        },
    )
    .unwrap();
    let records = candidate.yaml("trace/logic_mutations.yaml").unwrap();
    let row = records
        .root
        .get("mutations")
        .unwrap()
        .unwrap()
        .sequence()
        .unwrap()[0]
        .to_json()
        .unwrap();
    assert_eq!(row["to"], Value::Null);
    assert!(row["before"].as_str().unwrap().contains("Caller concept"));
}

#[test]
fn annotation_payload_cannot_close_comments_and_roundtrips_unicode_and_newlines() {
    let (_root, mut candidate) = make_working(&[("logic/experiments.md", "## E01: Experiment\n")]);
    add_claim(&mut candidate, "C01", "Caller statement");
    add_claim(&mut candidate, "C02", "Other statement");
    let comment = "雪 -->\n## injected heading\n<!-- embedded opener -- and final\n";
    write::plan_operation(
        &mut candidate,
        &WriteOperation::EntryAnnotate {
            target: selector("C01"),
            kind: "conflict".into(),
            references: vec!["C02".into()],
            comment: comment.into(),
        },
    )
    .unwrap();
    let source = candidate.text("logic/claims.md").unwrap();
    let technical = source
        .lines()
        .find_map(|line| {
            line.strip_prefix("<!-- ARA annotation: ")
                .and_then(|line| line.strip_suffix(" -->"))
        })
        .unwrap();
    assert!(!technical.contains("--"));
    let record: Value = serde_json::from_str(technical).unwrap();
    assert_eq!(
        record,
        json!({"kind":"conflict","references":["C02"],"comment":comment})
    );
    assert_eq!(
        markdown::headings(source)
            .iter()
            .filter(|h| h.heading == "injected heading")
            .count(),
        0
    );
    write::logic::validate_references(&candidate).unwrap();
}

#[test]
fn taste_appends_one_native_subsection_without_rewriting_prior_reactions() {
    let existing = "## C01: Claim\n- **Statement**: Keep exact\n- **Taste**:\n  - [2026-09-30] `endorse` on `claim` — Prior confirmed reaction.\n";
    let (_root, mut candidate) = make_working(&[("logic/claims.md", existing)]);
    write::plan_operation(&mut candidate,&WriteOperation::EntryTasteAppend{target:selector("C01"),record:json!({"date":"2026-10-01","tag":"uncertain","object":"framing","comment":"New exact comment\n\n## not a heading\n"})}).unwrap();
    let updated = candidate.text("logic/claims.md").unwrap();
    assert!(updated.starts_with(existing));
    let entry = write::logic::resolve(&candidate, &selector("C01")).unwrap();
    let fields = markdown::fields(updated, entry.body);
    assert_eq!(
        fields.iter().filter(|field| field.name == "Taste").count(),
        1
    );
    assert_eq!(
        markdown::decode_field(fields.iter().find(|field| field.name == "Taste").unwrap()),
        "- [2026-09-30] `endorse` on `claim` — Prior confirmed reaction.\n- [2026-10-01] `uncertain` on `framing` — New exact comment\n  \n  ## not a heading\n  "
    );
}

#[test]
fn paper_audit_captures_the_entire_exact_document_and_skips_noops() {
    let original =
        "---\ntitle: Original\nunknown: {custom: preserved}\n---\n# Research\nCaller prose.\n";
    let (_root, mut candidate) = make_working(&[("PAPER.md", original)]);
    let operation = WriteOperation::PaperEdit {
        frontmatter: fields(json!({"title":"Changed"})),
        heading: vec![],
        expected: None,
        content: None,
        audit: Some(write::RevisionContext {
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: "artifact-commitment".into(),
            provenance: "user-revised".into(),
            note: Some("Caller metadata update".into()),
        }),
    };
    write::plan_operation(&mut candidate, &operation).unwrap();
    assert_eq!(candidate.revisions[0].record["entry"], "PAPER.md");
    assert_eq!(candidate.revisions[0].record["before"], original);
    assert_eq!(
        candidate.revisions[0].record["after"],
        candidate.text("PAPER.md").unwrap()
    );
    assert!(
        write::plan_operation(&mut candidate, &operation)
            .unwrap()
            .no_op
    );
    assert_eq!(candidate.revisions.len(), 1);
}

#[test]
fn authored_vocabularies_are_strict_without_rewriting_legacy_source() {
    let original = "## C01: Legacy\n- **Statement**: Before\n- **Status**: reviewed\n- **Provenance**: older-native-dialect\n";
    let (_root, mut candidate) = make_working(&[("logic/claims.md", original)]);
    for set in [
        json!({"Status":"supportedd"}),
        json!({"Status":"reviewed"}),
        json!({"Status":"revised"}),
        json!({"Provenance":"paper"}),
    ] {
        let error = write::plan_operation(
            &mut candidate,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(set),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "write.field_value");
        assert_eq!(candidate.text("logic/claims.md").unwrap(), original);
    }
    write::plan_operation(
        &mut candidate,
        &WriteOperation::EntryEdit {
            target: selector("C01"),
            set: fields(json!({"Statement":"After"})),
        },
    )
    .unwrap();
    assert_eq!(
        write::logic::field_value(&candidate, &selector("C01"), "Status").unwrap(),
        "reviewed"
    );
    assert_eq!(
        write::logic::field_value(&candidate, &selector("C01"), "Provenance").unwrap(),
        "older-native-dialect"
    );
    for status in [
        "hypothesis",
        "untested",
        "testing",
        "supported",
        "weakened",
        "refuted",
        "withdrawn",
    ] {
        write::plan_operation(
            &mut candidate,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Status":status})),
            },
        )
        .unwrap();
        assert_eq!(
            write::logic::field_value(&candidate, &selector("C01"), "Status").unwrap(),
            status
        );
    }
    for provenance in ["user", "ai-suggested", "ai-executed", "user-revised"] {
        write::plan_operation(
            &mut candidate,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Provenance":provenance})),
            },
        )
        .unwrap();
        assert_eq!(
            write::logic::field_value(&candidate, &selector("C01"), "Provenance").unwrap(),
            provenance
        );
    }
    write::plan_operation(&mut candidate,&WriteOperation::HeuristicAdd{id:Some("H01".into()),title:"Technique".into(),fields:fields(json!({"Rationale":"Caller reason","Status":"active","Provenance":"user","Sensitivity":"Not specified in paper","Code ref":"src/run.rs"}))}).unwrap();
    for set in [
        json!({"Status":"supported"}),
        json!({"Sensitivity":"moderate"}),
    ] {
        assert_eq!(
            write::plan_operation(
                &mut candidate,
                &WriteOperation::EntryEdit {
                    target: selector("H01"),
                    set: fields(set)
                }
            )
            .unwrap_err()
            .code,
            "write.field_value"
        );
    }
    for status in ["active", "weakened", "retired"] {
        write::plan_operation(
            &mut candidate,
            &WriteOperation::EntryEdit {
                target: selector("H01"),
                set: fields(json!({"Status":status})),
            },
        )
        .unwrap();
        assert_eq!(
            write::logic::field_value(&candidate, &selector("H01"), "Status").unwrap(),
            status
        );
    }
}

#[test]
fn bounded_paper_bodies_never_select_frontmatter_comment_headings() {
    let original = "---\ntitle: Caller title\n## Body\nunknown: Caller metadata\n---\n# Research\n## Body\nCaller body.\n";
    let (_root, mut candidate) = make_working(&[("PAPER.md", original)]);
    write::plan_operation(
        &mut candidate,
        &WriteOperation::PaperEdit {
            frontmatter: Fields::new(),
            heading: vec!["Body".into()],
            expected: Some(source::digest(b"Caller body.\n")),
            content: Some("Changed body.\n".into()),
            audit: None,
        },
    )
    .unwrap();
    assert_eq!(
        candidate.text("PAPER.md").unwrap(),
        "---\ntitle: Caller title\n## Body\nunknown: Caller metadata\n---\n# Research\n## Body\nChanged body.\n"
    );
    let metadata_only = "---\ntitle: Caller title\n## Hidden\nunknown: Caller metadata\n---\n# Research\nCaller body.\n";
    let (_root, mut candidate) = make_working(&[("PAPER.md", metadata_only)]);
    let error = write::plan_operation(
        &mut candidate,
        &WriteOperation::PaperEdit {
            frontmatter: Fields::new(),
            heading: vec!["Hidden".into()],
            expected: Some(source::digest(b"unknown: Caller metadata\n---\n")),
            content: Some("Forbidden metadata replacement.\n".into()),
            audit: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "write.selector");
    assert_eq!(candidate.text("PAPER.md").unwrap(), metadata_only);
}

#[test]
fn unknown_yaml_and_registered_markdown_cannot_fabricate_native_claim_ids() {
    let paper = "---\ntitle: Caller knowledge\nknowledge_paths: [notes/context.md]\n---\n# Caller knowledge\n";
    let tree = "tree: []\nprivate: {id: C99}\n";
    let notes =
        "## C98: This is not a native claim\n- **Dependencies**: ordinary untyped package notes\n";
    let (_root, mut candidate) = make_working(&[
        ("PAPER.md", paper),
        ("trace/exploration_tree.yaml", tree),
        ("logic/experiments.md", "## E01: Native plan\n"),
        ("notes/context.md", notes),
    ]);
    assert!(write::logic::resolve(&candidate, &selector("C98")).is_err());
    add_claim(&mut candidate, "C01", "Caller statement");
    write::logic::validate_references(&candidate).unwrap();
    for target in ["C99", "C98"] {
        write::plan_operation(
            &mut candidate,
            &WriteOperation::EntryEdit {
                target: selector("C01"),
                set: fields(json!({"Dependencies":[target]})),
            },
        )
        .unwrap();
        assert!(write::logic::validate_references(&candidate).is_err());
    }
    assert_eq!(candidate.text("notes/context.md").unwrap(), notes);
    assert_eq!(candidate.text("trace/exploration_tree.yaml").unwrap(), tree);
}

#[test]
fn missing_only_initialization_commits_missing_directories_without_source_changes() {
    let paper = "---\ntitle: Existing caller research\n---\n# Existing caller research\n";
    let seeds = [
        ("PAPER.md", paper),
        (".gitignore", ".ara/\n"),
        ("trace/exploration_tree.yaml", "tree: []\n"),
        ("trace/sessions/session_index.yaml", "sessions: []\n"),
        ("trace/pm_reasoning_log.yaml", "entries: []\n"),
        ("staging/observations.yaml", "observations: []\n"),
        ("logic/claims.md", "# Claims\n"),
        ("logic/problem.md", "# Problem\n"),
        ("logic/solution/heuristics.md", "# Heuristics\n"),
        ("evidence/README.md", "# Evidence Index\n"),
    ];
    let (root, _candidate) = make_working(&seeds);
    let operation = WriteOperation::ArtifactInit {
        profile: "research-manager".into(),
        paper: Some(paper.into()),
        documents: BTreeMap::new(),
        missing_only: true,
    };
    let preview = write::execute(
        root.path(),
        std::slice::from_ref(&operation),
        ApplyMode::DryRun,
    )
    .unwrap();
    assert!(!preview.committed);
    assert!(preview.changed_paths.is_empty());
    let expected_directories = ["src", "evidence/tables", "evidence/figures"]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        preview
            .created_directories
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        expected_directories
    );
    assert!(!root.path().join(".ara").exists());
    for directory in ["src", "evidence/tables", "evidence/figures"] {
        assert!(!root.path().join(directory).exists());
    }
    let committed = write::execute(root.path(), &[operation], ApplyMode::Commit).unwrap();
    assert!(committed.committed);
    assert!(committed.changed_paths.is_empty());
    assert_eq!(
        committed
            .created_directories
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        expected_directories
    );
    for directory in source::INIT_DIRECTORIES {
        assert!(root.path().join(directory).is_dir());
    }
    for (path, expected) in seeds {
        assert_eq!(
            std::fs::read_to_string(root.path().join(path)).unwrap(),
            expected
        );
    }
}

#[test]
fn unregistering_knowledge_rejects_structural_history_targets_without_rewriting_history() {
    let paper = "---\ntitle: Caller knowledge\nknowledge_paths: [notes/context.md]\n---\n# Caller knowledge\n";
    let notes = "# Context\n## Section\nExact caller body.\n";
    let cases = [
        (
            "trace/sessions/2026-10-01_001.yaml",
            "logic_revisions:\n  - entry: {document: notes/context.md, heading: [Section]}\n    field: Body\n    before: Caller before\n    after: Caller after\n",
        ),
        (
            "trace/sessions/2026-10-01_001.yaml",
            "events_logged:\n  - id: notes/context.md#Section\n    summary: Caller event\n",
        ),
        (
            "staging/observations.yaml",
            "observations:\n  - id: O01\n    promoted: true\n    promoted_to: notes/context.md#Section\n",
        ),
        (
            "trace/aliases.yaml",
            "format: ara.aliases/v1\naliases:\n  - source_key: caller-source\n    label: imported\n    original: notes/context.md:Section\n    target: notes/context.md:Section\n    revision: caller-revision\n",
        ),
        (
            "trace/logic_mutations.yaml",
            "mutations:\n  - action: rename\n    from: logic/concepts.md:Previous\n    to: notes/context.md:Section\n    before: Caller before\n    after: Caller after\n",
        ),
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: thought\n    concepts: [notes/context.md#Section]\n",
        ),
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: thought\n    source_refs: [notes/context.md]\n",
        ),
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: thought\n    artifacts:\n      - name: Caller source\n        pointer: notes/context.md#Section\n        what: Caller description\n",
        ),
        (
            "trace/taste_log.yaml",
            "entries:\n  - id: T01\n    target: notes/context.md#Section\n",
        ),
        (
            "trace/aliases.yaml",
            "format: ara.aliases/v1\nsaved: &reference notes/context.md:Section\naliases:\n  - source_key: caller-source\n    label: imported\n    original: notes/context.md:Section\n    target: *reference\n    revision: caller-revision\n",
        ),
        (
            "trace/sessions/2026-10-01_001.yaml",
            "saved: &revisions\n  - entry: notes/context.md\n    field: Body\n    before: Before\n    after: After\nlogic_revisions: *revisions\n",
        ),
    ];
    for (path, history) in cases {
        let (_root, mut candidate) = make_working(&[
            ("PAPER.md", paper),
            ("notes/context.md", notes),
            (path, history),
        ]);
        write::logic::validate_references(&candidate).unwrap();
        write::plan_operation(
            &mut candidate,
            &WriteOperation::PaperEdit {
                frontmatter: fields(json!({"knowledge_paths":[]})),
                heading: vec![],
                expected: None,
                content: None,
                audit: None,
            },
        )
        .unwrap();
        assert_eq!(
            write::logic::validate_references(&candidate)
                .unwrap_err()
                .code,
            "write.registration_reference",
            "{path}"
        );
        assert_eq!(candidate.text(path).unwrap(), history);
        assert_eq!(candidate.text("notes/context.md").unwrap(), notes);
    }
}

#[test]
fn registry_removal_ignores_prose_physical_history_and_retired_redirect_sources() {
    let paper = "---\ntitle: Caller knowledge\nknowledge_paths: [notes/context.md]\n---\n# Caller knowledge\n";
    let notes = "# Context\n## Section\nCaller body.\n";
    let session = "events_logged:\n  - id: N01\n    summary: notes/context.md#Section is historical prose\nai_actions:\n  - files_changed: [notes/context.md]\nlogic_revisions:\n  - entry: N01\n    before: notes/context.md#Section\n    after: notes/context.md#Section\n";
    let mutations = "mutations:\n  - action: remove\n    from: notes/context.md:Section\n    to: null\n    before: notes/context.md#Section\n    after: ''\n";
    let tree = "tree:\n  - id: N01\n    type: thought\n    description: notes/context.md#Section\n    private: {target: notes/context.md#Section}\nprivate: {target: notes/context.md#Section}\n";
    let aliases = "format: ara.aliases/v1\naliases:\n  - source_key: caller-source\n    label: imported\n    original: notes/context.md:Section\n    target: logic/concepts.md:Stable\n    revision: caller-revision\n";
    let (_root, mut candidate) = make_working(&[
        ("PAPER.md", paper),
        ("notes/context.md", notes),
        ("trace/sessions/2026-10-01_001.yaml", session),
        ("trace/logic_mutations.yaml", mutations),
        ("trace/exploration_tree.yaml", tree),
        ("trace/aliases.yaml", aliases),
        ("logic/concepts.md", "## Stable\nCaller concept.\n"),
    ]);
    write::plan_operation(
        &mut candidate,
        &WriteOperation::PaperEdit {
            frontmatter: fields(json!({"knowledge_paths":[]})),
            heading: vec![],
            expected: None,
            content: None,
            audit: None,
        },
    )
    .unwrap();
    write::logic::validate_references(&candidate).unwrap();
    for (path, expected) in [
        ("notes/context.md", notes),
        ("trace/sessions/2026-10-01_001.yaml", session),
        ("trace/logic_mutations.yaml", mutations),
        ("trace/exploration_tree.yaml", tree),
        ("trace/aliases.yaml", aliases),
    ] {
        assert_eq!(candidate.text(path).unwrap(), expected);
    }
}

fn own_turn(working: &mut WorkingArtifact, session: &str, timestamp: &str) {
    if !working.exists(&format!("trace/sessions/{session}.yaml")) {
        write::plan_operation(working,&serde_json::from_value(json!({"op":"session.start","id":session,"date":&session[..10],"started":format!("{}T10:00",&session[..10]),"summary":"Synthetic audit fixture"})).unwrap()).unwrap();
    }
    write::plan_operation(
        working,
        &serde_json::from_value(
            json!({"op":"session.log","session":session,"timestamp":timestamp}),
        )
        .unwrap(),
    )
    .unwrap();
}
fn flush_revisions(working: &mut WorkingArtifact) {
    for revision in std::mem::take(&mut working.revisions) {
        write::sessions::append_revision(
            working,
            &revision.session,
            revision.turn,
            &revision.record,
        )
        .unwrap();
    }
}
fn rename(working: &mut WorkingArtifact, document: &str, heading: &[&str], name: &str, turn: u64) {
    let target = EntrySelector::Document {
        document: document.into(),
        heading: heading.iter().map(|part| part.to_string()).collect(),
        entry: None,
    };
    let selected = write::logic::resolve(working, &target).unwrap();
    let expected = source::digest(working.text(document).unwrap()[selected.range].as_bytes());
    write::plan_operation(
        working,
        &WriteOperation::EntryRename {
            target,
            name: name.into(),
            expected,
            references: vec![],
            session: Some("2026-10-01_001".into()),
            turn: Some(turn),
            signal: Some("user-directive".into()),
            provenance: Some("user".into()),
            rewrite_references: false,
        },
    )
    .unwrap();
    flush_revisions(working);
}

#[test]
fn rename_and_retired_identity_checks_use_literal_heading_vectors() {
    let concepts = "# Concepts\n\n## A/B\n- **Definition**: Literal slash\n\n## A\n### X\n- **Definition**: Nested child\n\n## Topic # exact\n- **Definition**: Literal hash\n";
    let (_root, mut working) = make_working(&[
        ("logic/concepts.md", concepts),
        ("trace/exploration_tree.yaml", "tree: []\n"),
    ]);
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:01");
    rename(
        &mut working,
        "logic/concepts.md",
        &["Concepts", "A/B"],
        "Archived literal",
        1,
    );
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:02");
    rename(
        &mut working,
        "logic/concepts.md",
        &["Concepts", "A", "X"],
        "B",
        2,
    );
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
    let ledger = working
        .yaml("trace/logic_mutations.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    assert_eq!(
        ledger["mutations"][0]["from_selector"],
        json!({"document":"logic/concepts.md","heading":["Concepts","A/B"],"entry":null})
    );
    assert_eq!(
        ledger["mutations"][1]["to_selector"],
        json!({"document":"logic/concepts.md","heading":["Concepts","A","B"],"entry":null})
    );
    let literal = EntrySelector::Document {
        document: "logic/concepts.md".into(),
        heading: vec!["Concepts".into(), "Archived literal".into()],
        entry: None,
    };
    let selected = write::logic::resolve(&working, &literal).unwrap();
    let expected =
        source::digest(working.text("logic/concepts.md").unwrap()[selected.range].as_bytes());
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryRename {
                target: literal,
                name: "A/B".into(),
                expected,
                references: vec![],
                session: Some("2026-10-01_001".into()),
                turn: Some(2),
                signal: Some("user-directive".into()),
                provenance: Some("user".into()),
                rewrite_references: false,
            }
        )
        .is_err()
    );
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:03");
    rename(
        &mut working,
        "logic/concepts.md",
        &["Concepts", "Topic # exact"],
        "New # exact",
        3,
    );
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
    let target = EntrySelector::Document {
        document: "logic/concepts.md".into(),
        heading: vec!["Concepts".into(), "New # exact".into()],
        entry: None,
    };
    let selected = write::logic::resolve(&working, &target).unwrap();
    let expected =
        source::digest(working.text("logic/concepts.md").unwrap()[selected.range].as_bytes());
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::EntryRename {
                target,
                name: "Topic # exact".into(),
                expected,
                references: vec![],
                session: Some("2026-10-01_001".into()),
                turn: Some(3),
                signal: Some("user-directive".into()),
                provenance: Some("user".into()),
                rewrite_references: false,
            }
        )
        .is_err()
    );
}

#[test]
fn rename_destination_collisions_are_scoped_to_exact_document_paths() {
    let (_root, mut working) = make_working(&[
        (
            "logic/concepts.md",
            "# Concepts\n\n## Left\n### Old\n- **Definition**: Left body\n\n## Right\n### Shared\n- **Definition**: Right body\n",
        ),
        (
            "logic/solution/method.md",
            "# Method\n## Shared\nOther document.\n",
        ),
    ]);
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:01");
    rename(
        &mut working,
        "logic/concepts.md",
        &["Concepts", "Left", "Old"],
        "Shared",
        1,
    );
    assert_eq!(
        write::logic::field_value(
            &working,
            &EntrySelector::Document {
                document: "logic/concepts.md".into(),
                heading: vec!["Left".into(), "Shared".into()],
                entry: None
            },
            "Definition"
        )
        .unwrap(),
        "Left body"
    );
    assert!(
        write::logic::resolve(
            &working,
            &EntrySelector::Document {
                document: "logic/concepts.md".into(),
                heading: vec!["Shared".into()],
                entry: None
            }
        )
        .is_err()
    );
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
}

#[test]
fn generic_claim_body_and_document_replacement_cannot_remove_canonical_entries() {
    let source = "# Claims\n\n## C01: Historical claim\n- **Statement**: Keep this claim\n\n## C02: Nested container\n### C03: Nested claim\n- **Statement**: Keep nested claim\n";
    let (_root, mut working) = make_working(&[("logic/claims.md", source)]);
    let operation = WriteOperation::DocumentReplace {
        document: "logic/claims.md".into(),
        heading: vec![],
        expected: source::digest(source.as_bytes()),
        content: "# Claims\n".into(),
    };
    assert!(write::plan_operation(&mut working, &operation).is_err());
    assert_eq!(working.text("logic/claims.md").unwrap(), source);
    let target = EntrySelector::Document {
        document: "logic/claims.md".into(),
        heading: vec![],
        entry: Some("C02".into()),
    };
    let selected = write::logic::resolve(&working, &target).unwrap();
    let expected = source::digest(source[selected.body].as_bytes());
    let operation = WriteOperation::LogicRevise {
        target,
        set: fields(json!({"Body":"Replaced container body\n"})),
        session: Some("2026-10-01_001".into()),
        turn: Some(1),
        signal: "user-directive".into(),
        provenance: "user".into(),
        note: None,
        expected: Some(expected),
        rewrite_references: false,
        references: vec![],
        action: None,
        split_into: vec![],
    };
    assert!(write::plan_operation(&mut working, &operation).is_err());
    assert_eq!(working.text("logic/claims.md").unwrap(), source);
    // Final validation also protects direct low-level document staging.
    working
        .replace_document("logic/claims.md", "# Claims\n", "generic body staging")
        .unwrap();
    assert_eq!(
        write::logic::validate_references(&working)
            .unwrap_err()
            .code,
        "write.claim_retention"
    );
}

#[test]
fn canonical_rename_is_owned_and_archived_while_withdrawal_and_merge_retain_claims() {
    let source = "# Claims\n\n## C01: Original\n- **Statement**: Exact old statement\n- **Status**: hypothesis\n\n## C02: Destination\n- **Statement**: New conclusion\n- **Status**: hypothesis\n";
    let (_root, mut working) = make_working(&[
        ("logic/claims.md", source),
        ("trace/exploration_tree.yaml", "tree: []\n"),
    ]);
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:01");
    rename(
        &mut working,
        "logic/claims.md",
        &["Claims", "C01: Original"],
        "C03",
        1,
    );
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
    assert_eq!(
        write::logic::claim_redirects_from_source(&working).unwrap()["C01"],
        "C03"
    );
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:02");
    write::plan_operation(
        &mut working,
        &WriteOperation::LogicRevise {
            target: selector("C03"),
            set: fields(json!({"Status":"withdrawn","Merged into":"C02"})),
            session: Some("2026-10-01_001".into()),
            turn: Some(2),
            signal: "user-directive".into(),
            provenance: "user".into(),
            note: None,
            expected: None,
            rewrite_references: false,
            references: vec![],
            action: None,
            split_into: vec![],
        },
    )
    .unwrap();
    flush_revisions(&mut working);
    write::logic::validate_references(&working).unwrap();
    assert!(write::logic::resolve(&working, &selector("C03")).is_ok());
    assert!(write::logic::resolve(&working, &selector("C02")).is_ok());
    assert_eq!(
        write::logic::field_value(&working, &selector("C03"), "Status").unwrap(),
        "withdrawn"
    );
}

#[test]
fn compiler_heuristic_source_bounds_and_code_refs_need_no_fake_pm_status() {
    for sources in [
        json!("Exact source, with commas"),
        json!(["First source", "Second source"]),
    ] {
        for code in [
            json!("src/solver.py:12"),
            json!(["src/solver.py:12", "src/config.yaml"]),
        ] {
            let (_root, mut working) = make_working(&[]);
            let supplied = fields(
                json!({"Rationale":"Synthetic compiler rationale","Source":sources,"Sensitivity":"Not specified in paper","Code ref":code,"Bounds":"Exact compiler bound"}),
            );
            write::plan_operation(
                &mut working,
                &WriteOperation::HeuristicAdd {
                    id: Some("H01".into()),
                    title: "Synthetic compiler heuristic".into(),
                    fields: supplied.clone(),
                },
            )
            .unwrap();
            for (key, value) in supplied {
                assert_eq!(
                    write::logic::field_value(&working, &selector("H01"), &key).unwrap(),
                    write::fields::value_text(&value)
                );
            }
            let text = working.text("logic/solution/heuristics.md").unwrap();
            assert!(!text.contains("**Status**"));
            assert!(!text.contains("**Provenance**"));
        }
    }
}

#[test]
fn historical_null_claim_tombstones_remain_readable_and_reserve_their_identity() {
    let before = "## C01: Retired historically\n- **Statement**: Original historical claim\n";
    let revision = json!({"turn":1,"entry":"C01","field":"entry","before":before,"after":"","signal":"user-directive","provenance":"user"});
    let session = source::render_yaml(
        &json!({"session":{"id":"2026-09-30_001","turn_count":1},"logic_revisions":[revision]}),
        0,
        "\n",
    );
    let ledger = source::render_yaml(
        &json!({"mutations":[{"from":"logic/claims.md:C01","to":null,"action":"remove","before":before,"after":"","session":"2026-09-30_001","turn":1,"signal":"user-directive","provenance":"user"}]}),
        0,
        "\n",
    );
    let (_root, mut working) = make_working(&[
        (
            "logic/claims.md",
            "# Claims\n\n## C02: Live\n- **Statement**: Present claim\n",
        ),
        ("trace/logic_mutations.yaml", &ledger),
        ("trace/sessions/2026-09-30_001.yaml", &session),
        ("logic/experiments.md", "## E01: Synthetic proof fixture\n"),
    ]);
    assert!(
        write::logic::claim_redirects_from_source(&working)
            .unwrap()
            .is_empty()
    );
    assert!(
        write::plan_operation(
            &mut working,
            &WriteOperation::ClaimAdd {
                id: Some("C01".into()),
                title: "Reused identity".into(),
                fields: claim_fields("Forbidden reuse")
            }
        )
        .is_err()
    );
    let result = write::plan_operation(
        &mut working,
        &WriteOperation::ClaimAdd {
            id: None,
            title: "New identity".into(),
            fields: claim_fields("Synthetic new claim"),
        },
    )
    .unwrap();
    assert_eq!(result.id.as_deref(), Some("C03"));
    write::logic::validate_references(&working).unwrap();
    assert_eq!(working.text("trace/logic_mutations.yaml").unwrap(), ledger);
    assert_eq!(
        working.text("trace/sessions/2026-09-30_001.yaml").unwrap(),
        session
    );
}

#[test]
fn new_node_concept_reference_can_follow_only_an_authenticated_rename() {
    let concepts = "# Concepts\n\n## Original concept\n- **Definition**: Synthetic definition\n";
    let (_root, mut working) = make_working(&[
        ("logic/concepts.md", concepts),
        ("trace/exploration_tree.yaml", "tree: []\n"),
    ]);
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:01");
    rename(
        &mut working,
        "logic/concepts.md",
        &["Concepts", "Original concept"],
        "Current concept",
        1,
    );
    write::plan_operation(
        &mut working,
        &WriteOperation::NodeAdd {
            id: None,
            kind: "question".into(),
            parent: "root".into(),
            title: "Synthetic concept use".into(),
            fields: fields(
                json!({"description":"Synthetic question","concepts":["Original concept"]}),
            ),
            depends_on: vec![],
        },
    )
    .unwrap();
    write::node::validate_references(&working).unwrap();
    let path = "trace/sessions/2026-10-01_001.yaml";
    working
        .replace_yaml_field(
            path,
            &[
                "logic_revisions".into(),
                write::positions::PathPart::Index(0),
            ],
            "before",
            &json!("Forged archive"),
        )
        .unwrap();
    assert!(write::node::validate_references(&working).is_err());
}

#[test]
fn ancestor_rename_archives_colliding_display_paths_with_exact_per_entry_owner_proof() {
    let original = "# Architecture\n\n## Parent\nContainer body.\n### A/B\nLiteral child body.\n### A\nNested parent body.\n#### B\nNested child body.\n";
    let document = "logic/solution/architecture.md";
    let (_root, mut working) = make_working(&[(document, original)]);
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:01");
    rename(
        &mut working,
        document,
        &["Architecture", "Parent"],
        "Renamed parent",
        1,
    );
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
    let ledger = working
        .yaml("trace/logic_mutations.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    let rows = ledger["mutations"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    let owner = working
        .yaml("trace/sessions/2026-10-01_001.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    let revisions = owner["logic_revisions"].as_array().unwrap();
    let headings = markdown::headings(original);
    for row in rows {
        let from: EntrySelector = serde_json::from_value(row["from_selector"].clone()).unwrap();
        let EntrySelector::Document { heading, .. } = &from else {
            panic!("native heading selector required")
        };
        let original_heading = headings
            .iter()
            .find(|source| {
                source
                    .path
                    .iter()
                    .copied()
                    .eq(heading.iter().map(String::as_str))
            })
            .unwrap();
        assert_eq!(row["before"], original[original_heading.range.clone()]);
        let to: EntrySelector = serde_json::from_value(row["to_selector"].clone()).unwrap();
        let current = write::logic::resolve(&working, &to).unwrap();
        assert_eq!(row["after"], working.text(document).unwrap()[current.range]);
        assert!(
            revisions
                .iter()
                .any(|revision| revision["before"] == row["before"]
                    && revision["after"] == row["after"]
                    && revision["signal"] == row["signal"]
                    && revision["provenance"] == row["provenance"]
                    && revision["turn"] == row["turn"])
        );
    }
    let colliding: Vec<_> = rows
        .iter()
        .filter(|row| row["from"] == "logic/solution/architecture.md:Architecture/Parent/A/B")
        .collect();
    assert_eq!(colliding.len(), 2);
    assert_ne!(colliding[0]["from_selector"], colliding[1]["from_selector"]);
}

#[test]
fn cached_headings_cannot_authorize_revision_references_after_direct_source_mutation() {
    const CLAIMS: &str = "logic/claims.md";
    let original = "# Claims\n\n## C01: Claim\n- **Statement**: Original statement\n";
    let (_root, mut working) = make_working(&[(CLAIMS, original)]);
    let revision = json!({"entry":"C01","field":"Statement"});
    write::logic::validate_revision_entry(&working, "2026-10-01_001", 1, &revision).unwrap();
    working.base.files.get_mut(CLAIMS).unwrap().bytes = original.replace("C01", "C02").into_bytes();
    assert_eq!(
        write::logic::validate_revision_entry(&working, "2026-10-01_001", 1, &revision)
            .unwrap_err()
            .code,
        "write.reference"
    );
    working.base.files.get_mut(CLAIMS).unwrap().bytes = original.as_bytes().to_vec();
    write::logic::validate_revision_entry(&working, "2026-10-01_001", 1, &revision).unwrap();
    working
        .files
        .insert(CLAIMS.into(), original.replace("C01", "C03").into_bytes());
    assert_eq!(
        write::logic::validate_revision_entry(&working, "2026-10-01_001", 1, &revision)
            .unwrap_err()
            .code,
        "write.reference"
    );
}

#[test]
fn cached_claim_redirect_cannot_authorize_a_tampered_owning_revision() {
    let source = "# Claims\n\n## C01: Original\n- **Statement**: Exact old statement\n- **Status**: hypothesis\n";
    let (_root, mut working) = make_working(&[
        ("logic/claims.md", source),
        ("trace/exploration_tree.yaml", "tree: []\n"),
    ]);
    own_turn(&mut working, "2026-10-01_001", "2026-10-01T10:01");
    rename(
        &mut working,
        "logic/claims.md",
        &["Claims", "C01: Original"],
        "C02",
        1,
    );
    assert_eq!(
        write::logic::claim_redirects_from_source(&working).unwrap()["C01"],
        "C02"
    );
    const OWNER: &str = "trace/sessions/2026-10-01_001.yaml";
    let tampered = working
        .text(OWNER)
        .unwrap()
        .replace("Exact old statement", "Tampered statement");
    working.files.insert(OWNER.into(), tampered.into_bytes());
    let error = write::logic::claim_redirects_from_source(&working).unwrap_err();
    assert_eq!(error.code, "write.redirect");
    assert!(error.message.contains("exact owning revision"));
}

fn decoded_fields(text: &str, start: usize) -> Vec<(String, String)> {
    markdown::fields(text, start..text.len())
        .iter()
        .map(|f| (f.name.to_owned(), markdown::decode_field(f).into_owned()))
        .collect()
}

#[test]
fn created_claim_block_uses_fixed_schema_inline_values_and_preserves_prior_bytes() {
    // CRLF existing entries with an alias label and hand-written list styles.
    let original = "# Claims\r\n\r\n## C01: Prior\r\n- **Tags**: evaluation, experimental-design\r\n- **Falsification criteria**: Kept label\r\n- **Dependencies**: [C02]\r\n\r\n## C02: Other\r\n- **Statement**: Independent\r\n";
    let (_root, mut working) = make_working(&[("logic/claims.md", original)]);
    let statement = "Line one 雪\r\n\n  indented\n";
    let input = fields(json!({
        "tags": "evaluation, experimental-design",
        "Dependencies": ["C01", "C02"],
        "Evidence basis": "[]",
        "Proof": ["table one", "a, b", "", "none", "[x]"],
        "Falsification criteria": "F text",
        "provenance": "user",
        "Status": "supported",
        "Sources": ["paper §3"],
        "Conditions": "none",
        "Statement": statement,
    }));
    let result = write::plan_operation(
        &mut working,
        &WriteOperation::ClaimAdd {
            id: None,
            title: "Order probe".into(),
            fields: input,
        },
    )
    .unwrap();
    assert_eq!(result.id.as_deref(), Some("C03"));
    write::logic::validate_references(&working).unwrap();
    let text = working.text("logic/claims.md").unwrap();
    assert!(text.starts_with(original), "prior bytes changed");
    let block = &text[original.len()..];
    assert_eq!(
        block,
        // Separator, heading and inline lines follow the file's CRLF; the
        // continuation value keeps LF structure around exact caller bytes.
        "\r\n## C03: Order probe\r\n- **Statement**:\n  Line one 雪\r\n  \n    indented\n  \n\
         - **Conditions**: none\r\n- **Sources**: [\"paper §3\"]\r\n- **Status**: supported\r\n\
         - **Provenance**: user\r\n- **Falsification**: F text\r\n\
         - **Proof**: [\"table one\",\"a, b\",\"\",\"none\",\"[x]\"]\r\n\
         - **Evidence basis**: []\r\n- **Dependencies**: [C01, C02]\r\n\
         - **Tags**: evaluation, experimental-design\r\n"
    );
    let body = original.len() + block.find("- **").unwrap();
    let decoded = decoded_fields(text, body);
    let expected: Vec<(String, String)> = [
        ("Statement", statement),
        ("Conditions", "none"),
        ("Sources", "[\"paper §3\"]"),
        ("Status", "supported"),
        ("Provenance", "user"),
        ("Falsification", "F text"),
        ("Proof", "[\"table one\",\"a, b\",\"\",\"none\",\"[x]\"]"),
        ("Evidence basis", "[]"),
        ("Dependencies", "[C01, C02]"),
        ("Tags", "evaluation, experimental-design"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect();
    assert_eq!(decoded, expected);
    let heading = markdown::headings(text)
        .into_iter()
        .find(|h| h.heading.starts_with("C03"))
        .unwrap();
    assert_eq!(heading.heading, "C03: Order probe");
    assert_eq!(
        write::logic::field_value(&working, &selector("C03"), "Falsification criteria").unwrap(),
        "F text"
    );
    // The existing alias label is untouched by creating a neighbour.
    assert_eq!(
        write::logic::field_value(&working, &selector("C01"), "Falsification").unwrap(),
        "Kept label"
    );
}

#[test]
fn created_entries_reject_revision_fields_unknown_fields_and_new_dangling_dependencies() {
    let original = "# Claims\n\n## C01: Prior\n- **Statement**: Kept\n";
    for (extra, code) in [
        (
            json!({"Last revised": "2026-10-01"}),
            "write.revision_required",
        ),
        (json!({"Merged into": "C01"}), "write.revision_required"),
        (json!({"Unknown label": "x"}), "write.field"),
        (json!({"Dependencies": "C01"}), "write.field_type"),
        (json!({"Dependencies": ["N01"]}), "write.field_type"),
    ] {
        let (_root, mut working) = make_working(&[("logic/claims.md", original)]);
        let mut input = claim_fields("Statement");
        input.extend(fields(extra.clone()));
        let error = write::plan_operation(
            &mut working,
            &WriteOperation::ClaimAdd {
                id: None,
                title: "Rejected".into(),
                fields: input,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, code, "{extra}");
        assert_eq!(working.text("logic/claims.md").unwrap(), original);
    }
    let (_root, mut working) = make_working(&[("logic/claims.md", original)]);
    let mut input = claim_fields("Statement");
    input.insert("Dependencies".into(), json!(["C99"]));
    write::plan_operation(
        &mut working,
        &WriteOperation::ClaimAdd {
            id: None,
            title: "Dangling".into(),
            fields: input,
        },
    )
    .unwrap();
    let error = write::logic::validate_references(&working).unwrap_err();
    assert_eq!(error.code, "write.reference");
}

#[test]
fn created_compiler_heuristic_keeps_scalar_source_and_complete_bounds_in_schema_order() {
    let original = "# Heuristics\n\n## H01: Prior\n- **Rationale**: Kept\n";
    let (_root, mut working) = make_working(&[("logic/solution/heuristics.md", original)]);
    let bounds = "Only synthetic data.\n  exact bound = 雪\n";
    write::plan_operation(
        &mut working,
        &WriteOperation::HeuristicAdd {
            id: None,
            title: "Compiler heuristic".into(),
            fields: fields(json!({
                "Tags": "a, b",
                "Code ref": ["src/run.rs:12", "src/[x].rs"],
                "Bounds": bounds,
                "Sensitivity": "Not specified in paper",
                "Source": "paper.pdf p3 «a, b»",
                "Rationale": "Reason",
            })),
        },
    )
    .unwrap();
    write::logic::validate_references(&working).unwrap();
    let text = working.text("logic/solution/heuristics.md").unwrap();
    assert!(text.starts_with(original));
    assert_eq!(
        &text[original.len()..],
        "\n## H02: Compiler heuristic\n- **Rationale**: Reason\n\
         - **Source**: paper.pdf p3 «a, b»\n- **Sensitivity**: Not specified in paper\n\
         - **Bounds**:\n  Only synthetic data.\n    exact bound = 雪\n  \n\
         - **Code ref**: [\"src/run.rs:12\",\"src/[x].rs\"]\n- **Tags**: a, b\n"
    );
    assert_eq!(
        write::logic::field_value(&working, &selector("H02"), "Bounds").unwrap(),
        bounds
    );
}

#[test]
fn created_heuristics_reject_revision_fields_and_accept_underscore_aliases() {
    let original = "# Heuristics\n\n## H01: Prior\n- **Rationale**: Kept\n";
    let base = json!({"Rationale":"Reason","Sensitivity":"low","code_ref":"src/run.rs"});
    for (extra, code) in [
        (
            json!({"Last revised": "2026-10-01"}),
            "write.revision_required",
        ),
        (
            json!({"last_revised": "2026-10-01"}),
            "write.revision_required",
        ),
        (json!({"Evidence basis": "claims only"}), "write.field"),
    ] {
        let (_root, mut working) = make_working(&[("logic/solution/heuristics.md", original)]);
        let mut input = fields(base.clone());
        input.extend(fields(extra.clone()));
        let error = write::plan_operation(
            &mut working,
            &WriteOperation::HeuristicAdd {
                id: None,
                title: "Rejected".into(),
                fields: input,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, code, "{extra}");
        assert_eq!(
            working.text("logic/solution/heuristics.md").unwrap(),
            original
        );
    }
    let (_root, mut working) = make_working(&[("logic/solution/heuristics.md", original)]);
    write::plan_operation(
        &mut working,
        &WriteOperation::HeuristicAdd {
            id: None,
            title: "Aliased".into(),
            fields: fields(base),
        },
    )
    .unwrap();
    assert_eq!(
        &working.text("logic/solution/heuristics.md").unwrap()[original.len()..],
        "\n## H02: Aliased\n- **Rationale**: Reason\n- **Sensitivity**: low\n- **Code ref**: src/run.rs\n"
    );
}

#[test]
fn created_claims_accept_underscore_aliases_and_write_schema_labels() {
    let (_root, mut working) = make_working(&[]);
    let mut input = claim_fields("Statement");
    input.insert("evidence_basis".into(), json!("Table 2"));
    input.insert("falsification_criteria".into(), json!("Alias"));
    input.remove("Falsification criteria");
    write::plan_operation(
        &mut working,
        &WriteOperation::ClaimAdd {
            id: None,
            title: "Aliased claim".into(),
            fields: input,
        },
    )
    .unwrap();
    let text = working.text("logic/claims.md").unwrap();
    assert!(text.contains(
        "- **Falsification**: Alias\n- **Proof**: E01 and complete prose\n- **Evidence basis**: Table 2\n"
    ));
}
