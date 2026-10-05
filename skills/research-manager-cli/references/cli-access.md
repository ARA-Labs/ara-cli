# CLI-only native artifact access

This is an access contract, not a new research method. All variants retain their pinned source procedures, evidence standards, roles and stopping rules. The live pin is `e52a925e9d03b4ada3008653e72f99b04116fca2`, not a verified reproduction of the historical paper condition. Binary coverage and upstream protocol decisions are **proposed/pending review**, not accepted or measured. The variant lock records actual digests; final CLI revision and observed proof must be supplied after integration.

## Boundary and failures

Use `ara -C <artifact> <command>` (or the same explicit artifact via `ARA_DIR`). Every read/write of `PAPER.md`, `logic/`, `trace/`, `staging/` and registered additional knowledge documents goes through `ara`, including initialization, session indexes, reasoning and taste. Do not use Read/Grep/Glob/Edit/Write, Python or shell filesystem operations on those knowledge bodies. Reading the skill's supplied instruction/reference/template pages remains direct. Source inputs and actual `rubric/`, `src/`, `data/`, `evidence/` bodies are not `ara` documents: read, search and write them directly with file tools where the baseline permits them (`ara show --document` rejects `rubric/`, `src/` and `evidence/` paths with `invalid_document` and lists those roots in `error.details.file_access`); the reader still cannot execute source code or leave its single ARA. Never use a source/evidence exemption to open a knowledge path.

Exit 1 means rejected/unknown/invalid operation; exit 2 means setup/IO failure. Report the exact failure and apply the source skill's original stopping/repair rule. Never fall back to direct knowledge-file access or retry automatically with a changed scientific payload. A failed knowledge batch is not successful compilation. Direct source/evidence writes are outside the knowledge transaction and cannot turn a failed batch into success.

Source contradictions remain recorded in the shared baseline contract, not repaired only in the CLI condition. The PM's early crystallization notes versus later current-snapshot rule, and compiler reference run-index contradictions, require the same reviewed disposition in both future comparison conditions. Pending F3/F4/F6/F7 decisions prevent declaring the variants integrated even if local code runs.

## Shell calls

Run each `ara` command as its own shell tool call: one invocation with quoted arguments and nothing else on the line. Do not pipe (`|`), redirect (`>`, `2>&1`), chain (`&&`, `;`), `cd` first, or glob (`*`, `?`, `[...]`). Quote every argument that contains spaces, `#`, `$` or other shell characters, such as `'logic/claims.md#C04'` and `'failure boundary'`. Some harnesses reject an unquoted `#`, and in a POSIX shell a `#` after a space starts a comment that silently drops the rest of the line. Some harnesses enforce this rule and reject a composed command before it runs. Other shells may allow composition, but the single-call form works in all of them, so use it everywhere.

Bound output with `ara`'s own options instead of `head`, `tail`, `grep` or `jq`: `ls <path>` lists one document instead of the whole artifact; `find --limit N` caps results and `find --context N` adds surrounding lines (long form only, because `-C` selects the artifact); `show --lines A:B` selects a line window and `show --max-bytes N` sets the response budget. A paged `show` block ends with `lines: S-E of N; truncated; next: --lines X:`. Rerun the same selection with that `--lines` value (and the same `--max-bytes`, if you set one). Do not read harness logs or temporary files to recover cut-off output.

## Reading: orient, search, read, cite

```sh
ara -C <artifact> status
ara -C <artifact> ls
ara -C <artifact> ls logic/claims.md
ara -C <artifact> find '<keyword query>' --context 2
ara -C <artifact> find '<keyword query>' --type claim --limit 5
ara -C <artifact> show 'logic/claims.md#C04'
ara -C <artifact> show logic/solution/method.md --lines 40:120
ara -C <artifact> show trace/exploration_tree.yaml --lines 300: --max-bytes 8192
ara -C <artifact> show C01 trace:N01
ara -C <artifact> path N01
ara -C <artifact> refs C01
ara -C <artifact> open
```

1. **Orient.** `ls` prints one line per knowledge document with its entry counts by kind (or heading count) and line count, then the direct-file roots. `ls <path>` lists that document's entries, or its heading addresses when it has none. `status` gives layer counts, next IDs and diagnostic codes.
2. **Search.** `find` is case-insensitive keyword search. Each result prints `<address> [<kind>] <source>` and the matching source lines (`N:`); `--context 2` adds neighbouring lines (`N-`). Narrow with `--type` and `--limit`.
3. **Read.** `show` an address that `ls`, `find` or an error printed. A native section prints `== <address>`, its `heading:` path, a `source_digest=… selector: …` line and its exact source bytes; a node, observation or session prints a projection and names the exact source read. Read a whole document or section, or the lines around a `find` hit with `--lines A:B`. Brief `show` stops at 16 KiB and pages at whole lines; follow `next:` until you have read every line you rely on.
4. **Cite** the address the output printed: `logic/claims.md#C04`, `trace:N01`, a canonical heading address such as `logic/problem.md#h/Problem/Observations`, or a document path. Never cite a generated display ID or a heading spelling you guessed.

A miss returns `unknown_id`; a selector that matches several sections or entries returns `ambiguous_heading`. Both print `candidates:` that `show` accepts. Choose the candidate whose address or heading path is the one you need, or run `ls <path>` to see the document's headings, instead of guessing another spelling. An ambiguous selector is an error, not permission to take the first candidate. `invalid_document` on a `rubric/`, `evidence/` or `src/` path means a file tool reads it.

`ls` enumerates structured entries; `status` identifies the selected artifact and knowledge revision. Do not infer that typed-entry enumeration exhausts arbitrary documents: inspect the native document inventory supplied by `ls`, the root Layer Index and registered `knowledge_paths`. Read all documents required by the source procedure. `find` is only a retrieval aid, never semantic ranking or sufficient evidence for a cited assertion. A hit line or excerpt is not a full source.

For grounding read the source document or its unique section with `show`: brief text prints the exact source bytes and the `source_digest` of the whole selection. A node, observation or session address prints a projection with no digest; read the exact source it names (such as `ara show --document trace/exploration_tree.yaml --source`) when you need the bytes or the digest. Use the original native anchor in the answer (`trace:N01`, `logic/claims.md#C01`, concept names and path/heading addresses); never cite a generated display ID. For YAML entries obtain the complete document as well as typed fields when source spelling, unknown fields, comments, raw observation context or full history matters. For Markdown obtain the whole document/unique section rather than treating a typed projection as exhaustive.

`path` and `refs` recover graph/native relations; distinguish structured references from possible prose mentions. `open` is local unresolved context, not proof of freshness or remote community progress. Verify every body you cite. Directly read only allowed actual evidence/source bodies, preserve all original source quotes and screenshots, and stay within reader isolation.

## Structured output and exact source before writes

Brief text is the reading format. Add `--json` where a step consumes structured fields: write and `apply` results with their `bindings`, `merge` reports and conflicts, `status --json` revision and file counts used in a report, `ls --json` typed counts, or rows another program parses. `--full` lifts the 160-character JSON excerpt bound and, on a brief `--source` read, the 16 KiB page budget; brief `show` already prints exact source either way.

A full source show means the whole selection: a brief `show` without `--lines` that prints no `truncated` line, pages that together cover lines 1 to N of the selection (`lines: S-E of N`), or `--source --full`, which is unbounded. A window you chose with `--lines` is not a full read even when it prints no `truncated` line. Before a guarded write (`document.replace`, `logic.revise` with `Body`, `entry.rename`, `entry.remove`, or `paper.edit` with `expected`), read the exact selection you will replace in full:

```sh
ara -C <artifact> show --document logic/solution/method.md --heading 'Method' --heading 'Step 3' --source --full
ara -C <artifact> show --document logic/solution/method.md --heading 'Method' --heading 'Step 3' --source --full --json
```

Pass its `source_digest` (JSON `digest`) as `expected` and build the replacement from the complete bytes you read. Every page prints the digest of the whole selection, so a page's digest never authorizes replacing content you have not seen; if the digest changes between pages, the source changed and you restart. A `selector: none` line means `document.replace` cannot target that selection; the reason follows `none`.

## JSONL transactions

Put each operation on its own JSON line in a request file **outside the artifact knowledge layer**, then:

```sh
ara -C <artifact> apply <request.jsonl> --dry-run --json
ara -C <artifact> apply <request.jsonl> --json
```

`apply - --json` consumes JSONL on stdin, which needs a pipe or redirect; under the shell-call rule pass the request file path instead. Dry run is optional access inspection, not a maturity judgment. Use one committed batch for related present-state/history/session/index changes. Operation results and `bindings` identify assigned IDs; do not scan files to allocate them. Omit `id` for allocation; only use documented provisional batch names when binding several operations in one transaction. The agent still decides every research signal, provenance, fact and relationship.

The following is the supported writer union; `?` means an optional key, not literal JSON syntax. Keys and value kinds are native wire names. Text payloads retain full prose/equations/unknown fields; JSON arrays/mappings must not be flattened into comma-separated strings.

| Operation | Complete request keys | Access use |
|---|---|---|
| `artifact.init` | `profile` (`research-manager` or `compiler`), `paper?` text, `documents?` path→text mapping, `missing_only?` boolean | Default requires new/empty root. Explicit `missing_only: true` creates missing seeds only, preserving existing defaults; supplied existing PAPER/document bytes must match exactly or the batch rejects. No success/Seal claim from seeding. |
| `document.create` | `document`, `content` | Complete new knowledge document. |
| `document.replace` | `document`, `heading?` string array, `expected` SHA-256 source digest, `content` | Low-level guarded replacement preserving untouched spans; not by itself the complete audited skill workflow. |
| `paper.edit` | `frontmatter?` mapping, `heading?` string array, `expected?`, `content?`, `audit?` | Root manifest/Layer Index and path registration. Existing-root integrated repairs supply `audit: {session,turn,signal,provenance,note?}` plus owning `session.log` in the same batch, preserving complete PAPER before/after. Primitive unaudited mode alone is not the full skill history workflow. |
| `node.add` | `id?`, `type`, `parent`, `title`, `fields` mapping, `depends_on?` array | Author only `question`, `decision`, `experiment`, `dead_end`, `pivot`. Required fields: question `description`; decision `choice` + `alternatives` array; experiment `result`; dead_end `hypothesis` + `failure_mode` + `lesson`; pivot `from` + `to` + `trigger`. Preserve source provenance/support/source refs. `parent: "root"` opens a root branch; a dead_end is immutable and always a leaf, never a parent. Legacy insight remains readable, not newly authored. |
| `edge.add` | `node`, `depends_on` | Append DAG cross-edge; never replace earlier edges. |
| `node.link_same_as` | `node`, `same_as` | Append from the later node to an existing, provably earlier target; retain both records. Proof uses timestamps or actual recorded `node.add` order, never numeric IDs or DFS order. Self, missing, unproved-order and cyclic links reject. |
| `claim.add` | `id?`, `title`, `fields` mapping | Complete source-dialect claim. |
| `heuristic.add` | `id?`, `title`, `fields` mapping | Complete source-dialect heuristic. Compiler fields retain singular scalar `Source`, full `Bounds` and source prose; do not manufacture PM `Status`/`Provenance` or fake values to satisfy the adapter. PM fields retain their source schema. |
| `entry.edit` | `target`, `set` mapping | Named permitted field edit only; not a generic historical YAML editor. |
| `logic.revise` | `target`, `set`, `session`, `turn` integer, `signal`, `provenance`, `note?`, `expected?` | Current-state field or `Body` changes with exact complete before/after and Last revised where applicable; requires owning `session.log` in the same batch. Body mode requires native Document target and exact selected-source digest `expected`. |
| `entry.rename` | `target`, `name`, `expected`, `references?`, `session?`, `turn?`, `signal?`, `provenance?` | Native rename plus redirects and guarded modeled mutable reference repair; archive full endpoints. |
| `entry.remove` | `target`, `expected`, `references?`, `session?`, `turn?`, `signal?`, `provenance?`, `redirect?` | Guarded eligible non-claim current-state removal only; never history deletion. Claims cannot be physically removed: PM withdrawal/merge uses audited `logic.revise` with retained Status/Merged into and repaired references; identity changes use canonical `entry.rename`. |
| `entry.annotate` | `target`, `kind`, `references`, `comment` | Add conflict annotation without rewriting original record, alongside unresolved decision. |
| `entry.taste_append` | `target`, `record` | Add confirmed user taste to claim/heuristic only; preserve prior taste/status. |
| `observation.stage` | `id?`, `content`, `potential_type`, `context?`, `provenance`, `timestamp`, `bound_to?` array | Full raw/context/provenance staging record. |
| `observation.promote` | `observation`, `to`, `id?`, `title`, `fields?`, `signal`, `target?`, `content?` | Atomic typed target plus original observation pointer tuple; signal chosen under unchanged closure rule. |
| `observation.mark_stale` | `observation`, `session_days` array, `reason` text, `audit: {session,turn,signal,provenance,note?}` | At least three distinct actual logged session-days after the last reference to the observation or its bound nodes. Requires owning new `session.log` turn in the same batch; atomically writes stale boolean and native reasoning notes with caller reason plus factual idle evidence. Retain history; never discard/promote. |
| `session.start` | `id?`, `date`, `started`, `summary` | Calendar-day grouping and CLI-assigned session identity. |
| `session.log` | `session`, `timestamp`, `summary?`, `events?`, `ai_actions?`, `claims_touched?`, `logic_revisions?`, `key_context?`, `open_threads?`, `ai_suggestions_pending?` | Complete turn append. `events` writes native `events_logged`; all other native arrays keep full shape. Rolling metadata/lists and index are audited together. |
| `record.append` | `id?`, `document`, `record` | Reasoning `{turn,notes}` to `trace/pm_reasoning_log.yaml`, or complete trace taste to `trace/taste_log.yaml`. |

`target` is either `{"id":"C01"}` or `{"document":"logic/concepts.md","heading":["native heading"],"entry":"native entry"}`; omit unused heading/entry. A `references` row is `{target,field,before,after}` with verbatim values. Historical refs resolve through retained redirects; do not rewrite old trace/session/staging prose.

Creation `id: "$name"` binds a provisional name (letter/underscore followed by letters/digits/underscores); later structured selectors/reference fields use exact `$name`. Wrong-kind/forward references reject; source prose/title/quotes containing dollar signs remain literal. A session binding may be used as `"$session#1"` in a reasoning turn. `expected` for document replacement/rename/remove is the returned selected-source SHA-256 digest, not an approximate text excerpt. PAPER body edits use the digest of the uniquely shown body/heading, not the whole frontmatter-inclusive file.

Concept references introduced by a write must name an exact existing native concept heading or authenticated alias; never mint a display ID or guess a name. For scalar-or-list `evidence` and `Code ref`, preserve the exact source value. CLI field arguments become arrays only when explicitly supplied as JSON beginning with `[`; otherwise they are exact scalar strings (or complete `@file` text), not comma-split lists.

The stale API's additional `reason` is the caller's explicit explanation of why the source idle rule applies, not a model-invented closure or scientific rationale. Supply `audit` for the new session turn owned by a `session.log` operation in the same batch (start the session if needed). The adapter verifies actual logged day history and recent observation/bound-node references, then binds caller reason and factual evidence to native `Reasoning.notes`/`trace/pm_reasoning_log.yaml` for that turn. Do not separately synthesize a second stale rationale with `record.append`; neither the flag nor its reasoning may commit alone.

Additional source-warranted knowledge outside the implicit boundary needs a safe relative `.md` path registered in `PAPER.md` frontmatter `knowledge_paths` by `paper.edit` or initial paper content, and its inventory clause. Reserved `rubric/`, `src/`, `data/`, `evidence/`, `trace/`, `staging/`, `.git/`, `.ara/`, absolute paths, traversal and symlink escapes cannot be registered as knowledge. A supplied rubric is a plain `rubric/requirements.md` file the compiler writes directly. This is access registration only: preserve the compiler's choice of natural layer, arbitrary appendix/taxonomy/method content and granularity; do not impose a template or invent content.

## PM complete turn and structural edits

Use `show --document` full reads for current logic, staged observations, today's session and index, and reasoning continuity. The cadence stays end-of-turn only. `artifact.init` with profile `research-manager` supplies the exact source seed set; `src/`/evidence bodies retain direct source permission after bootstrap. `session.start` allocates the calendar-day record; `session.log` appends all source arrays and updates rolling metadata/index while retaining complete previous/new views in reasoning history. Do not truncate `ai_actions`, `claims_touched`, `logic_revisions`, `key_context`, `open_threads`, `ai_suggestions_pending`, raw observation context, or notes. Preserve omitted versus empty values.

Append journey events with `node.add`, interpretation with `observation.stage`, rejected/accepted signal rationale with `record.append`. Promote through `observation.promote` only after the original closure signal; targets are `claim`, `heuristic`, `concept`, `constraint`, `architecture`, or `dead_end` on empirical refutation. Concept/constraint/architecture use native document heading selector plus complete `content`; `unknown` stays staged. The original observation survives including original provenance. CLI capability never grants an AI new authority to affirm, settle a contradiction or judge maturity.

Stage 4 uses `logic.revise` for complete verbatim before/after plus signal/session/turn/provenance, and one batch with owning `session.log` for coupled changes. Native concept/constraint/architecture/arbitrary prose revisions use `set: {"Body":"<complete exact selected body>"}` on the native document/heading target; initial source-grounded document creation has no invented prior revision. Split keeps the primary ID and creates the spin-off; merge retains lower ID and withdraws higher with Merged into plus reference redirects; generalization adds a new claim depending on narrower retained claims. Claim withdrawal/merge never uses physical `entry.remove`; canonical `entry.rename` uses guarded endpoints and redirects, retains all prior values and identities, and never deletes historical layers. Record the original source's full structural before/after in session history, chosen/rejected signals and near misses in reasoning. `logic/experiments.md` remains compiler-owned; PM cannot edit it or attach taste.

Taste still confirms the target first. `entry.taste_append` appends one full user record to a claim/heuristic; trace taste uses `record.append` and CLI T-ID allocation, never editing the trace node and never targeting a question. Attitude/object remain independent axes.

## Compiler generation, coverage and validation

Read every input page/appendix/code/evidence body directly under the unchanged evidence and epistemic rules. Initialize with `artifact.init` profile `compiler`, supply complete source-grounded bodies with document operations, and append source-supported DAG nodes/edges with node operations. The source/evidence write boundary is not an all-files transaction. Never rewrite earlier immutable trace because a validator failed.

Coverage repair re-reads actual source directly and current knowledge via full `show`; existing-knowledge field/body repair uses `logic.revise` with complete `Body`/fields and owning `session.log` in one batch, supplying the actual signal/provenance rather than fabricated history. Guarded primitive `document.replace` alone is not the complete audited skill workflow. Initial `document.create`/`artifact.init` emission has no invented prior revision. Run `ara check <artifact> --json` **and the entire original Seal checklist** (all semantic, visual, grounding, citation, code and evidence passes); the CLI structural checker is not a substitute for source-skill validation or reasoning. Report every real failure and exact artifact state. Use `status --json` for artifact location/full regular-file count/exact bytes and `ls --json` for native typed counts, and report only actually observed validation. Mutable conflict resolution and protected immutable repair are distinct audited adapters:

For existing root frontmatter/Layer Index repairs, use `paper.edit` with the strict audit context `{session,turn,signal,provenance,note?}` and owning `session.log` in the same batch; it archives complete exact PAPER before/after. `logic.revise` Body applies to mutable non-root knowledge; it is not a PAPER bypass. Initial paper emission through `artifact.init` does not invent a previous root revision.

For an ordinary **mutable** conflict only:

```sh
ara -C <artifact> merge resolve <conflict-id> --take ours --session <session-id> --turn <turn> --signal user-directive --provenance user-revised --json
```

Use `--take ours|theirs|base` only when the conflict's explicit permitted mutable resolutions include that choice. Retained candidate availability alone is not permission. Ordinary `merge resolve` cannot repair protected history.

For a captured **protected immutable** conflict only:

```sh
ara -C <artifact> merge repair --conflict-file <captured-conflict.json> --decision reject_incoming --expected-current <captured-current-fingerprint> --session <session-id> --turn <next-turn> --signal <actual-signal> --provenance <actual-provenance> --reason <explicit-reason> --json
```

The only protected decisions are `reject_incoming` and `restore_base`; arbitrary immutable replacement is forbidden. Preserve the complete captured conflict, all base/ours/theirs candidates, presence markers and fingerprints; supply the exact captured current fingerprint rather than a guessed digest. Name the active session's next turn explicitly and the actual signal/provenance/reason. The dedicated adapter verifies current source, retains full captured candidates, and appends complete before/after, decision and audit history without erasing prior records. A stale fingerprint or unsupported corrective span rejects. No generic `entry.edit`, ordinary resolution, or direct fallback may bypass protected repair. If the necessary source-preserving operation is unavailable, report the capability gap and stop integration rather than narrow the source procedure.

## Review and proof

`access-diff.json` maps the exact changed baseline clauses and all required operation rows to adapters. `variant-lock.json` freezes baseline and variant bytes, operation inventory revision and review state separately for 13a and 13b. Required rows remain proposed until a pinned actual-binary proof is attached. Representative tasks are `smoke-tasks/variant-scenarios.json`; the actual runner is the ara-cli repository's `scripts/agent-cli-acceptance.py --binary <prebuilt ara> --output <external proof.json>`. These are deterministic integration/fidelity checks, not scored research experiments or performance-equivalence evidence. The shared frontier/intention extension is separate and never loaded by these interface-only copies.

This revised access contract requires a fresh independent SkillAccessReview; an approval of an earlier scoped diff does not approve these bytes. Review remains pending, F1–F7 human approvals remain pending as recorded upstream, and the final CLI revision is not yet pinned. No agent-runtime execution or scored experiment result is claimed. Frontier/intention roles and dynamic registry belong only to the separate collective condition.
