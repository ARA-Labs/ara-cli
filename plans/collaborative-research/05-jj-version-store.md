# 05: Internal version store for snapshots, built on jj-lib
**Date:** 2026-10-03

Status: **draft**, awaiting review by the human developer. Target repository: `ara-cli`. Parent: [collaborative research plan series](README.md). This plan amends the approved [01: snapshot](01-ara-snapshot.md) output path. It starts with a spike; no implementation, dependency addition, or commit is approved by this draft.

## TL;DR

Agents interact with an ARA only through `ara`. Version control of an artifact's snapshots is an internal service of `ara-cli`, implemented with [`jj-lib`](https://crates.io/crates/jj-lib) and stored in the private `.ara/vcs/` directory. Agents never see jj: no jj command, term, identifier, file, or error appears in CLI output, agent docs, or skills. `ara` keeps capture, privacy filtering, locking, and the `ara.artifact/v1` fingerprint; jj-lib only stores the captured trees, their parent chain, and its operation log. Exported packages keep the exact directory layout from plan 01, so the runner, peers, and `ara merge --theirs` are unchanged.

## Problem

Plan 01 exports each snapshot as a full directory copy. Plan 02 then requires every predecessor snapshot to be kept, because later imports use the last imported revision as their base (`02-contribution-workflow.md`, "How integration preserves source and meaning"). A fork that publishes often therefore stores many near-identical copies, and nothing records the order of a fork's own snapshots except the external community record.

A version store gives:
- one stored copy of unchanged file contents across a fork's snapshots;
- an ordered history of a fork's snapshots, readable without the community record;
- a recoverable log of store changes.

Using plain Git for this was discussed and rejected as the capture mechanism: a commit records what was staged, not what `ara merge` reads, and Git knows nothing about ara's lock or transaction journal. Those reasons still apply. They argue against letting a VCS decide **what** is captured, not against using one to **store** what ara captured.

## Decision recorded

The human developer stated on 2026-10-03:
- An agent's interaction with an ARA goes through `ara-cli` only.
- `ara-cli` uses jj-lib for version control.
- Agents are not exposed to jj.
- jj's own merge and ignore behavior need not be adopted: `ara` already owns merge, and jj-lib is used as an extensible library.

## Constraints

- **Agent surface.** No jj vocabulary in stdout, stderr JSON, error codes, `--help`, `docs/agent-cli.md`, or skills. Store failures map to existing ara error classes.
- **Authority.** Knowledge files stay authoritative (README constraint). The store is a private derived record; deleting `.ara/vcs/` loses history but no artifact content. Published packages, not the store, are the publication authority.
- **Determinism.** CLI JSON stays byte-deterministic. jj commit and change IDs contain timestamps and random parts, so they never appear in output. Snapshots are addressed by the ara fingerprint.
- **Isolation from user configuration.** `ara` never reads the user's jj or Git configuration, never creates `.jj/` in the artifact, and never touches a surrounding Git repository.
- **Offline and model-free.** No network operation is invoked.
- **Wasm.** `ara-core` gains no jj dependency and still builds for `wasm32-unknown-unknown`.
- **Privacy rules.** `.ara/` is already private at every depth (`crates/ara-core/src/write/source.rs:208`, `crates/ara-core/src/merge/identity.rs:41`). The CLI already writes `.ara/` into the artifact's `.gitignore` (`source.rs:1148`). Placing the store inside `.ara/` adds no new private namespace and does not change the fingerprint scheme.

## Proposed design

### Placement: a bare jj repository under `.ara/vcs/`

Use `ReadonlyRepo::init` with `repo_path = <artifact>/.ara/vcs/repo` (jj-lib 0.45.1 `lib/src/repo.rs:209`; the path is arbitrary). Do **not** use `jj_lib::workspace::Workspace`: it always creates `<workspace_root>/.jj` (`lib/src/workspace.rs:122`), which would put jj state in the artifact root and require a new private namespace.

The first release uses no jj working copy. The repository holds commits only.

### Capture stays in ara; jj stores the result

`ara snapshot` keeps plan 01 steps 1–4 unchanged:
1. Take `ArtifactLock`. Reject a prepared transaction.
2. Capture with `ArtifactSnapshot::load_complete` and the unified core privacy rule.
3. Compute the `ara.artifact/v1` fingerprint and diagnostics from the captured bytes.

It then replaces the directory copy with a store record:
4. Write each captured file with `Store::write_file` (`lib/src/store.rs:239`), build the tree with its executable bits, and write a commit (`store.rs:175`). The parent is this artifact's previous snapshot commit. Author and committer are a fixed `ara` identity. The commit description carries the fingerprint and manifest digest.
5. Reload the tree from the store and compare its inventory, fingerprint, and modes with the capture. A mismatch is an internal error.
6. Reload the source with `load_complete`. A change gives `stale_snapshot_input`, as in plan 01. Objects already written stay unreferenced, and no snapshot is recorded.
7. Record the commit under the fingerprint (mechanism chosen in the spike: a per-fingerprint ref, or an index in `.ara/vcs/`). Release the lock.

ara does not use jj's working-copy snapshot. That path would need jj's matcher to repeat ara's privacy rule and would read the file system a second time, which is the drift plan 01 exists to prevent. A later plan may revisit this for speed.

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

### Agent-visible interface

| Command | Behavior |
|---|---|
| `ara snapshot create --json` | Capture and record. Returns `ara.snapshot/v1` with `fingerprint`, file count, diagnostic counts, and `recorded: true`, or `recorded: false` when the same fingerprint already exists. |
| `ara snapshot create --output <dir> --json` | Record, then export. Same result as plan 01. |
| `ara snapshot list --json` | Recorded fingerprints of this artifact in parent order, with file counts. No store identifiers. |
| `ara snapshot export <fingerprint> --output <dir> --json` | Write plan 01's package layout (`ara/` plus `snapshot.json`) with plan 01's safe, no-replace publication. Verify the exported fingerprint. |

New error codes: `unknown_snapshot` (exit 2, setup class) and `snapshot_store_unavailable` (exit 2, I/O class). Commands other than `snapshot` never open the store, so a corrupt store cannot affect reads, writes, or merges.

## Changes to approved plans

| Plan | Change |
|---|---|
| 01 | Steps 1–4, the manifest, the direct-writer precondition, the privacy fix, and the error codes stay. Steps 5–8 become "record, then optionally export". D-S4 changes from `snapshot` to `snapshot create`/`list`/`export`. Reopen D4 in the README for review. |
| 02 | None to the contract. The runner may use `export` to rebuild a predecessor package, but published packages remain the authority it retains. |
| 03, 04 | None. Plan 04's identity reconciliation is semantic and does not depend on storage. |

## Spike (before revising plan 01)

On a throwaway branch, with no version bump:

1. Create the repository at `.ara/vcs/repo` with `ReadonlyRepo::init`. Assert that no `.jj/` appears anywhere and that a hostile `~/.config/jj/config.toml` and `JJ_CONFIG` have no effect.
2. Record a capture containing an executable file, nested directories, an empty file, and a non-ASCII path. Load it back and assert byte, mode, and `ara.artifact/v1` fingerprint equality.
3. Record two snapshots that share most files. Report store growth against the changed bytes.
4. Measure: added crates, clean and incremental build time, release binary size delta, latency on `../Agent-Native-Research-Artifact/examples/the-ara-of-ara` and on a fixture with large `src/`/`evidence/` files. Confirm that `ara-core` still builds for wasm.
5. Confirm no network code path runs, and choose the jj-lib backend and feature flags that minimize dependencies.

Exit criteria: steps 1–2 pass, and the build and size cost is acceptable to the developer. If not, record the results and fall back to `gix` behind the same trait, or to plan 01's directory-only output. Spike results go into this plan before plan 01 is revised.

## Implementation steps (after the spike and approval)

1. Add `crates/ara-vcs` with the trait, the jj-lib adapter, and unit tests.
2. Add the `snapshot create|list|export` subcommands in `crates/ara-cli/src/snapshot.rs` on top of plan 01's shared capture.
3. Update plan 01 and README D4. Document only the agent-visible behavior in `docs/agent-cli.md`; record the jj-lib internals in a design record under `docs/`.
4. Bump the workspace patch version, update `Cargo.lock`, and add a `CHANGELOG.md` entry. Run the locked workspace gates, Clippy, native/wasm checks, and the dependency license review.

## Tests

- **Round trip.** Record, export, and merge the export with `--theirs`; the result equals merging the live fork.
- **Idempotence.** Recording an unchanged artifact returns `recorded: false` and adds no store commit.
- **Order.** `list` returns snapshots in parent order across several records.
- **Privacy.** `.ara/vcs/` never appears in a fingerprint, manifest, or export.
- **Isolation.** No `.jj/` is created. User jj configuration is ignored. A surrounding Git repository is unchanged.
- **Failure.** A missing store is created on first use. A corrupt store gives `snapshot_store_unavailable`, and all other commands still work. An unknown fingerprint gives `unknown_snapshot`.
- **Concurrency.** A source change between capture and recheck records nothing. Concurrent `snapshot create` calls serialize on `ArtifactLock`.
- **No jj on the agent surface.** Scan all snapshot outputs, errors, and help text for jj terms and store identifiers.
- **Determinism.** Identical inputs give byte-identical JSON across runs and machines.

## Alternatives considered

- **Directory-only output (approved plan 01).** Simplest; no shared storage and no history. Remains the fallback.
- **`gix` object store.** Lighter and enough for storage, but no operation log. Kept as the fallback behind the same trait.
- **Git CLI.** Adds an external executable and exposes behavior to user configuration and filters.
- **jj `Workspace` in the artifact root.** Creates `.jj/`, a new private namespace that agent code tools can see.
- **jj working-copy snapshot as the capture.** A second inventory rule and a second file-system read.

## Open decisions

J2, J3, and J5 carry recommendations backed by related work; they await the developer's approval. J1 and J4 need spike data. J6 is a timing question.

| ID | Question | Recommendation | Related-work basis |
|---|---|---|---|
| J1 | Confirm jj-lib after the spike, or fall back to `gix`? | Decide on spike data. | None applies. Agora runs Git through a Go service; the tools below wrap Git or a custom store. |
| J2 | Record history only on explicit `snapshot create`, or after every committed guarded write too? | Explicit snapshots only. If per-write history is needed later, record it at the start of the next command, as jj does, never as part of the write. | Agora records one commit per published contribution, not per edit. The ARA Live Research Manager commits on closure signals. DataLad (`save`, `run`), DVC (`commit`), and MLflow (runs) record at explicit points. jj records automatically, but at the start of the next command ([working-copy docs](https://github.com/jj-vcs/jj/blob/main/docs/working-copy.md)), so recording cannot fail after a write commits. Per-write history would also duplicate the before/after audit history that guarded writes already keep, and `src/`/`evidence/` are written by experiment tools, not by `ara`. |
| J3 | Command shape: `snapshot create`/`list`/`export`, or keep `snapshot [--output]` and add `list`/`export`? | `snapshot create`/`list`/`export`; one subcommand per action. Changes D-S4 and needs re-approval of README D4. | borg uses `create`/`list`/`extract`; restic uses `backup`/`snapshots`/`restore`; `git stash` uses `push`/`list`/`show`/`apply`. |
| J4 | jj-lib storage backend. | Its Git backend, the production backend, unless the spike shows a lighter supported option. | None applies; spike data. |
| J5 | Retention and garbage collection of the store. | Keep everything in the first release. A later policy must never prune a snapshot that was published or imported; other snapshots may be pruned only by an explicit policy. The store does not know publication state, so pruning needs a protected list from the runner. | Agora is append-only and its App. A asks a retained run to pin the full graph. Plan 02 requires every predecessor snapshot. borg and restic prune only by explicit keep policies (`prune`, `forget --keep-*`). |
| J6 | Read commands at a recorded snapshot (for example `ara --at <fingerprint> show N12`). | Out of scope now; `export` covers current needs. A later plan. | Common (`git show <rev>:<path>`, `jj -r <rev>`; Agora requires reproduction from a fresh checkout), so this decides when, not whether. |

Sources: Agora and the Live Research Manager from the Obsidian notes `Papers/Zhang2026-Agora` and `Analyses/Agora vs ARA as Research Records`; jj from its documentation. The borg, restic, `git stash`, DataLad, DVC, and MLflow behavior is from general knowledge and was not re-checked when this table was written.

## Next Steps

1. Review this draft and approve or change the J2, J3, and J5 recommendations.
2. Run the spike and record its results here.
3. Revise plan 01 and README D4 for approval before any implementation.
