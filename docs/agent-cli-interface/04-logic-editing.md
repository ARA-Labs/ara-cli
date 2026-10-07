# Native logic and bounded document editing

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Claim and heuristic creation/setters, typed document selectors, guarded structural edits and whole-body revisions share source/digest validation. Long CLI values support @file, @-, and @@ escaping; JSONL values remain literal. Claims retain audited withdrawal/merge entries. New claim and heuristic blocks use the fixed schema field order, inline single-line values, typed `[C01, C02]` dependency lists and lossless JSON for other lists; see [agent-cli.md](../agent-cli.md#authoring-commands-and-source-inputs). Field placement inside existing blocks (`entry.edit`, `logic.revise`) is unchanged.

Plan 19 (step 19e, C1) adds citation repair to restructures; see
[Citation repair](../agent-cli.md#citation-repair-for-restructures).
`entry.rename` and `entry.remove` (with a `redirect`) accept
`rewrite_references: true`; `logic.revise` accepts it for an explicit claim
merge (`Status: withdrawn` plus `Merged into`), or explicit `references` rows
for a merge, and `action: "split"` with `split_into` and a complete
`references` classification for a split. `write/citation_rules.rs` holds the rules
`ara show --with refs` shares with the writer (reference-field table with rewritable
flags, protected spans, the historical-source walker, concept-name
resolution); `write/logic/citations.rs` builds the typed inventory and the
`markdown_citations` API that `show --with refs` uses for logic entries;
`write/logic/row_mapping.rs` validates split and merge rows;
`write/logic/history_refs.rs` scans and validates history. The inventory covers: accepted reference fields of native entries, tokens resolved
through current headings and authenticated claim redirects, and located
possible mentions everywhere else. `write/logic/restructure.rs` applies each
repair through the ordinary exact before/after revision path, replaces the
textual dangling guard with the inventory guard for these operations, checks
every rewritten spelling after the change, and records typed historical
citations that final validation resolves through
`trace/logic_mutations.yaml` and the read-side identity index. Ambiguous history, unrepresentable spellings,
self-citations, merge cycles and unclassified split citers fail closed with
`details.locations`. On the agent-cli fixture (release binary) a rename
with generated repair commits in ≈28 ms against ≈22 ms with a hand-written
row; the difference is one build of the read-side identity index that
validates the four historical citations.

## Command simplification

The live agent routes are `status`, `ls`, `show`, `find`, `edit`, `claim set`, `heuristic set`, `apply` and `merge`. Creation, staging, promotion and session setup/logging use existing typed JSONL operations. `show --with path,refs`, `ls --unfinished` and `show --identity` retain ancestry, citation, inactivity and exact imported-identity behavior. Tooling remains unchanged. See the [migration guide](../agent-cli.md#command-simplification-migration) for inputs and result mappings; old verification reports remain frozen historical evidence.

## Boundaries and remaining gates

Unknown fields, arbitrary source prose and unrelated spans survive. Native concept names keep their own namespace. Compiler heuristics preserve singular Source and full Bounds without invented PM fields.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{logic,fields,documents}.rs; crates/ara-core/src/write/logic/{citations,restructure}.rs; crates/ara-cli/src/write.rs`.

Permanent consumer regressions: `write_logic_documents.rs, agent_writes.rs, agent_citation_rewrite.rs, agent_citation_reads.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
