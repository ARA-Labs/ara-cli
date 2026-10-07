# Atomic promotion and complete session continuity

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Staging retains original context/provenance. Promotion commits a complete destination and forward-pointer tuple together. Full session turns append all arrays and exact history; rolling fields/index updates are coupled to owning turns. Stale transitions require actual logged-day evidence and atomic reasoning.

Plan 19 (step 19b, workspace 0.1.25) adds writer-owned defaults. An omitted
`observation.stage`, `session.log`, `session.start` or taste timestamp, or the
inline taste date, uses the writer's single `batch_time` (UTC
`YYYY-MM-DDTHH:MM:SSZ`, read once after lock and recovery). A `session.log` that
omits `session` selects the one open session on its effective written date or,
with none, creates one from its summary and timestamp; several open candidates
are `write.session_ambiguous`. Closed sessions are never selected. Other open
sessions are reported as `open_sessions`. `sessions::next_turn` is the shared
owner allocator used by `apply` and the merge audit commands. Explicit
timestamps keep their exact text; date matching uses the written date and
monotonicity compares instants.

Step 19c adds operation-derived session rows. A batch with a `session.log`
appends its turn's mechanical `events_logged` rows for `node.add`,
`observation.stage`, `claim.add`, `heuristic.add` and `observation.promote`,
and `claims_touched` rows `created`, `crystallized` or `revised`,
after the ordered operations succeed (`write/bookkeeping.rs`). A promotion to a
named section adds the additive event `target` selector. Caller rows keep
their summaries and judgments; disagreeing rows fail with
`write.event_conflict` or `write.claim_touch_conflict`, a creation without
provenance fails with `write.event_provenance`, and unattributed creations in
multi-log batches fail with `write.owner_ambiguous`. The full contract is in
[Operation-derived session rows](../agent-cli.md#operation-derived-session-rows).

Step 19d makes `observation.mark_stale` `session_days` optional. The writer
builds the same explicit-reference history as `ls --unfinished`
(`write/history/`), excludes the stale operation's own turn, cuts days off at
the owning audit date, and records the full canonical list when the list is
omitted. A supplied list stays an exact verified subset: duplicates, invalid
dates, days not after the last reference, days after the audit date, days
without another logged turn and fewer than three days reject at
`session_days[i]`/`session_days`. Unknown day evidence refuses with
`write.stale_history_unknown`. The evidence record shape is unchanged and is
recomputed at validation. Contract: [Session history and
transactions](../agent-cli.md#session-history-and-transactions).

## Command simplification

The live agent routes are `status`, `ls`, `show`, `find`, `edit`, `claim set`, `heuristic set`, `apply` and `merge`. Creation, staging, promotion and session setup/logging use existing typed JSONL operations. `show --with path,refs`, `ls --unfinished` and `show --identity` retain ancestry, citation, inactivity and exact imported-identity behavior. Tooling remains unchanged. See the [migration guide](../agent-cli.md#command-simplification-migration) for inputs and result mappings; old verification reports remain frozen historical evidence.

## Boundaries and remaining gates

Historic content is immutable. Promotion tuple replay exceptions authenticate exact unresolved captured candidates; they do not authorize arbitrary changes after promotion.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{staging,sessions,records,clock,bookkeeping}.rs, write/history/`.

Permanent consumer regressions: `write_sessions_staging.rs, write_owner_clock.rs, write_bookkeeping.rs, write_history.rs, merge_yaml_layers.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
