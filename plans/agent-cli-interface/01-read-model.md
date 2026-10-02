# PR 01: extend the agent read model
**Date:** 2026-10-01

Status: **approved** 2026-10-01. Repository: `ARA-Labs/ara-cli`. Depends on approval of this plan, not on PR 00 format changes. Parent: [agent CLI interface](../agent-cli-interface.md). Rollout and shared checks: [PR index](README.md).

## TL;DR

Add staging, sessions, heuristics, experiment plans, and taste comments to the existing `Manifest` as optional collections. Preserve all published claim content needed by agents, including proof prose that the current parser reduces to experiment IDs. Keep filesystem loading native-only and keep the wire model usable by wasm. This PR supplies the data for query and search commands without changing the ARA file format or viewer interface.

## Problem

`parse_dir` currently loads the exploration tree, claims, paper metadata, logic sections, solution file bodies, and evidence. It does not load staging or sessions, and `heuristics.md` is only a recipe body. `claims.rs` recognizes Statement, Status, Proof, and Dependencies; Proof retains only `E` tokens. Agents need provenance, falsification criteria, tags, complete prose, promotion pointers, and session links to answer the parent plan's structural questions.

The parent names `T` references although its Phase 1 list omits taste-log loading. Include the existing optional `trace/taste_log.yaml` here so `show T01`, `refs`, and status do not advertise an unreadable entry kind. Concepts currently use terms, not numeric IDs; preserve that identity.

## Constraints

New collections use `serde(default, skip_serializing_if = "Vec::is_empty")`; new optional fields default and disappear when absent. Existing wire keys and frozen geometry stay unchanged. Existing `parse_sources(tree, claims)` remains available; optional-layer parsing does not introduce filesystem access into that API. No viewer panels, new protocol fields, alias format, or node-artifact/concept link grammar ships here.

Missing optional files mean empty collections. Present malformed optional files produce new stable `ARA2xx` warnings and retain any safely parsed entries; unreadable files must remain distinguishable from absent files in the native loading result used by new commands. Do not reinterpret staging `O` IDs as the problem document's local observation labels. Published spellings, especially Falsification versus Falsification criteria, must be checked against the pinned protocol fixture and documented without rewriting source.

## Proposed approach

Extend `crates/ara-core/src/manifest.rs`, `claims.rs`, `parse.rs`, `rules.rs`, and `lib.rs`. Add `agent_layers.rs` (new, native-gated) for optional-file readers with pure string parsers inside the module. Proposed collections are `observations`, `sessions`, `heuristics`, `experiment_plans`, and `taste_comments`; review the serialized names before publication. Store typed IDs, source identity, full prose, raw promotion destination, provenance, timestamps, and complete session event/history records needed by later commands. Keep alias resolution for PR 08.

Preserve the current `Claim.proof` token list for existing viewer consumers and add full proof content under a distinct optional field. Add optional claim provenance, falsification criteria, tags, conditions, sources, and revision pointer as found in the published format. This is additive model widening, not a second claim parser. Maintain a source-field representation for additional recognized content so later `show --full` and `refs` cannot silently omit evidence or multiline values. The normalized model is not a lossless editing model; PR 03 owns source spans and untouched-field guards.

1. Inventory the actual published field spellings against a pinned `the-ara-of-ara` copy. Record its full source revision and license in fixture attribution. Use `crates/ara-core/tests/fixtures/agent-cli/` (new), matching the existing fixture convention instead of inventing a root fixture tree.
2. Extend typed models and serde defaults. Find and update every `Manifest`, `Node`, and `Claim` constructor in core, viewer, wasm, and tests using language-server references where available. Keep all existing wire fields unchanged.
3. Add optional-layer readers. Read sessions in deterministic date/sequence order and cross-check their index rather than double-counting index rows as sessions. Preserve event turns, summaries, logic revisions, and references. Warn on duplicate layer IDs, malformed promotion destinations, and dangling session-index rows with newly allocated codes, not reused numbers.
4. Extend `parse_dir` composition after the base tree/claim parse. Preserve its current semantic-error contract. Supply an internal detailed load result for PR 02's status diagnostics without changing existing `validate` JSON shape. Do not present a duplicate-ID-truncated manifest as a complete read result.
5. Parse `H` headings in `logic/solution/heuristics.md` while retaining its existing recipe body. Parse `E` headings in `logic/experiments.md` and optional `T` records in `trace/taste_log.yaml`. Keep source order within documents.
6. Update native and wire compatibility tests, fixture provenance, and `docs/agent-cli.md` (new at implementation). Regenerate the embedded viewer if the changed core wire types alter its compiled wasm; the freshness script does not hash core sources.

## Alternatives considered

A query-only second model would avoid additive manifest fields but duplicate entry definitions and resolution logic. Use the existing manifest as recommended in Q1. Do not replace Proof's existing token list with prose, because that changes an existing wire type.

## Tradeoffs

The complete optional layers increase parse work even for existing native callers. Measure loading before adding caches or a separate partial loader. Tolerant optional parsing supports the corpus, but incomplete data must carry diagnostics so agents do not mistake omissions for absence.

## Migration

Old manifests deserialize with empty new collections. Artifacts need no rewrite. Coordinate the parser half of issue #61 to avoid duplicate work; viewer rendering remains outside this PR. Node artifact pointers and node-to-concept links from #62 and #63 require PR 00 approval and can be added by their own approved work without blocking these existing-format readers. Apply the index's functional-PR version, lockfile, changelog, and bundle rules at implementation.

## Verification and acceptance

Unit fixtures cover absent and malformed files, multiline Markdown fields, exact proof prose, promotion destinations, duplicate IDs, session chronology, missing index targets, heuristic/recipe coexistence, and the `O` namespace collision. Round-trip an old manifest and compare its serialized keys; new empty collections must not appear. Extend `crates/ara-core/tests/parse_fixtures.rs` and `rule_codes.rs` for behavior and diagnostic-code stability. Run the existing corpus no-panic sweep without requiring malformed artifacts to become clean.

Run `cargo run -p ara-cli -- layout <pinned-fixture> --json` as the binary smoke. Inspect concrete staging, session, heuristic, experiment, taste, and claim values in the emitted manifest, then run `validate` on an existing official fixture and confirm its existing output contract. Run the native and no-default-features wasm checks from the index. Acceptance requires each loaded field to match the source fixture, not merely nonempty collections.

## Next Steps

Review Q1 and the proposed optional wire names. Merge this PR before [PR 02](02-read-commands.md); PR 00 and baseline pinning can proceed independently.
