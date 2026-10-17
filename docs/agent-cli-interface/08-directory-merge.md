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

Plan 19 (step 19b): `merge resolve` and `merge repair` take their lock through
`write::lock_and_capture`, which recovers before reading the clock once.
`merge::AuditOwner` keeps `--session` explicit (never selected or created) and
makes `--turn`, `--timestamp` and `--summary` optional. The audit allocates the
session's next turn with `sessions::next_turn`; a supplied turn must equal it.
An omitted timestamp is the locked clock value, no longer the session's
`last_turn`, and the ledger `Resolution.time` records that effective
timestamp. An omitted summary keeps the rolling summary. The planners return
`merge::AuditedResolution` (candidate, session, turn), and reports add the
resolved `session` and `turn`. Protected-history repair checks are unchanged.

Plan 19 (step 19e): a split's primary `logic_revisions` rows may carry the
optional `action: split` and `split_into` keys. The merger treats
`split_into` as an opaque row field: unchanged rows merge normally, and an
incoming row whose `split_into` names a relocated identity is reported as an
`unsupported_structured_reference` conflict instead of being rewritten.

`identity::local_redirects` no longer rejects a valid authenticated rename
of one of two concept leaves that share a display key
(`logic/concepts.md#Term` for `Group A/Term` and `Group B/Term` in a
document without an H1): when the display key belongs to a different live
entry, the retired origin uses its exact heading-vector key, and a retired
suffix alias never shadows a live identity. A real reuse of the same heading
vector is still `merge.redirect_ambiguous`. `merge::resolve_locator` resolves a literal
locator against live sections, then authenticated mutation rows; `show` uses
it for retired spellings and `merge::check_citations` lets the writer check
historical literals against one identity view.

## Command simplification

The live agent routes are `status`, `ls`, `show`, `find`, `edit`, `claim set`, `heuristic set`, `apply` and `merge`. Creation, staging, promotion and session setup/logging use existing typed JSONL operations. `show --with path,refs`, `ls --unfinished` and `show --identity` retain ancestry, citation, inactivity and exact imported-identity behavior. Tooling remains unchanged. See the [migration guide](../agent-cli.md#command-simplification-migration) for inputs and result mappings; old verification reports remain frozen historical evidence.

## Boundaries and remaining gates

The final frozen binary passes all five 10k process samples at 868.62–894.49 ms
against the unchanged 1,000 ms gate. Earlier failed and first-launch measurements
remain attributed in the verification record; this is not a cold-cache guarantee.
Unknown/opaque data cannot be silently omitted. The pinned historical fixture
contains five dangling session-index entries and cannot be claimed as a successful
creation replay. The 100k probes remain report-only and retain actual failures.

## Code and proof boundaries

Implementation: `crates/ara-core/src/merge/; crates/ara-core/src/write/journal.rs`.

Permanent consumer regressions: `merge_identity.rs, merge_markdown_layers.rs, merge_yaml_layers.rs, merge_journal.rs, merge_journal_dirs.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
