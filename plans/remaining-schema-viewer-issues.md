# Claims, artifact pointers, and concept chips implementation plan

**Date:** 2026-10-02

>For agentic workers: Use the `implement` skill after the human approves this plan. Follow the tasks in order; do not commit, push, or open a pull request without a separate request.

**Goal:** Address issues #61, #62, and #63 by preserving claim falsification criteria and rendering node artifact pointers and links to glossary definitions.

**Architecture:** Extend the existing `ara-core` raw and normalized models, then extend the viewer's pure detail model and Leptos components. Keep glossary navigation in the existing modal and preserve native/wasm builds and old manifest compatibility.

**Tech stack:** Rust, serde, serde-saphyr, Leptos 0.8, wasm-bindgen-test, Trunk, and existing native integration fixtures.

## TL;DR

All three open issues require data-model changes as well as viewer changes. The format repository confirms the `Falsification criteria` claim label and uses glossary term names for per-node concepts. It does not define node-level YAML `artifacts:` or `concepts:` lists in the inspected exploration-tree specification; its visualizer derives those enrichments from other files and text. This plan proposes explicit YAML lists as CLI extensions, with term-name references for concepts, and needs approval of that refinement to #63 before implementation.

## Problem

[#61](https://github.com/ARA-Labs/ara-cli/issues/61) loses falsification criteria during claim parsing. `Claim` already carries proof experiment references and claim dependencies, but `ClaimView` discards them and the claim ID. Claim cards therefore omit information already available in the source.

[#62](https://github.com/ARA-Labs/ara-cli/issues/62) requests per-node artifact pointers with `name`, `pointer`, and `what`. Neither `RawNode` nor `Node` carries this data. [#63](https://github.com/ARA-Labs/ara-cli/issues/63) requests per-node concept links, but the current model has only a global glossary. The issue proposes concept IDs while the current glossary and upstream canonical example use term names.

## Constraints

This is a plan-only change on `plan/remaining-schema-viewer-issues`, based on local commit `677c217`. Implementation starts only after human review. The plan itself needs no version bump or embedded viewer rebuild, and remains uncommitted unless the human requests a commit.

New manifest fields must default when absent and be omitted when empty. Preserve source order, existing validation severity, layout geometry, and tree-only rendering. All source text is untrusted: render it as escaped text, never as raw HTML or a generated executable link. Native filesystem reads must stay outside the wasm-safe `parse_sources` path.

No external artifact fetches, file-serving routes, experiment registry, new concept-ID scheme, or automatic mention inference are included. In particular, `E##` proof references identify experiments in `logic/experiments.md`; they must not be linked to exhibits merely because an exhibit happens to have the same ID. This plan does not add the upstream viewer's other claim fields, such as Conditions or Sources.

## Proposed approach

### Which upstream sources govern the fields?

The inspected upstream revision is [`e52a925e9d03b4ada3008653e72f99b04116fca2`](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/tree/e52a925e9d03b4ada3008653e72f99b04116fca2). Pin references to this revision so later upstream edits do not change the plan's meaning.

| Source | Observed contract | Decision |
|---|---|---|
| [Directory schema](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/compiler/references/ara-schema.md), `logic/claims.md` | `Falsification criteria` is a claim field; Proof contains experiment IDs | Parse that label into optional manifest `falsification`; display Proof as text chips |
| [Exploration-tree specification](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/compiler/references/exploration-tree-spec.md) | Inspected node schema does not define artifact or concept lists | Document explicit node lists as CLI extensions |
| [Visualizer binding](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/research-visualizer/references/binding.md), resolution chain and B.4 | Artifact entries use name/pointer/what; per-node concepts contain verbatim term names and are inferred by name matching | Reuse the entry shape and term-name reference vocabulary; do not claim automatic upstream enrichment |
| [Visualizer parsing](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/research-visualizer/references/parsing.md), sections 6 and 8.2 | Claim labels occur in bullet and bold-leading forms; glossary anchors can be generated during visualization | Accept the documented claim label forms; do not expose generated positional glossary anchors as authored IDs |
| [ResNet concepts example](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/examples/resnet-ara-example/logic/concepts.md) | Concepts are `## Term` sections without authored IDs | Keep the existing `Concept.term` model |

The upstream visualizer's payload is not this CLI's manifest format. It names the per-node artifact array `artifact`; #62 explicitly proposes YAML `artifacts`. Keep the CLI's proposed plural field without adding a second spelling.

### What source syntax will the CLI accept?

Claim input retains the canonical Markdown label:

```markdown
## C01: Optimizer improves convergence
- **Statement**: The optimizer improves convergence in the measured regime.
- **Status**: supported
- **Falsification criteria**: The improvement disappears under a matched comparison.
- **Proof**: [E01, E02]
- **Dependencies**: [C02]

## C02: Matched comparisons isolate the optimizer
- **Statement**: The comparisons keep other training choices fixed.
```

The proposed node extensions are explicit author-supplied lists:

```yaml
tree:
  - id: N01
    type: experiment
    title: Compare Muon Optimizer against the baseline
    result: The matched comparison supports the claim.
    evidence: [C01]
    artifacts:
      - name: muon_optimizer.py
        pointer: src/execution/muon_optimizer.py
        what: Newton-Schulz orthogonalization and Nesterov momentum.
    concepts: [Muon Optimizer]
```

The glossary definition remains an existing `logic/concepts.md` section:

```markdown
## Muon Optimizer
- **Definition**: An optimizer using orthogonalized matrix updates.
- **Notation**: $W$
```

For #63, approving this plan approves term-name references in place of the issue's proposed concept IDs. Bindings still parse, serialize, render, and open the matching definition. Existing trees without these lists will not gain automatic bindings; that upstream behavior is outside this plan.

### Which model fields change?

Add these fields and type in `crates/ara-core/src/manifest.rs`, following its existing derives:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeArtifact {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pointer: String,
    #[serde(default)]
    pub what: String,
}
```

Insert the following fields into the existing structs, not into new parallel models:

```rust
// Claim
#[serde(default, skip_serializing_if = "Option::is_none")]
pub falsification: Option<String>,

// Node
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub artifacts: Vec<NodeArtifact>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub concepts: Vec<String>,
```

Add matching default-empty fields to `RawNode` and carry them through `Normalizer::dfs`. Reuse the artifact entry type at the raw layer. Export `NodeArtifact` alongside the other manifest types in `crates/ara-core/src/lib.rs`. Missing entry strings default to empty; non-string values follow the existing malformed-YAML diagnostic path. The viewer omits empty subfields and completely blank entries.

`ClaimView` gains the claim ID, optional falsification text, raw proof refs, and claim dependencies. `DetailModel` gains the node's artifact entries and resolved concept views. Retain unresolved concept names as visible, non-interactive chips. Match an explicit concept reference against the full `Concept.term`, trimming reference whitespace and comparing case-insensitively. An ambiguous match is non-interactive; never choose an arbitrary definition. Deduplicate resolved concept chips by their matched term while preserving first-reference order. Do not treat partial names or substrings as explicit bindings.

### How will the detail pane and glossary behave?

Extend existing claim cards in EVIDENCE. Show the mono claim ID, current title/statement/status, a labeled falsification paragraph, proof chips, and claim-dependency chips. Omit absent optional rows. Unknown status values retain the current neutral styling. Proof and dependency chips are textual references; this task does not add a claim-navigation panel.

Render ARTIFACTS after RESULT and before SOURCES with the existing `CollapsibleBlock` pattern. Each entry shows its available name, monospace pointer, and description. Pointer strings are display values, including external pointer text; clicking must not fetch or execute them. A node whose only content is artifacts or concepts must not show the empty-node placeholder.

Put CONCEPTS after EVIDENCE and before BUILT ON. A resolved term is a button that opens the existing Glossary modal with the matching definition in view and keyboard focus on its card. Keep the definition's existing `MathText` rendering. An unresolved or ambiguous reference is a muted text chip with an explanation and no click action.

Lift glossary open/filter/target signals into `App`; pass the required signals to `GlossaryPanel` and a required `Callback<String>` to `DetailPane`. Update every component caller and browser test without optional compatibility props. Chip activation clears a stale filter and sets the exact matched term. The header launcher clears the target and continues to open the full, filterable glossary.

Give concept cards `tabindex="-1"` and a node reference for focusing the selected card. Schedule card focus in a cancellable animation-frame callback after the existing Modal open effect captures the invoking element and focuses the dialog. Run the callback only if the modal is still open and the target still matches; cancel it on cleanup. Closing the modal restores the invoking chip's focus. A manifest replacement must clear a target that no longer resolves. Use the card's node reference for focus and scrolling, without turning concept names into CSS selectors or DOM IDs.

## Alternatives considered

| Approach | Benefit | Cost / reason not selected |
|---|---|---|
| Explicit node lists with glossary term references (proposed) | Meets the issues' direct parse/manifest/render path; uses current glossary definitions | Existing trees need authored lists; extensions need documentation |
| Port upstream derived binding | Existing artifacts can gain chips from text mentions and pointer indexes | Requires source-to-node association rules and inferred/explicit distinction beyond the issues' proposed fields |
| Add authored concept IDs to the glossary | Direct ID-based references | Changes the concepts source syntax without support in the canonical example; introduces migration work |

## Tradeoffs

Explicit lists give authors control and preserve provenance, but they do not automatically reproduce every enriched published hub page. This limit must appear in the feature documentation and review discussion. If the human wants derived binding instead, revise and approve this plan before implementation; do not silently substitute a name scanner or attach a global artifact index to every node.

Term names fit the inspected source format and avoid positional IDs changing when glossary sections move. Renaming a term requires updating its node references. Unknown names remain visible so the reader can see the broken binding, while the parser can still process a tree without any glossary file.

## Migration

Old JSON manifests deserialize because all added fields default. Empty new fields serialize away. Trees without the proposed node lists keep their current behavior, except that existing canonical claims now retain and render falsification criteria and existing proof/dependency fields become visible.

Adding public Rust struct fields requires updating all struct literals across the workspace, including tests and benches. Before implementing, use language-server references for affected exported types/components where available; otherwise use the existing symbol search tools. Update every caller in the same cutover. Inspect snapshot changes: existing canonical claim fixtures can legitimately gain falsification text, while fixtures without new payload must not gain empty keys.

## Which files will change?

| File | Responsibility |
|---|---|
| `crates/ara-core/src/claims.rs` | Parse canonical falsification labels and preserve current claim extraction |
| `crates/ara-core/src/manifest.rs` | New optional claim field, artifact entry type, and node lists |
| `crates/ara-core/src/schema.rs` | Recognize proposed YAML node fields |
| `crates/ara-core/src/parse.rs` | Carry node lists through normalization |
| `crates/ara-core/src/lib.rs` | Export the artifact entry type |
| `crates/ara-core/tests/parse_fixtures.rs` | Parse/serialize compatibility and functional directory coverage |
| `crates/ara-core/tests/snapshots/` | Review affected canonical-claim snapshots |
| `crates/ara-viewer/src/detail.rs` | Complete claim cards, artifact block, concept-chip resolution and callbacks |
| `crates/ara-viewer/src/lib.rs` | Own shared glossary navigation signals |
| `crates/ara-viewer/src/panels.rs` | Targeted glossary card focus and full-panel launcher behavior |
| `crates/ara-viewer/public/styles.css` | Existing-token styling, muted unresolved chips, wrapping paths and focus indication |
| `crates/ara-viewer/tests/web.rs` | User-visible rendering, navigation and focus regression tests |
| Other existing Rust literals/callers found by references | Migrate affected struct/component construction |
| `docs/manifest-schema.md`, `docs/hub-parity.md` | Document fields, extensions, and limits; retire the artifact-pointer deferral |
| `Cargo.toml`, `Cargo.lock`, published local dependency pins | Patch bump for the eventual functional PR, not for this plan |
| `CHANGELOG.md` | Eventual Unreleased entries for all three issues |
| `crates/ara-cli/assets/viewer/`, `crates/ara-cli/assets/viewer.source-hash` | Regenerate the shipped embedded UI after implementation |

## Implementation steps

### Task 1: Preserve claim falsification (#61)

- [ ] Add a failing behavioral test in `claims.rs` for the canonical `- **Falsification criteria**:` label. Assert the full criterion, Proof IDs, and Dependencies IDs, not merely that parsing succeeds.
- [ ] Cover absent/blank criteria and an unknown claim label. Use a two-claim input to prove values do not leak across headers. Cover upstream's documented bold-leading label form in the same parser tests.
- [ ] Add `Claim.falsification`. Extend the existing label parser to recognize the documented leader shapes, then dispatch the normalized `falsification criteria` label to `non_empty`. Keep missing fields optional and unknown labels ignored.
- [ ] Extend `ClaimView` and claim-card rendering. Migrate all `Claim` literals, and add browser assertions for the claim ID, exact criterion, experiment proof refs and claim dependencies. Include an old claim without criteria and an unrecognized status.
- [ ] Update official manifest snapshots only where canonical source criteria are newly retained. Do not re-pin tests that depend only on wording or an incidental implementation.

Focused command during the regression cycle:

```bash
cargo test -p ara-core --locked claims::tests
```

Before the fix, the new preservation assertion fails (or the new field is not yet available). After implementation, the canonical criterion and existing reference vectors must equal the source values. Browser execution is included in the final verification task below.

### Task 2: Carry and render artifact pointers (#62)

- [ ] Add a failing parse/JSON integration test using the node YAML above. Assert the exact three entry values and source order for two entries, including an external pointer string. Check that `artifacts` is not reported as an unknown node key.
- [ ] Add a populated old-shape manifest fixture in the existing compatibility test: at least one node and one claim without the new fields. Assert default-empty node arrays and absent claim falsification after deserialization.
- [ ] Add `NodeArtifact`, raw/normalized lists, and the public export. Migrate all `Node` literals to initialize both new lists, so #63 can use the same cutover.
- [ ] Extend `DetailModel` and `is_empty`; render the ARTIFACTS block between RESULT and SOURCES. Show available subfields and omit blank rows. Use ordinary escaped text and CSS wrapping for long pointers.
- [ ] Add native model tests for artifact-only nodes and browser tests for block order, omission on empty input, selection changes and HTML-looking pointer text staying inert.

Use `tempfile::tempdir()` for the functional `parse_dir` test: write `trace/exploration_tree.yaml`, `logic/claims.md`, and `logic/concepts.md` from the examples in this plan. Assert the values survive `parse_dir` and manifest serialization without mutating pinned upstream fixtures.

### Task 3: Bind terms and open definitions (#63)

- [ ] Add failing parser tests showing that `concepts: [Muon Optimizer]` survives normalization and JSON serialization. Verify absent lists retain old behavior and unknown term names remain in the manifest.
- [ ] Carry `RawNode.concepts` into `Node.concepts`; resolve chip views against `Manifest.concepts` in the pure viewer model. Test exact case-insensitive full-name matching, substring rejection, ambiguous names, missing definitions, and deduplication order.
- [ ] Lift the glossary signals into App, add the required detail callback, and update every caller. The callback sets the matched term, clears the filter, and opens the glossary.
- [ ] Render concept buttons and muted unresolved chips. Add targeted card focus after Modal's focus-capture effect, preserve MathText, and clear invalid targets on manifest replacement. Keep normal launcher filtering available.
- [ ] Add browser tests that activate a non-first term, assert its exact definition and active card, close with Esc, and assert focus returns to the initiating chip. Repeat after setting a filter that would otherwise hide that term. Cover header opening the full glossary, unknown chips being non-interactive, and manifest replacement invalidating a target.

The functional browser fixture must have at least two different definitions and a node referencing the second term. This distinguishes correct target navigation from simply opening a modal.

### Task 4: Verify the compiled path and shipped UI

- [ ] Run the focused native suites, then the workspace checks once after the related implementation tasks have landed. These commands use the existing CI conventions:

```bash
cargo test -p ara-core --locked --test parse_fixtures
cargo test -p ara-viewer --locked --lib
cargo test --workspace --locked
cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
wasm-pack test --headless --chrome crates/ara-viewer --locked
```

- [ ] Create a temporary artifact using the complete sample sources above. Generate its manifest and launch a source-built viewer. Observe all claim metadata, artifact values, and the concept-to-definition interaction in a real browser at desktop and narrow widths. Check keyboard activation, Esc, focus restoration, and no horizontal page overflow from a long pointer.
- [ ] Exercise an unchanged official artifact to prove omitted fields still render correctly. Use its parsed claims to observe newly retained canonical falsification criteria.
- [ ] After the browser smoke succeeds, document the fields and extension status in `docs/manifest-schema.md` and `docs/hub-parity.md`. Rewrite this plan as `docs/claim-artifact-concept-details.md` and remove the temporary plan only when the approved implementation is complete.
- [ ] For the eventual functional PR, bump the current workspace patch version (currently `0.1.21`) and update affected published dependency pins/lockfile consistently. Add Unreleased entries referencing #61, #62 and #63. The plan-only branch requires neither change now.
- [ ] Regenerate the embedded viewer and test the actual default `ara serve` UI against the same temporary artifact:

```bash
scripts/embed-viewer.sh
scripts/embed-viewer.sh --check
cargo run -p ara-cli --locked -- layout /tmp/ara-schema-viewer-smoke --json
cargo run -p ara-cli --locked -- serve /tmp/ara-schema-viewer-smoke --port 8080
```

`/tmp/ara-schema-viewer-smoke` denotes the temporary artifact created for this smoke; choose an unused directory and port rather than overwrite existing user files. Expected observations are the exact C01 criterion, E01/E02 proof refs, C02 dependency, artifact pointer, and Muon Optimizer definition reached from its chip. The freshness command must report the embedded viewer is up to date. Stop the server and remove only the temporary files created for the smoke.

## How will acceptance be checked?

| Issue criterion | Evidence required |
|---|---|
| #61 criterion parsed and present in manifest | Claim-parser unit assertion plus serialized parsed manifest |
| #61 ID, criterion, proofs and dependencies shown | Browser claim-card assertions and actual served-page observation |
| #61 absent/unknown fields degrade | Old populated JSON, missing/blank criteria, unknown label/status cases |
| #62 pointers parse and serialize | Pure parse and native directory integration assertions for exact values |
| #62 block renders or omits | Browser block-order/empty tests and actual served-page observation |
| #63 bindings parse and serialize | Term-name list assertions; review approves the representation correction |
| #63 chips reach the definition | Browser interaction with the second term, exact definition, focus and Esc return |
| All three parser/viewer unit coverage | Native parser/model tests and existing wasm browser suite |
| Old manifests and shipped binary work | Populated old-shape JSON plus regenerated default embedded serve smoke |

## Next Steps

1. Review the explicit-extension scope and the term-name correction to #63. Approve or revise those decisions before implementation.
2. Implement tasks 1 through 3 in the existing parser/viewer architecture, then perform task 4's runtime and distribution checks.
3. Commit or create a pull request only after a separate human request. Suggested plan commit message: `docs(plan): scope remaining schema and viewer issues`.
