# 01: `ara snapshot`, an offline exact capture of an artifact
**Date:** 2026-10-03 (revised 2026-10-03: internal version store)

Status: **approved** by the human developer on 2026-10-03 for the capture rules: the Capture section, the export publication steps, D-S1 to D-S3, and D-S5. **Revision pending re-approval and a spike:** the internal version store built on jj-lib, and the command shape (D-S4 revised, D-S6 to D-S10). If the spike fails or the revision is rejected, the approved directory-only command `ara snapshot --output <dir>` stands unchanged; its approved text is this file at commit `5fb8f3f`. Implementation pending. Target repository: `ara-cli`. Parent: [collaborative research plan series](README.md). The other CLI/core work in this series is [04: peer-feedback merges](04-peer-feedback-merge.md). No implementation or commit is performed by this documentation.

## TL;DR

`ara snapshot create` captures every nonprivate file of an artifact under the artifact lock, computes the merge fingerprint, and records the capture in a private version store under `.ara/vcs/`. `ara snapshot export <fingerprint> --output <new-dir>` writes the package that peers, the runner, and `ara merge --theirs` read: `<new-dir>/ara/` plus a `snapshot.json` manifest with the fingerprint, a per-file digest, size, and mode, and the diagnostics. `ara snapshot list` shows recorded fingerprints in order.

Capture shares the merger's inventory and fingerprint rules. Export exposes output only after verification. A point-in-time capture requires cooperating CLI writers or caller-enforced quiescence of direct writers.

The store is implemented with [`jj-lib`](https://crates.io/crates/jj-lib), but agents never see jj. No jj command, term, identifier, file, or error appears in CLI output, agent docs, or skills. `ara` decides what is captured; jj-lib only stores captured trees, their order, and its operation log.

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

```sh
ara -C ./forks/a snapshot create --json
ara -C ./forks/a snapshot create --output ./packages/a-0007 --json
ara -C ./forks/a snapshot list --json
ara -C ./forks/a snapshot export 3b9c…e41a --output ./packages/a-0007 --json
```

| Command | Behavior |
|---|---|
| `snapshot create` | Capture and record. Returns `ara.snapshot/v1` with `fingerprint`, file count, diagnostic counts, and `recorded: true`, or `recorded: false` when the same fingerprint is already recorded. |
| `snapshot create --output <dir>` | Record, then export. The package equals the approved directory-only output. |
| `snapshot list` | Recorded fingerprints of this artifact in parent order, with file counts. No store identifiers. |
| `snapshot export <fingerprint> --output <dir>` | Write the package for a recorded fingerprint with the export publication steps below. Verify the exported fingerprint. |

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
- The stdout result echoes `fingerprint`, file count, the diagnostic counts, `recorded`, and `output` when a package was exported.

| Exit | Code | When |
|---|---|---|
| 0 | | Snapshot recorded; package exposed at `--output` when requested. |
| 1 | `stale_snapshot_input` | The source changed between capture and the final recheck. Nothing is recorded or exposed. |
| 2 | `pending_transaction` | A prepared transaction exists. This reuses the reads' existing setup error; recovery stays with the guarded write path. |
| 2 | `output_exists`, `overlapping_roots` | Unsafe output location. |
| 2 | `unknown_snapshot` | `export` names a fingerprint that is not recorded. (Revision.) |
| 2 | `snapshot_store_unavailable` | The private store is unreadable or corrupt. I/O class. (Revision.) |
| 1 | existing `write.path` and encoding codes | Nonprivate symlinks, unsupported source types, or non-UTF-8 paths, preserving core semantic-error classification. |
| 2 | existing I/O and lock codes | Unreadable files, lock failure, or output I/O failure. |

Commands other than `snapshot` never open the store, so a corrupt store cannot affect reads, writes, or merges.

## Behavior

### Capture (approved)

1. When `--output` is given, resolve the source root and `--output` through its existing parent, then validate that the output does not exist and the roots are disjoint. Move the overlap comparison from `crates/ara-cli/src/merge.rs` to a shared helper, retaining merge's existing error contract. Reject output-parent aliasing into the source.
2. Acquire `ArtifactLock` (D-S1), then reject a prepared transaction with `pending_transaction`; do not recover or mutate source history.
3. Capture with `ArtifactSnapshot::load_complete` after unifying core privacy filtering. Inventory every nonprivate regular file, reject nonprivate symlinks, and record bytes, permissions, and digests.
4. Compute `merge::fingerprint` and collect diagnostics from the captured bytes, not a later read of the live source.

### Record into the private store (revision)

5. Write each captured file into the store, build the tree with its executable bits, and write a commit whose parent is this artifact's previous snapshot. Author and committer are a fixed `ara` identity. The commit description carries the fingerprint and manifest digest.
6. Reload the tree from the store and compare its inventory, fingerprint, and modes with the capture. A mismatch is an internal error.
7. Reload the source with `load_complete`. If the nonprivate inventory, fingerprint, or modes differ, fail with `stale_snapshot_input`. Objects already written stay unreferenced, and no snapshot is recorded. This detects observed changes but cannot prove that an unrestricted direct writer never interleaved a multi-file edit or changed and restored bytes.
8. Index the commit under the fingerprint (mechanism chosen in the spike: a per-fingerprint ref, or an index in `.ara/vcs/`). If the fingerprint is already recorded, keep the existing record and report `recorded: false`.

### Export (approved publication steps, now applied to export)

9. Write the captured bytes into a temporary sibling directory `<output-parent>/.<name>.ara-snapshot-<nonce>`. Set the captured modes, then write `snapshot.json` and sync the files and directories.
10. Reload the temporary copy with `load_complete`. Compare the complete exported inventory, fingerprint, and modes against the capture; a mismatch is an internal error.
11. Publish the temporary directory at `--output` with an atomic no-replace operation. A concurrent creator must receive `output_exists`, never have its directory replaced. Sync the output parent before reporting durable success. Failures before publication leave no package at `--output`; failures after publication but before durability acknowledgment report I/O failure and may leave a complete verified output that the caller must inspect before retrying.

`snapshot create --output` runs steps 1–11 under one lock. `snapshot export` takes the lock, loads the recorded tree from the store, and runs steps 9–11 with that tree as the capture.

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
- **Determinism.** CLI JSON stays byte-deterministic. jj commit and change IDs contain timestamps and random parts, so they never appear in output. Snapshots are addressed by the ara fingerprint.
- **Isolation from user configuration.** `ara` never reads the user's jj or Git configuration, never creates `.jj/` in the artifact, and never touches a surrounding Git repository.
- **Offline and model-free.** No network operation is invoked.
- **Wasm.** `ara-core` gains no jj dependency and still builds for `wasm32-unknown-unknown`.
- **Privacy rules.** `.ara/` is already private at every depth (`crates/ara-core/src/write/source.rs:208`, `crates/ara-core/src/merge/identity.rs:41`). The CLI already writes `.ara/` into the artifact's `.gitignore` (`source.rs:1148`). Placing the store inside `.ara/` adds no new private namespace and does not change the fingerprint scheme.

### Placement and configuration

Use `ReadonlyRepo::init` with `repo_path = <artifact>/.ara/vcs/repo` (jj-lib 0.45.1 `lib/src/repo.rs:209`; the path is arbitrary). Do **not** use `jj_lib::workspace::Workspace`: it always creates `<workspace_root>/.jj` (`lib/src/workspace.rs:122`), which would put jj state in the artifact root and require a new private namespace. The first release uses no jj working copy; the repository holds commits only. Files and commits are written with `Store::write_file` and `Store::write_commit` (`lib/src/store.rs:239`, `:175`).

`ara` does not use jj's working-copy snapshot. That path would need jj's matcher to repeat ara's privacy rule and would read the file system a second time, which is the drift this plan exists to prevent.

jj-lib is configured with `UserSettings::from_config` over `StackedConfig::with_defaults()` plus an explicit ara layer (`lib/src/settings.rs:135`, `lib/src/config.rs:663`). No user, repository, or environment configuration file is loaded. Commit signing is off.

### Crate boundary

Add a native-only crate, `crates/ara-vcs`, that wraps jj-lib behind a small trait:

```rust
pub trait SnapshotStore {
    fn record(&mut self, capture: &ArtifactSnapshot, manifest: &SnapshotManifest) -> Result<Recorded, StoreError>;
    fn list(&self) -> Result<Vec<SnapshotEntry>, StoreError>;          // ordered by parent chain
    fn load(&self, fingerprint: &Fingerprint) -> Result<ArtifactSnapshot, StoreError>;
    fn export(&self, fingerprint: &Fingerprint, output: &Path) -> Result<(), StoreError>;
}
```

No jj-lib type crosses this trait. jj-lib's async API is driven with a minimal blocking executor inside the crate. Monthly jj-lib API changes stay in this crate, and replacing jj-lib with another store (for example `gix`) touches only this crate.

### Effect on other plans

| Plan | Change |
|---|---|
| 02 | None to the contract. The runner may use `export` to rebuild a predecessor package, but published packages remain the authority it retains. |
| 03, 04 | None. Plan 04's identity reconciliation is semantic and does not depend on storage. |

### Spike (before the revision is re-approved)

On a throwaway branch, with no version bump:

1. Create the repository at `.ara/vcs/repo` with `ReadonlyRepo::init`. Assert that no `.jj/` appears anywhere and that a hostile `~/.config/jj/config.toml` and `JJ_CONFIG` have no effect.
2. Record a capture containing an executable file, nested directories, an empty file, and a non-ASCII path. Load it back and assert byte, mode, and `ara.artifact/v1` fingerprint equality.
3. Record two snapshots that share most files. Report store growth against the changed bytes.
4. Measure: added crates, clean and incremental build time, release binary size delta, latency on `../Agent-Native-Research-Artifact/examples/the-ara-of-ara` and on a fixture with large `src/`/`evidence/` files. Confirm that `ara-core` still builds for wasm.
5. Confirm no network code path runs, and choose the jj-lib backend and feature flags that minimize dependencies.

Exit criteria: steps 1–2 pass, and the build and size cost is acceptable to the developer. If not, record the results here and fall back to `gix` behind the same trait, or to the approved directory-only command.

## Implementation steps

1. Add `crates/ara-cli/src/snapshot.rs` with the argument structs and commands, and register `Command::Snapshot` in `crates/ara-cli/src/main.rs`.
2. Move `disjoint_roots` to a shared CLI helper and use it from both merge and snapshot.
3. Reuse `ArtifactSnapshot::load_complete`, `merge::fingerprint`, `ArtifactLock`, and `journal::pending_prepared`. Unify private-path filtering in `crates/ara-core/src/write/source.rs` and `crates/ara-core/src/merge/identity.rs` so all capture paths exclude the same private namespaces before reading them. Preserve the fingerprint scheme; test nested exclusions and existing merge consumers. Review core behavior changes for embedded-viewer regeneration under `docs/agent-cli.md`.
4. After the spike and re-approval, add `crates/ara-vcs` with the trait, the jj-lib adapter, and unit tests, and wire `create`, `list`, and `export` to it. Without re-approval, ship the approved `ara snapshot --output` command from steps 1–3 only.
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
- **Store (revision).**
  - Recording an unchanged artifact returns `recorded: false` and adds no store commit.
  - `list` returns snapshots in parent order across several records.
  - `export` of an older fingerprint reproduces its package exactly; an unknown fingerprint gives `unknown_snapshot`.
  - `.ara/vcs/` never appears in a fingerprint, manifest, or export.
  - No `.jj/` is created; user jj configuration is ignored; a surrounding Git repository is unchanged.
  - A missing store is created on first use. A corrupt store gives `snapshot_store_unavailable`, and all other commands still work.
  - Scan all snapshot outputs, errors, and help text for jj terms and store identifiers.
  - Identical inputs give byte-identical JSON across runs and machines.

Before review: targeted tests, binary smoke, then the full locked workspace, format, all-target Clippy, and native/wasm checks.

## Approved decisions

| ID | Decision | Selected behavior and reason |
|---|---|---|
| D-S1 | Locking | Take `ArtifactLock`, including creation of `.ara/` on a never-written artifact. Cooperating CLI writes serialize; the runner separately quiesces direct writers. |
| D-S2 | Source key | Omit `--source-key` from snapshot. The contribution envelope owns source identity and `ara merge --source-key` supplies it at import. |
| D-S3 | Diagnostic errors | Report only; publication policy decides. Guarded writes promise no new errors, so existing diagnostic errors do not by themselves prevent capture. Source I/O and capture/parse failures still reject. |
| D-S4 | Name | Approved: `snapshot`, matching the core type and merge vocabulary. Revision pending: subcommands `snapshot create`/`list`/`export` (below). |
| D-S5 | Public command | Provide a supported CLI command for agents and the Python runner rather than add Python core bindings. |

## Decisions pending re-approval (revision)

These were J1–J6 in the earlier draft plan 05, which this revision replaces. D-S4, D-S7, and D-S9 carry recommendations backed by related work. D-S6 and D-S8 need spike data. D-S10 is a timing question.

| ID | Question | Recommendation | Related-work basis |
|---|---|---|---|
| D-S4 (J3) | Command shape: `snapshot create`/`list`/`export`, or keep `snapshot [--output]` and add `list`/`export`? | `snapshot create`/`list`/`export`; one subcommand per action. Needs re-approval of README D4. | borg uses `create`/`list`/`extract`; restic uses `backup`/`snapshots`/`restore`; `git stash` uses `push`/`list`/`show`/`apply`. |
| D-S6 (J1) | Confirm jj-lib after the spike, or fall back to `gix`? | Decide on spike data. | None applies. Agora runs Git through a Go service; the tools below wrap Git or a custom store. |
| D-S7 (J2) | Record history only on explicit `snapshot create`, or after every committed guarded write too? | Explicit snapshots only. If per-write history is needed later, record it at the start of the next command, as jj does, never as part of the write. | Agora records one commit per published contribution, not per edit. The ARA Live Research Manager commits on closure signals. DataLad (`save`, `run`), DVC (`commit`), and MLflow (runs) record at explicit points. jj records automatically, but at the start of the next command ([working-copy docs](https://github.com/jj-vcs/jj/blob/main/docs/working-copy.md)), so recording cannot fail after a write commits. Per-write history would also duplicate the before/after audit history that guarded writes already keep, and `src/`/`evidence/` are written by experiment tools, not by `ara`. |
| D-S8 (J4) | jj-lib storage backend. | Its Git backend, the production backend, unless the spike shows a lighter supported option. | None applies; spike data. |
| D-S9 (J5) | Retention and garbage collection of the store. | Keep everything in the first release. A later policy must never prune a snapshot that was published or imported; other snapshots may be pruned only by an explicit policy. The store does not know publication state, so pruning needs a protected list from the runner. | Agora is append-only and its App. A asks a retained run to pin the full graph. Plan 02 requires every predecessor snapshot. borg and restic prune only by explicit keep policies (`prune`, `forget --keep-*`). |
| D-S10 (J6) | Read commands at a recorded snapshot (for example `ara --at <fingerprint> show N12`). | Out of scope now; `export` covers current needs. A later plan. | Common (`git show <rev>:<path>`, `jj -r <rev>`; Agora requires reproduction from a fresh checkout), so this decides when, not whether. |

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

1. Freeze the manifest and error schema, then implement shared capture filtering and the approved capture rules.
2. Review the revision: approve or change D-S4, D-S7, and D-S9. Run the spike and record its results here; decide D-S6 and D-S8.
3. Run the documented capture, store, publication, and round-trip checks before the runner adopts the command. After implementation, move the completed design into `docs/` under repository policy.
