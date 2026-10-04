# 04: Preserve native identities through peer-feedback merges
**Date:** 2026-10-03

Status: **approved** by the human developer on 2026-10-03 after design review. Implementation pending. Target repository: `ara-cli`, with portable provenance contract changes owned by `Agent-Native-Research-Artifact`. Parent: [collaborative research plan series](README.md). This plan adds required CLI/core work beyond [01: snapshot](01-ara-snapshot.md). Approval covers the design, not a completed fix, an upstream protocol release, a commit, or a paid run.

## TL;DR

Workers may merge published peer or canonical knowledge into their own forks and publish again. The current merger can reject that return path even when every prior merge succeeded and the caller supplies the correct source key and predecessor. Reconcile proven original identities before allocating destination IDs, and preserve source facts separately from each destination's import history. Keep conflicting interpretations and protected-history checks intact; do not solve the failure by dropping aliases or assigning a new fork identity.

## Problem

The design review exercised the following sequence with the actual `ara 0.1.23` binary. A shared seed contained C01. Forks A1 and B1 each added an unrelated local C77; B2 began as a copy of B1.

| Operation | Observed result |
|---|---|
| Canonical imports B1 using the seed | Exit 0, no unresolved conflicts. |
| Canonical imports A1 using the seed | Exit 0, no unresolved conflicts. |
| B2 imports A1 using the seed | Exit 0, no unresolved conflicts. |
| Canonical imports B2 using exact B1 as base and the same `fork-b` key | Exit 1, `merge.alias_conflict`, conflicting alias `fork-a:C77`. |

Before the last operation, `ara resolve fork-a:C77` returned C03 in canonical and C78 in B2. This is the same original A claim arriving through two routes, a diamond-shaped import history. It is not a scientific disagreement or a worker editing protected history. The temporary review fixtures were removed; the implementation must retain this scenario as a regression test.

The relevant implementation is [identity allocation](../../crates/ara-core/src/merge/identity.rs) and [merge planning and foreign history](../../crates/ara-core/src/merge/mod.rs). Allocation currently reuses prior mappings for the current transport source key before combining inherited aliases. Foreign-history comparison also includes destination-specific import context. Fixing the alias collision alone is insufficient: importing the same source revision independently at different destinations must not turn differing import times or local mappings into a source-history violation.

## Constraints

- Keep `ara` deterministic, offline, and model-free. The runner supplies immutable local snapshots and verifies their contribution identities.
- Preserve original source-qualified identities, immutable source bytes, audit history, and conflict candidates. Equal text, equal local IDs, display labels, or package parentage alone do not prove identity.
- Keep a fork's stable source key across peer imports and later publications. Do not reset provenance to evade a conflict.
- Reconcile identity separately from content. Two routes to one original claim may carry different mutable interpretations; they must produce one identity with a normal content conflict when warranted.
- Preserve fail-closed behavior for forged or ambiguous aliases, changed protected history, unknown ledger fields, source regression, and unproven ancestry.
- Do not install incoming code or evidence. External-file disposition and package-backed reads follow [02](02-contribution-workflow.md#how-code-and-evidence-conflicts-are-closed).
- Follow the [zero-cost contract](README.md#zero-cost-when-collaboration-is-unused). No store or runner dependency enters merge. Ordinary commands must not build an origin graph, capture extra evidence bodies, or initialize import metadata just because peer-feedback support is installed.

This work does not introduce automatic semantic deduplication, arbitrary graph repair, concurrent canonical writers, or a network merge service. It establishes identity for entries whose shared origin is supported by validated native provenance and exact retained inputs.

## Proposed approach

### Resolve original identities before allocating local IDs

Build an origin-to-destination mapping from validated base, ours, and incoming provenance before allocating IDs. An origin includes source key, native identity namespace and selector; retained source revisions establish which version and content the mapping covers. Follow existing audited redirects without conflating node, claim, document, and heading namespaces.

For an incoming entry with a proven origin already present in ours, reuse that destination identity. A previously recorded mapping for the current source must agree with this result. If one origin has incompatible destinations that cannot be reconciled through existing audited redirects, reject with complete evidence rather than choosing one. Allocate fresh IDs only for genuinely new origins; two unrelated local C77 entries remain distinct.

Use the resulting mapping for structured references, aliases, protected-history comparisons, and content merge planning. Do not rewrite quoted history or opaque prose. Identity reconciliation must not silently choose incoming content when the same original entry changed along both routes. Compare the preserved source/base versions and retain the normal mutable conflict candidates.

### Preserve source facts and every import event

Distinguish the immutable source revision from an import of that revision into a particular destination. A source revision binds its exact source bytes and original identities. Import events retain destination-specific base, predecessor, timestamp, local mappings, and audit ownership. Two destinations importing the same source at different times create different import events, not conflicting source facts.

Compare shared source facts when validating repeated foreign provenance; do not require destination-local events to be byte-identical. Preserve each event's original record and route identity so that later transport and replay cannot duplicate, overwrite, or reinterpret it as a new source revision. Matching revision fingerprints do not excuse conflicting original source content or unsupported identity assertions.

Keep source advancement and receipt recovery distinct. The merger advances a source against its exact last imported revision. The runner returns a retained receipt for an older contribution already integrated into the same destination; it never replays that older contribution as a source rollback.

### Support the complete feedback loop

Acceptance includes both peer-to-worker-to-canonical and canonical-to-worker-to-canonical paths. A new fork may start from a published non-seed snapshot if its starting revision and inherited provenance are retained and verified. Existing forks keep their source identity and previous-source snapshots when they absorb canonical updates.

Protected histories remain immutable after identity relocation. Imported unresolved conflicts retain their source ownership; a destination cannot invent a local resolution for a conflict whose allowed choices are empty. The source owner resolves it, publishes a later revision, and the destination imports that resolution. A clean mutable edit rejected on scientific grounds still needs an explicit audited logic revision, not an identity exception.

### Preserve the ordinary paths

Run origin reconciliation only where the merge or identity operation requires it. An artifact without import metadata keeps its existing lightweight read/write path. Keep ordinary parser loads, guarded-write `ArtifactSnapshot::load`, and lazy identity snapshots separate from `load_complete`. Do not add eager origin-index construction to command dispatch or repeat full ledger decoding for each entry. Validate present provenance with the same fail-closed guarantees; absence of collaboration use is not a reason to trust malformed metadata.

Measure existing single-source and linear-history imports separately from diamonds and canonical feedback. Preserve validated common paths when the inputs do not require multi-route reconciliation. Reuse parsed provenance and mappings within one operation rather than rereading or copying the history for each lookup. The [series measurement gate](README.md#how-the-zero-cost-requirement-is-checked) applies to every affected CLI stage, including default and store-enabled builds.

### Implementation steps

Phase 1 freezes the portable representation; phase 4 implements and verifies feedback merges. Each stage or declared substage has its own PR targeting `feat/collaborative-ara`, following the [stage PR instructions](README.md#stage-pr-instructions). Link the owning protocol PR and gate the new writer format on reader adoption. Keep the identity and source-history fix coherent; do not ship an intermediate alias-error bypass.

1. Add the demonstrated diamond scenario to `crates/ara-core/tests/merge_identity.rs` and a real-binary case to `crates/ara-cli/tests/agent_merge_identity.rs`. Preserve the failing-before evidence. Add a case where independent imports have different timestamps even when their destination IDs happen to agree.
2. Specify the source-fact and import-event representation in the owning protocol contract. Inspect all exported API references before changing types. Use the current ledger format where it can preserve these distinctions without reinterpreting history; if a new schema is required, freeze its migration and unknown-field rules before editing persisted records.
3. Update `crates/ara-core/src/merge/identity.rs` to reconcile validated origins before allocation, and `crates/ara-core/src/merge/mod.rs` to use that mapping consistently and compare foreign source facts separately from import events. Update the shared ledger types and readers as required by the frozen representation.
4. Update directory and Git adapters only where the shared contract requires it. Migrate every affected caller and identity consumer; do not add a bypass flag, ignore alias errors, or retain an obsolete write format.
5. Run real directory and local-Git feedback scenarios, including source advancement, protected-history refusal, local and inherited conflicts, and restart/replay through the runner in [02](02-contribution-workflow.md#engineering-checks).
6. Document the delivered behavior in `docs/agent-cli.md` and a design record under `docs/`. For each functional PR, update the workspace patch version, all affected local package versions in `Cargo.lock`, and `CHANGELOG.md`. Run a non-locked workspace check after the version update, then the locked gates. Core behavior changes require reviewing and rebuilding the embedded viewer, not relying only on its source freshness hash.

## What must pass before the runner enables peer imports?

| Scenario | Required behavior |
|---|---|
| Demonstrated A/B diamond | One canonical identity for A's original claim; B's unrelated original claim stays distinct. No alias rejection or duplicate native entry. |
| Different independent import times and mappings | Both import histories survive; shared source facts agree after proven identity mapping. |
| A known entry edited on both routes | One identity with retained base/ours/theirs content and the expected mutable conflict, not silent deduplication. |
| Canonical feedback and a non-seed starting fork | Histories and original aliases remain resolvable after the return import. |
| Later A and B revisions | Exact predecessor checks still apply; previously allocated identities remain stable. |
| Latest replay and older receipt lookup | No duplicate entries or import events; the runner returns the older receipt without regressing native source state. |
| Forged alias, ambiguous origin, or protected-history edit | Reject before destination mutation with complete conflict evidence. |
| External code/evidence and inherited conflicts | Files stay in their frozen payloads; only allowed local acknowledgments succeed; source-owned resolutions arrive through later imports. |
| Local-Git feedback | Same native identity/content results as directory mode, with Git ancestry checks retained. |
| No import metadata; ordinary reads and guarded writes | Same behavior and lightweight loads, with no new origin graph, store access, or opaque-body capture. |
| Existing simple and long linear-history imports | Retain identity/history correctness and pass the predeclared latency, memory, and I/O regression gate against the baseline. |

Run unit, integration, and functional tests plus the actual CLI smoke. Assert native bodies, source-qualified resolution, exact history, conflict candidates, and absence of duplicates, not just successful exit codes. Finish with locked workspace tests, formatting, all-target Clippy, native/wasm checks, and the embedded-viewer verification required by the affected code.

## Alternatives considered

- **Package-only reuse forever.** It avoids the failing merge route but prevents workers from maintaining native knowledge that incorporates peers. Rejected as the full design; read-only package inspection remains a useful operation.
- **Change the returning fork's source key or drop inherited aliases.** This loses identity continuity and provenance. Rejected.
- **Deduplicate by text or numeric ID.** Unrelated findings can share either. Rejected.
- **Ignore foreign-history differences.** This can hide protected-history edits. Compare the correct source facts while preserving destination-specific events instead.

## Tradeoffs

Reconciling proven origins adds complexity to the merger and may require a portable provenance schema revision. The demonstrated feedback failure requires this work, but accepting additional import routes must not weaken history checks. Retaining predecessor snapshots and receipts costs storage and keeps the evidence for identity mappings recoverable.

## Migration

Existing ARAs without import metadata keep their current behavior. Existing ledgers and aliases remain evidence: do not delete or rewrite old protected records to make the new planner accept them. If their origin is insufficiently established, fail with a specific repair requirement rather than guessing. Any schema transition must retain original bytes, provide an explicit audited migration where needed, and be adopted by every reader before the new writer format is enabled.

The current binary lacks the approved feedback capability. Keep native peer imports disabled until the regression and feedback acceptance cases pass. Package-only reuse does not satisfy those acceptance criteria.

## Next Steps

1. Retain the review's counterexample as failing regressions and freeze the identity/provenance representation with the protocol owner.
2. Implement origin reconciliation and history transport together, then exercise the complete return path.
3. Enable runner peer imports only after the required checks pass. After implementation, move this plan's completed design into `docs/` and remove the plan under repository policy.
