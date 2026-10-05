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

## Boundaries and remaining gates

Historic content is immutable. Promotion tuple replay exceptions authenticate exact unresolved captured candidates; they do not authorize arbitrary changes after promotion.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{staging,sessions,records,clock}.rs`.

Permanent consumer regressions: `write_sessions_staging.rs, write_owner_clock.rs, merge_yaml_layers.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
