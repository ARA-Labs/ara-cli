# Complete source-aware directory merge and recovery

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

The merger inventories complete layers, preserves ours, relocates incoming collisions and records exact source identities for replay. Literal heading vectors remain authoritative. Mutable conflicts retain complete candidates; protected changes reject. Durable private preimages/candidate digests/created-directory manifests support rollback and recovery.

The planner shares exact cached YAML indexes with the working artifact, limits
session-index derivation to session sources, and reuses the proposed identity
inventory while still checking the final alias/ledger bytes. `MergePlan.validation`
retains the base and candidate manifests produced by the full guarded validation
pass; native duplicate advice consumes these views instead of normalizing both
trees again. They describe those validated bytes, not later knowledge mutations.
The working artifact retains the strictly decoded immutable merge ledger and
reuses it for import authentication and final metadata validation only while its
actual raw bytes are identical. Direct candidate/preimage map mutations, malformed
replacement bytes and deletion cannot reuse stale authorization.
Durable staging, source rechecks, fsync, rollback and recovery remain unchanged.

The unpublished native ledger stores `revision.files` mapping values,
`transport.bytes`, and `imported_resolution.evidence` as canonical padded RFC 4648
standard base64 strings. The shared strict codec rejects malformed/noncanonical
text and legacy integer arrays; decoding recovers arbitrary original bytes.
Captured-source fingerprints still bind decoded bytes, not their encoding.
Duplicate paths and unknown record fields remain errors. Public conflict/report
`MergeValue` byte arrays are unchanged; this is not a second record grammar.

## Boundaries and remaining gates

10k process timing remains a recorded failed acceptance gate on this runner. Unknown/opaque data cannot be silently omitted. The pinned historical fixture contains five dangling session-index entries and cannot be claimed as a successful replay.

## Code and proof boundaries

Implementation: `crates/ara-core/src/merge/; crates/ara-core/src/write/journal.rs`.

Permanent consumer regressions: `merge_identity.rs, merge_markdown_layers.rs, merge_yaml_layers.rs, merge_journal.rs, merge_journal_dirs.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
