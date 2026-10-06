# Native discovery, queries and source reads

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

status, ls, show, path, refs and open share explicit-directory/environment/upward discovery, versioned JSON and 0/1/2 exits. Full source reads and exact heading vectors preserve native names, unknown fields, UTF-8 and digests. Structured references remain separate from possible prose mentions.

Plan 19 (step 19d, workspace 0.1.25) adds measured observation inactivity to
`open`. Each observation row gains `turns_since_reference`,
`session_days_since_reference`, `last_reference_turn`, `last_reference_date`,
`reference_basis`, `evidence_sources`, `history_status` and
`history_diagnostics`; the `ara.open/v1` envelope and existing fields are
unchanged. Counts come from explicit references only (exact observation or
bound-node IDs in typed turn fields or caller-written turn text) on a timeline
of validated session records, per-turn stamps and authenticated merge aliases.
Unknown chronology is `null` with a diagnostic; overlapping sessions leave the
day count known. A stored `stale: true` stays visible. The shared extractor is
`ara_core::write::history`, also used by `observation.mark_stale`. Contract:
[Observation inactivity in `open`](../agent-cli.md#observation-inactivity-in-open).

## Boundaries and remaining gates

Corpus errors are defined outcomes, not clean parses. Timing gates remain fixed. Keyword ranking does not replace full-source grounding.

## Code and proof boundaries

Implementation: `crates/ara-cli/src/{agent,context,output}.rs; crates/ara-core/src/query.rs; crates/ara-core/src/write/history/{mod,collect}.rs`.

Permanent consumer regressions: `agent_reads.rs, parse_fixtures.rs, write_history.rs, agent_writes.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
