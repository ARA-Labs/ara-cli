# 01: `ara snapshot`, an offline exact capture of an artifact
**Date:** 2026-10-03 (revised: exact capture identity and opt-in storage)

Status: **approved** by the human developer on 2026-10-03 for the capture rules, with the requested exact-identity and zero-cost revisions below. **Still pending re-approval and a spike:** the internal version-store backend and command shape (D-S4, D-S6, D-S8 to D-S10). Without that approval, implement the directory-only command `ara snapshot --output <dir>` with the corrected manifest contract; its earlier capture baseline is this file at commit `5fb8f3f`. D-S7 now requires explicit snapshots only. Implementation pending. Target repository: `ara-cli`. Parent: [collaborative research plan series](README.md). The other CLI/core work is [04: peer-feedback merges](../../docs/collaborative-research/peer-feedback-merge.md). This documentation revision performs no implementation or commit.

## TL;DR

`ara snapshot` captures every nonprivate file under the artifact lock and exports `<new-dir>/ara/` plus `snapshot.json`. The manifest carries both the existing native merge fingerprint and a separate exact capture ID that also binds modes and diagnostics. The proposed `create`/`list`/`export` interface adds an optional private version store under `.ara/vcs/`; export addresses a capture ID, never the mode-blind native fingerprint.

Capture shares the merger's inventory and fingerprint rules. Export exposes output only after verification. A point-in-time capture requires cooperating CLI writers or caller-enforced quiescence of direct writers.

The proposed store uses [`jj-lib`](https://crates.io/crates/jj-lib) behind a non-default build feature and a separately selected store-enabled distribution. Agents never see jj commands, terms, identifiers, files, or errors. Default builds retain directory capture without compiling the store backend. `ara` decides what is captured; jj-lib only stores captured trees, their order, and its operation log.

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

Snapshots also need storage. Plan 02 requires every predecessor snapshot to be kept, because later imports use the last imported revision as their base ([02](02-contribution-workflow.md#how-integration-preserves-source-and-meaning)). With directory output only, a fork that publishes often stores many near-identical full copies, and nothing records the order of a fork's own snapshots except the external community record. A version store keeps one copy of unchanged file contents, an ordered snapshot history readable without the community record, and a recoverable log of store changes.

A Git commit of the fork is not a substitute for capture. A commit records what was staged, not what `ara merge` reads: files matched by `.gitignore` and uncommitted edits are silently omitted. Git also knows nothing about ara's lock or transaction journal, so a commit can record a half-applied write. These reasons argue against letting a version-control system decide **what** is captured, not against using one to **store** what `ara` captured.

## Decision recorded

The human developer stated on 2026-10-03:
- An agent's interaction with an ARA goes through `ara-cli` only.
- `ara-cli` uses jj-lib for version control.
- Agents are not exposed to jj.
- jj's own merge and ignore behavior need not be adopted: `ara` already owns merge, and jj-lib is used as an extensible library.

## Interface

This subcommand interface remains pending re-approval. In the proposed interface, the default build supports `snapshot create --output`; omitting `--output` requires the store-enabled build. `list` and `export` also require that build. The approved directory-only spelling remains the implementation path until the command-shape decision is made; do not ship both spellings as compatibility aliases.

```sh
ara -C ./forks/a snapshot create --json
ara -C ./forks/a snapshot create --output ./packages/a-0007 --json
ara -C ./forks/a snapshot list --json
ara -C ./forks/a snapshot export sha256:8f21… --output ./packages/a-0007 --json
```

| Command | Behavior |
|---|---|
| `snapshot create` | Store-enabled build only: capture and record. Returns `capture_id`, native `fingerprint`, file and diagnostic counts, and `recorded: true`, or `recorded: false` when the exact capture ID is already recorded. |
| `snapshot create --output <dir>` | Default build: capture and export without accessing the store. Store-enabled build: record, then export. Both produce the same package for the same captured inputs and schema. |
| `snapshot list` | Store-enabled build: recorded capture IDs in parent order, with native fingerprints and file counts. No backend identifiers. |
| `snapshot export <capture-id> --output <dir>` | Store-enabled build: export the recorded tree and original manifest. Verify the exact capture ID, native fingerprint, and captured modes. |

| Argument | Meaning |
|---|---|
| `-C <dir>` / `ARA_DIR` / discovery | Selects the source artifact, the same as every other command. |
| `--output <dir>` | Must not exist. Neither it nor the source root may contain the other. |
| `--json` | Emit an `ara.snapshot/v1` result on stdout. Errors use the standard stderr JSON error. |

Package layout:

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
  "capture_id": "sha256:8f21…",
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
- `capture_id = SHA-256("ara.capture/v1\0" || RFC8785(manifest_without_capture_id))`, rendered as `sha256:<lowercase hex>`. The canonical manifest binds the native fingerprint, sorted file paths, digests, sizes, full captured modes, exclusions, and diagnostics. Phase 1 fixes all array ordering and mode representation. It excludes machine-local paths, timestamps, store identifiers, and per-invocation fields such as `recorded` and `output`.
- `files` lists only existing nonprivate regular files, never absent canonical-source placeholders. It records each `FileSnapshot.digest` and captured mode, because the fingerprint doesn't cover modes. Sorting follows the snapshot's `BTreeMap` order; the schema fixes SHA-256 prefix formatting.
- `diagnostics` comes from the same validation `ara status` reports, evaluated on the capture. Store the original canonical manifest bytes; export does not rerun current validation or regenerate an older manifest. A changed diagnostic manifest produces a different capture ID even when the native fingerprint is unchanged.
- The stdout result echoes `capture_id`, `fingerprint`, file count, diagnostic counts, and `output` when exported. `recorded` is present only when the store was used. Directory-only capture does not claim to have recorded or deduplicated history.

| Exit | Code | When |
|---|---|---|
| 0 | | Snapshot recorded or directory-only package published, according to the selected operation. |
| 1 | `stale_snapshot_input` | The source changed between capture and the final recheck. Nothing is recorded or exposed. |
| 2 | `pending_transaction` | A prepared transaction exists. This reuses the reads' existing setup error; recovery stays with the guarded write path. |
| 2 | `output_exists`, `overlapping_roots` | Unsafe output location. |
| 2 | `unknown_snapshot` | `export` names a capture ID that is not recorded. (Revision.) |
| 2 | `snapshot_store_unavailable` | The private store is unreadable or corrupt. I/O class. (Revision.) |
| 2 | `snapshot_store_disabled` | The default build receives `list`, `export`, or `create` without `--output`. Reject before capture; do not install or initialize a backend. (Revision.) |
| 1 | existing `write.path` and encoding codes | Nonprivate symlinks, unsupported source types, or non-UTF-8 paths, preserving core semantic-error classification. |
| 2 | existing I/O and lock codes | Unreadable files, lock failure, or output I/O failure. |

Commands other than `snapshot` never open, initialize, migrate, repair, or maintain the store. A corrupt store cannot affect reads, writes, or merges. No post-write hook, next-command hook, or background task records history. The [series zero-cost contract](README.md#zero-cost-when-collaboration-is-unused) applies to both builds.

## Behavior

### Capture (approved)

1. When `--output` is given, resolve the source root and `--output` through its existing parent, then validate that the output does not exist and the roots are disjoint. Move the overlap comparison from `crates/ara-cli/src/merge.rs` to a shared helper, retaining merge's existing error contract. Reject output-parent aliasing into the source.
2. Acquire `ArtifactLock` (D-S1), then reject a prepared transaction with `pending_transaction`; do not recover or mutate source history.
3. Capture with `ArtifactSnapshot::load_complete` after unifying core privacy filtering. Inventory every nonprivate regular file, reject nonprivate symlinks, and record bytes, permissions, and digests.
4. Compute `merge::fingerprint`, collect diagnostics from the captured bytes, and construct the canonical manifest and capture ID. Do not reread the live source for diagnostics.

### Record into the private store (revision)

5. In a store-enabled build, write each captured file and the canonical manifest into the store, then build the tree and a commit whose parent is the last newly recorded capture. Author and committer are a fixed `ara` identity. Bind the capture ID and native fingerprint to the record. Preserve full captured file modes in the manifest; backend executable bits alone cannot represent modes such as `0640` versus `0644`.
6. Reload the tree and manifest from the store and compare file bytes, inventory, native fingerprint, full modes, and capture ID with the capture. A mismatch is an internal error. The store must retain the manifest as well as the tree.
7. Reload the source with `load_complete`. If the nonprivate inventory, fingerprint, or modes differ, fail with `stale_snapshot_input`. Objects already written stay unreferenced, and no snapshot is recorded. This detects observed changes but cannot prove that an unrestricted direct writer never interleaved a multi-file edit or changed and restored bytes.
8. Index the commit by capture ID (mechanism chosen in the spike: a per-capture ref or an index in `.ara/vcs/`). If that exact ID is already recorded, verify and retain its record and report `recorded: false`. Equal native fingerprints with different modes or manifests remain separate captures. Listing orders unique records by their parent chain; repeating an old capture does not append a new history event.

### Export (approved publication steps, now applied to export)

9. Write captured bytes into a temporary sibling directory `<output-parent>/.<name>.ara-snapshot-<nonce>`. Restore full captured modes from the manifest, then write the original canonical `snapshot.json` bytes and sync files and directories. An inability to represent or restore a declared mode rejects rather than silently normalizing it.
10. Reload the temporary copy with `load_complete`. Verify inventory, file bytes, native fingerprint, full modes, and manifest-derived capture ID against the capture; a mismatch is an internal error.
11. Publish the temporary directory at `--output` with an atomic no-replace operation. A concurrent creator must receive `output_exists`, never have its directory replaced. Sync the output parent before reporting durable success. Failures before publication leave no package at `--output`; failures after publication but before durability acknowledgment report I/O failure and may leave a complete verified output that the caller must inspect before retrying.

Store-enabled `snapshot create --output` runs steps 1–11 under one lock. Directory-only capture skips store steps 5, 6, and 8 but still performs source recheck step 7 before publication. `snapshot export` validates the output location as in step 1, takes the lock, loads and verifies the recorded tree and original manifest, and runs steps 9–11 without recapturing the live source. A failed export after successful recording leaves the recorded capture available; it does not roll back history.

The lock excludes cooperating CLI commits. It does not freeze code writers, background processes, or writers on another host. For a point-in-time package, the caller must stop all direct writers before capture and keep them stopped through the complete package freeze in [02](02-contribution-workflow.md#how-publication-works). Double loading is a change detector, not a filesystem transaction. The public command documents this precondition rather than claiming isolation it cannot enforce.

The command doesn't:
- mutate the source, apart from the lock file `.ara/lock` (D-S1) and the private store under `.ara/vcs/`;
- record a source key or authenticate anyone;
- fetch missing dependencies, follow external object pointers, run code, or publish.

External objects, execution metadata, and declared inputs outside the native root belong to the runner's envelope ([02](02-contribution-workflow.md#what-identifies-a-contribution)).

Limits:
- Empty directories aren't captured; the fingerprint has no directory entries.
- Memory scales with artifact size, the same as `ara merge`. Large evidence should be an external pinned object.
- Readers inspect an exported `ara/` with existing commands, and integrators pass it to `ara merge --theirs`.

## Internal version store (revision)

### Constraints

- **Agent surface.** No jj vocabulary in stdout, stderr JSON, error codes, `--help`, `docs/agent-cli.md`, or skills. Store failures map to ara error classes.
- **Authority.** Knowledge files stay authoritative (README constraint). The store is a private derived record; deleting `.ara/vcs/` loses history but no artifact content. Published packages, not the store, are the publication authority.
- **Determinism.** JSON is byte-deterministic for the same captured inputs, schema, operation, and store state. Backend timestamps and random IDs never appear in output. Exact captures are addressed by `capture_id`; the native fingerprint retains its existing merge meaning.
- **Isolation from user configuration.** `ara` never reads the user's jj or Git configuration, never creates `.jj/` in the artifact, and never touches a surrounding Git repository.
- **Offline and model-free.** No network operation is invoked.
- **Wasm.** `ara-core` gains no jj dependency and still builds for `wasm32-unknown-unknown`.
- **Privacy rules.** `.ara/` is already private at every depth (`crates/ara-core/src/write/source.rs:208`, `crates/ara-core/src/merge/identity.rs:41`). The CLI already writes `.ara/` into the artifact's `.gitignore` (`source.rs:1148`). Placing the store inside `.ara/` adds no new private namespace and does not change the fingerprint scheme.

### Placement and configuration

Use `ReadonlyRepo::init` with `repo_path = <artifact>/.ara/vcs/repo` (jj-lib 0.45.1 `lib/src/repo.rs:209`; the path is arbitrary). Do **not** use `jj_lib::workspace::Workspace`: it always creates `<workspace_root>/.jj` (`lib/src/workspace.rs:122`), which would put jj state in the artifact root and require a new private namespace. The first release uses no jj working copy; the repository holds commits only. Files and commits are written with `Store::write_file` and `Store::write_commit` (`lib/src/store.rs:239`, `:175`).

`ara` does not use jj's working-copy snapshot. That path would need jj's matcher to repeat ara's privacy rule and would read the file system a second time, which is the drift this plan exists to prevent.

jj-lib is configured with `UserSettings::from_config` over `StackedConfig::with_defaults()` plus an explicit ara layer (`lib/src/settings.rs:135`, `lib/src/config.rs:663`). No user, repository, or environment configuration file is loaded. Commit signing is off.

### Crate boundary

Add a native-only crate, `crates/ara-vcs`, as an optional CLI dependency enabled only by the non-default `snapshot-store` feature. Default build commands and release packaging must not enable it through workspace feature unification or default members. Publish the store-enabled variant separately with an explicit installation choice. The default dependency graph excludes jj-lib and store-only transitive dependencies; `ara-core`, Wasm, and the viewer never depend on `ara-vcs`.

Wrap the backend behind a small trait. `StoredCapture` owns the captured artifact and its original canonical manifest; loading only an `ArtifactSnapshot` would lose exact export metadata.

```rust
pub trait SnapshotStore {
    fn record(&mut self, capture: &ArtifactSnapshot, manifest: &SnapshotManifest) -> Result<Recorded, StoreError>;
    fn list(&self) -> Result<Vec<SnapshotEntry>, StoreError>;          // ordered by parent chain
    fn load(&self, capture_id: &CaptureId) -> Result<StoredCapture, StoreError>;
    fn export(&self, capture_id: &CaptureId, output: &Path) -> Result<(), StoreError>;
}
```

No jj-lib type crosses this trait. jj-lib's async API is driven with a minimal blocking executor inside the crate. Monthly jj-lib API changes stay in this crate, and replacing jj-lib with another store (for example `gix`) touches only this crate.

### Effect on other plans

| Plan | Change |
|---|---|
| 02 | Bind the capture ID and native fingerprint in the envelope. The runner may export a retained capture to rebuild a predecessor package; published packages remain its authority. Mode-only captures share a merge revision but not an exact package identity. |
| 03, 04 | Native source bindings and merge provenance retain their existing fingerprint semantics. Lara execution inputs remain pinned by the enclosing package; identity reconciliation does not depend on the store. |

### Spike (before the revision is re-approved)

Use a disposable implementation branch with no version bump. Submit its findings and reproducible measurements through a separate phase-2 spike PR targeting `feat/collaborative-ara`; do not land throwaway backend code. Apply the [stage PR instructions](README.md#stage-pr-instructions).

1. Create the repository at `.ara/vcs/repo` with `ReadonlyRepo::init`. Assert that no `.jj/` appears anywhere and that a hostile `~/.config/jj/config.toml` and `JJ_CONFIG` have no effect.
2. Record captures containing executable and non-executable permission changes, nested directories, an empty file, and a non-ASCII path. Load and export them with exact bytes, full modes, canonical manifests, native fingerprints, and capture IDs. Prove that a mode-only change keeps the native fingerprint but produces a distinct capture ID and exportable record.
3. Record two snapshots that share most files. Report store growth against the changed bytes.
4. Run the [zero-cost checks](README.md#how-the-zero-cost-requirement-is-checked) for default and store-enabled builds, including ordinary command paths. Measure capture latency and store growth separately on `../Agent-Native-Research-Artifact/examples/the-ara-of-ara` and large `src/`/`evidence/` fixtures. Inspect dependency trees and actual release packaging, not only feature declarations. Confirm native and Wasm builds.
5. Confirm no network code path runs. Choose backend feature flags that minimize opt-in cost without adding store dependencies to the default build.

**Spike result (2026-10-04):** [phase-2b evidence](../../docs/verification/collaborative-research/phase-2b-spike/README.md). jj-lib 0.45.1 with its Git backend passes every correctness check once the adapter isolates gix from user Git configuration. The opt-in cost is +162 crates, +8.3 MB stripped, and about +20 s clean build. The full ordinary-command zero-cost run for a store-enabled `ara` still needs the feature-gated integration. D-S4, D-S6, and D-S8 await the developer's re-approval.

Exit criteria: all correctness and zero-cost gates pass. The developer separately reviews the measured opt-in build, size, and capture costs before backend approval. A smaller backend does not waive default-build isolation. If the spike fails, record the results and use the directory-only command, or propose `gix` behind the same optional boundary.

## Implementation steps

1. Follow the [stage PR instructions](README.md#stage-pr-instructions): phase 1 freezes identity, manifest, errors, and measurement contracts; phase 2 uses separate capture, spike-evidence, and optional-store substages, each with its own PR to `feat/collaborative-ara`. Add `crates/ara-cli/src/snapshot.rs` and register `Command::Snapshot` without startup or ordinary-command store initialization.
2. Move `disjoint_roots` to a shared CLI helper and use it from both merge and snapshot.
3. Reuse `ArtifactSnapshot::load_complete`, `merge::fingerprint`, `ArtifactLock`, and `journal::pending_prepared`. Unify private-path filtering in `crates/ara-core/src/write/source.rs` and `crates/ara-core/src/merge/identity.rs` so all capture paths exclude the same private namespaces before reading them. Preserve the fingerprint scheme; test nested exclusions and existing merge consumers. Review core behavior changes for embedded-viewer regeneration under `docs/agent-cli.md`.
4. After the spike and re-approval, add the optional `ara-vcs` dependency, adapter, tests, and explicitly selected release variant. Wire `create`, `list`, and `export` to exact capture IDs. Without re-approval, ship the directory-only `ara snapshot --output` command with the corrected manifest contract. Keep normal reads and guarded writes on their existing lightweight load paths.
5. Document the agent-visible commands and the direct-writer precondition in `docs/agent-cli.md`; record the jj-lib internals in a design record under `docs/`, not in agent docs. For the functional PR, bump the workspace patch version in `Cargo.toml`, update the local package versions in `Cargo.lock` with a non-locked workspace check, and add a `CHANGELOG.md` entry under `[Unreleased] / Added`. Then run the locked gates and the dependency license review.

## Tests

Add them to `crates/ara-cli/tests/`, likely a new `agent_snapshot.rs`, and run the real binary:

- **Round trip.** The snapshot's fingerprint equals the fingerprint `ara merge` reports for the same artifact used as `--theirs`. Merging an exported snapshot and merging the live fork produce identical results.
- **Coverage.** Opaque `src/` and `evidence/` files are included with exact bytes; executable modes are preserved and listed. `.ara/`, `.git/`, and reserved temporary paths are excluded at every depth, including private paths inside knowledge directories. Private contents must not be inspected; absent canonical files must not be materialized as empty files.
- **Rejections.** Existing output, overlapping or aliased roots, a nonprivate symlink, and a prepared transaction each reject with the documented code and leave no new `--output`. Race another creator at final publication and assert its output remains untouched.
- **Concurrent change.** A known file change or mode change between capture and recheck gives `stale_snapshot_input` with no recorded snapshot, no output, and no leftover temporary directory. Exercise serialization with a cooperating CLI writer and concurrent `snapshot create` calls. The runner smoke separately proves direct-writer quiescence; do not claim the double-read test establishes unrestricted-writer isolation.
- **Publication durability.** Exercise failures before publication and after publication but before acknowledgment. The former exposes no output; the latter exposes at most a complete verified package and an explicit I/O failure, never false success or a partly copied output.
- **Diagnostics.** An artifact with existing diagnostic errors still snapshots under D-S3 report-only, and the manifest lists the errors.
- **Reads.** `status`, `show`, and `ls` on an exported `ara/` return the same results as on the source.
- **Exact identity.** Changing only a file mode, including `0640` to `0644`, preserves the native fingerprint but changes the capture ID. Retain and export both versions with their original full modes and canonical manifests. A changed manifest cannot overwrite a prior record under the same native fingerprint.
- **Pay-for-use.** Exercise ordinary reads, writes, fixes, and merges with absent, corrupt, unreadable, and large stores under both build variants. Prove no store access or automatic capture and no new opaque-body reads on ordinary paths. Run the series performance checks and default dependency/distribution checks; timing alone does not prove isolation.
- **Store (revision).**
  - Recording identical captured bytes, modes, and manifest returns `recorded: false` and adds no store commit.
  - `list` returns snapshots in parent order across several records.
  - Exporting an older capture ID reproduces its package exactly, including its original diagnostics; an unknown capture ID gives `unknown_snapshot`.
  - `.ara/vcs/` never appears in a fingerprint, manifest, or export.
  - No `.jj/` is created; user jj configuration is ignored; a surrounding Git repository is unchanged.
  - Only explicit `create` initializes a missing store. `list` on a missing store is empty and `export` returns `unknown_snapshot`, without creating one. A corrupt store gives `snapshot_store_unavailable` only to store-using operations.
  - Scan all snapshot outputs, errors, and help text for jj terms and backend identifiers.
  - Identical captured inputs, operation, schema, and store state give byte-identical JSON across runs and machines. The intentional `recorded: true` to `false` transition is not an identity change.

Before review: targeted tests, binary smoke, then the full locked workspace, format, all-target Clippy, and native/wasm checks.

## Approved decisions

| ID | Decision | Selected behavior and reason |
|---|---|---|
| D-S1 | Locking | Take `ArtifactLock`, including creation of `.ara/` on a never-written artifact. Cooperating CLI writes serialize; the runner separately quiesces direct writers. |
| D-S2 | Source key | Omit `--source-key` from snapshot. The contribution envelope owns source identity and `ara merge --source-key` supplies it at import. |
| D-S3 | Diagnostic errors | Report only; publication policy decides. Guarded writes promise no new errors, so existing diagnostic errors do not by themselves prevent capture. Source I/O and capture/parse failures still reject. |
| D-S4 | Name | Approved: `snapshot`, matching the core type and merge vocabulary. Revision pending: subcommands `snapshot create`/`list`/`export` (below). |
| D-S5 | Public command | Provide a supported CLI command for agents and the Python runner rather than add Python core bindings. |
| D-S7 (J2) | Automatic history | Explicit snapshot creation only. No history capture during ordinary commands, after guarded writes, at next-command startup, or in background tasks. Required by the developer's zero-cost principle. |

## Decisions pending re-approval (revision)

These retain decision IDs from the earlier revision. D-S7 is now fixed by the zero-cost requirement above. D-S4 and D-S9 remain recommendations; D-S6 and D-S8 need spike evidence. D-S10 remains out of scope.

| ID | Question | Recommendation | Related-work basis |
|---|---|---|---|
| D-S4 (J3) | Command shape: `snapshot create`/`list`/`export`, or keep `snapshot [--output]` and add `list`/`export`? | `snapshot create`/`list`/`export`; one subcommand per action. Needs re-approval of README D4. | borg uses `create`/`list`/`extract`; restic uses `backup`/`snapshots`/`restore`; `git stash` uses `push`/`list`/`show`/`apply`. |
| D-S6 (J1) | Confirm jj-lib after the spike, or fall back to `gix`? | Decide on spike data. | None applies. Agora runs Git through a Go service; the tools below wrap Git or a custom store. |
| D-S8 (J4) | jj-lib storage backend. | Its Git backend, the production backend, unless the spike shows a lighter supported option. | None applies; spike data. |
| D-S9 (J5) | Retention and garbage collection of the store. | Keep everything in the first release. A later policy must never prune a snapshot that was published or imported; other snapshots may be pruned only by an explicit policy. The store does not know publication state, so pruning needs a protected list from the runner. | Agora is append-only and its App. A asks a retained run to pin the full graph. Plan 02 requires every predecessor snapshot. borg and restic prune only by explicit keep policies (`prune`, `forget --keep-*`). |
| D-S10 (J6) | Read commands at a recorded snapshot (for example `ara --at <capture-id> show N12`). | Out of scope now; explicit `export` covers current needs. Any later design must retain zero-cost ordinary reads. | Historical reads are common in version stores, but no implementation is approved here. |

Sources: Agora and the Live Research Manager from the Obsidian notes `Papers/Zhang2026-Agora` and `Analyses/Agora vs ARA as Research Records`; jj from its documentation. The borg, restic, `git stash`, DataLad, DVC, and MLflow behavior is from general knowledge and was not re-checked when this table was written.

## Alternatives considered

- **Runner-side copy and hash.** Re-implements the core capture rules in Python; any drift breaks fingerprint agreement with `ara merge`. Rejected (see Problem).
- **A Git commit of the fork as the capture.** Omits ignored and unstaged files and ignores ara's lock and transaction journal. Rejected as capture; Git-format storage of an `ara` capture remains possible through D-S8.
- **Directory-only output (approved baseline).** Simplest; no shared storage and no history. Remains the fallback.
- **`gix` object store.** Lighter and enough for storage, but no operation log. Kept as the fallback behind the same trait.
- **Git CLI as the store.** Adds an external executable and exposes behavior to user configuration and filters.
- **jj `Workspace` in the artifact root.** Creates `.jj/`, a new private namespace that agent code tools can see.
- **jj working-copy snapshot as the capture.** A second inventory rule and a second file-system read.

## Next Steps

1. Freeze the canonical manifest, exact capture ID, mode representation, and error schema in phase 1. Keep `ara.artifact/v1` unchanged.
2. Review D-S4 and D-S9. Run the phase-2 spike with zero-cost evidence; decide D-S6 and D-S8 before an optional-store implementation PR. Every substage targets `feat/collaborative-ara`.
3. Run the documented capture, store, publication, and round-trip checks before the runner adopts the command. After implementation, move the completed design into `docs/` under repository policy.
