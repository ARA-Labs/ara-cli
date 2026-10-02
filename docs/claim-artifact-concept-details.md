# Claim metadata, artifact pointers, and concept definitions
**Date:** 2026-10-02

## TL;DR

The parser preserves claim falsification criteria, and the viewer shows claim IDs, experiment proof references, and claim dependencies. Explicit node `artifacts` lists carry names, pointers, and descriptions. Explicit node `concepts` lists name glossary terms and open the matching definition with keyboard focus. These node lists are CLI extensions; the parser does not infer bindings from mentions or fetch artifact pointers.

## Problem

Issues [#61](https://github.com/ARA-Labs/ara-cli/issues/61), [#62](https://github.com/ARA-Labs/ara-cli/issues/62), and [#63](https://github.com/ARA-Labs/ara-cli/issues/63) requested claim falsification, per-node artifact pointers, and per-node concept navigation. `Claim` lacked falsification, while the detail model discarded existing proof references and dependencies. Nodes carried neither artifact entries nor glossary references.

The approved plan uses glossary term names instead of the concept IDs proposed in #63. The inspected upstream [ResNet glossary](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/examples/resnet-ara-example/logic/concepts.md) defines terms with `## Term` headings and no authored IDs. References remain pinned to that upstream revision.

## Constraints

New fields default when absent and disappear from JSON when empty. Source order, validation severity, geometry, and tree-only rendering remain unchanged. The pure `parse_sources` path stays safe for WebAssembly; native `parse_dir` reads the optional glossary file.

All source strings are untrusted. Artifact pointers render as escaped display text, including external strings, without links, execution, file-serving routes, or fetches. Experiment `E##` proof references remain text; an exhibit with the same ID does not establish an experiment-to-exhibit binding. The feature does not add an experiment registry, a concept-ID scheme, automatic mention inference, or other upstream claim fields such as Conditions and Sources.

## Proposed approach

### Which source forms are supported?

The [upstream directory schema](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/compiler/references/ara-schema.md) names `Falsification criteria`. The [parsing guide](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/research-visualizer/references/parsing.md) documents bullet and bold-leading labels, including `**Statement.**`. The claim reader accepts these label forms and strips an optional trailing label period before dispatch. Blank criteria become absent values; unknown labels are ignored.

```markdown
## C01: Optimizer improves convergence
- **Statement**: The optimizer improves convergence in the measured regime.
- **Status**: supported
- **Falsification criteria**: The improvement disappears under a matched comparison.
- **Proof**: [E01, E02]
- **Dependencies**: [C02]

## C02: Matched comparisons isolate the optimizer
**Statement.** The comparisons keep other training choices fixed.
```

The [exploration-tree specification](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/compiler/references/exploration-tree-spec.md) at the inspected revision does not define node-level artifact or concept lists. The CLI accepts the following explicit extensions. It uses the name/pointer/what entry shape and full term-name vocabulary from the [visualizer binding guide](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/blob/e52a925e9d03b4ada3008653e72f99b04116fca2/skills/research-visualizer/references/binding.md), but does not reproduce that visualizer's inferred enrichment.

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

```markdown
## Muon Optimizer
- **Definition**: An optimizer using orthogonalized matrix updates.
- **Notation**: $W$
```

### Which models carry the data?

`Claim.falsification` is an optional string. `Node.artifacts` is a list of exported `NodeArtifact` entries, whose `name`, `pointer`, and `what` strings default to empty. `Node.concepts` is a list of strings. `RawNode` recognizes both lists, and normalization copies them in author order. Malformed list entries follow the existing malformed-YAML diagnostic path.

The viewer's pure `DetailModel` retains claim IDs, criteria, raw proofs, and claim dependencies. It omits wholly blank artifact entries from the display model without changing the manifest. Concept resolution trims reference whitespace and compares the full `Concept.term` case-insensitively. It rejects substring matches. A unique match creates a resolved chip; missing or ambiguous matches retain explanatory non-interactive chips. Resolved duplicates collapse in first-reference order.

### How does the detail pane navigate?

EVIDENCE claim cards show a mono ID, title, statement, status, and optional labeled falsification, proof, and dependency rows. Unknown statuses keep neutral styling. CONCEPTS follows EVIDENCE and precedes BUILT ON. ARTIFACTS follows RESULT and precedes SOURCES. Artifact entries omit blank subfields and wrap long paths. Nodes containing only artifacts or concepts do not show the empty-node placeholder.

`App` owns the glossary open, filter, and target signals. A required `DetailPane` callback sets the exact resolved term, clears the stale filter, and opens the existing Glossary modal. The header launcher clears the target and filter to show the full glossary, which remains filterable.

Each glossary card has `tabindex="-1"` and a node reference. A cancellable animation-frame callback focuses and scrolls the target after Modal captures the invoking element and focuses its dialog. The callback checks the open state, exact target, and connected card before acting. Cleanup cancels pending frames. Escape closes through the existing Modal behavior and restores the invoking chip's focus. Manifest replacement clears a target whose term is removed, renamed, or ambiguous. Concept names never become generated DOM IDs or selectors, and definition fields retain `MathText` rendering.

Editing the glossary filter clears the navigation target before updating the query. Matching cards therefore do not steal focus from the input as the user types.

## Alternatives considered

| Option | Benefit | Reason not selected |
|---|---|---|
| Explicit lists with term-name references | Preserves author choices and fits existing glossary headings | Chosen; authors must supply node lists |
| Port upstream inferred binding | Could enrich existing artifacts from mentions and pointer indexes | Requires association rules and inferred/explicit distinctions beyond these issues |
| Introduce authored glossary IDs | Gives references independent of term wording | Changes existing source syntax and requires migration without canonical-example support |

## Tradeoffs

Explicit lists preserve provenance but do not automatically reproduce enriched published hub pages. Old trees gain newly preserved claim metadata, not inferred artifact or concept bindings. Term references avoid positional anchors changing when glossary sections move; renaming a term requires updating node references. Missing definitions remain visible, so readers can identify broken bindings even when no glossary file exists.

## Migration

Old populated JSON manifests deserialize with absent criteria and empty node arrays. Empty arrays and absent criteria do not serialize. Public Rust struct literals and all viewer component callers migrated together; no optional compatibility props or aliases remain. Official snapshots gained only criteria already present in their source claims. Workspace version `0.1.22` includes the updated local dependency pins and regenerated embedded viewer.

## Which checks exercise the behavior?

Core unit tests cover canonical and dotted bold-leading labels, missing and blank values, unknown labels, and isolation across claims. Integration tests exercise in-memory parsing, a temporary artifact directory, exact serialized artifact values and concepts, partial entry defaults, malformed entries, and populated old-manifest compatibility. Native viewer model tests cover artifact-only nodes and full-term concept matching, missing/ambiguous terms, substring rejection, and deduplication order.

Browser regressions assert the complete claim metadata, artifact order and inert HTML-looking text, selection changes, and the exact second glossary definition with card focus. They cover stale filters, Escape focus return, full-panel launching, unresolved chips, and manifest replacement. Runtime smoke used source-built assets at desktop and 375-pixel widths, checked long-path wrapping without horizontal page overflow, and observed an unchanged official artifact's canonical criterion. The same sample also exercises the embedded default `ara serve` distribution.

```bash
cargo test -p ara-core --locked claims::tests
cargo test -p ara-core --locked --test parse_fixtures
cargo test -p ara-viewer --locked --lib
cargo test --workspace --locked
cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
wasm-pack test --headless --chrome crates/ara-viewer --locked
scripts/embed-viewer.sh
scripts/embed-viewer.sh --check
```

## Next Steps

Authors can add explicit node lists using the examples above and regenerate manifests with `ara layout`. Keep term references synchronized when renaming glossary headings. Inferred enrichment and new claim fields require separate source contracts and review.
