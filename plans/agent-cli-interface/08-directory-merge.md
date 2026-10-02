# PR 08: Merge complete ARA directories

**Date:** 2026-10-01

Status: **approved** 2026-10-01. The durable transaction journal moved here from PR 03, and the merge timing threshold was set, at approval. Target repository: `ARA-Labs/ara-cli`. Parent: [Agent CLI interface, Phase 3](../agent-cli-interface.md#phase-3-ara-merge). PR map and shared release gates: [README](README.md). Dependencies: [PR 06: batch apply](06-batch-apply.md) and approved F1 through F4 plus the merge-specific decisions in [PR 00: protocol contracts](00-protocol-contracts.md). The lossless source and transaction interfaces originate in [PR 03](03-guarded-node-writes.md); session and promotion rules originate in [PR 05](05-staging-and-sessions.md). Neither [PR 07: same-as links](07-same-as-links.md) nor search is a prerequisite. [PR 09](09-git-merge.md) supplies Git snapshots to this same merger.

## TL;DR

Add a complete three-way directory merger that understands trace entries, observations, sessions, mutable logic entries, and references across the artifact. Compute the whole result in memory and commit it through the guarded multi-file writer from PR 06. This PR also adds the durable crash-recovery journal that PR 03 deferred, so every write command gains it. Preserve our text, map imported identities consistently, and report every unresolved field conflict with the values needed to decide it. Shipping depends on approval of portable import identity and conflict records, because a log of renamed IDs alone cannot make repeated merges safe.

## Problem

The parent plan describes two forks that each allocate `N124` under different parents. Git can accept both appends without a textual conflict, but the resulting artifact has a duplicate node ID. The current normalizer in `crates/ara-core/src/parse.rs` drops the duplicate node and its subtree from its normalized view. A merger built from that view could lose a branch before it starts.

The current model is also incomplete as a write source. `schema.rs` retains unknown field names through `IgnoredAny`, not their values. `parse_dir` loads tree, claims, selected logic documents, `PAPER.md`, and evidence, but does not load staging or complete session history. `Manifest` identifies concepts by term, related work by its existing string ID, and solution recipes by filename stem. These identities cannot all be treated as numeric claim IDs. PRs 01 through 06 must supply complete source views and the write coverage needed before merge can ship.

Existing `fix.rs` provides useful precise-text and reparse guards, including counting repeated errors. It writes tree and claims with `std::fs::write`; it is neither an atomic multi-file transaction nor a merge implementation. This PR reuses the planned writer, not that persistence path. The current `ara check` exit distinction in `crates/ara-cli/src/check.rs` provides the error convention to retain.

## Constraints

This PR ships all Phase 3 knowledge layers together. A nodes-only or nodes-and-claims merge cannot be presented as complete while sessions, promotions, aliases, mutable knowledge documents, or imported references remain unsafe. It uses no network, LLM, database, search index, or automatic semantic deduplication. `duplicate_candidates` remains an empty array until [PR 11](11-duplicate-warnings.md). If F5 has shipped, known `same_as` references pass through the shared scanner; otherwise preserve that source field and report uncertainty rather than require F5. The parent forbids writes to `src/` and evidence bodies: inventory those paths for external merge/review needs, but do not copy, modify, delete, or rewrite them. Any broader file-merge scope needs explicit parent approval.

The merge planner is a pure in-memory function. Directory loading, locking, time acquisition, and commit belong to native adapters. Gate merge and its source/writer dependencies behind `ara-core`'s existing `native` feature; the wasm parser, viewer wire format, and layout behavior stay unchanged. The merger must preserve unknown field values, comments, scalar style, untouched whitespace, complete Markdown bodies, and all session lists. A clean normalized `Manifest` is necessary for known semantic checks, but cannot prove preservation of the complete source.

Retain the parent's `ara merge --base <dir> --theirs <dir> [--as bob] [--dry-run] [--json]` interface and PR 02's destination discovery. Inputs must describe the same artifact lineage. Base and theirs are read-only snapshots; the only persistent writes are to ours. Reject overlapping source and destination roots, unsafe path traversal, unsupported symlinks or special files, and ambiguous input identities before commit. The exact source-enrollment rule remains a review gate below.

## Proposed approach

### Which protocol decisions must be approved first?

F1 through F4 are prerequisites, not approval of all rules below. Ask PR 00 to record these additions or a reviewed alternative. The recommendations make the design executable without describing them as existing protocol features.

| Decision | Proposed rule and why it needs review |
| --- | --- |
| Every imported identity | Persist a mapping for every imported entry, including an unchanged ID, session identity, path-based document identity, and nonnumeric selector. F1's renamed-only aliases miss noncolliding imports and cannot prevent their duplication on replay. |
| Stable source identity | Separate an opaque source key from the display label and a source-revision fingerprint. An optional `--source-id <key>` spelling is proposed, not approved. The parent `--as` interface can remain available if labels are unique, permanently bound to one fork lineage, and never reused. Reject an ambiguous enrollment instead of guessing from basename, contents, or directory path. |
| Revisions of the same fork | Retain the same source key across new source revisions and reuse every established mapping. Persist the prior imported source values so later changes can use the last imported revision as their effective ancestor. Reject an older or unrelated revision whose lineage cannot be established. |
| Same-value concurrent changes | When both sides change a mutable field to the same semantic value, take that value without conflict. Our lexical representation wins. The parent's shorthand that every two-sided edit conflicts needs this explicit refinement. |
| Conflict lifetime | Persist unresolved conflicts and explicit resolution decisions in portable artifact files. Replaying identical source bytes must report the same unresolved conflicts and exit 1 without another append or allocation. A successful prior import is not evidence that a conflict was decided. |
| Protected content changes | Detect changes or deletion of base trace, staging, immutable session rows, and any other protocol-protected content on either side. Do not permit `ara edit` or a generic conflict resolver to bypass F4. Rejection, a new append-only corrective record, and an explicitly approved restoration operation are distinct choices. A complete resolution workflow needs a protocol decision. |
| Promotion disagreement | `promoted: true` wins over `false` only when the resulting target and crystallization metadata form a valid promotion. Two different non-null promotion targets are a conflict, even when both flags are true. |
| Deletion | Permit agreed mutable-entry deletion and one-sided mutable deletion only when the other side is unchanged and all resulting references validate. Delete-versus-edit is a conflict. Protected deletion is rejected; both forks deleting a protected entry does not authorize it. |
| Session history | Approve identity, append-union rules, and the mutable metadata whitelist for `last_turn`, `turn_count`, `summary`, index counts, and open-thread state. The F4 mention of `events_logged` alone does not cover full session records or two forks appending the same turn number. |
| Opaque content | Preserve full incoming values in conflict evidence when an unsupported field or file cannot be merged safely. Never omit it because a typed reader does not understand it. Rules for evidence, source artifacts, manifest metadata, and unknown extension documents must be explicit. |

Recommend keeping F1's `trace/aliases.yaml` for address redirects and introducing `trace/merge_log.yaml` as a new, append-only portable merge journal, subject to PR 00 approval. Its proposed record kinds are source enrollment, source revision, import mapping, conflict, and resolution. Each revision records the source key, label, base fingerprint, source fingerprint, predecessor where known, and all mapping results. Conflict records retain exact base, ours, and theirs values or entry/file bytes, their selectors, and content fingerprints. Store enough prior source field state to merge a later revision without requiring access to the old source directory. Neither `.ara/` cache data nor a printed report can be the only copy of this information.

The journal schema, storage of large values, source-key enrollment, conflict command spelling, and protected-content repair policy are approval gates. If reviewers prefer an expanded aliases schema or another portable file, update PRs 00, 01, 02, 06, 08, and 09 before implementation. Do not create a local-only workaround while leaving replay acceptance unmet.

### Which files and interfaces change?

All new paths below are proposed. Interfaces from earlier PRs are planned dependencies, not APIs in the current checkout.

| File | Change |
| --- | --- |
| `crates/ara-core/src/merge/mod.rs` (new) | Pure `plan_merge` orchestration, `MergeOptions`, `MergePlan`, `MergeReport`, and typed errors. Native-gated exports, with no filesystem access inside the planner. |
| `crates/ara-core/src/merge/identity.rs` (new) | Source/revision checks, scoped entry identity, deterministic allocation, full import maps, alias validation, and portable journal records. |
| `crates/ara-core/src/merge/layers.rs` (new) | Three-way field decisions, protected-entry checks, full session reconciliation, document-body decisions, and deletion handling. |
| `crates/ara-core/src/merge/rewrite.rs` (new) | Reuse PR 02's scanner over source-tagged text ranges and PR 03's exact text positions. Produce structured rewrite facts and prose-review records. |
| `crates/ara-core/src/write/source.rs` and `positions.rs` (new in PR 03) | Extend the lossless artifact inventory only where needed for aliases, merge journal, and complete layer adapters. Never load merge inputs by serializing a `Manifest`. |
| `crates/ara-core/src/write/intent.rs` and `transaction.rs` (new in PR 03) | Add exact merge intent and journal updates to the existing operation union. Add the durable transaction journal and crash recovery (moved here from PR 03) behind the existing `execute` interface, so every earlier write command gains it. Reuse lock, preimages, validation, and multi-file commit. Add approved conflict-resolution operations without a second writer. |
| `crates/ara-core/src/lib.rs` | Export native-only merge entry points. Keep wasm callers and `Manifest` serialization unchanged unless an approved read-model change requires otherwise. |
| `crates/ara-cli/src/merge.rs` (new) and `src/main.rs` | Parse directory arguments, discover ours, snapshot inputs, run one planner/transaction, and render the report through the shared output contract. |
| `crates/ara-cli/src/resolve.rs` (new) | Implement `ara resolve bob:N124` using the same validated alias/import identities as reads and merge. |
| `crates/ara-cli/tests/cli.rs` | Add actual-binary directory, resolve, replay, conflict, and failure scenarios using existing `assert_cmd` and `tempfile` conventions. |
| `crates/ara-core/tests/merge.rs` (new) and `tests/fixtures/merge/` (new) | Known-result and generated-history tests, including lossless unknown fields and session histories. |
| `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `docs/agent-cli.md` (new if still absent) | Apply the future functional-release gates and document the approved merge protocol. |

Use the planned `ArtifactSnapshot::load`, `WorkingArtifact`, `WriteOperation`, `validate_intent`, and transaction executor from PRs 03 and 06. Add a proposed `WriteOperation::Merge` carrying a validated `MergePlan`, or an equivalent internal intent that the same engine accepts. It must execute as one operation, not as a list of subprocess calls to `ara add` or `ara edit`.

```rust
// Proposed native-gated interfaces. Planning itself has no I/O or clock access.
pub fn plan_merge(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    options: &MergeOptions,
) -> Result<MergePlan, MergeError>;

pub struct MergePlan {
    pub intent: MergeIntent,
    pub report: MergeReport,
}

// CLI adapters supply source identity, captured merge time, and fingerprints.
// The writer validates and commits intent together with aliases and merge records.
```

PR 03's exclusive `.ara/lock` covers destination reload, allocation, guard, and commit for cooperating writers. This PR adds the durable transaction journal that PR 03 deferred, because a merge rewrites many files of a real artifact. Under `.ara/transactions/`, persist preimages, existence flags, and candidate digests in a prepared record and sync it before the first rename; sync parent directories and write a durable committed marker before reporting success. Writers recover under the exclusive lock before planning: roll back a prepared transaction, keep a committed one, and stop for manual repair if a current digest shows an external edit. Read commands take no lock; they check for a prepared record with one directory listing and return exit 2 with a `pending_recovery` error instead of reading a mixed state. Raw readers and external editors remain outside the guarantee; a series of renames does not provide filesystem-global atomic visibility.

Capture base and theirs through the shared lossless reader, without locks, before acquiring the destination write lock. If an input has a prepared transaction record, reject it and require a writer in that checkout to recover first; this command must not repair a read-only source. Recheck captured source fingerprints before accepting the plan, and reject a changed input rather than committing a mixture of revisions. Canonicalize and compare roots before lock acquisition so aliases or nested paths cannot create source/destination overlap or self-deadlock.

### How are imports mapped and replayed?

Identify base entries by their approved layer identity, not by coincidental equality of titles or prose. Retain our IDs and entry order. Import each source-only numeric N/O/C/H/E/T identity using the next unused destination ID in deterministic source order, as the parent Phase 3 proposes, even when the source number is currently free. The E namespace here is experiment plans, not evidence objects sharing a local label. Reserve IDs from ours, prior mappings, and pending imports first. Every incoming structural reference uses the completed map, including forward references, parents, and cross-edges. The allocation policy itself should be confirmed in PR 00 because a collision-only policy is a reasonable alternative.

Nonnumeric identities keep their existing form. A base concept is keyed by its term; a related-work item keeps its existing ID syntax; a recipe is keyed by its path; other logic sections use PR 04/06's approved selectors. Source-only concept or path collisions with different content become identity conflicts, not invented numeric IDs. A source-only session with a colliding `YYYY-MM-DD_NNN` identity receives the next free sequence for that date. Record all these results, including identity maps that leave the visible address unchanged, in the portable journal.

Before comparing reference-bearing fields, map each side's references into the destination identity space. Two raw `C05` promotion targets can refer to different fork entries, while two differently spelled aliases can resolve to one entry. Equality and conflict decisions use resolved target identity, then preserve the winning side's source representation through its permitted rewrite. Fingerprint the complete captured artifact inventory, excluding `.git`, temporary files, and private `.ara/` transaction/cache data; define canonical fingerprint encoding in the approved journal contract.

Validate alias logs from all three snapshots before import. Aliases with identical origin and target coalesce; a differing target for the same source-qualified origin is a conflict. Rewrite imported alias targets through the source map, retain their origin scope, and prevent cycles or unresolved targets. Resolve aliases transitively with deterministic ambiguity errors; `ara resolve` returns exit 1 for an unknown or ambiguous address and exit 2 for unreadable alias data. Import prior source alias/journal provenance without confusing its original source key with the current transport label.

A source key identifies a fork lineage, its label identifies a user-facing name such as `bob`, and a fingerprint identifies particular bytes. A new revision of Bob's fork must keep earlier mappings. For entries previously imported from that source, compare our present value and the incoming value against the last imported source value where its ancestry is established. This prevents a later one-sided source edit from being misclassified as a conflict merely because both values differ from the original directory base. Base-known entries and entries with no prior import use the supplied common base. A changed source fingerprint alone neither proves lineage nor authorizes replacing protected content.

For an identical revision, read prior mappings and unresolved decisions rather than allocating or appending again. A previously clean merge returns exit 0 and a byte-for-byte no-op. A previously conflicting merge returns exit 1 and reports the same unresolved conflict IDs, even though no files change. If the source advances while a conflict remains open, retain the original evidence and append an explicitly linked successor conflict only when its incoming value changes. A local edit resolves a tracked conflict only through an explicit resolution intent recorded in the same transaction; an unrelated edit or replay does not clear it.

### What is the complete layer decision table?

Session rows need stable occurrence identities as well as values. Propose a journal locator containing source key, original session identity, list name, and append ordinal, with a preserved fingerprint for each base row. This distinguishes independent same-turn events with identical text and makes repeated incoming rows recognizable. Approve how shared base rows get one identity and how new rows retain fork provenance before implementing union; content equality alone must not collapse two genuine events.

Field presence is a value: absent, null, and an empty string are distinct unless the approved schema equates them. Compare approved semantic values for known fields, preserving our spelling when equal. Compare exact bytes for opaque values. Lists use the layer's approved list rule, not a generic set union that loses order or duplicate events.

| Layer or case | Result |
| --- | --- |
| Base-known trace node or staged observation | Keep protected content and parent identity unchanged; union new children separately. Reconcile only F4-approved mutable pointer fields. Detect protected modifications on ours as well as theirs, including unknown fields. |
| Source-only trace/staging/taste/reasoning entries | Preserve complete entry bytes and append them under the mapped structural parent. Retain the source's order and approved record identity. Never invent a parent from date grouping. |
| Promotion false/true | Take the valid true promotion and its mapped target together. A flag without a valid target, conflicting crystallization metadata, or two different true targets produces a promotion conflict. |
| Claims, heuristics, concepts, related work | Compare every approved field, title, unknown extension field, and residual body against the effective base. One-sided changes win; equal two-sided changes coalesce; differing two-sided changes keep ours and retain both candidates in a conflict. |
| `logic/problem.md`, `logic/experiments.md`, solution bodies, remaining mutable logic documents | Use PR 04/06's bounded selectors where available. Treat a residual prose block or opaque body as one value: a one-sided change wins, equal changes coalesce, divergent edits conflict with full bytes. Whole-file copying must not overwrite disjoint changes to parsed entries. |
| Base-known session | Union approved append-only rows without collapsing distinct events that share a turn number. A row known in base cannot be edited or removed. Merge complete `events_logged`, `ai_actions`, `claims_touched`, `logic_revisions` before/after values, `key_context`, pending suggestions, and open-thread history under the approved session contract. |
| Independently created session with a date/sequence collision | Allocate a free sequence and rewrite incoming filename, `session.id`, index entry, turn-qualified references, and all source pointers together. Preserve the complete record, not just the index. |
| Session metadata and index | Derive counts from the approved merged rows when the protocol says they are derived; select `last_turn` only under an approved timestamp rule. Treat different summaries/open-thread state as mutable-field conflicts unless an approved rule resolves them. Build index entries from final session identities; reject duplicate or dangling index identities. |
| Existing aliases and merge records | Union by approved record identity, remap imported targets, and preserve full conflict history. Differing mappings are conflicts, never last-write-wins. |
| Mutable deletion | Both sides deleting the same entry is accepted. A one-sided delete against an unchanged peer is accepted only after reference validation; a delete against an edit conflicts. Keep our presence or absence and retain the other full candidate. |
| Protected edit or deletion | Refuse the violating merge with exit 1 and unchanged artifact bytes until an approved repair or rejection workflow supplies a valid history. Report base, ours, and theirs evidence. Equality of two illegal edits does not make them legal. |
| `PAPER.md`, evidence, `src/`, extension files | Inventory every path, but mutate only the approved knowledge-document allowlist from PR 06. Root PAPER changes require that PR's explicit scope approval. Leave all code and evidence body bytes unchanged; report external changes, missing pointer targets, and references that would need an out-of-scope rewrite for agent review or Git handling. Unknown knowledge extension documents require an approved adapter or a full-content conflict, never silent omission or arbitrary file copying. |

Complete coverage requires a documented decision and retained evidence for every in-scope incoming entry, field, and file. Unsafe opaque edits remain unresolved. If the final inventory reveals another protected field or a required path with no protocol rule, add it to PR 00's approval gate. Do not label an incomplete layer adapter as supported.

### Which references may the merger rewrite?

Reuse PR 02's scanner for structured references and bounded prose tokens. Scope each rewrite by layer and target kind: the `O1` list in problem framing is not automatically staging observation `O01`, and a concept term is not a numeric node ID. Rewrite source-derived parents, `also_depends_on`, evidence and Proof references, promotion pointers, alias targets, session events, logic revisions, turn-qualified session pointers, and supported Markdown/file anchors through one completed mapping.

Track text origin at field or span level throughout planning. A field taken from theirs is eligible for reference rewriting; a field retained from ours is not, even when both live in the same entry. Our text and base text retained as ours must remain byte-identical outside the explicitly selected mutable change. For mixed append-only lists, rewrite only new incoming rows. This also applies to session `logic_revisions.before` and `.after`, which retain their historical wording apart from approved identifier relocation.

Rewrite complete ID tokens with known destinations in incoming prose, and list every such change in `needs_review` with path, selector, old token, new token, and confidence. Retain ambiguous prose unchanged and list it for review. Preserve quoted historical addresses, cross-artifact qualified references, URLs, and code examples unless the scanner's approved rule identifies them as local references. Unknown structured reference-like values require an explicit unresolved decision if importing them would risk a broken link. Binary content is never text-rewritten; unsafe binary/path collisions remain full-file conflicts.

### How are conflicts reported and resolved?

Retain the parent's `format: ara.merge/v1`, `renamed`, `rewritten`, `needs_review`, `duplicate_candidates`, and `logic_conflicts`. Define additive proposed fields for source key/revision, complete `imports`, `conflicts`, `unresolved_count`, changed paths, and applied/dry-run state. `logic_conflicts` remains a documented logic-specific projection of `conflicts`, not a second independent conflict store. Each conflict has a stable ID, kind, source identity/revision, entry selector, field or file, exact base/ours/theirs values, and allowed resolution operations. Human output gives a compact summary and references these IDs; JSON retains complete values without prose truncation.

For ordinary differing mutable fields, commit safe imports, mappings, and conflict records as one guarded transaction while keeping our conflicted value. Exit 1 until all conflicts from this merge are resolved. A source or input protocol violation, candidate validation failure, or uncertain identity rejects the whole artifact mutation; the report still includes its evidence. Review-only prose suggestions are visible without becoming blocking semantic conflicts unless a required reference cannot be kept valid. This distinction must be part of the approved report contract.

Propose an explicit conflict-resolution operation in the existing writer union, with a reviewed CLI spelling such as `ara merge resolve <conflict-id> --take ours|theirs|base`. Ordinary claim/edit operations can use the same intent when the caller explicitly names the conflict. Resolution validates current value fingerprints, applies only a permitted mutable change, retains exact prior values in the session history required by the protocol, and appends the decision atomically. `--take ours` is an explicit decision, not inference from the default value. A stale conflict fingerprint is rejected rather than applying to a later edit.

Protected-content conflicts need different choices. Rejecting an incoming invalid mutation can append a decision without changing protected entry content, if the protocol approves that record. Accepting it through `ara edit` is forbidden. Restoring an already mutated local protected entry or importing its content as a new corrective entry requires an approved repair/correction rule and complete provenance. F4 currently does not provide that path. Mark this as a shipping blocker instead of promising that Phase 2 edits can resolve every conflict.

### What are the implementation steps?

1. Confirm PR 00 decisions, then freeze the source-key, portable journal, deletion, promotion, session, and conflict-resolution contracts. Ensure PR 06's operation union can represent every selected change and resolution without direct-file fallbacks.
2. Extend the lossless source inventory and parse aliases/journal records. Validate source lineage, address namespaces, record identities, and every input path before allocating IDs.
3. Implement deterministic mappings for all entry kinds and source revisions. Reserve existing and historical targets first; produce the complete mapping before any field or prose rewrite.
4. Implement the full layer decision table, including same-value concurrent edits, opaque residual bodies, session histories, protected-edit rejection, and deletion/reference interactions. Retain both candidates for every unresolved conflict.
5. Apply only source-origin rewrites through the shared scanner and exact text-position engine. Build a complete expected source delta, candidate source view, alias updates, and merge journal records in memory.
6. Validate the full candidate against known semantics, complete source intent, alias consistency, session/index consistency, and the no-new-error occurrence rule. Reuse the writer's recovery and multi-file commit with destination preimages rechecked under its exclusive lock.
7. Add directory CLI and alias resolve surfaces with JSON/human parity, dry run, and the 0/1/2 contract. Dry run uses the same plan and validation and writes no source files, aliases, portable journal, source keys, or sessions. Follow PR 06's operational lock/recovery policy and disclose any maintenance effect; do not promise total filesystem immutability if lock provisioning creates private metadata.
8. Exercise known-result, generated-history, real-binary, failure-injection, and timing acceptance below, then apply shared release/documentation gates. These are future implementation steps; this draft runs no build or test.

## Alternatives considered

Plain text merging cannot reliably preserve tree parentage or distinguish two unrelated `N124` nodes. Serializing normalized models would discard unknown field values, complete history, and lexical content. Merging only a convenient subset of layers would leave valid-looking but dangling promotion or session references; the parent asks for a complete merge.

A renamed-only alias log is smaller than an import journal, but cannot remember a source entry whose visible ID did not change, accepted mutable source values, or an unresolved conflict after a successful partial import. A private cache cannot travel with the artifact. Stable fork IDs embedded in every source artifact are another possible identity scheme; optional explicit source enrollment avoids requiring that format change, but needs a reviewer-approved uniqueness rule.

Always keeping ours with a printed conflict is easy to implement, but the conflict disappears from a replay unless it is persisted. Always refusing any mutable conflict avoids a partially decided artifact, but loses the parent's safe-import/report workflow. The proposal commits safe changes and portable unresolved decisions together; protected violations still reject the operation.

## Tradeoffs

A portable merge journal increases artifact size, especially when conflicts preserve full opaque bodies and prior imported field states. It preserves replay identity and complete evidence without access to an old fork. Use content-addressed payload records only if PR 00 approves their portability and garbage-collection rules; do not make essential evidence disposable.

The source-side-only rewrite rule limits automatic repair. An ambiguous historical mention remains visible for review, and a changed opaque binary may block import. Those limits are preferable to changing our prose or silently renaming unrelated identifiers. Complete source validation is more work than model comparison, but the existing parser's lossy projections make it necessary.

Locking and durable rollback provide one coherent operation for cooperating CLI clients. They do not make raw filesystem readers transactional. The implementation must state that limit, keep lock hold time bounded by one plan/commit, and record timing separately for pure planning and durable I/O.

## Migration

This documentation-only PR requires no version change. The future functional PR bumps the then-current patch in workspace `Cargo.toml`, refreshes `Cargo.lock` with a non-locked `cargo check --workspace`, confirms only the four local workspace package versions changed unless dependencies were deliberately added, and then runs README's locked gates under Rust 1.94.1. Add the changelog entry and the approved command/schema details to `docs/agent-cli.md`, creating it only if the shared design-document cutover requires it.

Artifacts without aliases or merge records remain readable and writable. A first merge creates approved portable metadata through the same transaction as imported content. Do not infer full replay identity from old renamed-only logs: ambiguous older records need explicit source enrollment or a reviewed migration, otherwise reject safely. Do not regenerate existing layers or renumber ours during migration. Native-only additions need no viewer bundle rebuild; if an approved contract changes `Manifest` or wasm-facing core behavior, inspect every literal/consumer and manually rebuild the embedded viewer as required by the shared gates.

## Verification and acceptance

### Which deterministic cases have known results?

Add proposed native tests in `crates/ara-core/tests/merge.rs`, with fixtures under the proposed `tests/fixtures/merge/` directory. State full expected identities, edges, values, retained bytes, conflicts, aliases, and session history; a successful parse alone is insufficient.

| Case | Required result |
| --- | --- |
| Disjoint appends and same-parent appends | Preserve both complete branches and correct parents, retain our order, allocate deterministic imported IDs, and produce valid cross-edges. |
| Duplicate `N124` on different fork parents | Keep our `N124`, import their `N124` under its actual mapped parent with a fresh ID, and rewrite only their references. |
| Different fields of one claim | Keep both changes, with no conflict and a full intended-delta match. |
| Same field, equal edits | Keep our lexical form, no conflict, including the approved equivalence rules for absent/null/list values. |
| Same field, different edits | Keep ours, retain exact base/ours/theirs values, persist one conflict, and return exit 1 on initial merge and unchanged replay. |
| Unchanged-ID import or nonnumeric identity | Record identity mapping even without a rename; unchanged replay is a byte-level no-op. Conflicting concept/path identities are reported without invented IDs. |
| Renamed label, reused label, advancing or older source revision | Approved label change retains source identity; label reuse rejects ambiguity; a later valid source edit reuses targets and the previous imported values; a regression cannot overwrite new content. |
| Prose and structured references in mixed-origin documents | Rewrite every supported incoming target, retain our spans exactly, report ambiguous tokens, and preserve unrelated numeric text. |
| Protected content and unknown field changes | Detect mutation on either fork, including two equal illegal mutations, refuse it without source changes, and retain full evidence. |
| Promotion races | Same target coalesces; false/true chooses the valid true record; different targets or inconsistent target metadata leave explicit conflicts. |
| Deletion and dangling pointers | Mutable one-sided delete against unchanged content succeeds only when valid; delete/edit conflicts; protected deletion fails without mutation. |
| Full session collisions and shared-session appends | Renumber independently created colliding sessions with complete pointers/index updates; preserve distinct same-turn events, all revisions, context, actions, suggestions, and approved metadata. |
| Existing aliases, conflicts, and opaque files | Preserve history; differing alias mappings or unsupported field/file edits retain full conflicts. Never silently drop source extensions. |
| Candidate cycles, malformed YAML, corrupted ledger, and stale input | Reject the transaction, report the actual rule, and leave all artifact file bytes unchanged. |

Generate random valid base/ours/theirs histories across every supported layer, not just flat node sets. Check no duplicate destination identities, no loss of valid union entries, correct mapped parents/edges, and resolution of every imported address including identity mappings. Check `ara check` on clean generated results, complete source preservation, deterministic allocation, same-value coalescence, and no-op replay. For generated conflicting histories, replay must preserve bytes and keep the same unresolved conflict IDs and exit classification; deliberately invalid histories must reject without mutation. Do not assert commutativity, since keeping ours and preserving our IDs makes the merge directional.

### How will the real binary prove the duplicate-N124 fix?

Create disposable base, ours, and theirs artifacts using the existing CLI test conventions. Base has two valid parents and no `N124`; ours adds `N124` under the first parent; theirs adds `N124` under the second, plus structured references, an incoming claim Proof or prose mention, an observation/promotion pointer, and a session event pointing to it. Keep the maximum destination node ID at 124 so the expected imported ID is `N125`. Include an unknown node field containing nested data and assert its bytes survive.

Run the built binary, after implementation and approval, with these proposed commands:

```sh
ara -C "$OURS" merge --base "$BASE" --theirs "$THEIRS" --as bob --dry-run --json
ara -C "$OURS" merge --base "$BASE" --theirs "$THEIRS" --as bob --json
ara -C "$OURS" resolve bob:N124
ara check "$OURS" --json
ara -C "$OURS" merge --base "$BASE" --theirs "$THEIRS" --as bob --json
```

The dry run reports `bob:N124 -> N125` and changes no source bytes. Commit returns exit 0, leaves our `N124` and its prose unchanged, gives their branch `N125` under the second parent, and rewrites every supported source pointer. Resolve prints `N125`; check reports no semantic errors or unfixed format drift; replay returns exit 0 and changes no source or portable-history bytes. Repeat with a differing `C05.Statement`: first merge and replay both return exit 1, and replay preserves the same portable conflict instead of falsely reporting success. Resolve it through the approved writer operation, then replay must preserve the resolution and return exit 0. These commands are planned acceptance, not observations from this documentation turn.

### How will failures and timing be measured?

Use proposed failure-injection cases for unreadable inputs, overlapping roots, lock contention, source changes during capture, destination preimage changes, invalid candidate references, disk-full/temp-write failure, a middle rename failure, interrupted commit, and failed rollback. Ordinary operational failures return exit 2; readable invalid content, rejected protocol changes, unknown identities, and unresolved conflicts return exit 1. Rollback leaves exact preimages on ordinary failure; incomplete recovery fails closed and is surfaced distinctly. Verify aliases, journal, session index, and content never diverge after cooperative recovery. Base and theirs remain unchanged in every scenario.

Measure pure planning, input loading, validation, durable commit, and total real-binary wall time separately on graphs with 100, 1,000, 10,000, and 100,000 nodes, including proportional references and session history. Record hardware, byte sizes, repeated-run distributions, memory use, and the source scanner's precision/recall on the parent corpus. Approved threshold: total real-binary merge time under 1 s at 10,000 nodes on the read-command performance runner, matching the read budget at that size. Report 100,000-node results without a pass/fail gate. A miss blocks acceptance. Preserve the separate read-command limits rather than claiming they also define merge's durable-write budget. Reject quadratic repeated reparsing or whole-artifact copying per imported entry; allocation and mapping should be linear or sort/index bounded in input size.

## Next Steps

1. Review the source-key/label uniqueness rule, complete import journal, and unresolved-conflict lifecycle with PR 00. Settle the optional source-ID and conflict-resolution CLI spellings without declaring them approved.
2. Approve session history/metadata, promotion disagreement, deletion, opaque-path, and protected-content repair rules. Block implementation or release where any named layer lacks a complete decision.
3. After PR 06 lands, implement one complete merger and run the specified binary, property, failure, and timing acceptance. Feed the same planner and source identity contract into PR 09.
