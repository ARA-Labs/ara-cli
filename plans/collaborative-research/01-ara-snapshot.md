# 01: `ara snapshot`, an offline exact capture of an artifact
**Date:** 2026-10-03

Status: **approved** by the human developer on 2026-10-03 after design review. Implementation pending. Target repository: `ara-cli`. Parent: [collaborative research plan series](README.md). The other CLI/core work in this series is [04: peer-feedback merges](04-peer-feedback-merge.md). No implementation or commit is performed by this documentation approval.

## TL;DR

Add `ara -C <fork> snapshot --output <new-dir> --json`. It copies every nonprivate file of the artifact into `<new-dir>/ara/` and writes `<new-dir>/snapshot.json`. The manifest records the merge fingerprint, a per-file digest, size, and mode, and the artifact's diagnostics. Capture shares the merger's inventory and fingerprint rules. It preserves captured bytes and exposes the output only after verification; a point-in-time capture requires cooperating CLI writers or caller-enforced quiescence of direct writers.

## Problem

The runner in [02](02-contribution-workflow.md) has to freeze a fork before publishing it, and peers and the integration PM must later read exactly those bytes. Today no supported interface does this:

- `ara merge` captures a complete artifact (`ArtifactSnapshot::load_complete`) and hashes it (`merge::fingerprint`), but only as a merge input. The fingerprint appears in merge reports and the source history, not as a standalone capture.
- `show --source` digests cover one selected source, not the artifact.
- The plan-14 channel smoke used a knowledge-only inventory, which leaves out `src/` and `evidence/`.

The runner (Python, in `ara-eval`) could copy the directory and hash it itself. It would then have to re-implement the core's capture rules:

- skip `.git/`, `.ara/`, and reserved temporary paths at every depth using one core privacy predicate;
- reject symlinks;
- refuse to read past a prepared transaction;
- reproduce the `ara.artifact/v1` fingerprint byte for byte.

Any drift between the two implementations means a published fingerprint no longer matches what `ara merge` sees on import. One CLI command keeps a single implementation that agents, the runner, and merge share.

The current core needs a small capture correction, not a second CLI filter. `load_complete` first calls `load`, whose knowledge-directory walk can visit nested `.ara/` or `.git/` paths before the complete inventory excludes them. The fingerprint's `private_path` filter is crate-private. Unify the capture and fingerprint predicates in core and apply exclusions before descending into private directories; otherwise a snapshot can inspect or copy bytes that its merge fingerprint ignores. This is part of the approved snapshot work.

## Interface

```sh
ara -C ./forks/a snapshot --output ./packages/a-0007 --json
```

| Argument | Meaning |
|---|---|
| `-C <dir>` / `ARA_DIR` / discovery | Selects the source artifact, the same as every other command. |
| `--output <dir>` | Required. Must not exist. Neither it nor the source root may contain the other. |
| `--json` | Emit an `ara.snapshot/v1` result on stdout. Errors use the standard stderr JSON error. |

Output layout:

```text
packages/a-0007/
├── ara/            # every captured file, with captured bytes and modes
└── snapshot.json   # manifest
```

Illustrative manifest (digests abbreviated; the versioned schema is frozen in phase 1):

```json
{
  "format": "ara.snapshot/v1",
  "fingerprint_scheme": "ara.artifact/v1",
  "fingerprint": "3b9c…e41a",
  "root": "ara",
  "files": [
    {"path": "logic/claims.md", "sha256": "a1f0…", "size": 4182, "mode": "0644"},
    {"path": "src/train.py", "sha256": "77d2…", "size": 9120, "mode": "0755"},
    {"path": "trace/exploration_tree.yaml", "sha256": "c4e8…", "size": 20311, "mode": "0644"}
  ],
  "excluded": [".ara/", ".git/", "temporary paths"],
  "diagnostics": {
    "errors": 0,
    "warnings": 2,
    "items": [{"severity": "warning", "code": "…", "path": "logic/claims.md", "message": "…"}]
  }
}
```

- `fingerprint` is `merge::fingerprint` over the captured snapshot: SHA-256 over sorted paths and bytes, not modes.
- `files` lists only existing nonprivate regular files, never absent canonical-source placeholders. It records each `FileSnapshot.digest` and captured mode, because the fingerprint doesn't cover modes. Sorting follows the snapshot's `BTreeMap` order; the schema fixes SHA-256 prefix formatting.
- `diagnostics` comes from the same validation `ara status` reports.
- The stdout result echoes `output`, `fingerprint`, file count, and the diagnostic counts.

| Exit | Code | When |
|---|---|---|
| 0 | | Snapshot exposed at `--output`. |
| 1 | `stale_snapshot_input` | The source changed between capture and the final recheck. Nothing is exposed. |
| 2 | `pending_transaction` | A prepared transaction exists. This reuses the reads' existing setup error; recovery stays with the guarded write path. |
| 2 | `output_exists`, `overlapping_roots` | Unsafe output location. |
| 1 | existing `write.path` and encoding codes | Nonprivate symlinks, unsupported source types, or non-UTF-8 paths, preserving core semantic-error classification. |
| 2 | existing I/O and lock codes | Unreadable files, lock failure, or output I/O failure. |

## Behavior

1. Resolve the source root and `--output` through its existing parent, then validate that the output does not exist and the roots are disjoint. Move the overlap comparison from `crates/ara-cli/src/merge.rs` to a shared helper, retaining merge's existing error contract. Reject output-parent aliasing into the source.
2. Acquire `ArtifactLock` (D-S1), then reject a prepared transaction with `pending_transaction`; do not recover or mutate source history.
3. Capture with `ArtifactSnapshot::load_complete` after unifying core privacy filtering. Inventory every nonprivate regular file, reject nonprivate symlinks, and record bytes, permissions, and digests.
4. Compute `merge::fingerprint` and collect diagnostics from the captured bytes, not a later read of the live source.
5. Write captured bytes into a temporary sibling directory `<output-parent>/.<name>.ara-snapshot-<nonce>`. Set the captured modes, then write `snapshot.json` and sync the files and directories.
6. Reload the temporary copy with `load_complete`. Compare the complete exported inventory, fingerprint, and modes against the capture; a mismatch is an internal error.
7. Reload the source with `load_complete`. If the nonprivate inventory, fingerprint, or modes differ, delete the temporary directory and fail with `stale_snapshot_input`. This detects observed changes but cannot prove that an unrestricted direct writer never interleaved a multi-file edit or changed and restored bytes.
8. Publish the temporary directory at `--output` with an atomic no-replace operation. A concurrent creator must receive `output_exists`, never have its directory replaced. Sync the output parent before reporting durable success, then release the lock. Failures before publication leave no package at `--output`; failures after publication but before durability acknowledgment report I/O failure and may leave a complete verified output that the caller must inspect before retrying.

The command's lock excludes cooperating CLI commits. It does not freeze code writers, background processes, or writers on another host. For a point-in-time package, the caller must stop all direct writers before capture and keep them stopped through the complete package freeze in [02](02-contribution-workflow.md#how-publication-works). Double loading is a change detector, not a filesystem transaction. The public command documents this precondition rather than claiming isolation it cannot enforce.

The command doesn't:
- mutate the source (apart from the lock file `.ara/lock`, see D-S1);
- record a source key or authenticate anyone;
- fetch missing dependencies, follow external object pointers, run code, or publish.

External objects, execution metadata, and declared inputs outside the native root belong to the runner's envelope ([02](02-contribution-workflow.md#what-identifies-a-contribution)).

Limits:
- Empty directories aren't captured; the fingerprint has no directory entries.
- Memory scales with artifact size, the same as `ara merge`. Large evidence should be an external pinned object.
- Readers inspect the exported `ara/` with existing commands, and integrators pass it to `ara merge --theirs`.

## Implementation steps

1. Add `crates/ara-cli/src/snapshot.rs` with the argument struct and command, and register `Command::Snapshot` in `crates/ara-cli/src/main.rs`.
2. Move `disjoint_roots` to a shared CLI helper and use it from both merge and snapshot.
3. Reuse `ArtifactSnapshot::load_complete`, `merge::fingerprint`, `ArtifactLock`, and `journal::pending_prepared`. Unify private-path filtering in `crates/ara-core/src/write/source.rs` and `crates/ara-core/src/merge/identity.rs` so all capture paths exclude the same private namespaces before reading them. Preserve the fingerprint scheme; test nested exclusions and existing merge consumers. Review core behavior changes for embedded-viewer regeneration under `docs/agent-cli.md`.
4. Document the command and its direct-writer precondition in `docs/agent-cli.md`. For the functional PR, bump the workspace patch version in `Cargo.toml`, update the local package versions in `Cargo.lock` with a non-locked workspace check, and add a `CHANGELOG.md` entry under `[Unreleased] / Added`. Then run the locked gates.

## Tests

Add them to `crates/ara-cli/tests/`, likely a new `agent_snapshot.rs`, and run the real binary:

- **Round trip.** The snapshot's fingerprint equals the fingerprint `ara merge` reports for the same artifact used as `--theirs`. Merging the snapshot and merging the live fork produce identical results.
- **Coverage.** Opaque `src/` and `evidence/` files are included with exact bytes; executable modes are preserved and listed. `.ara/`, `.git/`, and reserved temporary paths are excluded at every depth, including private paths inside knowledge directories. Private contents must not be inspected; absent canonical files must not be materialized as empty files.
- **Rejections.** Existing output, overlapping or aliased roots, a nonprivate symlink, and a prepared transaction each reject with the documented code and leave no new `--output`. Race another creator at final publication and assert its output remains untouched.
- **Concurrent change.** A known file change or mode change between capture and recheck gives `stale_snapshot_input` with no output or leftover temporary directory. Exercise serialization with a cooperating CLI writer. The runner smoke separately proves direct-writer quiescence; do not claim the double-read test establishes unrestricted-writer isolation.
- **Publication durability.** Exercise failures before publication and after publication but before acknowledgment. The former exposes no output; the latter exposes at most a complete verified package and an explicit I/O failure, never false success or a partly copied output.
- **Diagnostics.** An artifact with existing diagnostic errors still snapshots under D-S3 report-only, and the manifest lists the errors.
- **Reads.** `status`, `show`, and `ls` on the exported `ara/` return the same results as on the source.

Before review: targeted tests, binary smoke, then the full locked workspace, format, all-target Clippy, and native/wasm checks.

## Approved decisions

| ID | Decision | Selected behavior and reason |
|---|---|---|
| D-S1 | Locking | Take `ArtifactLock`, including creation of `.ara/` on a never-written artifact. Cooperating CLI writes serialize; the runner separately quiesces direct writers. |
| D-S2 | Source key | Omit `--source-key` from snapshot. The contribution envelope owns source identity and `ara merge --source-key` supplies it at import. |
| D-S3 | Diagnostic errors | Report only; publication policy decides. Guarded writes promise no new errors, so existing diagnostic errors do not by themselves prevent capture. Source I/O and capture/parse failures still reject. |
| D-S4 | Name | `snapshot`, matching the core type and merge vocabulary. |
| D-S5 | Public command | Provide a supported CLI command for agents and the Python runner rather than add Python core bindings. |

## Next Steps

1. Freeze the manifest and error schema, then implement shared capture filtering and the snapshot command.
2. Run the documented capture, publication, and round-trip checks before the runner adopts the command. After implementation, move the completed design into `docs/` under repository policy.
