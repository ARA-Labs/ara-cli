# PR 04: edit mutable logic and approved pointer fields
**Date:** 2026-10-01

Status: **approved** 2026-10-01. Repository: `ARA-Labs/ara-cli`. Parent: [agent CLI interface](../agent-cli-interface.md). Shared rollout and verification: [PR index](README.md). Depends on [PR 03](03-guarded-node-writes.md) and its approved [PR 00](00-protocol-contracts.md) contracts. Compiler-wide document creation and complete revision recording are completed in [PR 06](06-batch-apply.md), not assumed here.

## TL;DR

Add claim and heuristic creation and field updates, plus `ara edit` for mutable logic entries and approved trace/staging pointers. Use the existing node and claim architecture, preserving full source content that the normalized model does not retain. Introduce bounded document selectors for concepts, related work, experiment plans, and existing logic sections without inventing protocol IDs. Parse long text as complete field values, including Markdown continuation lines. This PR supplies editing primitives but does not claim that the compiler or research-manager can already run entirely through the CLI.

## Problem

`claims.rs` reads one line per recognized bullet and reduces Proof to experiment tokens. It ignores Conditions, Sources, evidence prose, and other fields, while `sections.rs` deliberately tolerates incomplete Markdown. Editing against those normalized values could erase valid text or silently accept a truncated multiline replacement. Concepts have term-name identity, related work has `RW` headings, and solution recipes have filenames; treating them as new numeric entry kinds would change the protocol.

The parent allows editing logic but limits trace/staging edits to F4. The current manager skill requires full before/after history for logic revisions, while its mutable session metadata and stale flag exceed the parent's pointer whitelist. Add narrowly typed operations and obtain PR 00 approval for those permissions. Session commands in PR 05 and atomic skill workflows in PR 06 must preserve the required history.

## Constraints

Every operation uses PR 03's complete snapshot, source positions, exclusive lock, intent guard, and transaction. No whole-file serialization, generic YAML path setter, arbitrary filesystem target, or edit-through-shell subprocess is allowed. Existing immutable node content, historical session entries, and observation content remain protected. F4-approved fields also need operation-specific transition rules; a whitelist is not permission to erase an event list or change an already-final promotion target. No status-transition judgment, closure inference, number grounding, or provenance upgrade runs inside `ara`.

Long-text input is resolved before the checkout lock. Split `--set key=value` at the first `=` only. A value beginning with `@file` reads the named UTF-8 file, `@-` reads stdin, and proposed `@@text` means literal `@text`; other values are literal. Preserve embedded `=`, quotes, blank lines, and trailing newlines without shell interpretation. At most one field may consume stdin; repeated assignments to the same canonical field reject rather than using last-wins behavior. JSON list fields such as Dependencies accept typed JSON arrays; prose fields remain strings. Unknown fields and incompatible types reject with the key and expected type. Review this grammar before documenting it as stable.

## Proposed approach

Extend the new native `crates/ara-core/src/write/` module with `logic.rs` and `fields.rs` (new), and extend existing `claims.rs` and PR 01's `agent_layers.rs` for complete multiline field parsing. Update existing `sections.rs` only where its field reader must retain a supported replacement. Extend the CLI write adapter from PR 03 and `main.rs`; keep field resolution and rendering in the library rather than duplicating them across commands. Proposed `WriteOperation` variants are `ClaimAdd`, `HeuristicAdd`, and `EntryEdit`; convenience setters become `EntryEdit` with the correct selector.

```json
{"op":"claim.add","id":"C06","title":"Finding","fields":{"Statement":"Mechanism described in full","Conditions":"Boundary","Status":"hypothesis","Dependencies":["C05"]}}
{"op":"heuristic.add","title":"Technique","fields":{"Rationale":"Why it works","Sensitivity":"unknown","Code ref":["pending"]}}
{"op":"entry.edit","target":{"id":"C05"},"set":{"Statement":"New complete wording","Proof":"E02 and its evidence explanation"}}
{"op":"entry.edit","target":{"document":"logic/concepts.md","heading":["Attention"]},"set":{"Definition":"A complete definition"}}
{"op":"entry.edit","target":{"document":"logic/related_work.md","entry":"RW01"},"set":{"DOI":"source DOI"}}
{"op":"entry.edit","target":{"document":"logic/experiments.md","entry":"E01"},"set":{"Setup":"A complete setup"}}
{"op":"entry.edit","target":{"id":"O12"},"set":{"crystallized_via":"verbal-affirmation"}}
```

`target` is a tagged choice: one globally resolvable ID, or an allowed document plus an existing entry/heading path. Do not identify a concept by `C` or a recipe by `H` unless the source already defines that ID. Require exactly one matching source block. Reject duplicate headings/IDs and use the document path to distinguish an experiment plan from an evidence object with the same `E` label. A document selector is a CLI locator, not a stored protocol identity. The proposed CLI form is `ara edit --document logic/concepts.md --heading Attention --set Definition=@definition.md`; repeated `--heading` values form a hierarchy. Existing `ara edit C05 --set Statement=@-` remains the short form.

The field registry records approved spelling, canonical lookup name, value type, source syntax, reference namespace, and allowed mutation for each entry kind. Claims cover all fields required by the pinned source, including Statement, Conditions, Sources, Status, Provenance, Falsification criteria or its reviewed source spelling, Proof prose, Evidence basis, Dependencies, Tags, and Last revised. Heuristics cover Rationale, Sources, Status, Provenance, Sensitivity, Bounds, Code ref, and Last revised. Preserve unrecognized bullets and prose even when they cannot yet be edited. Claim dependencies resolve to claims; experiment Proof links use the plan namespace; Code ref and source citations remain caller-supplied pointers rather than invented evidence. Falsification versus Falsification criteria is a protocol/parser compatibility decision, not a reason to silently rename source fields.

1. Review the field registry against PR 01 and the approved pinned protocol. Keep existing wire fields additive; use full source values for edits. Scan claim/heuristic IDs across source occurrences and reuse PR 03's allocator. Define required creation fields from the approved schema, and report missing fields before planning. Do not invent unsupported values or fill `[pending]` without the caller's instruction.
2. Extend PR 02's shared Markdown scanner with structural block/field positions rather than adding a second fence parser. A labeled field owns its indented continuation lines and nested lists until the next peer field or section boundary. Headings and bullet-like text inside fenced code are content. When supplied text contains a heading-looking line, render a valid indented continuation and have the parser recover the original field text; reject any representation that cannot round-trip exactly. Preserve untouched comments, unknown labels, body prose, and their order.
3. Add field-specific source edits for claims, heuristics, concepts, related work, and experiment plans. Existing `logic/problem.md` and `logic/solution/*.md` blocks can use reviewed heading selectors for existing fields; replacing or creating complete document bodies is reserved for PR 06. Reject selector path escapes and symlinks. Changes to entry IDs or headings need a reviewed reference-migration intent, so ordinary setters cannot silently rename them.
4. Implement the F4 whitelist as a native policy shared by all writes. Proposed trace/staging pointer fields are `promoted`, `promoted_to`, and `crystallized_via`; append-only cross-edge authorization comes from PR 03. Session event lists use typed append operations in PR 05, never unrestricted `--set`. Reject title/type/provenance/content changes on historical trace/staging records, and cite the approved protocol section in errors. Reject a lone `promoted: true` or destination update that would break promotion invariants; PR 05's `promote` provides the complete transition. `stale`, rolling session fields, and conflict annotations need explicit PR 00 decisions.
5. Validate exact source deltas for each field and reparse the complete candidate. Preserve full text outside target spans and complete unknown source values. Check introduced cross-reference errors and dependency cycles. Detect duplicate recognized field occurrences rather than editing the first one and hiding the rest. For unchanged assigned values, return a no-op without rewriting bytes.
6. Expose the adapters `ara claim add`, `ara claim set C05`, `ara heuristic add`, `ara heuristic set H03`, and `ara edit`. Use PR 02 output/error conventions and proposed `format: "ara.claim/v1"`, `"ara.heuristic/v1"`, and `"ara.edit/v1"` results with target, changed fields/paths, diagnostics, and committed status. A success means a guarded edit, not that the research judgment was correct or its required session history has already been recorded.
7. Add implementation tests and docs. Bump the then-current patch in `Cargo.toml`, refresh `Cargo.lock` using non-locked `cargo check --workspace` before locked checks, and update `CHANGELOG.md` and `docs/agent-cli.md`. Apply the index's Rust 1.94.1 gates. Update every affected model literal/consumer and manually rebuild the embed if parser/wire changes affect wasm; version-only bumps alone do not require it.

## Alternatives considered

A universal `--set yaml.path=value` would bypass immutable-layer permissions and allow accidental list replacement. Keep a reviewed field registry and typed append/promotion operations. Numeric IDs for every logic section would simplify lookup but change existing concept and recipe identities, so retain source-native identities and explicit document selectors.

Whole-block replacement is useful for the compiler but should not stand in for a field editor. PR 06 adds separate bounded document operations with their own full-body intent and coverage checks. This keeps an ordinary field setter's promise precise.

## Tradeoffs

The Markdown scanner must be stricter about targeted edits than the tolerant current readers. A file can remain readable while a write is rejected because its duplicate field or ambiguous heading cannot be safely changed. Retaining full prose adds optional model data, but protects consumers from silently shortened values. Exact source guards also retain fields that the viewer does not display.

Individual field commands cannot promise research-manager's complete cross-file revision history until session and batch operations are available. Documentation must say that CLI-backed manager integration remains blocked through PR 06. The engine performs syntax/reference validation, while the skills continue to decide status transitions, evidence quality, and whether a revision is warranted.

## Migration

No artifact-wide rewrite is required. Existing claims, concepts, related-work headings, and solution filenames remain their identities. Recognize approved legacy field spellings on read and preserve the spelling already present on edit; new entries use the approved canonical spelling. Do not introduce compatibility aliases for unapproved protocol grammar. For unsupported required fields discovered by PR 12, add a reviewed operation or mark skill integration blocked; direct-file fallback is not a migration strategy.

## Verification and acceptance

Add proposed unit tests in `write/logic.rs`, `write/fields.rs`, and the existing field parsers, plus functional tests in `crates/ara-cli/tests/cli.rs`. Cover complete claim/heuristic creation, no-op setters, exact multiline text including blank/trailing lines and code fences, Proof prose beyond ID tokens, Conditions/Sources/falsification retention, stdin/file input, literal `@`, embedded equals, invalid UTF-8, repeated keys, missing required fields, and unknown field/type errors. Check untouched prose/comments and unknown fields byte-for-byte. A malicious candidate changing an omitted field must fail the intent guard.

Exercise concept term names, `RW01`, plan `E01`, document-qualified namespace collisions, nested heading selectors, duplicate targets, unknown selectors, path traversal, and immutable trace/staging edits. Verify wrong-kind node fields survive unrelated edits. Test claim dependency cycles and dangling links; an error must leave all sources unchanged. For pointer edits, cover invalid promotion combinations, backward transitions, and unauthorized stale/session changes. Keep tests about visible behavior rather than implementation symbol names or copies of constants.

For a proposed real-binary smoke, use a disposable approved fixture containing `C05`, `H03`, and concept heading `Attention`. Prepare `statement.md` and `definition.md` with several paragraphs, quotes, and code, set `SMOKE_ARA` to that fixture, and run after implementation:

```sh
ara claim set C05 -C "$SMOKE_ARA" --set Statement=@statement.md --json
ara heuristic set H03 -C "$SMOKE_ARA" --set Rationale='Reason = observed mechanism' --json
ara edit -C "$SMOKE_ARA" --document logic/concepts.md --heading Attention --set Definition=@definition.md --json
ara show C05 H03 -C "$SMOKE_ARA" --full --json
ara edit N01 -C "$SMOKE_ARA" --set title='rewrite history' --json
ara validate "$SMOKE_ARA" --json
```

Expect complete supplied text in readback, changes limited to selected mutable fields, exit 1 for the immutable title update, and no introduced validation errors. Inspect actual files and compare all unrelated bytes. No commands or tests run during this documentation task.

## Next Steps

Approve field spellings, selector syntax, long-text escapes, and operation-specific F4 rules. Review any required ID/heading rename operation before adding it. Build [PR 05](05-staging-and-sessions.md) next, and require [PR 06](06-batch-apply.md) plus [PR 12](12-pin-skill-contracts.md) coverage before calling these commands a complete compiler or manager interface.
