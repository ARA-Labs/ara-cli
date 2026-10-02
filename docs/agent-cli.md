# Agent CLI design and command reference

The agent interface reads and edits native ARA files offline. It uses one source
snapshot per command, preserves unrelated source bytes, and records imported
identities and mutable changes so later reads and merges can resolve them.
Scientific experiments, installed-agent execution, and human protocol approval
are separate acceptance gates. The delivery evidence records their status.

## Selecting an artifact and reading it

`-C <directory>` takes precedence over `ARA_DIR`. Otherwise discovery walks the
current directory's ancestors, checking each ancestor and its `ara/` child for
`trace/exploration_tree.yaml`. Explicit roots must be real directories. Reads
never create a lock, journal, or artifact structure. Explicit source-document
reads can inspect a partially initialized root.

```sh
ara -C ./ara status --json
ara -C ./ara ls --type question --under N01 --status open --json
ara -C ./ara show N01 C01 --full --json
ara -C ./ara show --document logic/problem.md --source --full --json
ara -C ./ara show --document logic/solution/architecture.md \
  --heading Architecture --heading 'A/B' --full --json
ara -C ./ara path N12 --json
ara -C ./ara refs C01 --json
ara -C ./ara open --json
ara -C ./ara find 'failure boundary' --limit 10 --full --json
```

| Command | Behavior |
|---|---|
| `status` | Layer counts, diagnostics and advisory next IDs |
| `ls` | Source-order entries; intersecting type, subtree, date, status and provenance filters |
| `show` | Entry projection, relations via `--with`, full body or bounded native document |
| `path` | Root-to-node nesting, with cross-edges kept distinct |
| `refs` | Typed references with source spans, separately reported possible prose mentions |
| `open` | Unfinished questions/experiments, unpromoted observations and active continuity |
| `find` | Stateless keyword ranking over loaded knowledge |
| `resolve` | Resolve a qualified imported identity through the portable identity records |

Results use command-specific `ara.<command>/v1` JSON formats. Success goes to
stdout; a JSON error goes to stderr with `code`, `message`, and applicable ID,
line or details. Exit 0 means completion, exit 1 means a semantic rejection or
unresolved merge, and exit 2 means setup, argument, I/O or lock failure.
`--fields` projects supported fields while retaining transaction/result identity.
Default excerpts are bounded to 160 Unicode characters; `--full` retains source
content. Incomplete source representation must not be reported as success.

Native nodes, claims, heuristics, observations, sessions, experiment plans,
concepts and typed documents keep their own namespaces. A concept named `C05`
is a concept name, not an instruction to relocate claim C05. Repeated `--heading`
values form an exact vector: `['A/B']` differs from `['A','B']`. Ambiguous legacy
flattened selectors reject when requested. Unknown fields and complete source
bodies remain available in full reads. `--source` returns original UTF-8 source
rather than regenerated normalized YAML/Markdown. Source digests refer to the
selected exact bytes; replacement preconditions must use the corresponding
source selection. Pure parser and native source spans are both bounded; see
[deep-tree-parsing.md](deep-tree-parsing.md).

## Authoring commands and source inputs

Convenience commands cover `add node`, `add edge`, `edit`, `claim add/set`,
`heuristic add/set`, `stage`, `promote`, `session start/log`, and `link --same-as`.
`apply` exposes the complete typed operation contract in
[`write/mod.rs`](../crates/ara-core/src/write/mod.rs).

```sh
ara -C ./ara add node --type question --parent N01 --title 'Boundary behavior' \
  --set description=@description.txt --provenance ai-suggested --json
ara -C ./ara add edge N02 --depends-on N01 --json
ara -C ./ara apply /tmp/request.jsonl --dry-run --json
ara -C ./ara apply /tmp/request.jsonl --json
```

CLI text values accept `@file` or `@-` for complete external UTF-8 input; stdin
can be consumed once. `@@text` escapes a literal leading `@`. These expansions
belong to CLI inputs. JSONL values are literal JSON and do not expand `@file`.
For `--set`, only an explicit JSON array starting with `[` is decoded as a list;
ordinary scalar text, including numbers and booleans, stays exact text.

The five creation kinds and required source payloads are question (`description`),
decision (`choice`, `alternatives`), experiment (`result`), dead_end (`hypothesis`,
`failure_mode`, `lesson`) and pivot (`from`, `to`, `trigger`). Explicit replay IDs
satisfy the same validation as allocated IDs. IDs do not authorize unsupported
kinds or missing source content. Legacy `observation` nodes and legacy pivot
payload names cannot be silently rewritten into this creation contract.

| Operations | Purpose |
|---|---|
| `node.add`, `edge.add`, `node.link_same_as` | Append a typed node or dependency/equivalence pointer |
| `claim.add`, `heuristic.add`, `entry.edit` | Create or edit allowed native logic/metadata fields |
| `observation.stage`, `observation.promote`, `observation.mark_stale` | Stage, atomically crystallize, or audit stale observations |
| `session.start`, `session.log` | Create sessions and append complete turns/history |
| `document.create`, `document.replace`, `paper.edit` | Create bounded documents or replace exact mutable source selections |
| `record.append`, `entry.taste_append`, `entry.annotate` | Append typed records or permitted annotations |
| `logic.revise`, `entry.rename`, `entry.remove` | Audit mutable changes and structural identity transitions |
| `artifact.init` | Initialize the `compiler` or `research-manager` seed profile |

New fields preserve native scalar/list forms. Named concepts must already
exist or resolve through an authenticated alias. Claims are retained in audited
withdrawal/merge states or renamed canonically; physical claim deletion is not
allowed. Compiler heuristics retain singular scalar `Source` and complete Bounds
prose without inventing PM fields. Extra knowledge documents are bounded by
native registration/allowlists; arbitrary filesystem writing is not an operation.

Trace and staging content is immutable except for declared pointer/metadata
transitions. Terminal nodes cannot acquire children. `same_as` points from a
later node to a provably earlier existing node using recorded time or actual
creation order; numeric IDs and preorder do not establish chronology.

## Batches, audit ownership and recovery

Each nonempty JSONL line is one operation. A creation ID such as `$question`
binds its allocated native identity for later structured references. BatchBinding handling
keeps each binding in its declared namespace: unknown, duplicate, forward and wrong-kind
uses reject. Ordinary prose and historical before/after text stay literal.
Dry-run identities and turns are tentative. The whole batch plans in one working
snapshot and validates the declared source delta before any source commit.
A later failure leaves source bytes unchanged and reports the failing line.

Session logging retains complete events, actions, touched claims, revisions,
context, threads and suggestions. Mutable logic revisions carry exact
before/after source history, signal, provenance and the owning next session turn.
Stale transitions need at least three distinct actual logged session days after
last observation/bound-node use, a caller-supplied reason and an atomic owning
`session.log` turn. They do not infer a scientific rationale from elapsed time.
Promotion creates its complete destination and forward pointers together;
immutable original observation content and prior turns remain exact.

Commit-mode writes take a persistent artifact lock, recover any prior prepared
transaction, snapshot native inputs, plan/validate, recheck inputs, stage exact
candidate files, then commit through a durable private journal. The private
`.ara/transactions` payload namespace retains preimages, candidate identities,
digests and approved created directories. A prepared-to-committed rename marks
the commit point. Failure before that point rolls back original bytes and
original directory existence; recovery validates the private namespace before
using it. Reads and dry runs only probe that namespace and reject a prepared
transaction without creating a lock or reading mixed source state. Corrupt
private state is an I/O-class error, not an empty pending marker shortcut.
`ara check --fix` shares this lock/recovery/transaction path and retains its
per-rule no-new-errors and normalized-equivalence guards.

The lock coordinates cooperating CLI writers in one checkout. Raw file readers
and direct external writers do not acquire that lock. Portable identity/conflict
records live in native knowledge files; private transaction payloads are not
portable merge evidence.

## Directory and Git merge

```sh
ara -C ./ours merge --base ./base --theirs ./theirs --as peer --dry-run --json
ara -C ./ours merge --base ./base --theirs ./theirs --as peer --json
ara -C ./ours merge --git peer-branch --as peer --json
ara -C ./ours resolve 'peer:N02' --json
```

Directory roots must be nonoverlapping. The planner reads complete native layers,
including opaque files. It preserves ours, allocates incoming collisions within
the correct namespace, rewrites actual structured incoming references, and
records unchanged as well as relocated identities for byte-identical replay.
Exact heading vectors survive current and archived identity lookup. Qualified
references to descendants use the complete persisted token mapping. Closed
session histories compare in destination identity/occurrence space during replay;
actual frozen history edits still reject before mutation.

Mutable conflicts preserve exact base/ours/theirs candidates, keep ours and
commit safe imports with portable unresolved records; exit 1 exposes unfinished
work. Protected violations block mutation and carry complete conflict evidence.
Unknown/opaque source changes are evaluated explicitly rather than dropped from
inventory or rewritten as regenerated normalized content. Ambiguous prose tokens
and quoted historical values remain opaque and visible for review.

`merge resolve <conflict> --take ours|theirs|base` requires an owning session,
next turn, signal and provenance. Protected conflicts use the separate audited
`merge repair --conflict-file ... --decision reject_incoming|restore_base
--expected-current ... --session ... --turn ... --signal ... --provenance ...
--reason ...` path with exact captured candidates/current fingerprint. Generic
entry edits and mutable resolution cannot override immutable history.

Git mode resolves a local ref and merge base with the installed Git executable.
It captures exact trees and blobs into temporary snapshots without changing
branches, staging the worktree, invoking a shell command supplied by the user,
or requiring a remote. Blob captures are filter-neutral; user clean/smudge
filters must not regenerate source bytes. Reports retain the Git object/source
identity and phase timings. Git convenience setup is measured separately from
native merge planning/validation/commit phases.

## Keyword search and duplicate advice

Search tokenizes maximal Unicode alphanumeric runs, lowercases Unicode text,
and uses BM25 (`k1=1.2`, `b=0.75`). It indexes complete loaded knowledge once;
source/evidence bodies are excluded. There is no persistent index, model,
network or semantic judgment. Type filtering and result limits are explicit.
Duplicate advice uses weighted set Jaccard over title/substantive body with
threshold 0.8, at most 64 retrieval candidates and ten reported candidates.
Advice is nonblocking on additions and merge, can be disabled with
`--no-duplicate-check`, and never deduplicates or merges scientific claims.

The frozen source-heldout labels and criteria under
`crates/ara-core/tests/fixtures/agent-cli/search` support bounded functional verification.
Four singleton queries and one crafted duplicate per split cannot establish
population recall or natural/paraphrase duplicate quality. Automated criteria
review is not human protocol approval. Fixed gates must not be retuned after
measurement.

## Compatibility and delivery gates

The JSON additions preserve missing optional fields and published experiment
`fields.status`; common non-experiment status is optional. Existing JSON callers
can ignore additive fields. Public Rust types/constructors and node-kind bodies
have changed, so source compatibility is distinct from JSON compatibility.
Workspace version remains 0.1.22 for this integration. The minor/major release
decision remains pending; no tag or release is implied by engineering checks.

Core behavior affects wasm even when the embedded-viewer source hash does not.
Delivery requires a manual `scripts/embed-viewer.sh` rebuild with pinned Rust
and compatible wasm-bindgen, then `--check`; the hash alone cannot prove the
new core behavior. Browser UI and installed-agent runtime smoke are owned by
the final integration owner. The final CLI PR must merge into `main` with a
merge commit, preserving feature-branch bookkeeping. The protocol repository
has its own commit and PR. Neither integration engineering nor the native
107-operation proof claims an E0–E6 scientific experiment. Historical paper
pinning remains unresolved: 465 available native questions versus 450 reported
published questions.

Final observed checks and remaining gates: [delivery verification](verification/agent-cli-2026-10-02/README.md).
