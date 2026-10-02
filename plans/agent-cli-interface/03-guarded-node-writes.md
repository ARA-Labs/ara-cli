# PR 03: guarded node writes and the native transaction engine
**Date:** 2026-10-01

Status: **approved** 2026-10-01 (v1 scope revised at approval: no reader locks, durable journal moved to [PR 08](08-directory-merge.md)). Repository: `ARA-Labs/ara-cli`. Parent: [agent CLI interface](../agent-cli-interface.md). Shared rollout and verification: [PR index](README.md). Dependencies: [PR 02](02-read-commands.md) and approved F1 through F4 in [PR 00](00-protocol-contracts.md).

## TL;DR

Add `ara add node` and `ara add edge` through a native library that plans precise source edits before writing anything. Allocate IDs under one exclusive checkout lock and prove that the complete source change matches the operation. Promotion and batches share the multi-file commit built here: temporary files, per-file renames, and in-process rollback of preimages on ordinary failure. Reads take no lock and existing commands keep their current I/O. A durable crash-recovery journal is deferred to PR 08, where merges first rewrite many files on real artifacts. The upstream protocol decisions must be approved before these writes ship.

## Problem

The parent requires append operations, unique IDs across concurrent processes, preserved comments, and exact semantic guards. `crates/ara-core/src/fix.rs` provides useful in-memory edit and diagnostic-containment patterns, but `fix_dir` currently calls `std::fs::write` separately for tree and claims. It has neither an exclusive checkout lock nor an atomic multi-file commit. Its normalized-manifest guard is insufficient for general authoring because `schema.rs` discards unknown values through `IgnoredAny` and `parse.rs` drops known fields on the wrong node kind.

A correct write must preserve information the current model omits. That includes unknown document and node values, duplicate occurrences, YAML scalar style, comments, metadata, and fields such as `status` on a decision. Alias normalization and dropped redundant edges also make the normalized graph an incomplete account of source changes. The first shipped write needs a source-level guard; adding that after authoring commands would leave an unsafe release between the two.

## Constraints

All filesystem loading, locking, and mutation live behind `ara-core`'s `native` feature. Pure query code from PR 02 stays wasm-safe. Existing `validate`, `layout`, and `check` output contracts stay unchanged. Discovery, JSON errors, `-C`, and exit codes use PR 02: semantic rejection is 1, setup or I/O failure is 2, and successful writes are 0. This PR does not decide F1 through F4, enforce CLI-only use, merge forks, or add arbitrary trace editing.

Hold the exclusive OS advisory lock on the persistent file `.ara/lock` from snapshot loading through allocation, planning, validation, and commit. Never unlink the lock file, since replacing its inode lets two processes lock different files. Add `.ara/` to the artifact's existing ignore rules without replacing their contents. Locks apply only to cooperating processes in one checkout; they are not distributed locks, and direct editors remain outside the guarantee. Blocking acquisition is the default. Verify lock behavior on macOS and Linux local filesystems in the first implementation step.

The ignore-rule change is an explicit operational edit to the artifact-root `.gitignore`, not a silent change outside the transaction. Snapshot it, append only a missing `.ara/` rule, and include it in the guarded commit and rollback. Do not edit a parent repository's ignore file. Before any rename, compare the loaded source digests and existence state again to detect noncooperating edits; this reduces stale writes but cannot eliminate races with direct editors. Reads never take the lock and never create `.ara/`, so read commands keep working on read-only artifacts and pay no lock cost against the read speed budget.

## Proposed approach

Add `crates/ara-core/src/write/mod.rs`, `source.rs`, `positions.rs`, `intent.rs`, `transaction.rs`, and `node.rs` (all new, native-only). Export the library through existing `lib.rs`; wire argument parsing in `crates/ara-cli/src/main.rs` and thin write adapters in `crates/ara-cli/src/write.rs` (new). Use PR 02's library/discovery module rather than creating a second discovery path. Add a native optional advisory-lock dependency in `crates/ara-core/Cargo.toml` after checking the pinned toolchain and platform support. The proposed interfaces are `ArtifactSnapshot::load(root)`, `WorkingArtifact`, `WriteOperation`, `plan_operation(&mut WorkingArtifact, operation) -> OperationResult`, `validate_intent(base, candidate, intent)`, and `execute(root, operations, mode) -> WriteReport`.

`ArtifactSnapshot` retains exact bytes, file existence, permissions, and a digest for every in-scope knowledge document and preserves unrecognized documents as opaque bytes. Its source index retains every mapping entry and scalar value, including fields absent from `Manifest`; it must not use `RawNode.extra` as the unknown-value store. `positions.rs` locates structural byte ranges using YAML parser events and a fence-aware Markdown index, never an ID substring search. A native-only YAML event-parser dependency and its byte-position correctness are a review gate. Prototype Unicode, CRLF, block scalars, flow collections, comments, duplicate keys, tags, anchors, and aliases before committing to a library. Ambiguous or unsupported target syntax rejects the operation byte-identically with a specific reason; it never triggers whole-file serialization.

The public operation shapes below are proposals for review and become the tags consumed by PR 06. `fields` contains typed JSON values in the library; the CLI parses each accepted field according to its schema. An optional `id` is a requested concrete ID for deterministic replay and must be unused; normal interactive creation omits it. PR 06 additionally interprets a creation `id` beginning with `$` as a batch binding, never as a stored ID.

```json
{"op":"node.add","id":"N13","type":"experiment","parent":"N12","title":"Check the boundary","fields":{"result":"observed result","provenance":"ai-executed"},"depends_on":["N03"]}
{"op":"edge.add","node":"N13","depends_on":"N04"}
```

`parent: "root"` inserts a root entry; other parents resolve to one unambiguous node. Scan all source ID occurrences under the lock, reserve the largest numeric suffix plus one, preserve existing spellings, and use the protocol's minimum two-digit style for generated IDs. Reject suffix overflow and ambiguous numeric-equivalent IDs such as `N1` and `N01` before allocation. Do not fill gaps or renumber existing entries. Adding an already-present dependency is a documented no-op. Reject self-links, unknown nodes, and new dependency cycles. Reject a new ancestor dependency as redundant rather than claiming a stored edge that the current parser drops. F4 must explicitly authorize appending `also_depends_on` to an existing immutable trace node; the parent pointer whitelist alone does not authorize this command.

1. Pin the approved write-format rules and permitted node fields. Reconcile compiler/manager dialect differences, including decision `status`, pivot aliases, optional `parent`, and any F6 node pointer additions, with PR 00. Preserve existing wrong-kind or unknown fields, but reject newly authored fields not approved for that node kind.
2. Build complete snapshots and structural source positions. Index nested child ranges without interpreting `id:` text in a block scalar as a node. Preserve surrounding bytes and line endings. Render inserted values with correct YAML escaping; choose literal block scalars and chomping indicators that retain the supplied text exactly.
3. Add explicit intents for one new node, its nesting relation, its requested fields, and requested dependency additions. Verify bytes outside the declared edit spans are identical. Compare a complete source-value representation after subtracting the intended delta, including unknown values and document metadata. Separately parse the entire candidate artifact and require no new error occurrences by diagnostic identity and multiplicity. Fatal base or candidate syntax rejects writes; unrelated recoverable diagnostics may remain if IDs and target positions are unambiguous.
4. Implement an exclusive `ArtifactLock` for writers only. Hold preimages and existence flags in memory, write candidates to same-filesystem temporary files, sync them, and apply sorted atomic per-file renames. Retain original permissions. Never follow a destination symlink outside the artifact; reject path escapes, duplicate destinations, or unsupported file types.
5. On ordinary rename or sync failure, restore all preimages under the lock, including removing files created by the transaction. If rollback fails, return exit 2 listing every affected path and its intended and original digests; do not claim unchanged files. A process crash between renames can leave a partially applied multi-file write. Until PR 08 adds the durable journal, document this limit, rely on `ara check` to surface dangling pointers, and on Git to restore. Keep the `execute` interface journal-ready so PR 08 adds durability without changing callers.
6. Leave `validate`, `layout`, `serve`, and read commands unlocked and unchanged. Route `check --fix` persistence through the engine so it takes the writer lock and cannot interleave with `ara add`, while retaining existing fix guards and its public result semantics. `serve` already reloads on change; a transient invalid manifest during a rename sequence is replaced by the next reload.
7. Add CLI adapters, consumer-visible tests, and the functional release changes: bump the then-current patch in `Cargo.toml`, refresh `Cargo.lock` with non-locked `cargo check --workspace` before final locked gates, and update `CHANGELOG.md` and `docs/agent-cli.md`. Apply the index's Rust 1.94.1 and wasm checks. If wire/parser changes affect compiled wasm, check every constructor/consumer and rebuild the viewer embed manually; version-only bumps do not require it.

## Alternatives considered

Serializing the normalized manifest loses fields and creates large diffs. A create/delete lock file without an OS lock leaves stale ownership after crashes and has inode races. A rename-only implementation without preimages cannot undo a partial promotion. Use precise source edits, a persistent advisory lock, and in-memory preimage rollback.

A durable journal with shared reader locks was the draft design. It protects against process crashes, but it makes every read take a lock, requires write access for reads, and delays the read experiments that the research goal needs first. Agent experiments run in disposable workspaces, and Git restores user artifacts. The journal therefore ships with merge in PR 08.

A versioned-directory swap could give stronger reader isolation but changes the artifact layout and normal file access. It is outside the parent scope. The proposed journal maintains the existing paths and makes its visibility limits explicit.

## Tradeoffs

A transaction is all-or-none for successful operations and for ordinary failures whose rollback succeeds. Per-file rename is atomic, but the filesystem does not atomically replace several paths together, so any reader can observe a mixed state for the microseconds between renames, and a process crash in that window leaves it on disk. Network filesystems without reliable advisory locks and direct editors are outside the guarantee. Do not advertise a broader guarantee.

The source index adds work to small appends. Measure complete command cost before introducing caches. Unrecognized fields can remain in place, but source constructs whose location or effect cannot be proven safe produce a rejection with the offending path and syntax.

## Migration

Artifacts keep their paths and current source style. Existing output schemas remain intact; new write JSON uses `format: "ara.add/v1"` with operation, assigned ID, changed paths, diagnostics, and committed status. Do not publish assigned IDs as committed on a failure. `.ara/` contains operational metadata, not research records, and is ignored by Git. Release notes must state the crash window for multi-file writes and that concurrency safety covers only cooperating `ara` writers.

## Verification and acceptance

Add proposed native unit tests alongside the new write modules and functional cases in existing `crates/ara-cli/tests/cli.rs`. Test nested/root insertion, missing children lists, empty trees, root dialects, explicit replay IDs, leading-zero collisions, overflow, unknown parent, self/cycle/redundant dependencies, and repeated-edge no-op. Compare exact supplied multiline text after reparsing and unchanged surrounding bytes. Seed unknown nested mappings and wrong-kind fields, then deliberately construct a candidate that changes them while leaving `Manifest` unchanged; the guard must reject it. Include block-scalar fake IDs, fenced Markdown headers, CRLF, non-ASCII offsets, comments, malformed targets, and duplicate source occurrences.

Inject failures at each temporary-write, sync, and rename boundary. A recoverable failure must leave all source paths byte-identical, including their existence state. A failed rollback must return exit 2 with the affected paths rather than success. Race 8 processes adding 50 nodes each and require 400 unique new nodes, exact intended fields, and no introduced errors. Also race `check --fix` against `ara add` so it cannot bypass the lock, and confirm `ara validate` and read commands succeed on a read-only copy without creating `.ara/`.

The following is a proposed real-binary smoke after implementation. Create a disposable copy of an approved fixture with `N01` and another independent root `N02`; never write the checked-in corpus. Set `SMOKE_ARA` to that copy and run:

```sh
ara add node -C "$SMOKE_ARA" --type experiment --parent N01 --title 'Boundary check' --set result='observed result' --provenance ai-executed --json
ara add edge N03 -C "$SMOKE_ARA" --depends-on N02 --json
ara show N03 -C "$SMOKE_ARA" --full --json
ara validate "$SMOKE_ARA" --json
ara add node -C "$SMOKE_ARA" --type experiment --parent N999 --title 'Rejected append' --json
```

Expect `N03`, its exact result/provenance, the declared cross-edge, and successful validation. The unknown-parent command must return exit 1 and leave the source unchanged. Inspect the actual source diff and preserved comments, not only test output. These are future verification steps; this documentation task runs no code or checks.

## Next Steps

Prototype the source-parser choice first and confirm lock portability. Implementation waits on F1–F4 approval in PR 00, including the authorization for added cross-edges. [PR 04](04-logic-editing.md), [PR 07](07-same-as-links.md), and later merge reuse this engine; none may substitute normalized-manifest-only guards or direct writes.
