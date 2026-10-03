# 01: `ara snapshot`, an offline exact capture of an artifact
**Date:** 2026-10-03

Status: draft for human review. Target repository: `ara-cli`. Parent: [collaborative research plan series](README.md). This is the only `ara-cli` implementation in the series.

## TL;DR

Add `ara -C <fork> snapshot --output <new-dir> --json`. It copies every nonprivate file of the artifact into `<new-dir>/ara/` and writes `<new-dir>/snapshot.json`. The manifest records the merge fingerprint, a per-file digest, size, and mode, and the artifact's diagnostics. Capture uses the same inventory and fingerprint code as `ara merge`, so a published snapshot is exactly what a later `ara merge --theirs` reads. The command is offline, never changes the source artifact, and exposes the output only after verifying it.

## Problem

The runner in [02](02-contribution-workflow.md) has to freeze a fork before publishing it, and peers and the integration PM must later read exactly those bytes. Today no supported interface does this:

- `ara merge` captures a complete artifact (`ArtifactSnapshot::load_complete`) and hashes it (`merge::fingerprint`), but only as a merge input. The fingerprint appears in merge reports and the source history, not as a standalone capture.
- `show --source` digests cover one selected source, not the artifact.
- The plan-14 channel smoke used a knowledge-only inventory, which leaves out `src/` and `evidence/`.

The runner (Python, in `ara-eval`) could copy the directory and hash it itself. It would then have to re-implement the core's capture rules:

- skip `.git/`, `.ara/`, and temporary paths (`private_path`, `is_temporary_path`);
- reject symlinks;
- refuse to read past a prepared transaction;
- reproduce the `ara.artifact/v1` fingerprint byte for byte.

Any drift between the two implementations means a published fingerprint no longer matches what `ara merge` sees on import. One CLI command keeps a single implementation that agents, the runner, and merge share.

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

Illustrative manifest (field names are unreviewed):

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
- `files` repeats the per-file `FileSnapshot.digest` and adds the mode, because the fingerprint doesn't cover modes. Sorting follows the snapshot's `BTreeMap` order.
- `diagnostics` comes from the same validation `ara status` reports.
- The stdout result echoes `output`, `fingerprint`, file count, and the diagnostic counts.

| Exit | Code | When |
|---|---|---|
| 0 | | Snapshot exposed at `--output`. |
| 1 | `stale_snapshot_input` | The source changed between capture and the final recheck. Nothing is exposed. |
| 2 | `pending_transaction` | A prepared transaction exists. This reuses the reads' existing setup error; recovery stays with the guarded write path. |
| 2 | `output_exists`, `overlapping_roots` | Unsafe output location. |
| 2 | existing write/IO codes | Symlinks, unreadable files, non-UTF-8 paths, or lock failure, reused from `ArtifactSnapshot` and `ArtifactLock`. |

## Behavior

1. Resolve the source root and validate `--output`: it must not exist and must be disjoint from the root. Reuse `disjoint_roots` from `crates/ara-cli/src/merge.rs`, moved to a shared helper.
2. Acquire `ArtifactLock` (see D-S1), then reject a prepared transaction with `pending_transaction`.
3. Capture with `ArtifactSnapshot::load_complete`. This inventories every nonprivate regular file, rejects symlinks, and records bytes, permissions, and digests.
4. Compute `merge::fingerprint` and collect diagnostics.
5. Write captured bytes, not a fresh read from disk, into a temporary sibling directory `<output-parent>/.<name>.ara-snapshot-<nonce>`. Set the captured modes, then write `snapshot.json` and sync.
6. Reload the temporary copy with `load_complete`. Check that its fingerprint and modes match the capture; a mismatch is an internal error.
7. Reload the source with `load_complete`. If its fingerprint or modes changed, delete the temporary directory and fail with `stale_snapshot_input`. This matches merge's `stale_merge_input` recheck and catches direct file-tool writers that don't take the lock.
8. Rename the temporary directory to `--output` and release the lock. A failure before this rename leaves no complete package at `--output`.

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
3. Build entirely on public core APIs: `ArtifactSnapshot::load_complete`, `merge::fingerprint`, `ArtifactLock`, and `journal::pending_prepared`. No `ara-core` change is expected. If one becomes necessary, review whether the embedded viewer needs regenerating (see the compatibility notes in `docs/agent-cli.md`).
4. Document the command in `docs/agent-cli.md`, bump the patch version in `Cargo.toml`, and add a `CHANGELOG.md` entry under `[Unreleased] / Added`.

## Tests

Add them to `crates/ara-cli/tests/`, likely a new `agent_snapshot.rs`, and run the real binary:

- **Round trip.** The snapshot's fingerprint equals the fingerprint `ara merge` reports for the same artifact used as `--theirs`. Merging the snapshot and merging the live fork produce identical results.
- **Coverage.** Opaque `src/` and `evidence/` files are included with exact bytes; executable modes are preserved and listed; `.ara/`, `.git/`, and temporary files are excluded.
- **Rejections.** Existing output, output inside the source, source inside the output, a symlink in the artifact, and a prepared transaction each reject with the documented code and leave no `--output`.
- **Concurrent change.** A file changed between capture and recheck, through a test hook or a direct writer, gives `stale_snapshot_input` with no `--output` and no leftover temporary directory.
- **Diagnostics.** An artifact with existing diagnostic errors still snapshots under D-S3 report-only, and the manifest lists the errors.
- **Reads.** `status`, `show`, and `ls` on the exported `ara/` return the same results as on the source.

Before review: targeted tests, binary smoke, then the full locked workspace, format, all-target Clippy, and native/wasm checks.

## Decisions needed

| ID | Decision | Options | Recommendation |
|---|---|---|---|
| D-S1 | Locking | (a) Take `ArtifactLock` during capture, which creates `.ara/` on a never-written artifact; (b) stay lock-free like other reads and rely on the double-load recheck plus the prepared-transaction probe. | (a). Without the lock, a cooperating writer's multi-file commit could interleave with the capture and still pass the recheck. Waiting for the lock is cheaper than a reject-and-retry loop in the runner. |
| D-S2 | Source key in the manifest | (a) Accept `--source-key` and record it, as in the original draft; (b) omit it. The envelope records source identity, and `ara merge --source-key` supplies it at import. | (b). A caller-supplied key in the manifest adds no guarantee and duplicates the envelope. |
| D-S3 | Diagnostic errors | (a) Report only; publication policy decides; (b) refuse unless `--allow-errors`. | (a). A successful guarded write only promises no *new* errors, so legacy artifacts may already carry some. The runner's publication policy owns the decision. |
| D-S4 | Name | `snapshot`, `export`, or `capture`. | `snapshot`: it matches the core type and the merge vocabulary. |
| D-S5 | Public command vs. internal adapter | (a) Public CLI command; (b) no command, with the runner calling core through bindings. | (a). There are no Python bindings, and one supported command is what both agents and runners can call. |
