# PR 09: Build merge inputs from local Git history

**Date:** 2026-10-01

Status: Draft for review. Target repository: `ARA-Labs/ara-cli`. Parent: [Agent CLI interface, Phase 3](../agent-cli-interface.md#phase-3-ara-merge). PR map and shared gates: [README](README.md). Dependency: [PR 08: directory merge](08-directory-merge.md), including approval of its source identity, portable import journal, and conflict rules in [PR 00](00-protocol-contracts.md). This PR implements parent question Q6 by calling the `git` binary. It does not introduce a second merge engine.

## TL;DR

Add `ara merge --git <ref>` as a local Git snapshot adapter for the directory merger. Compute the common ancestor from pinned HEAD and theirs commit IDs, materialize base and committed theirs into private temporary directories, and merge into our current working artifact. Preserve staged and unstaged local content, leave Git's index and branch state unchanged, and never fetch missing objects. Report the chosen commits and fail without artifact mutation when ancestry, object availability, or temporary materialization is unsafe.

## Problem

The directory merger requires a common base and a source artifact. Agents working on Git branches should not have to create those copies manually or resolve tree appends as text. The parent plan already chooses the Git binary over a Rust Git library. The inspected `crates/ara-cli/src/main.rs` exposes validate, layout, check, and serve, and the CLI source search found no Git subprocess helper to reuse.

HEAD supplies the commit used for ancestry. The destination content comes from the current working artifact, including staged and unstaged changes present on disk. Taking ours from a commit or index would lose those changes. PR 08 provides the planner and transaction; this wrapper supplies the snapshots and provenance without changing their merge rules.

## Constraints

Use only locally available Git objects and native subprocesses. Do not run `git fetch`, pull, checkout, merge, reset, commit, worktree add, submodule update, LFS download, hooks, external merge drivers, or clean/smudge filters. The command merges ARA content and leaves repository refs, HEAD, index bytes, Git merge state, and unrelated files unchanged. No network or LLM call belongs inside `ara`.

The destination is the ARA selected by PR 02's normal `-C`/environment/upward lookup. It must lie within a non-bare Git working tree. The same repository-relative artifact root must exist at the selected base and theirs commits. Do not search a different historical ARA or guess a rename from a title. Supporting root relocation would need an explicit reviewed interface and is not included here. Linked worktrees and detached HEAD are valid if the path and ancestry checks succeed.

This is a native-only CLI adapter. It invokes PR 08's pure planner and PR 03/06's guarded transaction, keeping Git subprocess code out of the core wasm boundary. Review the Git-mode rules below before implementation; the parent chooses `git` but does not approve ancestry ambiguity, shallow-history, source-label, or materialization behavior.

## Proposed approach

### What exactly does `--git` mean?

The proposed syntax extends the PR 08 command:

```sh
ara -C <ours-dir> merge --git <ref> [--as bob] [--dry-run] [--json]
```

`--git` is mutually exclusive with directory-mode `--base` and `--theirs`. Retain the directory form unchanged. An optional source-ID flag, if approved in PR 08, follows that same contract here; it is not a mandatory new flag approved by this plan. Clap reports conflicting mode arguments with exit 2 before any artifact load or mutation.

| Input | Exact proposed meaning |
| --- | --- |
| Ours commit | Resolve local `HEAD` to a commit object once. It is used only for ancestry and provenance. Reject unborn HEAD. Detached HEAD works. |
| Ours content | Capture the current artifact files from the working tree through the lossless snapshot loader under the destination writer lock. Include staged and unstaged content as it exists on disk, plus supported local untracked artifact files. Do not reconstruct the index or silently use HEAD's files. |
| Theirs commit | Resolve `<ref>` once with `git rev-parse --verify --end-of-options <ref>^{commit}`. A tag is peeled to a commit. Accept a locally resolvable revision expression; reject tree/blob-only or missing objects. |
| Theirs content | Use the committed artifact at that exact commit ID. A dirty neighboring checkout is irrelevant and cannot become theirs by accident. |
| Base commit | Run `git merge-base --all <ours-commit> <theirs-commit>` using immutable object IDs. Require exactly one merge-base commit. |
| Base content | Use the committed artifact at that merge-base ID and the destination's same repository-relative root. |
| Merge time | Capture once in the native adapter and pass through PR 08's options. It does not participate in content identity or create a fresh replay record. |
| Display label | Use explicit `--as` when given. If omitted, propose the supplied ref spelling as the label, validated by PR 08's label grammar and uniqueness rule. A label does not prove fork identity. |

Reject unrelated histories instead of manufacturing an empty base. Reject multiple merge bases instead of choosing the first or recursively inventing a virtual ancestor. Reject a shallow repository for Git-mode merge until complete ancestry is available locally; return a clear diagnostic without attempting to unshallow it. If reviewers prefer support for an explicit base commit or shallow-history proof, approve that additional contract separately. A single result from incomplete history is not proof of the true common ancestor.

Reject unmerged index entries within the artifact because the working content is not an unambiguous input to the guarded writer. Unmerged entries elsewhere in the repository do not block an unrelated valid artifact. Require all tracked objects needed for base and theirs to be present locally. In partial clones, prohibit lazy object fetching and report missing blobs or trees. Dirty ours is allowed; index-only changes that differ from disk remain index-only and untouched.

### How does Git identity fit the directory import journal?

Use PR 08's source key and label enrollment, not a commit ID as permanent fork identity. Record repository/artifact enrollment context, pinned base/ours/theirs commit IDs, and exact content fingerprints in the portable merge journal. A commit ID is an immutable revision; a branch or label can move. Replaying the same committed theirs against unchanged ours must reuse the same complete mapping and open-conflict state as directory mode.

For an advancing source, prove that the previously recorded theirs commit is an ancestor of the newly pinned theirs commit with a local ancestry query. Reuse the established source key and imported targets. Use PR 08's last-imported source field values for already imported entries, rather than treating every source revision as an unrelated new fork. Ref movement during the command cannot change the pinned commit; the report identifies the revision actually consumed.

A branch reset, rebase, unrelated commit, or label reuse that fails source ancestry proof is an identity decision, not a reason to overwrite earlier mappings. Reject it with the source-key history and required explicit enrollment decision. PR 08's optional source-ID spelling and uniqueness-only `--as` enrollment remain review gates. Absolute checkout paths, Git directory paths, repository URLs, and branch names cannot by themselves establish portable fork identity. Do not add a network repository-identity lookup.

Changing HEAD during capture invalidates the ancestry context. Recheck the pinned HEAD commit before destination commit and reject if it changed. A later update of the named theirs ref does not alter the pinned source and needs no retry. Distinguish a recorded source revision from the current branch tip in output so agents do not think a newer tip was merged.

### How are temporary inputs materialized safely?

Use a private native temporary directory with `base/` and `theirs/` children managed by an RAII owner. `tempfile` is currently a CLI dev dependency in `crates/ara-cli/Cargo.toml`; move or add it to runtime dependencies for this adapter only after reviewing the needed API. Temporary trees must be outside the destination and excluded from artifact discovery and output. Never write inside `.git` or register worktrees.

Enumerate each pinned commit's tree with Git plumbing and NUL-delimited output. A proposed approach uses `git ls-tree -r -z --full-tree <commit>` and filters exact repository-relative path components in Rust. Enumerating tree metadata avoids pathspec glob ambiguity for artifact roots containing spaces, brackets, or other special characters. Materialize only descendants of the validated artifact prefix. Parse mode, object kind, object ID, and path separately; reject malformed output, absolute paths, `..` components, case/normalization collisions on the host filesystem, and paths that would escape or overwrite a previously created entry. Use create-new file semantics and no-follow checks for the private tree.

Read blobs by immutable object ID through one bounded `git cat-file --batch` process per materialization, using length-framed binary payloads. Do not spawn one child per artifact file, interpolate shell strings, or process a pathname as a revision. Verify object kind, declared size, complete payload length, and child exit status before exposing a snapshot to the merger. Stream large blobs into temporary files; use the lossless source loader's planned text/binary distinction and approved file-size/depth limits. Preserve regular-file content bytes and mode metadata for PR 08's file policy.

Accept ordinary regular Git modes `100644` and `100755`. Reject symlink mode `120000`, gitlink mode `160000`, unsupported modes, and an artifact nested inside a submodule. Do not follow a symlink or initialize a submodule to fill a missing layer. Git LFS pointer files are the committed bytes; do not run an LFS client or substitute absent binary content. Surface a relevant pointer as opaque source content under PR 08's rules. Base and theirs must contain readable `trace/exploration_tree.yaml`; missing roots or malformed content produce explicit errors, not an empty artifact.

Launch Git with `std::process::Command` and argument arrays. For every invocation, including repository discovery, remove inherited variables that could redirect repository, worktree, index, or object resolution. Set `GIT_NO_LAZY_FETCH=1`, `GIT_TERMINAL_PROMPT=0`, and `GIT_NO_REPLACE_OBJECTS=1`; pass explicit configuration disabling filesystem-monitor helpers and network protocols. Do not allow credentials, global aliases, hooks, attributes, or filters to become an execution path. Git commands are fixed plumbing commands, not user-supplied subcommands. Verify the supported Git version honors the no-lazy-fetch setting before shipping; if the guarantee cannot be established, refuse Git mode on that version rather than risk network access.

Bound and sanitize captured stderr without including temporary paths or terminal-control injection in human output. On subprocess failure, missing object, interrupted materialization, or validation failure, drop both temporary trees and terminate/reap child processes before returning. An uncatchable process kill can leave a private OS-temporary directory; it must contain no live lock or repository state and be safe to remove. Do not promise RAII cleanup after SIGKILL.

### Which files and interfaces change?

All new paths below are proposed. PR 08 supplies the merger and existing planned CLI surface, not a current implementation in this checkout.

| File | Change |
| --- | --- |
| `crates/ara-cli/src/merge.rs` (new in PR 08) | Add mutually exclusive directory/Git argument mode and dispatch. Use one report renderer and one native merger invocation. |
| `crates/ara-cli/src/merge/git.rs` (new) | Repository discovery, pinned commit resolution, unique merge-base selection, shallow/unmerged checks, safe materialization, ancestry provenance, and cleanup. |
| `crates/ara-cli/src/main.rs` | Wire Git mode through the existing planned merge command. No separate `git merge` behavior. |
| `crates/ara-cli/Cargo.toml` | Add the reviewed runtime temporary-directory dependency if required. No Rust Git library dependency. |
| `crates/ara-core/src/merge/mod.rs` and `identity.rs` (new in PR 08) | Consume optional pinned Git provenance through the reviewed source/revision contract; keep planning free of subprocess or filesystem calls. |
| `crates/ara-cli/tests/cli.rs` | Add temporary local Git fixtures and real-binary acceptance/failure cases using the current `assert_cmd`/`tempfile` conventions. |
| `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `docs/agent-cli.md` (new if still absent) | Apply future functional-release gates and document Git semantics, local-only guarantees, errors, and source enrollment. |

```rust
// Proposed native CLI adapter interface, not a current API.
struct GitMergeInputs {
    temporary_root: tempfile::TempDir,
    base_dir: PathBuf,
    theirs_dir: PathBuf,
    provenance: GitMergeProvenance,
}

fn prepare_git_inputs(
    destination: &Path,
    reference: &str,
) -> Result<GitMergeInputs, GitMergeInputError>;

// GitMergeProvenance carries repo-relative artifact root and pinned commit IDs.
// Destination snapshot, planning, validation, and commit use PR 08 unchanged.
```

The adapter owns the temporary tree until planning, validation, and any commit finish. Materializing base/theirs can happen before the destination lock because their object IDs are immutable. The writer then acquires its one exclusive lock, recovers pending transactions, reloads ours, and verifies the HEAD/ancestry context before planning. PR 08's final preimage check still applies to dirty worktree content. Avoid locking our artifact while waiting on unnecessary Git subprocess work.

### What are the implementation steps?

1. Approve mutually exclusive modes, unique-base/no-history rules, shallow-history rejection, default label, source ancestry enrollment, and supported Git version/no-network guarantee. Use the PR 08 protocol gates rather than adding an independent identity cache.
2. Implement fixed-argv repository and commit discovery. Pin HEAD and theirs, reject invalid/unborn/bare/outside-repository inputs, detect unmerged artifact paths, and require one complete merge base.
3. Implement safe NUL-delimited tree enumeration and bounded binary blob materialization into private temporary roots. Reject symlinks/gitlinks, unsafe/colliding paths, missing objects, and absent historical artifact roots before invoking merge.
4. Pass temporary snapshots and Git provenance to PR 08. Acquire the shared writer lock, capture current ours, recheck HEAD, and reuse the exact planner, conflict records, source-origin rewrites, dry run, and commit path.
5. Render Git provenance alongside the existing `ara.merge/v1` report. Preserve its renamed/imports/conflicts/needs-review fields and distinguish rejected inputs from a committed merge with unresolved conflicts.
6. Prove behavior with the local Git fixture and failure/timing scenarios below, then apply README's functional release gates. This plan performs no implementation or checks now.

## Alternatives considered

A Rust Git library avoids a runtime Git requirement but adds dependency and behavior surface for a feature whose parent already selects the binary. Temporary worktrees make Git state visible and can run checkout-related machinery. Native extraction of raw object bytes avoids both registration and filters.

`git archive` is compact, but Git attributes can omit files or substitute content, and archive extraction requires another safety policy. Plumbing enumeration and raw blob reads provide exact committed bytes. Reading ours from HEAD or the index would ignore local content; the destination must be the current worktree snapshot.

Selecting the first merge base is simpler, but criss-cross histories can produce more than one valid base. Combining them requires a separate virtual-base contract. An explicit safe refusal keeps PR 08's three-way meaning intact and leaves that additional feature for a reviewed proposal.

## Tradeoffs

Git is an external prerequisite. A missing or unsupported executable is a clear exit-2 error; there is no library fallback. Strict shallow/submodule/symlink rejection excludes some repositories but makes the local-only and safe-materialization guarantees testable.

Materializing two temporary artifact trees adds disk I/O and peak storage proportional to the selected snapshots. Streaming blob payloads and one batch child avoid repeated allocation and process-start costs. Git history resolution may cost more than directory loading, so measure that overhead separately instead of hiding it in the merger's timing.

Current-worktree ours supports dirty research work, but index bytes can intentionally differ from disk after the merge. The command must state that it did not stage files. Agents still use Git to review and commit the resulting artifact; `ara` does not repair unrelated repository conflicts or manage branches.

## Migration

This documentation-only PR needs no version bump. The future functional PR bumps the then-current workspace patch, refreshes `Cargo.lock` with a non-locked `cargo check --workspace` before locked gates, and reviews all expected local package-version and approved dependency changes. Run README's gates under pinned Rust 1.94.1, add a changelog entry, and update `docs/agent-cli.md` with Git mode and failure semantics.

Directory mode and its portable records remain unchanged. Artifacts do not acquire Git-dependent identity grammar; Git provenance is optional metadata on the approved merge revision record. This adapter is native CLI code and does not require a viewer bundle rebuild unless an approved shared core/wire change affects wasm. If that happens, inspect all `Manifest` literals/consumers and use the shared manual embed-rebuild gate rather than relying on the viewer-input-only freshness hash.

## Verification and acceptance

### What must the local Git fixture prove?

Create a disposable local repository in the proposed `cli.rs` tests with a committed artifact at a fixed root, two valid parent nodes, and a shared base commit. Create a Bob branch that adds `N124` under the second parent, references it in a claim, observation, and complete session record, and commits those bytes. Return to our branch, add a different `N124` under the first parent, and commit it. Keep our highest node ID at 124 so the expected imported ID is `N125`. Then make a supported local staged change and a different unstaged change inside ours. Pin commit dates and repository-local user configuration in the fixture; do not use external remotes or the user's global Git settings.

After the feature is implemented, run the built binary against that fixture using these proposed commands:

```sh
ara -C "$REPO/ara" merge --git bob --as bob --dry-run --json
ara -C "$REPO/ara" merge --git bob --as bob --json
ara -C "$REPO/ara" resolve bob:N124
ara check "$REPO/ara" --json
ara -C "$REPO/ara" merge --git bob --as bob --json
```

Dry run returns exit 0 and reports the actual base/ours/theirs commit IDs without any artifact mutation. Commit keeps our `N124` and both dirty changes, imports Bob's branch as `N125` under its actual parent, and maps every committed source pointer. Resolve prints `N125`, and check finds no semantic errors or unfixed format drift. Snapshot Git index bytes, HEAD, refs, and merge-state files before the invocation and prove they are unchanged afterward. Replay changes no artifact bytes and returns exit 0. Inspect every full session field and unknown source extension instead of checking only node counts.

Build the same inputs as directories and assert the Git and directory paths produce the same content decisions, mappings, reference rewrites, and conflict values, allowing only explicit Git provenance metadata to differ. With a conflicting `C05.Statement`, both first merge and replay return exit 1 and preserve the same unresolved decision. Advance Bob with a committed source edit and prove old import IDs remain stable. Move the Bob ref during execution and prove only the pinned commit is consumed; changing HEAD during capture must reject with no destination mutation.

### Which failure fixtures are required?

| Fixture | Required result |
| --- | --- |
| Missing Git, unsupported Git, bare repository, destination outside a working tree, or unborn HEAD | Exit 2 with an actionable prerequisite error and no artifact or Git mutation. |
| Bad ref or missing local commit/tree/blob | Exit 2 identifying unavailable local data; no fetch, credential prompt, or fallback to working-tree theirs. |
| Unrelated histories, multiple merge bases, or shallow history | Exit 1 for an unsupported/ambiguous ancestry decision, with pinned context where available and no mutation. |
| Unmerged index paths inside the artifact | Exit 1 before planning; unmerged files outside the artifact do not alter supported merge behavior. |
| Artifact path absent in base or theirs, including a rename across commits | Exit 1 with the exact repository-relative root; never merge another discovered historical artifact. |
| Nested submodule, symlink blob, special mode, unsafe path, or host-filesystem collision | Exit 1 for unsupported input; no escaped temporary write or followed link. |
| Partial clone missing a required object | Refuse locally with exit 2. A fixture fetch sentinel proves no remote/lazy fetch was attempted. |
| Malformed committed YAML, unsafe source identity, or invalid candidate | Use PR 08's exit-1 rejection and unchanged-artifact rule. |
| Temporary write or Git child failure, disk-full, interruption, or commit I/O failure | Exit 2, reaped children and cleaned temporary roots on catchable paths, and exact destination rollback/recovery under the shared writer contract. |

Create additional known-result local histories for an ancestor theirs, identical HEAD/theirs, detached HEAD, linked worktree, annotated tag, criss-cross merge bases, default-label collision, and a reset/rebased source that fails lineage proof. A no-change source must preserve dirty ours. Verify filenames containing spaces, newlines, non-ASCII characters, and Git pathspec metacharacters are handled as bytes under the declared supported-path contract. Use recorded full expected results; a bare successful invocation is insufficient.

Instrument the Git subprocess adapter in scoped tests so remote access, filters, hooks, and fsmonitor invocation would fail conspicuously. Exercise real Git plumbing in the binary fixture, not only mocked responses. Verify temporary roots disappear after success, conflict, dry run, malformed input, and subprocess failure; document the uncatchable-kill limit. Reuse PR 08's property tests for content semantics instead of implementing different Git-only merge rules.

### What timing and output evidence is needed?

Measure revision resolution, merge-base selection, tree enumeration, blob materialization, core planning, durable commit, and total binary time separately. Use the same 100 through 100,000-node artifacts as PR 08 and include both a small local repository and a repository with unrelated tree entries to expose full-tree enumeration overhead. Record bytes materialized, peak temporary storage, child count, memory, hardware, and repeated-run distributions. The parent has no approved Git-mode timing threshold; choose and pin one with reviewers before a pass claim. Preserve PR 08's full-layer and no-quadratic-work requirements.

JSON uses `ara.merge/v1` with an additive reviewed `git` provenance object containing pinned commits and the repository-relative artifact path. Source-ref spelling is diagnostic, not identity. Human and JSON modes must agree on result classification: 0 for a clean merge or replay, 1 for unresolved merge conflicts or rejected readable/ancestry content, and 2 when the command cannot obtain its inputs or complete an operation. Do not expose random temporary paths as stable output. These are future acceptance requirements, not checks run while drafting this plan.

## Next Steps

1. Review unique merge-base, complete-local-history, committed-theirs/current-worktree-ours, default-label, and no-network Git semantics alongside PR 08's identity decisions.
2. Implement the native adapter only after PR 08 provides a complete merger and approved portable records. Keep one planner, one writer, and one output contract.
3. Run the real Git fixture, directory/Git equivalence, failure, cleanup, and timing scenarios, then document the exact supported Git versions and limits.
