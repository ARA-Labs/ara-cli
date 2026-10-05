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
ara -C ./ara show 'logic/solution/architecture.md#h/Architecture/A%2FB' --json
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

Native documents are `PAPER.md`, `logic/**/*.md`, `trace/**/*.yaml`,
`staging/observations.yaml` and paths registered in `PAPER.md` `knowledge_paths`.
`rubric/`, `evidence/` and `src/` are file-access roots: agents read and search
them with their own file tools. `show --document` on one of them, on a bare
root such as `rubric`, or on any path outside the native set, fails with
`invalid_document` and a `details` object:
`hint` (plain text), `native_documents`, `registered_documents` and
`file_access` (`["rubric/", "evidence/", "src/"]`). The check runs on the path
alone, so a registry entry cannot serve a file-access path. `find` and `ls` do
not index these roots, and rubric headings such as `R84` are not entry IDs.

Native nodes, claims, heuristics, observations, sessions, experiment plans,
concepts and typed documents keep their own namespaces. A concept named `C05`
is a concept name, not an instruction to relocate claim C05. Unknown fields
and complete source bodies remain available in full reads. `--source` returns
original UTF-8 source rather than regenerated normalized YAML/Markdown. Source
digests refer to the selected exact bytes; replacement preconditions must use
the corresponding source selection. Pure parser and native source spans are
both bounded; see [deep-tree-parsing.md](deep-tree-parsing.md).

### Headings and canonical addresses

Repeated `--heading` values form an exact vector: `['A/B']` names one heading
containing a slash and differs from `['A','B']`. A heading read tries these
tiers in order and stops at the first tier that matches anything; if that
tier matches more than one section, the read rejects instead of trying a
weaker tier:

1. the exact full heading vector;
2. an exact suffix of the vector, then the native-ID shorthand (`C04` selects
   `C04: Title`);
3. a recorded rename or merge redirect, when the artifact has identity records
   (`trace/logic_mutations.yaml`, `trace/merge_log.yaml`, `trace/aliases.yaml`)
   and the read is not `--source`;
4. tolerant suffix matches on segments trimmed of surrounding whitespace and
   lowercased with Unicode lowercase (locale-independent; no accent stripping
   or Unicode normalization): equality, then a prefix of the source heading,
   then a source heading ending in a literal `...` whose nonempty stem starts
   a longer request. An exact heading containing `...` matches at tier 1 or 2.

A canonical address names one section of the current source. A document is
its path. A heading is `path#h/<segment>/<segment>` with the full original
heading vector. Path components and segments escape every byte outside
`A-Z a-z 0-9 - . _ ~` as uppercase `%XX`, so `path#h/A%2FB` and `path#h/A/B`
differ and a literal `;` is `%3B`. When the full vector repeats, the address
ends in `;occurrence=N`, the one-based source-order position among those
sections; an address without it rejects for a repeated vector. An occurrence
is a read locator for one source snapshot, not a durable identity after
edits, and no write selector accepts it.

`ara show <address>` accepts canonical addresses alongside entry IDs, bare
native IDs (`C04`), `trace:N09` and `logic/claims.md#C04`. Canonical addresses
resolve exactly, with no redirect or tolerant tier. The input is decoded once
and then passes the normal document boundary: malformed escapes, invalid
UTF-8, absolute paths and traversal reject (`invalid_address` or
`invalid_document`). A positional whole-document path is decoded the same
way; its raw spelling still reads when no decoded path matches. A legacy
`path#Method/Step 3` is tried only when no entry has that selector, and
resolves only when exactly one section has a heading suffix spelled that way
with `/` joins; each segment may use its native-ID shorthand
(`logic/problem.md#O1`). If recorded identities call the selector ambiguous,
it stays ambiguous even when one current section matches.

Source-document rows keep `heading` (the requested or redirected vector) and
add `heading_path` (the section's full source vector) and `address`. The
`digest` still covers exactly the selected bytes.

A miss returns `unknown_id`; more than one match, whether sections or
entries, returns `ambiguous_heading`. Both carry `details.candidates`, at most
40 canonical addresses from loaded knowledge, and `details.capped`, which is
true when more existed. When a heading read finds several sections, the
candidates are those sections in source order. Otherwise they are the
document's headings (for a document selector) or the artifact's entries,
ranked by edit distance to the request and then by source order. When
identity records cannot be indexed (for example a corrupt alias file), a read
that reaches them fails with `identity_lookup_failed` instead of an ordinary
miss.
Selection errors from `show`, `path`, `refs` and `ls --under` never carry
internal `merge.*` codes; `ara resolve` and `merge` keep theirs.

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
`knowledge_paths` rejects `rubric/`, `evidence/` and `src/` entries, and no
operation creates, edits, renames or removes a `rubric/` document or `R` entry.
A compiler writes `rubric/requirements.md` as a plain file.

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

Files under `rubric/`, `evidence/` and `src/` merge as external read-only bytes.
Identical or unchanged incoming bytes need no decision. An incoming addition,
change or deletion that differs from the destination becomes an
`external_read_only` conflict: ours stays in place and `ours` is the only
allowed resolution, so acknowledging it never copies incoming bytes. These files
get one file-level identity (`layer: "external"`) and no entry identities.

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
`NodeArtifact` retains arbitrary typed extra values and optional source fields;
Rust literals must supply its `extra` map. Its richer value domain does not
implement `Eq`. Legacy dotted bold-field labels still resolve without changing
their original Markdown spelling or bytes.

Rubric migration: `rubric/requirements.md` is no longer a native document. The
public `EntryKind::Requirement` variant, native `R` IDs and rubric write and
merge-entry handling are removed. Existing rubric records (session
`logic_revisions`, `trace/logic_mutations.yaml` renames and removals, and
`trace/aliases.yaml` imports of rubric entries, local or incoming) stay
byte-exact and readable as history. They no longer resolve or redirect, so
`show R04` fails like any unknown ID, and no replacement identity is invented.
A retained private journal that names a `rubric/` target still recovers: a
committed one is only verified and retired, and a prepared one rolls back to
its authenticated preimage bytes. New transactions cannot target `rubric/`.
Before this change, an artifact holding such a rename record failed
`show <id>` and `merge` with `merge.redirect_data`. A `PAPER.md` that registers
a `rubric/` path now has an invalid registry, as one that registers `evidence/`
already did.

Workspace version is 0.1.23 for this integration. The minor/major release
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

## Agent skills

The skills that teach agents these commands live in [`skills/`](../skills); see
[agent-cli-skills.md](agent-cli-skills.md) for their layout, provenance and
change rules.
