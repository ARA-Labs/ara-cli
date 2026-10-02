# PR 07: link nodes that describe the same finding
**Date:** 2026-10-01

Status: draft for review. Repository: `ARA-Labs/ara-cli`. Parent: [agent CLI interface](../agent-cli-interface.md). Shared rollout and verification: [PR index](README.md). Depends on [PR 03](03-guarded-node-writes.md), read support from PR 02, and approved F5 in [PR 00](00-protocol-contracts.md). This PR does not block [PR 06](06-batch-apply.md) or [PR 08](08-directory-merge.md).

## TL;DR

Add `ara link N131 --same-as N128` after the protocol approves `same_as` and its trace-mutability exception. Preserve both nodes and their source history, adding only the approved pointer on the caller-selected node. Extend `show` and `refs` so agents can read the relation without treating it as a dependency or deleting a duplicate. Reuse PR 03's source guard and transaction engine. Any batch support is added to PR 06's existing union only if that PR has already landed.

## Problem

The parent wants a way to record that two nodes describe the same finding without replacing or deleting either historical record. Current `RawNode` and `Manifest` have no `same_as` field; adding YAML alone would produce an unknown-field warning and would not expose the pointer to `show` or structural `refs`. The relation also changes an existing trace record, so F5 must approve that specific forward-pointer mutation instead of relying on a generic edit command.

Node IDs and dependency edges serve different purposes. A duplicate-finding annotation must not enter cycle detection for research dependencies, alter parent nesting, or redirect claims and sessions automatically. Later merge may rewrite imported pointers as structured references, but alias resolution and merge identity belong to PR 08.

## Constraints

F5 is a recommendation in the parent, not an approved published field. Approval must specify the field spelling/type, direction, permitted transitions, and meaning of the phrase "later node." Do not infer creation chronology from numeric IDs, DFS order, or imported source labels. This draft proposes that the caller selects the node receiving the annotation. If the approved protocol requires a provable chronological ordering, define an authoritative creation-order source before shipping the command.

Keep both nodes, all their fields, nesting, dependencies, evidence, and session history intact. No LLM duplicate detector, merge of content, canonical-node deletion, alias log update, transitive dependency inference, or viewer panel is part of this PR. Unknown source fields remain protected by PR 03. New parsing and pure relation queries must preserve wasm compatibility; only file mutation and locking are native-only.

## Proposed approach

Add optional `same_as: Vec<NodeId>` to `Node` in existing `crates/ara-core/src/manifest.rs` using `serde(default, skip_serializing_if = "Vec::is_empty")`. Add the approved raw field in `schema.rs`, preserve source order in `parse.rs`, and allocate stable diagnostics in `rules.rs` for malformed, dangling, or self-referential annotations as approved. Do not put the relation in `LinkKind::DependsOn` or `bindings`. Extend PR 02's pure query/structured reference collector and read adapters so `show --with same_as` exposes outgoing and incoming annotated nodes and `refs` classifies the field as a structural reference. The proposed query result relation names must be reviewed before publication.

Add `crates/ara-core/src/write/links.rs` (new, native-only), extend the operation enum and CLI adapter from PR 03, and add a `Link` command in existing `main.rs`. The operation shape is proposed:

```json
{"op":"node.link_same_as","node":"N131","same_as":"N128"}
```

The CLI supports `ara link N131 --same-as N128 [--json]`. Both endpoints must resolve to one node in the locked snapshot. Adding a pointer already present is an idempotent no-op with no byte changes. Reject unknown endpoints, self-links, duplicate source IDs, invalid existing pointer syntax, and unauthorized node mutations. Preserve existing pointer order and append the new target. No removal or replacement command ships without an approved protocol rule. The annotation's direction is caller-selected under this proposal; the receiver is normally the later finding, but the tool does not invent a creation date.

1. Review F5's exact syntax, direction, chronological wording, and mutability exception upstream. Decide whether reciprocal and chained annotations are valid. This draft proposes directional stored annotations with incoming-reference queries, no automatic reciprocal writes, and no transitive collapse of nodes. If F5 instead defines equivalence classes, review cycle/canonicalization and read-output semantics explicitly before implementation.
2. Widen raw and normalized models and every `Node` literal/consumer in core, viewer, wasm, and tests. Old manifests must deserialize with an empty relation and emit no new key when empty. Parse stored `same_as` values independently of dependency cycle detection. Retain unrelated unknown and wrong-kind source fields.
3. Add source-position insertion or list append using PR 03, with an intent for exactly one pointer addition to one node. Reparse the whole candidate and check reference validity and diagnostic containment. The complete-source comparison must prove no node content, dependency, parent, claim binding, or session record changed. Unsupported target syntax rejects byte-identically instead of serializing the node.
4. Add CLI output using proposed `format: "ara.link/v1"`, source/target, relation, changed paths, no-op flag, diagnostics, and committed status. Use PR 02 discovery and exit codes. Implement outgoing/incoming `show` data and structural `refs` for this field; an annotation must be inspectable immediately after a successful write.
5. If PR 06 has landed, register `node.link_same_as` in its existing operation union and binding/reference registry using the same planner. Otherwise keep the library variant ready for that later integration without making PR 06 depend on this feature. Tell PR 08's reference-rewrite registry that approved `same_as` values are structured node pointers, preserving import-side-only rewrites.
6. Add implementation tests and docs, bump the then-current patch in `Cargo.toml`, refresh `Cargo.lock` through non-locked `cargo check --workspace` before locked gates, and update `CHANGELOG.md` and `docs/agent-cli.md`. Apply the index's Rust 1.94.1 checks. Manually rebuild the viewer embed for the wire/core change; the embed hash does not detect changed core sources and normalizes version-only differences.

## Alternatives considered

Deleting one duplicate node erases source history and breaks references. Redirecting every citation to a chosen canonical node also changes the meaning of existing records. An additive typed pointer preserves the historical nodes and leaves content reconciliation to the researcher.

A symmetric dependency edge would reuse the existing graph model but misrepresent sameness as research causality and could create dependency cycles. Keep this relation separate and let reads report both incoming and outgoing annotations without rewriting both nodes.

## Tradeoffs

Directional annotations can leave chains or conflicting judgments that need researcher review. The CLI validates storage and references, not whether the findings are genuinely identical. If the approved format allows chains, queries return the recorded relations explicitly rather than silently computing a canonical representative.

The write inherits PR 03's cooperative-lock and recovery limits. A single-file rename is atomic, but multi-operation batches still have per-file visibility limits for raw readers. This feature changes the wire model even without a viewer display change, so all constructors and the embedded wasm need attention.

## Migration

Existing artifacts and manifests have an empty relation. No nodes are automatically linked or renumbered, and unapproved legacy equivalence fields are not silently converted. Existing unknown fields remain untouched by other writes. Imported alias labels are supported only after PR 08 implements its approved source identity/resolution contract; do not advertise `bob:N131` acceptance before that work exists.

## Verification and acceptance

Add proposed parser/query unit cases in the affected existing modules, write tests in the new native links module, and functional tests in existing `crates/ara-cli/tests/cli.rs`. Cover old-manifest compatibility, absent/nonempty serialization, outgoing/incoming structural refs, dangling/self endpoints, ambiguous IDs, unknown fields, multiline source style, and repeated-link no-op. Verify complete node content and parent/dependency/binding/session data are unchanged. A deliberately altered unknown field in a candidate must fail the source intent guard even when the normalized graph would appear unchanged.

Test that a same-finding annotation does not create or remove a dependency edge or participate in dependency cycle detection. Add approved reciprocal/chain behavior cases only after F5 settles their semantics. If batch support is present, test typed provisional endpoints, a later batch failure restoring the source, and the normal transaction fault/recovery contract. Coordinate PR 08's source-side structured-pointer rewrite scenario without making merge a dependency of this PR.

The proposed real-binary smoke uses a disposable approved fixture containing `N128` and `N131`, with their chronology compliant with the final F5 rule. Set `SMOKE_ARA` to that copy and run after implementation:

```sh
ara link N131 -C "$SMOKE_ARA" --same-as N128 --json
ara show N131 N128 -C "$SMOKE_ARA" --with same_as --full --json
ara refs N128 -C "$SMOKE_ARA" --json
ara link N131 -C "$SMOKE_ARA" --same-as N128 --json
ara link N131 -C "$SMOKE_ARA" --same-as N131 --json
ara validate "$SMOKE_ARA" --json
```

Expect one stored pointer on `N131`, its outgoing/incoming relation in readback, a structural citation in `refs`, no source change on repeat, exit 1 on the self-link, and no new validation errors. Inspect both node bodies and dependency paths to prove neither record was collapsed or reparented. This documentation task runs no commands or checks.

## Next Steps

Approve F5 and the separate relation/query shapes before implementation. Review the chronology rule and reciprocal/chain semantics without treating numeric ID order as history. Integrate the optional operation into PR 06 and the structured-reference registry into PR 08 when those PRs are available, without blocking either release on F5.
