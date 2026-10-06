//! Heading resolution tiers, canonical read addresses and read-facing errors.
use assert_cmd::Command;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use tempfile::TempDir;

const DOC: &str = "logic/solution/method.md";

fn put(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}
fn artifact(document: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    put(
        dir.path(),
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Root\n",
    );
    put(
        dir.path(),
        "logic/claims.md",
        "# Claims\n\n## C01: Mechanism\n- **Statement**: Known.\n- **Status**: hypothesis\n",
    );
    put(dir.path(), DOC, document);
    dir
}
fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR").arg("-C").arg(root);
    command
}
fn show(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .arg("show")
        .args(args)
        .args(["--full", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}
fn reject(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .arg("show")
        .args(args)
        .arg("--json")
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    value["error"].clone()
}
fn heading(root: &Path, parts: &[&str]) -> Value {
    let mut args = vec!["--document", DOC];
    for part in parts {
        args.extend(["--heading", *part]);
    }
    show(root, &args)["entries"][0].clone()
}
fn heading_error(root: &Path, parts: &[&str]) -> Value {
    let mut args = vec!["--document", DOC];
    for part in parts {
        args.extend(["--heading", *part]);
    }
    reject(root, &args)
}
fn digest(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}
fn candidates(error: &Value) -> Vec<&str> {
    error["details"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect()
}

#[test]
fn exact_full_vector_and_exact_suffix_win_before_tolerant_tiers() {
    let dir = artifact(
        "# Method\n## B\nsuffix\n## Results\nupper\n## results\nlower\n# B\ntop\n## Step 3: Train\ntrain\n",
    );
    // The full vector ['B'] names the top-level section, not the suffix match.
    assert_eq!(
        heading(dir.path(), &["B"])["content"],
        "top\n## Step 3: Train\ntrain\n"
    );
    assert_eq!(heading(dir.path(), &["Method", "B"])["content"], "suffix\n");
    // Exact case wins over the case-insensitive collision.
    assert_eq!(heading(dir.path(), &["results"])["content"], "lower\n");
    assert_eq!(heading(dir.path(), &["Results"])["content"], "upper\n");
    // Without an exact match, normalized equality collides and rejects.
    let error = heading_error(dir.path(), &["RESULTS"]);
    assert_eq!(error["code"], "ambiguous_heading");
    assert_eq!(
        candidates(&error),
        [
            "logic/solution/method.md#h/Method/Results",
            "logic/solution/method.md#h/Method/results"
        ]
    );
    assert_eq!(error["details"]["capped"], false);
    // Trimmed, case-insensitive unique prefix.
    let train = heading(dir.path(), &["  step 3 "]);
    assert_eq!(train["content"], "train\n");
    assert_eq!(train["digest"], digest("train\n"));
    assert_eq!(train["heading"], json!(["  step 3 "]));
    assert_eq!(train["heading_path"], json!(["B", "Step 3: Train"]));
    assert_eq!(
        train["address"],
        "logic/solution/method.md#h/B/Step%203%3A%20Train"
    );
    // Native-ID shorthand still selects claims.
    let claim = show(
        dir.path(),
        &["--document", "logic/claims.md", "--heading", "C01"],
    );
    assert!(
        claim["entries"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Known.")
    );
}

#[test]
fn trailing_ellipsis_matches_only_in_the_final_tier() {
    let dir = artifact(
        "# Method\n## Eval on long...\nlong\n## Results...\nliteral\n## Results on short inputs\nshort\n## ...\nbare\n",
    );
    assert_eq!(
        heading(dir.path(), &["Eval on long sequences"])["content"],
        "long\n"
    );
    // An exact heading containing `...` wins first.
    assert_eq!(heading(dir.path(), &["Results..."])["content"], "literal\n");
    // A unique prefix is a stronger tier than the ellipsis tier.
    assert_eq!(
        heading(dir.path(), &["results on short"])["content"],
        "short\n"
    );
    // A bare `...` has an empty prefix and never matches a longer request.
    assert_eq!(
        heading_error(dir.path(), &["Unrelated"])["code"],
        "unknown_id"
    );
}

#[test]
fn literal_slash_vectors_have_distinct_canonical_addresses_that_round_trip() {
    let dir = artifact("# Arch\n## A/B\nliteral\n## A\n### B\nnested\n");
    let literal = heading(dir.path(), &["Arch", "A/B"]);
    let nested = heading(dir.path(), &["Arch", "A", "B"]);
    assert_eq!(literal["content"], "literal\n");
    assert_eq!(nested["content"], "nested\n");
    assert_eq!(literal["address"], "logic/solution/method.md#h/Arch/A%2FB");
    assert_eq!(nested["address"], "logic/solution/method.md#h/Arch/A/B");
    for read in [&literal, &nested] {
        let address = read["address"].as_str().unwrap();
        let again = show(dir.path(), &[address])["entries"][0].clone();
        assert_eq!(again["content"], read["content"]);
        assert_eq!(again["digest"], read["digest"]);
        assert_eq!(again["heading"], read["heading_path"]);
        assert_eq!(again["address"], address);
    }
    // Legacy flattened input resolves only when it identifies one section.
    let error = reject(dir.path(), &["logic/solution/method.md#Arch/A/B"]);
    assert_eq!(error["code"], "ambiguous_heading");
    assert_eq!(
        candidates(&error),
        [
            "logic/solution/method.md#h/Arch/A%2FB",
            "logic/solution/method.md#h/Arch/A/B"
        ]
    );
    let unique = artifact("# Method\n## Step 3\nthree\n");
    let read = show(unique.path(), &["logic/solution/method.md#Method/Step 3"]);
    assert_eq!(read["entries"][0]["content"], "three\n");
    assert_eq!(
        read["entries"][0]["address"],
        "logic/solution/method.md#h/Method/Step%203"
    );
    // Entry IDs, native forms and whole documents stay accepted positionally.
    for id in ["C01", "logic/claims.md#C01", "trace:N01", "N01"] {
        assert!(
            show(unique.path(), &[id])["entries"][0]
                .get("kind")
                .is_some()
        );
    }
    let whole = show(unique.path(), &["logic/claims.md"]);
    assert_eq!(whole["entries"][0]["address"], "logic/claims.md");
}

#[test]
fn percent_escaping_encodes_reserved_bytes_and_rejects_malformed_addresses() {
    let dir = artifact("# Arch\n## Résumé; 50% done\nescaped\n");
    put(dir.path(), "logic/my notes.md", "# Notes\n## Plan\nplan\n");
    let read = heading(dir.path(), &["Arch", "Résumé; 50% done"]);
    assert_eq!(
        read["address"],
        "logic/solution/method.md#h/Arch/R%C3%A9sum%C3%A9%3B%2050%25%20done"
    );
    let again = show(dir.path(), &[read["address"].as_str().unwrap()]);
    assert_eq!(again["entries"][0]["content"], "escaped\n");
    let notes = show(dir.path(), &["logic/my%20notes.md#h/Notes/Plan"]);
    assert_eq!(notes["entries"][0]["content"], "plan\n");
    assert_eq!(
        notes["entries"][0]["address"],
        "logic/my%20notes.md#h/Notes/Plan"
    );
    let whole = show(dir.path(), &["logic/my%20notes.md"]);
    assert_eq!(whole["entries"][0]["address"], "logic/my%20notes.md");
    assert_eq!(whole["entries"][0]["content"], "# Notes\n## Plan\nplan\n");
    for malformed in [
        "logic/solution/method.md#h/Arch/A%2",
        "logic/solution/method.md#h/Arch/%ZZ",
        "logic/solution/method.md#h/Arch/%FF",
        "logic/solution/method.md#h/Arch;occurrence=0",
        "logic/solution/method.md#h/Arch;occurrence=01",
        "logic/solution/method.md#h/Arch;sort=1",
    ] {
        assert_eq!(reject(dir.path(), &[malformed])["code"], "invalid_address");
    }
    for outside in [
        "%2E%2E/outside.md#h/A",
        "../outside.md#h/A",
        "/etc/outside.md#h/A",
        "rubric/requirements.md#h/R01",
    ] {
        assert_eq!(reject(dir.path(), &[outside])["code"], "invalid_document");
    }
}

#[test]
fn duplicate_full_vectors_need_an_occurrence() {
    let dir = artifact("# M\n## Dup\none\n## Dup\ntwo\n");
    let error = heading_error(dir.path(), &["Dup"]);
    assert_eq!(error["code"], "ambiguous_heading");
    let expected = [
        "logic/solution/method.md#h/M/Dup;occurrence=1",
        "logic/solution/method.md#h/M/Dup;occurrence=2",
    ];
    assert_eq!(candidates(&error), expected);
    let unqualified = reject(dir.path(), &["logic/solution/method.md#h/M/Dup"]);
    assert_eq!(unqualified["code"], "ambiguous_heading");
    assert_eq!(candidates(&unqualified), expected);
    for (address, body) in expected.iter().zip(["one\n", "two\n"]) {
        let read = show(dir.path(), &[address])["entries"][0].clone();
        assert_eq!(read["content"], body);
        assert_eq!(read["digest"], digest(body));
        assert_eq!(read["address"], *address);
        assert_eq!(read["heading"], json!(["M", "Dup"]));
    }
    assert_eq!(
        reject(
            dir.path(),
            &["logic/solution/method.md#h/M/Dup;occurrence=3"]
        )["code"],
        "unknown_id"
    );
    // Native cited forms do not depend on the merge index representing
    // every unrelated document.
    for id in ["logic/claims.md#C01", "logic/claims.md:C01", "trace:N01"] {
        assert!(
            show(dir.path(), &[id])["entries"][0].get("id").is_some(),
            "{id}"
        );
    }
    let legacy = reject(dir.path(), &["logic/solution/method.md#M/Dup"]);
    assert_eq!(legacy["code"], "ambiguous_heading");
    assert_eq!(candidates(&legacy), expected);
    // Canonical addresses never fall back to tolerant matching.
    assert_eq!(
        reject(
            dir.path(),
            &["logic/solution/method.md#h/m/dup;occurrence=1"]
        )["code"],
        "unknown_id"
    );
}

#[test]
fn candidates_are_capped_ranked_and_deterministic() {
    let mut document = String::from("# Items\n");
    for index in 1..=45 {
        document.push_str(&format!("## Item {index:02}\nbody {index}\n"));
    }
    let dir = artifact(&document);
    let first = heading_error(dir.path(), &["Item 7"]);
    assert_eq!(first["code"], "unknown_id");
    assert_eq!(first["details"]["capped"], true);
    let listed = candidates(&first);
    assert_eq!(listed.len(), 40);
    assert_eq!(listed[0], "logic/solution/method.md#h/Items/Item%2007");
    assert_eq!(heading_error(dir.path(), &["Item 7"]), first);
    let entry = reject(dir.path(), &["C1"]);
    assert_eq!(entry["code"], "unknown_id");
    assert_eq!(candidates(&entry)[0], "C01");
    assert_eq!(entry["details"]["capped"], false);
}

#[test]
fn heading_misses_in_ledger_artifacts_do_not_leak_merge_codes() {
    let dir = artifact("# Method\n## Step 3: Train\ntrain\n");
    put(dir.path(), "trace/logic_mutations.yaml", "mutations: []\n");
    let miss = heading_error(dir.path(), &["Missing"]);
    assert_eq!(miss["code"], "unknown_id");
    assert_eq!(
        candidates(&miss),
        [
            "logic/solution/method.md#h/Method",
            "logic/solution/method.md#h/Method/Step%203%3A%20Train"
        ]
    );
    assert_eq!(reject(dir.path(), &["C99"])["code"], "unknown_id");
    assert_eq!(
        heading(dir.path(), &["step 3"])["content"],
        "train\n",
        "tolerant tiers still run after the archived lookup misses"
    );
    // A corrupt alias index keeps a truthful, non-merge diagnostic, with or
    // without a mutation ledger beside it.
    put(dir.path(), "trace/aliases.yaml", "aliases: 7\n");
    let aliases_only = artifact("# Method\n");
    put(aliases_only.path(), "trace/aliases.yaml", "aliases: 7\n");
    assert_eq!(
        reject(aliases_only.path(), &["trace:N01"])["code"],
        "identity_lookup_failed"
    );
    for error in [
        heading_error(dir.path(), &["Missing"]),
        reject(dir.path(), &["C99"]),
    ] {
        let code = error["code"].as_str().unwrap();
        assert_eq!(code, "identity_lookup_failed", "{error}");
        assert!(!error.to_string().contains("merge."), "{error}");
    }
    // Exact reads never consult the identity records.
    assert_eq!(
        heading(dir.path(), &["Method"])["heading"],
        json!(["Method"])
    );
}

#[test]
fn native_shorthand_mixes_with_literal_segments_in_vectors_and_legacy_fragments() {
    let dir = artifact("# Top\n## C04: Alpha\nalpha\n## C04b extra\nbeta\n");
    // Literal `Top` plus shorthand `C04` is an exact tier, ahead of the
    // tolerant prefix that would also match `C04b extra`.
    assert_eq!(heading(dir.path(), &["Top", "C04"])["content"], "alpha\n");
    assert_eq!(
        show(dir.path(), &["logic/solution/method.md#Top/C04"])["entries"][0]["content"],
        "alpha\n"
    );
    put(
        dir.path(),
        "logic/problem.md",
        "# Problem\n## O1: First observation\nfirst\n## O2: Second\nsecond\n## O2\nliteral\n",
    );
    let structured = show(
        dir.path(),
        &["--document", "logic/problem.md", "--heading", "O1"],
    );
    let legacy = show(dir.path(), &["logic/problem.md#O1"]);
    assert_eq!(legacy["entries"][0]["content"], "first\n");
    assert_eq!(
        legacy["entries"][0]["digest"],
        structured["entries"][0]["digest"]
    );
    // A legacy fragment still has to identify exactly one section.
    let error = reject(dir.path(), &["logic/problem.md#O2"]);
    assert_eq!(error["code"], "ambiguous_heading");
    assert_eq!(
        candidates(&error),
        [
            "logic/problem.md#h/Problem/O2%3A%20Second",
            "logic/problem.md#h/Problem/O2"
        ]
    );
}

#[test]
fn positional_document_paths_pass_the_document_boundary() {
    let dir = artifact("# Method\n");
    for outside in [
        "/etc/hosts",
        "../outside.md",
        "logic/%2E%2E/PAPER.md",
        "%2Fetc%2Fhosts",
        "rubric/requirements.md",
    ] {
        let error = reject(dir.path(), &[outside]);
        assert_eq!(error["code"], "invalid_document", "{outside}");
        assert!(error["details"]["file_access"].is_array(), "{outside}");
    }
    // A valid path naming no document is an ordinary miss.
    assert_eq!(
        reject(dir.path(), &["logic/missing.md"])["code"],
        "unknown_id"
    );
}

#[test]
fn occurrence_one_names_a_unique_vector_and_higher_occurrences_miss() {
    let dir = artifact("# M\n## Only\nonly\n");
    let read = show(
        dir.path(),
        &["logic/solution/method.md#h/M/Only;occurrence=1"],
    );
    assert_eq!(read["entries"][0]["content"], "only\n");
    assert_eq!(
        read["entries"][0]["address"],
        "logic/solution/method.md#h/M/Only"
    );
    assert_eq!(
        reject(
            dir.path(),
            &["logic/solution/method.md#h/M/Only;occurrence=2"]
        )["code"],
        "unknown_id"
    );
}

#[test]
fn colliding_entry_keys_report_an_ambiguity() {
    let dir = artifact("# M\n");
    put(
        dir.path(),
        "logic/concepts.md",
        "# Concepts\n\n## N01\n\n- **Definition**: Named like a node.\n",
    );
    let error = reject(dir.path(), &["N01"]);
    assert_eq!(error["code"], "ambiguous_heading");
    assert!(!error["message"].as_str().unwrap().contains("section"));
    // Scoped native forms still select one namespace each.
    assert_eq!(
        show(dir.path(), &["trace:N01"])["entries"][0]["kind"],
        "question"
    );
    assert_eq!(
        show(dir.path(), &["logic/concepts.md#N01"])["entries"][0]["kind"],
        "concept"
    );
}

#[test]
fn recorded_ambiguity_is_not_overridden_by_a_unique_current_section() {
    use ara_core::write::{self, ArtifactSnapshot, EntrySelector, WorkingArtifact, WriteOperation};
    const ARCH: &str = "logic/solution/architecture.md";
    let dir = artifact("# M\n");
    put(dir.path(), ARCH, "# Top\n## A\n### B\nnested\n");
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    for value in [
        json!({"op":"session.start","id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Rename fixture"}),
        json!({"op":"session.log","session":"2026-10-01_001","timestamp":"2026-10-01T10:01"}),
    ] {
        write::plan_operation(&mut working, &serde_json::from_value(value).unwrap()).unwrap();
    }
    let target = EntrySelector::Document {
        document: ARCH.into(),
        heading: vec!["Top".into(), "A".into()],
        entry: None,
    };
    let range = write::logic::resolve(&working, &target).unwrap().range;
    let expected = write::source::digest(working.text(ARCH).unwrap()[range].as_bytes());
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryRename {
            target,
            name: "Q".into(),
            expected,
            references: vec![],
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: Some("user-directive".into()),
            provenance: Some("user".into()),
            rewrite_references: false,
        },
    )
    .unwrap();
    for revision in std::mem::take(&mut working.revisions) {
        write::sessions::append_revision(
            &mut working,
            &revision.session,
            revision.turn,
            &revision.record,
        )
        .unwrap();
    }
    for (path, bytes) in &working.files {
        put(dir.path(), path, std::str::from_utf8(bytes).unwrap());
    }
    // One current section spells `A/B`; the archived vector ['Top','A','B']
    // spells it too, so the recorded identities call `#A/B` ambiguous.
    let renamed = std::fs::read_to_string(dir.path().join(ARCH)).unwrap();
    assert!(renamed.contains("## Q\n"), "{renamed}");
    put(dir.path(), ARCH, &format!("{renamed}## A/B\nliteral\n"));
    let error = reject(dir.path(), &["logic/solution/architecture.md#A/B"]);
    assert_eq!(error["code"], "ambiguous_heading", "{error}");
    // The structured selector and the canonical address still read it.
    assert_eq!(
        show(dir.path(), &["logic/solution/architecture.md#h/Top/A%2FB"])["entries"][0]["content"],
        "literal\n"
    );
}

/// Stdout of a successful brief (default text) read.
fn brief(root: &Path, args: &[&str]) -> String {
    let output = ara(root).args(args).assert().success().get_output().clone();
    String::from_utf8(output.stdout).unwrap()
}
/// The address that leads each item line of brief `ls`, `find`, `open`,
/// `path` or `refs` output: the text before the first tab or ` [`.
fn addresses(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with("target: "))
        .filter(|line| !line.starts_with("direct files: "))
        // A prose mention leads with `source:line`, not an address.
        .filter(|line| !line.contains("\tpossible mention\t"))
        .map(|line| {
            let line = line.trim_start();
            let end = [line.find('\t'), line.find(" [")]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(line.len());
            line[..end].to_owned()
        })
        .collect()
}
/// The `== <address> [<label>]` header line of a single-block brief `show`.
fn show_header(root: &Path, address: &str) -> String {
    let stdout = brief(root, &["show", address]);
    let header = stdout.lines().next().unwrap().to_owned();
    assert!(header.starts_with("== "), "{address}: {stdout}");
    header
}
fn header_address(header: &str) -> &str {
    header[3..].split_once(" [").unwrap().0
}

#[test]
fn colliding_entry_namespaces_print_addresses_that_round_trip() {
    let dir = artifact("# M\n");
    put(
        dir.path(),
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Root widget\n    children:\n      - id: N02\n        type: experiment\n        title: Widget run\n        evidence: [C02]\n        result: widget measured\n",
    );
    put(
        dir.path(),
        "logic/claims.md",
        "# Claims\n\n## C01: Mechanism\n- **Statement**: Known widget.\n- **Status**: hypothesis\n\n## C02: Other\n- **Statement**: Bound.\n- **Status**: hypothesis\n",
    );
    put(
        dir.path(),
        "logic/concepts.md",
        "# Concepts\n\n## C01\n\n- **Definition**: A widget concept.\n\n## N02\n\n- **Definition**: Named like a node widget.\n",
    );
    ara(dir.path()).arg("status").assert().success().stderr("");
    let reads_as = |address: &str, label: &str| {
        let header = show_header(dir.path(), address);
        assert!(header.contains(label), "{address} read as {header}");
    };

    // Document-scoped listings name one namespace each.
    let claims = addresses(&brief(dir.path(), &["ls", "logic/claims.md"]));
    assert_eq!(claims, ["logic/claims.md#C01", "C02"]);
    reads_as(&claims[0], "[claim C01]");
    let concepts = addresses(&brief(dir.path(), &["ls", "logic/concepts.md"]));
    assert_eq!(concepts, ["logic/concepts.md#C01", "logic/concepts.md#N02"]);
    for address in &concepts {
        reads_as(address, "[concept ");
    }
    let experiments = addresses(&brief(dir.path(), &["ls", "--type", "experiment"]));
    assert_eq!(experiments, ["trace/exploration_tree.yaml#N02"]);
    reads_as(&experiments[0], "[experiment]");
    // A key no other entry shares stays bare.
    assert_eq!(
        addresses(&brief(dir.path(), &["ls", "--type", "question"])),
        ["N01"]
    );

    // Every producer prints addresses that read back.
    for args in [
        &["find", "widget"][..],
        &["open"],
        &["path", "trace:N02"],
        &["refs", "C02"],
    ] {
        let found = addresses(&brief(dir.path(), args));
        assert!(!found.is_empty(), "{args:?}");
        let unique: std::collections::BTreeSet<_> = found.iter().collect();
        assert_eq!(unique.len(), found.len(), "{args:?}: {found:?}");
        for address in &found {
            show_header(dir.path(), address);
        }
    }
    let refs = brief(dir.path(), &["refs", "logic/claims.md#C01"]);
    assert!(refs.contains("target: logic/claims.md#C01\n"), "{refs}");
    let refs = brief(dir.path(), &["refs", "C02"]);
    assert!(
        refs.lines()
            .any(|line| line.starts_with("trace/exploration_tree.yaml#N02\t")),
        "{refs}"
    );

    // A projection header names the entry it shows.
    let header = show_header(dir.path(), "trace:N02");
    assert_eq!(header_address(&header), "trace/exploration_tree.yaml#N02");
    assert_eq!(show_header(dir.path(), header_address(&header)), header);

    // Miss candidates keep both namespaces apart, and each one reads.
    let error = reject(dir.path(), &["C01"]);
    assert_eq!(error["code"], "ambiguous_heading");
    let listed = candidates(&error);
    for expected in ["logic/claims.md#C01", "logic/concepts.md#C01"] {
        assert!(listed.contains(&expected), "{listed:?}");
    }
    for address in listed {
        show_header(dir.path(), address);
    }

    // `find --full` embeds each colliding entry's projection.
    let output = ara(dir.path())
        .args(["find", "widget", "--full", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    for result in value["results"].as_array().unwrap() {
        assert_eq!(result["entry"]["kind"], result["kind"], "{result}");
    }
}

#[test]
fn escaped_recipe_document_addresses_round_trip() {
    for (name, escaped) in [
        ("my notes", "logic/solution/my%20notes.md"),
        ("caf\u{e9}", "logic/solution/caf%C3%A9.md"),
    ] {
        let dir = artifact("# M\n");
        let path = format!("logic/solution/{name}.md");
        put(dir.path(), &path, "# Notes\n\nA widget recipe.\n");
        for args in [
            &["ls"][..],
            &["ls", "--type", "solution"],
            &["find", "widget"],
        ] {
            let found = addresses(&brief(dir.path(), args));
            assert!(found.iter().any(|a| a == escaped), "{args:?}: {found:?}");
        }
        let header = show_header(dir.path(), escaped);
        assert_eq!(header_address(&header), escaped, "{header}");
        // The JSON read keeps the recipe projection of the raw spelling.
        let read = show(dir.path(), &[escaped]);
        assert_eq!(read, show(dir.path(), &[&path]), "{name}");
        assert_eq!(read["entries"][0]["kind"], "solution");
        assert_eq!(read["entries"][0]["key"], path.as_str());
        // Decoding stays one pass: a doubly escaped spelling names nothing.
        let twice = escaped.replace('%', "%25");
        assert_eq!(reject(dir.path(), &[&twice])["code"], "unknown_id");
    }
}
