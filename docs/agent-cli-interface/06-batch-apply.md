# One-snapshot typed batches and operation coverage

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Strict JSONL decoding preserves physical line numbers and rejects duplicate keys. Ordered operations share typed provisional bindings and one working snapshot. References substitute in declared fields while prose stays literal. The compiler/research-manager union includes bounded initialization, complete documents, root edits, structural transitions and coupled audit history.

Plan 19 (step 19b) adds the owner anchor. `write::execute_at` takes an
injectable clock and calls it once after lock and recovery (dry runs: once,
without a lock); `execute` passes the system clock, and the released binary has
no clock override. `plan_batch` treats a batch's sole `session.log` as the owner
of omitted `session`/`turn` (or reasoning `record.turn`) when any operation
omits them. The anchor needs a nonempty summary and must precede every
omitting line; its turn is reserved at its own line and recorded as
`WorkingArtifact::owner` (`sessions::OwnerAnchor`), the attachment point for
later operation-derived rows. Omitting with zero or several logs, a missing
summary, a late anchor, and explicit values that differ from the anchor fail as
`write.owner_required`, `write.owner_ambiguous`, `write.owner_summary`,
`write.owner_order` and `write.owner_mismatch` at the physical line. Pending
revisions are still attached after the ordered operations succeed. Fully
explicit batches keep their existing ordering and multi-log behavior.

## Boundaries and remaining gates

Native operation proof covers the 107-row inventory: 88 required CLI operations plus 19 explicit permitted access/output cases. Unchanged historical replay is blocked by recorded dialect/ancestor/leaf incompatibilities and missing session-index files; no historical-import API is invented.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{mod,batch,intent,clock}.rs; scripts/agent-cli-acceptance.py`.

Permanent consumer regressions: `write_batch_engine.rs, write_owner_clock.rs, agent_writes.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
