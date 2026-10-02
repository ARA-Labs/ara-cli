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

Original and candidate YAML indexes share the working artifact's invocation-local
cache. Initial merge inventory seeds only indexes paired with identical preimage
bytes. Validators reuse the full node index and check cached Markdown headings
against their exact current bytes. Direct preimage/candidate replacement, unchanged
digest fields, deletion, and altered owning-session history revoke cache authority.

Native captures with at least 3 MiB of combined exploration-tree source inventory
their three immutable inputs concurrently (two scoped workers plus the caller).
Results retain base/ours/theirs error precedence. Smaller captures and wasm use
sequential early-exit parsing; writes, observers, validation and commit remain
ordered on the caller.

For committing native CLI merges with at least 1,000 candidate nodes, enabled
duplicate advice reads only the validated immutable manifests on one scoped worker
while the caller performs the durable commit. Commit failure still returns the
original error and no advice; advice never reads or mutates knowledge files.
`commit_ms` and `advisory_ms` are elapsed task durations and may overlap. The
process/`operation_ms` gate remains unchanged; small merges and dry runs stay
sequential.

The unpublished native ledger stores `revision.files` mapping values,
`transport.bytes`, and `imported_resolution.evidence` as canonical padded RFC 4648
standard base64 strings. The shared strict codec rejects malformed/noncanonical
text and legacy integer arrays; decoding recovers arbitrary original bytes.
Captured-source fingerprints still bind decoded bytes, not their encoding.
Duplicate paths and unknown record fields remain errors. Public conflict/report
`MergeValue` byte arrays are unchanged; this is not a second record grammar.

Native normalization accelerates bounded, unique-key JSON documents using the same
typed schema. Duplicate keys, resource-limit violations and unsupported scalar
semantics fall back to the original YAML parser, preserving its diagnostics.
YAML merge fields borrow unchanged indexed values; immutable-field semantic
comparisons are memoized, and three-way name unions do not allocate per record.
Historical unknown fields remain opaque even when their spelling resembles a
native reference. These optimizations do not remove syntax or reference checks.

Flow-map child insertion uses quoted keys, retaining valid JSON when all existing
and incoming fragments are JSON. Native SHA-256 uses RustCrypto's guarded CPU
dispatch and software fallback; digest bytes and durable rechecks are unchanged.

## Boundaries and remaining gates

10k process timing remains a recorded failed acceptance gate on this runner. Unknown/opaque data cannot be silently omitted. The pinned historical fixture contains five dangling session-index entries and cannot be claimed as a successful replay.

## Code and proof boundaries

Implementation: `crates/ara-core/src/merge/; crates/ara-core/src/write/journal.rs`.

Permanent consumer regressions: `merge_identity.rs, merge_markdown_layers.rs, merge_yaml_layers.rs, merge_journal.rs, merge_journal_dirs.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
