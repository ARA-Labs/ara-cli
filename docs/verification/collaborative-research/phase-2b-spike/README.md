# Phase 2b: plan 01 storage spike, jj-lib as the private snapshot store

Status: evidence only. Backend selection (D-S6, D-S8) and the command shape (D-S4) still need the developer's re-approval before the optional store is implemented.

**Answer:** jj-lib 0.45.1 with its Git backend passes all five spike checks, with one condition. Inside jj-lib, the Git library (gix) reads the user's global Git configuration. Our adapter has to block that by setting two environment variables when the process starts. With that fix in place, every correctness check passes.

The cost of turning the store on is real. A binary with the store adds about 162 crates that the current `ara` build does not use. Stripped, it is 8.3 MB larger than a store-free baseline, and a clean release build takes about 20 seconds longer. The default `ara` build pays none of this, because the store would sit behind a build feature that is off by default.

**Recommendation:** confirm jj-lib for D-S6 (jj-lib or the gix fallback) and its Git backend for D-S8 (which jj-lib storage backend). Make the Git-configuration fix and an exact version pin conditions of approval. Whether the build and size cost above is acceptable is the developer's call. A gix-only store would cost less to build (see "What a gix-only store would cost"), but we would have to write the history log and the index ourselves.

All raw numbers are in `results.json`. The spike source and the scripts that produced every number are on the unmerged branch [`spike/jj-snapshot-store@2da6849`](https://github.com/ARA-Labs/ara-cli/tree/2da6849d0ed123c18abc22a61f992f497f392385/spike/jj-store); backend code does not land on the integration branch.

## What was built

The spike is a throwaway Rust project, preserved on [`spike/jj-snapshot-store@2da6849`](https://github.com/ARA-Labs/ara-cli/tree/2da6849d0ed123c18abc22a61f992f497f392385/spike/jj-store) and never merged. It changed nothing in the `ara-cli` crates.

- **Input.** A package written by the real `ara -C <artifact> snapshot --output <dir> --json` (release build of phase 2a, version 0.1.24, #101). A package is an `ara/` directory plus the canonical `snapshot.json`.
- **Adapter.** `src/jj_store.rs` on the spike branch implements the plan's `SnapshotStore` trait: `record`, `list`, `load`, and `export`, keyed by capture ID. No jj-lib type crosses the trait. jj-lib's async calls run on `pollster`, a small blocking executor that jj-lib already depends on.
- **Location.** The store lives at `<artifact>/.ara/vcs/repo` and is created by `ReadonlyRepo::init`. It does not use `Workspace` and has no working copy. The store is first built in a temporary sibling directory and then renamed into place.
- **Configuration.** jj-lib sees only its built-in defaults plus one layer that ara supplies. That layer sets author and committer to `ara <ara@localhost>`, fixes commit and operation timestamps at 1970-01-01, sets the random seed to 0 and hostname and username to `ara`, and turns signing off (`signing.backend = "none"`, `behavior = "drop"`). The adapter never loads user, repository, or environment configuration for jj.
- **One commit per capture.** The commit's tree mirrors the package: `ara/<path>` for every file, plus `snapshot.json` holding the original manifest bytes. Git stores only an executable bit, so full modes such as 0640 come from the stored manifest. The change ID is derived from the capture ID, which makes commits reproducible.
- **Recording order.** `record` writes the files, the tree, and the commit. Next, it clears caches and reads everything back from the store, comparing bytes, modes, manifest, and capture ID with the input. Only after that check does it add the index entry and publish, in a single jj operation. This follows plan steps 5, 6, and 8.
- **Index: one bookmark per capture.** Each capture gets a bookmark named `capture-<hex>` in the jj operation's view. We picked this over a separate index file for three reasons:
  - The bookmark is published in the same atomic step as the commit, so a crash cannot leave the index and the history disagreeing.
  - The operation log covers it, so it can be recovered and undone like any other jj change.
  - There is no extra file to keep consistent.

  The cost is that every operation rewrites the whole view. With 2 captures the view was 232 bytes, so this stays small for hundreds or thousands of captures. `list` walks parent links back from the single head commit and checks each step against the bookmarks.
- **Baseline.** `baseline/` on the spike branch is the same package read, verify, and export code with no jj-lib. It is used for the cost comparison.
- **Environment.** rustc 1.94.1 (e408947bf 2026-03-25), aarch64-apple-darwin, macOS (Darwin 27.0.0).
- **jj-lib.** Version 0.45.1, pinned as `=0.45.1`, minimum Rust version 1.89. We set `default-features = false, features = ["git"]`. jj-lib's features are `default = [git]`, `git = [dep:gix]`, `testing`, and `watchman`, so `["git"]` is the smallest set that includes the Git backend, and it equals the default set.
- **gix.** Version 0.87.1, a direct optional dependency because `gix::hash::Kind` is needed for initialization. Its enabled features come from jj-lib: attributes, blob-diff, command, excludes, index, max-control, max-performance-safe, pack-cache-lru-*, parallel, sha1, sha256.

## Check 1: Is the store isolated from user configuration? PASS, after one fix

We ran the same record, list, and export in three environments:

- two runs with an empty home directory;
- one run with a hostile setup:
  - a `~/.config/jj/config.toml` and a `JJ_CONFIG` file that set another user, signing with GPG, a different timestamp, a random seed, and a different hostname;
  - the jj-cli environment variables `JJ_USER`, `JJ_EMAIL`, `JJ_TIMESTAMP`, and `JJ_RANDOMNESS_SEED`;
  - a hostile `~/.gitconfig` and XDG Git config that set another user, `commit.gpgsign`, `init.defaultBranch`, compression 0, `hooksPath`, `fsmonitor`, and `objectFormat=sha256`;
  - the Git variables `GIT_AUTHOR_*`, `GIT_COMMITTER_*`, `GIT_CONFIG_GLOBAL`, and `GIT_CONFIG_COUNT/KEY/VALUE`.

Fake `git`, `gpg`, `gpgsm`, `ssh`, and `ssh-keygen` programs on `PATH` logged any attempt to run them.

| Result | Evidence |
|---|---|
| No `.jj/` anywhere | 0 `.jj` directories in any run, inside or outside the artifact |
| Store placement | Only `.ara/vcs/repo/{store,op_store,op_heads,index,submodule_store}` |
| The two clean runs are byte-identical | Same hash over all 406 store files (`594ba649…`) |
| jj configuration has no effect | The jj config file, `JJ_CONFIG`, and `JJ_*` variables changed nothing. jj-lib reads no configuration on its own; only jj-cli does. |
| Git author and committer variables have no effect | Store identical |
| `GIT_DIR`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES` | Store identical; no objects were redirected |
| No program was started | The fake-binary log stayed empty in every run |
| **Without the fix, Git configuration leaks in** | `~/.gitconfig`, XDG Git config, `GIT_CONFIG_GLOBAL`, and `GIT_CONFIG_COUNT` changed the stored bytes. `HEAD` became `refs/heads/evilbranch`, and the compression level changed the size of object files. Commit IDs, `list` output, and exports stayed identical. **A malformed `~/.gitconfig` made the store unusable**: init failed with "Failed to initialize git repository", because jj-lib opens gix in strict-configuration mode. |
| **With the fix, every variant matches the clean run** | All 13 variants were identical to the clean store, including the malformed-config case. The full hostile run gave the same store hash as the clean runs. |

The leak happens because jj-lib opens gix with default options, which read the system, global, and XDG Git configuration and `GIT_CONFIG_*`. jj-lib offers no way to pass gix stricter permissions. The spike's fix runs first thing in `main`, before any thread exists. It removes `GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG`, and `GIT_CONFIG_SYSTEM`, and sets `GIT_CONFIG_NOSYSTEM=1` and `GIT_CONFIG_GLOBAL=/dev/null`. In Rust 2024 those calls are `unsafe`. A cleaner long-term fix would be an upstream jj-lib option to open gix in isolated mode.

## Check 2: Are captures stored and exported exactly? PASS (20 of 20)

The fixture was `the-ara-of-ara` plus a few extra files:

- `src/run.sh` with mode 0755;
- a nested `src/deep/er/x.py`;
- an empty `evidence/empty.txt`;
- a non-ASCII path, `evidence/résumé-数据.md`;
- `PAPER.md` with mode 0640.

The artifact sat inside an outer Git repository. We took three captures:

- c1: the fixture as described;
- c2: `PAPER.md` changed from 0640 to 0644;
- c3: `run.sh` changed from 0755 to 0644.

| Check | Result |
|---|---|
| Mode-only changes keep the fingerprint and change the capture ID | All three shared one fingerprint and had three different capture IDs |
| Record new captures | All three returned `recorded: true` |
| Re-record c1 after c3 | `recorded: false`. Object count stayed at 387 and operation count at 3, so no new commit or operation was written. |
| `list` order | c1, c2, c3, which is parent order |
| Export c1, c2, and c3 | Exported `snapshot.json` matched the package byte for byte, and file bytes and full modes matched. We also re-ran the real `ara snapshot` on each exported `ara/`, and it produced the same capture ID and fingerprint. `PAPER.md` exported with mode 0640. |
| Unknown capture ID | Exit 2 with `unknown_snapshot`; no output directory was created |
| Missing store | `list` returned an empty list and `export` returned `unknown_snapshot`. Neither created `.ara/vcs`. |
| Export to an existing directory | Rejected |
| `.ara/vcs` stays out of captures | A new `ara snapshot` after recording gave c3's capture ID again |
| Outer Git repository | `.git` was byte-identical before and after |
| Agent-visible output | The JSON contained none of the terms jj, commit, change_id, operation, bookmark, or git |
| Corrupt store (missing operation heads, or a bogus backend type) | `snapshot_store_unavailable` |

The same checks also all passed with jj-lib's non-Git `SimpleBackend` (built with `--no-default-features`).

## Check 3: How much does the store grow when little changes?

| Fixture | Store after the first capture | Change before the second capture | Store growth from that change |
|---|---|---|---|
| `the-ara-of-ara`: 332 files, 6.79 MB | 4.12 MB, 0.61 times the fixture (the Git backend compresses text) | Added 32 bytes to `logic/claims.md`, a 10 KB file | **+33.8 KB** of file size (+77.8 KB counting 4 KB disk blocks), in 12 new files |
| Fixture plus 64 random 1 MiB files: 73.9 MB | 74.9 MB, 1.01 times the fixture | Changed 16 bytes in one 1 MiB file | **+1.14 MB** (+1.19 MB on disk), in 13 new files |

Where the growth goes:

- **The manifest is stored again in full each time.** It is 58.6 to 68.2 KB raw and 19 to 31 KB compressed, and it changes whenever any file digest changes. That is most of the small-edit cost.
- **Changed files are stored whole.** The Git backend writes each object as its own compressed file and never stores differences between versions. A 16-byte edit to a 1 MiB file therefore cost a full new blob of 1,106,018 bytes. Random data grew about 5.5% when the backend compressed it.
- **Everything else is small.** jj's operation log and index added about 0.8 KB per record, and trees plus the commit added about 2 KB.

For comparison, a directory-only package costs the full 6.8 MB or 73.9 MB for every capture.

Two captures that differed only by a mode change took 4.15 MB with the Git backend and 6.95 MB with `SimpleBackend`, which does not compress.

## Check 4: What does the store cost?

### Time per command

These are median wall-clock times over 5 runs with warm caches. One warm-up run was discarded.

| | `the-ara-of-ara` | Large fixture (+64 MiB) |
|---|---|---|
| `ara snapshot --output` (capture only, no store) | 1158 ms | 1505 ms |
| Record a new capture into a fresh store, including init (in-process time) | 201 ms (174) | 1106 ms (955) |
| Capture plus record | **1359 ms** | **2612 ms** |
| Record after one file changed | 97 ms | 587 ms |
| Re-record an identical capture (verifies the stored copy) | 60 ms | 452 ms |
| `list` | 7 ms | 8 ms |
| Export | 1305 ms | 1735 ms |

The spike also re-reads the package from disk, which takes 21 ms and 143 ms. A real implementation would hand the capture over in memory and skip that. Export and capture times are dominated by syncing each file to disk, which macOS makes expensive. The spike's export and the existing `ara snapshot` behave the same way here.

### Build and size cost

| Build | Crates (normal dependencies) | Clean release build | Stripped binary |
|---|---|---|---|
| Baseline (package code, no store) | 16 | 1.6 s | 0.46 MB |
| **Spike with jj-lib and the Git backend** | **203** | **22.2 s** | **8.71 MB** |
| Spike with jj-lib and `SimpleBackend` | 121 | 17.0 s | 5.30 MB |
| gix only, as a probe (see below) | 104 | 6.2 s | 2.46 MB |

- The current `ara-cli` default build uses 146 crates. The Git-backend spike adds **162 crates that `ara-cli` does not already use** (48 of them are `gix-*` crates). The `SimpleBackend` variant adds 86.
- Each build was timed once on the same machine, from an empty target directory and with dependencies already downloaded.
- These numbers measure the opt-in cost only. The real `ara-cli` default build would not compile any of this.
- **Wasm:** `cargo check --target wasm32-unknown-unknown -p ara-core` on the phase-2a code passes. `ara-core` has 41 dependency crates for Wasm, none of them jj or gix, and its `Cargo.toml` is unchanged.

**Not measured here:** the plan's full zero-cost checks for a store-enabled `ara` binary (ordinary commands with an absent, corrupt, or large store, plus tracing to prove the store is never touched). Those need the store wired into `ara-cli` behind its feature, which belongs to the next stage. This spike measured the store in isolation.

## Check 5: Can the store reach the network? PASS: no reachable network code

- **No networking libraries in the dependency graph.** reqwest, hyper, h2, http, curl, ssh2, libssh2, openssl, rustls, native-tls, tokio, mio, socket2, and watchman_client are all absent.
- **Network-related crates present, with nothing usable compiled in.** `gix-transport` 0.59.2 is built with no features, so there is no blocking, async, or HTTP client. `gix-protocol` 0.65.1 is built with defaults only, so there is no handshake or fetch. `gix-url` only parses URLs.
- **No network code in the binary.** The symbol table has 0 code symbols from `jj_lib::git_subprocess` (the module that runs `git fetch` and `git push`) and 0 from `gix_transport` or `gix_protocol`. The only `jj_lib::git` function linked in is `GitSettings::from_settings`. The fetch and push names that do appear are tracing metadata, not code. There are no socket symbols such as `TcpStream`, `getaddrinfo`, or `connect`.
- **Code that can start programs is linked but unreachable.** jj-lib's GPG and SSH signing backends are compiled in and could start `gpg` or `ssh-keygen`. Signing is off in our configuration, and no program was started in any check-1 run.
- **Conclusion.** No network code path can be reached from the adapter.

## Recommendation

- **D-S6: confirm jj-lib.** It met every correctness gate: exact bytes and modes, distinct capture IDs for mode-only changes, deduplication, parent order, unknown IDs, no `.jj/`, an untouched outer Git repository, deterministic store bytes, and no network. In exchange for its cost we get three things ready-made: a crash-safe operation log, an index that updates in one atomic step with the history, and storage code someone else maintains. Keep gix as the fallback if the developer decides the build and size cost is too high.
- **D-S8: use the Git backend.** `SimpleBackend` is about 3.4 MB smaller and pulls in 82 fewer crates. But it stores files uncompressed (1.7 times larger on this artifact), its cleanup step does nothing, and as far as we know jj treats it as a test backend rather than one for real repositories (general knowledge, not re-checked). The Git backend's storage format can also be inspected with standard Git tools.
- **Conditions of approval:**
  1. Isolate Git configuration (the environment fix above, or an upstream option).
  2. Pin the exact jj-lib version.
  3. Map backend error text to ara errors before anything reaches an agent (see risk 3).

## Risks

1. **Git configuration leak.** Covered in check 1. The fix must run before any thread starts, and every future jj-lib or gix upgrade must re-run the hostile-config test.
2. **No storage of differences.** Every changed file is stored whole. Packing versions together would need `git gc`, which jj-lib runs by starting the external `git` program, and that conflicts with the no-external-program rule. Large evidence that changes often should become external pinned objects, as the plan already says.
3. **Backend wording in error messages.** Error codes were clean, but messages carried backend wording such as "Failed to read operation heads" and "Unsupported commit backend type". A real implementation must replace these with ara wording.
4. **API churn.** jj-lib releases monthly and its API changes. The adapter is about 250 lines in one module, so the work per upgrade is limited, but the exact pin and a re-test are required.
5. **Concurrent writers.** If two writers record at once, jj ends up with two operation heads. jj-lib's `load_at_head` merges them, which means a read such as `list` can write a merge operation, and a second commit head would make `list` fail. ara's lock serializes recorders, so the product would rely on the lock, as the plan intends.
6. **No real timestamps.** Commit and operation timestamps are fixed at 1970 so the store is deterministic. As a result, the operation log carries no wall-clock times.
7. **Linked but unused code.** The signing backends and jj's revision-query and merge code are compiled in. That is part of the 8.3 MB.

## What a gix-only store would cost

`gix-probe/` on the spike branch is a minimal gix program with only the `sha1` feature. It creates a bare repository and writes a blob, a tree, a commit, and a ref. It needs 104 crates, a 6.2 s clean build, and a 2.46 MB stripped binary, compared with 203 crates, 22.2 s, and 8.71 MB for jj-lib. It also pulls in `gix-transport` and `gix-protocol` with no client features. Treat these numbers as a lower bound: a full gix adapter would still need its own history log, crash recovery, and atomic index, which jj-lib already provides.

## How to reproduce

From `spike/jj-store/` on the spike branch:

1. Run `cargo build --release`.
2. Run the scripts in `harness/`:
   - `check1.sh`, and `check1b.sh` (with `SPIKE_NO_GIT_ISOLATION=1` to see the leak without the fix);
   - `check2.py` (set `J=<binary>` to test another backend);
   - `check3_4.py`;
   - `check4_build.sh`.
