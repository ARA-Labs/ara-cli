# Native discovery, queries and source reads

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

status, ls, show, path, refs and open share explicit-directory/environment/upward discovery, versioned JSON and 0/1/2 exits. Full source reads and exact heading vectors preserve native names, unknown fields, UTF-8 and digests. Structured references remain separate from possible prose mentions.

## Boundaries and remaining gates

Corpus errors are defined outcomes, not clean parses. Timing gates remain fixed. Keyword ranking does not replace full-source grounding.

## Code and proof boundaries

Implementation: `crates/ara-cli/src/{agent,context,output}.rs; crates/ara-core/src/query.rs`.

Permanent consumer regressions: `agent_reads.rs, parse_fixtures.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
