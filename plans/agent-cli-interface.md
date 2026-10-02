# Plan: agents read and write an ARA through `ara`, not through files

Status: **draft for review**. Covers every phase. Each phase gets its own
detailed plan (or a section update here) before its code is written.

## Summary

Today an agent works on an ARA the way a person edits a folder of notes: it
reads YAML and Markdown with Read/Grep, finds the highest existing ID by
reading the file, and writes with Write/Edit. This plan makes the `ara` binary
the agent's interface instead:

1. **Read commands** (`ara ls`, `show`, `path`, `refs`, `open`, `status`) answer
   structural questions in one call, with JSON output.
2. **Write commands** (`ara add`, `ara edit`, `ara stage`, `ara promote`,
   `ara apply`) assign IDs, check the result, and edit the files in place.
3. **`ara merge`** combines two copies of one ARA, renumbering colliding IDs and
   rewriting the references to them, and writes a short report of what still
   needs a decision.
4. **`ara find`** adds keyword search, and the same index warns about likely
   duplicate nodes when adding or merging.
5. Copy ARA's existing skills into CLI-backed variants in the protocol repo,
   preserving their research procedures and changing their artifact access.
   Keep the original skills as the evaluation baseline.

Phase 1 changes no file format and does not depend on any open decision below,
so it can start as soon as this plan is approved. Phases 2 and 3 need a few
format decisions from the protocol repo first (Phase 0).

The idea, its motivation, and the experiments that will test it are written up
in the "CLI-Mediated ARA" research note (Obsidian vault, `Ideas/`). This plan is
the engineering side of that note.

## Why change: what goes wrong today

Checked on 2026-10-01 against `ARA-Labs/Agent-Native-Research-Artifact` @
`e52a925` (`examples/the-ara-of-ara`) and this repo @ `598361c`.

- **Every writer picks IDs on its own.** research-manager's ID rule is "Always
  read the target file to find the highest existing ID before assigning a new
  one" (`skills/research-manager/references/event-taxonomy.md`). Two agents
  working on two branches both create `N124`. `git merge` reports no conflict
  because the nodes sit under different parents. Only `ara validate` notices
  (`nodes[N124]: duplicate node id`), and nothing tells the agent how to fix it.
- **Appends collide as text.** Two branches that each add a root node get a git
  conflict inside a 2,423-line `trace/exploration_tree.yaml`. In that file,
  indentation decides which node is the parent, so resolving the conflict by
  hand can silently move a subtree.
- **Reading costs a lot of tokens.** the-ara-of-ara has 116 trace nodes and
  about 300 KB of text under `trace/` (roughly 75k tokens). To
  answer "which dead ends sit under N12?" an agent today greps nested YAML, and
  a grep hit does not carry the node's type or its ancestors. The same question
  is a lookup once the tree is parsed, and `ara validate` parses the whole
  artifact in under 10 ms.
- **The protocol assumes a single writer.** research-manager says so
  explicitly, and staging promotion edits entries in place
  (`promoted: false → true`). Several agents working in parallel therefore have
  no safe way to share one ARA.

## Goals

- An agent can answer common structural questions about an ARA with one
  command and a few hundred tokens of output.
- An agent never has to choose an ID, compute indentation, or re-read a file to
  write to it.
- Two copies of an ARA that grew apart can be merged by one command in
  milliseconds, with no tokens spent on renumbering. The agent only sees the
  items that need judgment.
- Every command works without network access and without an LLM. That keeps
  `ara`'s existing promise.

## Non-goals

- Replacing git. `ara merge` merges ARA content; git still versions the files.
- A database or server. The files in `trace/`, `staging/` and `logic/` stay the
  source of truth. Any index is a cache that can be deleted.
- Changing the viewer. New manifest fields are additive, and the viewer may
  ignore them.
- Writing to `src/` or `evidence/` bodies. Agents keep editing code and evidence
  files directly; the CLI covers the knowledge layers (`trace/`, `staging/`,
  `logic/`).

## Rules that apply to every phase

**References.** Every command accepts and prints the IDs the protocol already
uses: `N12` (node), `C05` (claim), `H03` (heuristic), `E02` (experiment plan),
`O07` (staged observation), `T01` (taste comment), and session IDs such as
`2026-04-15_001`. After a merge, a reference can also carry a source label:
`bob:N124` (see Phase 3).

**Finding the ARA.** Today every command takes the directory as an argument.
Agents call these commands many times, so the new commands also accept
`-C <dir>`, then `ARA_DIR`, and otherwise search upward from the current
directory for the nearest folder that contains `trace/exploration_tree.yaml`
(checking `./ara/` at each level). Existing commands keep their positional
argument.

**Output.** Human-readable text by default. `--json` prints a stable,
documented shape with a top-level `"format": "ara.<command>/v1"` key, so the
shape can change later without breaking callers. `--fields a,b,c` limits which
fields are printed. Long prose fields are cut to one line unless `--full` is
given. That keeps a typical answer small.

**Exit codes.** These match `ara check`: `0` success, `1` the command ran but
found a problem (unknown ID, rejected write, merge conflicts left open), `2`
the command could not run (no ARA found, unreadable file). Errors go to stderr.
With `--json`, errors are also JSON so an agent can parse them.

**Where code lives.**

- Query logic is pure and goes in a new `ara-core::query` module that compiles
  for wasm, so the viewer can reuse it later.
- Parsing the extra layers, writing, merging and search read or write files, so
  they live behind the `native` feature, as `fix.rs` and `evidence.rs` already
  do. Search lives in `ara-cli` because its dependency should not reach the
  wasm build.
- `ara-cli` stays a thin argument parser over library calls, so the protocol's
  tools or tests can call the same code directly.

**How files are written.** Writes are text edits at precise positions, the same
technique `ara check --fix` uses (`crates/ara-core/src/fix.rs`). The CLI never
re-serializes a whole file from the parsed model, so comments, key order and
author style survive, and git diffs show only the new lines. Every write
follows the same guard `--fix` uses:

1. compute the new text in memory;
2. parse it;
3. reject the write unless it introduces no new errors **and** the parsed
   change is exactly the intended one (for example, one new node with these
   fields under this parent, and nothing else changed);
4. write to a temporary file and rename it into place.

**Speed budget.** Each read command must finish in under 100 ms on
the-ara-of-ara, including process start, and in under 1 s on a synthetic ARA
with 10,000 nodes. Tests enforce both numbers.

**Versioning.** Each phase ships as one or more PRs. Each PR bumps the patch
version and adds a CHANGELOG entry, per `CLAUDE.md`. When a phase is done, its
part of this plan moves into a design doc, `docs/agent-cli.md`.

## Phase 0: format decisions in the protocol repo

Phase 1 does not wait for these. Phases 2 and 3 do. I will open one proposal in
`ARA-Labs/Agent-Native-Research-Artifact` covering all of them, so the format
changes once instead of several times.

| # | Decision | My recommendation |
|---|---|---|
| F1 | Where to record renumbering after a merge | A new append-only file, `trace/aliases.yaml`. Each entry records `from` (for example `bob:N124`), `to` (`N131`), the merge date, and the source label. |
| F2 | Whether forks need a special ID form, such as `N124~bob` | No. Forks keep assigning normal IDs, and `ara merge` renumbers when IDs collide. The alias log answers references to the old number. That needs no grammar change. |
| F3 | How promotion of a staged observation is recorded | Keep today's in-place edit (`promoted: true`, `promoted_to`). The merge rule is that `true` wins over `false`. A separate promotion event would also work, but it changes more of the protocol for little gain. |
| F4 | Which trace and staging fields may change after they are written | Write down the pointer fields the protocol already allows to change (`promoted`, `promoted_to`, `crystallized_via`, session `events_logged`). The CLI refuses edits to any other field in `trace/` or `staging/`. |
| F5 | How to mark two nodes as the same finding | A `same_as: [N131]` field on the later node. Neither node is deleted. |
| F6 | Claim fields the CLI must read | `Provenance`, `Falsification criteria` and `Tags` are already in the published `claims.md` but are not modeled here. This is issue #61. Node artifact pointers (#62) and node→concept links (#63) go into the same proposal. |
| F7 | The rule that agents write only through the CLI | Add it to the copied skills' write protocols. Phase 5 preserves the original skills as the file-based baseline. |

## Phase 1: read commands (no format change)

**What the model is missing.** `parse_dir` reads the tree, `logic/claims.md`,
`concepts.md`, `related_work.md`, `problem.md`, `logic/solution/*.md`
(as plain recipe bodies), `PAPER.md` and `evidence/`. To answer the questions
agents ask, it must also read:

- `staging/observations.yaml` (`O##` entries, with `promoted` and `promoted_to`);
- `trace/sessions/*.yaml` and `session_index.yaml` (which nodes each session
  logged);
- heuristics in `logic/solution/heuristics.md` as typed `H##` entries rather
  than one Markdown body;
- experiment plans in `logic/experiments.md` (`E##`);
- the claim fields from F6 that are already published (`Provenance`,
  `Falsification criteria`, `Tags`). This overlaps with #61's parser half.

Each addition is an optional manifest field that is omitted when empty, so
existing manifests and the viewer are unaffected (see
`docs/manifest-schema.md`, "Logical model extensibility"). Files that are
missing produce empty lists, not errors. New warnings get new `ARA2xx` codes.

**Commands.**

| Command | Answers | Example |
|---|---|---|
| `ara status` | Counts per layer, the next free ID for each prefix, the latest session, and the number of errors and warnings. Meant as the first call in an agent session. | `ara status --json` |
| `ara ls` | Lists entries, filtered by type, subtree, date, status or provenance. | `ara ls --type dead_end --under N12 --since 2026-04-01` |
| `ara show` | One or more entries, with chosen relations attached. | `ara show N62 --with parents,children,claims,sessions` |
| `ara path` | The ancestor chain from the root to a node, one line per step. | `ara path N85` |
| `ara refs` | Everything that cites an ID: structured fields first, then mentions in prose. | `ara refs C05` |
| `ara open` | Work that is not finished: question nodes with no children, observations not yet promoted, claims still marked `hypothesis`, `[pending]` bindings, and observations not referenced for 3 or more sessions (research-manager's "stale" rule). | `ara open --json` |

`ara ls --type` accepts node types (`question`, `experiment`, `dead_end`,
`decision`, `insight`, `pivot`) and the other entry kinds (`claim`,
`heuristic`, `observation`, `session`, `exhibit`, `concept`).

**Finding prose mentions.** `ara refs` scans text fields for whole-word
matches of the ID. Short IDs produce false matches. For example, "E2" in prose
may mean a section of the paper rather than the plan `E02`. Structured
references are therefore listed separately from prose matches and marked as
certain, and prose matches are marked as possible. Phase 3's reference
rewriting uses the same scanner, so its accuracy is measured once, on the real
corpus (see Testing).

**Tests.**

- Unit tests per command on small inline fixtures.
- Snapshot tests of `--json` output on a copy of the-ara-of-ara checked into
  `tests/fixtures/`. A snapshot change shows up in review.
- A no-panic run over the `ara-paperbench` corpus (32 artifacts), reusing the
  approach in `docs/real-corpus-no-panic.md`.
- Timing tests for the speed budget, using a generator that builds synthetic
  trees with 100, 1,000 and 10,000 nodes.

**Done when** every command above works on the-ara-of-ara and on the corpus,
meets the speed budget, and has its documentation in `docs/agent-cli.md`.

## Phase 2: write commands

Starts after F1–F4 are agreed. F5 only affects `ara link --same-as`, which can
ship last.

**Commands.**

| Command | Effect |
|---|---|
| `ara add node --type T --parent N12 --title "..." [--set key=value ...] [--depends-on N3] [--provenance P]` | Appends a node under its parent, with the next free `N` ID, and prints that ID. `--parent root` adds a root node. |
| `ara add edge N40 --depends-on N12` | Adds an `also_depends_on` edge. Refuses an edge that would create a cycle. |
| `ara edit <ID> --set key=value` | Changes fields. Allowed on `logic/` entries, and on the pointer fields from F4 in `trace/` and `staging/`. Anything else is refused with an explanation that cites the protocol rule. |
| `ara claim add` / `ara claim set C05 --set Statement="..."` | Creates or edits a claim. `ara heuristic add/set` does the same for heuristics. |
| `ara stage --content "..." --potential-type claim` | Adds an `O` observation with the next free ID. |
| `ara promote O12 --to claim [--set ...]` | Creates the claim or heuristic and marks the observation promoted, in one write. |
| `ara session start` / `ara session log --node N131` | Creates today's session record with the next sequence number, or appends to it. |
| `ara link N131 --same-as N128` | Records F5. |
| `ara apply ops.jsonl [--dry-run]` | Applies a list of the operations above. Either all of them succeed or none is written. Inside the batch, `"id": "$a"` names a new entry so later operations can refer to it (`"parent": "$a"`) before its real ID exists. |

**Long text.** Prose fields can be long and full of quotes, which is awkward in
a shell argument. Any `--set key=value` also accepts `key=@file` and `key=@-`
(read from stdin).

**Several processes, one checkout.** Each write takes an exclusive lock on
`.ara/lock` (a gitignored directory) for the few milliseconds it runs, so two
agents in the same working copy cannot both claim `N124`. Agents in separate
working copies are Phase 3's problem.

**Enforcing CLI-only writes.** This phase adds no enforcement. Once the skills
use the CLI (Phase 5), we look at whether direct edits still happen. Then we
choose between an `ara check` rule (which would build on #40's per-rule
configuration) and removing Write/Edit on `ara/` from the skills. Open
question Q4.

**Tests.**

- For every command: the written text matches a golden file, a second parse
  shows exactly the intended change, and comments and key order elsewhere are
  untouched.
- Rejection tests: unknown parent, cycle, an edit to a field that may not
  change, an ID that already exists.
- `ara apply` with a failing operation in the middle leaves every file
  byte-identical.
- A concurrency test: 8 processes each add 50 nodes to one checkout, and the
  result has 400 new nodes with unique IDs and no errors.
- A round trip: replay the-ara-of-ara's node creations in session order
  through `ara apply`, and compare the resulting tree with the real one, node
  by node.

## Phase 3: `ara merge`

Starts after Phase 2, because the merge reuses its write code.

**Command.** `ara merge --base <dir> --theirs <dir> [--as bob] [--dry-run] [--json]`
merges into the ARA found by the usual lookup. `--base` is the copy both sides
started from. `--as` sets the source label used in the alias log; it defaults
to the name of the theirs directory. A later slice adds `--git <ref>`, which
builds the base and theirs copies from git history so the agent does not have
to.

**What the merge does, per layer.**

1. **Trace nodes and staged observations.** These are append-only, so the merge
   takes the union.
   - Entries that only theirs has get the next free IDs on our side.
   - Their parents and `also_depends_on` targets are mapped through the same
     renumbering.
   - Changes theirs made to entries from the base are allowed only in pointer
     fields (F4), with `promoted: true` winning. Any other change is reported
     as a conflict.
2. **Sessions.** Session files from theirs that clash with ours on
   `date_seq` get the next free sequence number for that date.
   `session_index.yaml` is merged as a union.
3. **References.** After renumbering, the merge rewrites every reference that
   came from the theirs side:
   - structured fields (`also_depends_on`, `evidence`, `Proof`, `promoted_to`,
     session `events_logged`) are always rewritten;
   - prose mentions are rewritten and also listed in the report for an agent
     to confirm.

   Text that came from our side is never touched. In the-ara-of-ara, about 50
   files cite node or claim IDs and about 30 prose lines do, so this step has
   to be done by the tool.
4. **Logic layer** (`claims.md`, heuristics, concepts, related work). Each
   entry is compared field by field against the base:
   - a field that changed on one side only takes that side's value;
   - a field that changed on both sides keeps our value and is reported as a
     conflict, with both values;
   - new entries from theirs are renumbered like nodes.
5. **Alias log.** Every renumbering is appended to `trace/aliases.yaml` (F1).
   `ara resolve bob:N124` prints `N131`. Merging the same theirs a second time
   finds its entries in the alias log and changes nothing.

**The report.** This is what the agent reads instead of the two ARAs:

```json
{
  "format": "ara.merge/v1",
  "renamed": {"bob:N124": "N131"},
  "rewritten": {"structured": 14, "prose": 3},
  "needs_review": [{"node": "N131", "field": "reasoning", "match": "C4"}],
  "duplicate_candidates": [{"a": "N128", "b": "N131", "score": 0.91}],
  "logic_conflicts": [{"claim": "C05", "field": "Statement", "ours": "...", "theirs": "..."}]
}
```

`duplicate_candidates` stays empty until Phase 4. The exit code is `1` while
any conflict is left open. The agent resolves conflicts with the Phase 2
commands (`ara claim set`, `ara edit`, `ara link --same-as`).

**Tests.** No LLM is involved, so everything is a deterministic test.

- Property tests that generate random base, ours and theirs histories:
  - merging the same theirs twice changes nothing the second time;
  - no duplicate IDs remain;
  - no node is lost compared with the true union;
  - every old ID resolves through the alias log;
  - the result passes `ara check`.
- A suite of hand-built cases, each with a known correct result:
  - appends that touch nothing in common;
  - appends under the same parent;
  - the same claim edited in different fields;
  - the same claim edited in the same field;
  - renumbering with references in prose.
- A replay of the duplicate-`N124` case described above, merged with `ara merge`
  instead of `git merge`.
- Timing on graphs from 100 to 100,000 nodes.

## Phase 4: search and duplicate warnings

Depends only on Phase 1's read model, so it can run in parallel with Phases 2
and 3.

- **`ara find "query" [--type ...] [--limit 10]`** ranks entries by keyword
  relevance (BM25, the standard ranking formula search engines use) and
  returns IDs with a one-line excerpt. The agent then calls `ara show` on the
  results it wants.
- **Start without a stored index.** the-ara-of-ara's whole trace is about 300 KB. At
  that size, ranking every entry from scratch in memory should take
  milliseconds, so there is nothing to cache, keep fresh, or gitignore. The
  research note suggests tantivy with a cache under `.ara/cache/`. We switch to
  that only if the 10,000-node timing test misses the speed budget.
- **Duplicate warnings.** `ara add node` prints, without blocking, the closest
  existing nodes above a similarity threshold. `ara merge` fills
  `duplicate_candidates` the same way. The agent decides; the CLI never merges
  two nodes on its own.
- **Meaning-based search** (local embeddings through a small model such as a
  bge-class model, via fastembed) is built only if the research experiments show
  keyword search missing paraphrases. It sits behind a cargo feature that is off
  by default. Calling a hosted embedding API is out, because it would break the
  no-network promise.

**Tests.** A small set of questions over the-ara-of-ara, each paired with the
entries that should be returned. The test measures how many of those appear in
the top 10. Duplicate warnings are tested on pairs of nodes known to be
duplicates and pairs known to be distinct.

## Phase 5: CLI-backed copies of the ARA skills (protocol repo)

Copy the existing ARA skills and put `ara` between those skills and the
artifact. Skill changes live in the protocol repo; experiment configuration
lives in the external harness. Preserve the original skills for comparison.

For paper-benchmark comparisons, pin the skill files and reference pages used
by the paper's corresponding experiment. Record their source revision and
the mapping from each task to its skill. Copy from that revision, not from
the skill version installed locally. For live research-manager and compiler
experiments, also pin the source skills and keep unchanged baseline copies.

The CLI-only variants retain the source skills' research instructions:
reasoning steps, evidence standards, staging and promotion rules, roles,
and stopping criteria. Change only artifact access instructions and the
documentation needed to use the commands:

| Existing skill operation | CLI-backed operation |
|---|---|
| Read or grep knowledge-layer files to locate entries and relations | Use `ara ls`, `show`, `path`, `refs`, `open`, and `find`; retrieve the needed prose with `--full`. |
| Read files to choose the next ID, then write or edit an entry | Use the corresponding `ara add`, `edit`, `claim`, or `heuristic` command; the CLI assigns new IDs. |
| Stage an observation, or promote it when the skill's closure rule fires | Use `ara stage` or `ara promote`, retaining the same closure decision and provenance. |
| Write a compiled artifact through many file edits | Emit operations and call `ara apply`; retain the compiler's content and evidence requirements. |
| Maintain session records and revision history | Use session commands and batch operations that preserve every record required by the source skill. |
| Read or write code and evidence bodies | Keep direct file access, as required by the non-goals. |

Revise research-foresight's "no index layer" instruction only in its copied
variant, to permit `ara find`; relevance judgment stays with the agent.
Count the command reference and all other supplied skill documentation in
the experiment's token cost. The CLI must cover each required knowledge-layer
operation before its copied skill is considered integrated; missing support
does not justify dropping a source-skill step or silently reverting to file
writes.

Frontier views and shared intentions are a separate collective-research
extension to these copies. Keep their instructions separate from the
CLI-only access substitutions, including any change to the single-writer
rule. Their detailed plan must specify intention publication and refresh
across forks before implementation; private fork files alone are not shared
intentions. The target workflow includes this extension, but it must not
change the CLI-only condition used to measure the interface.

**Done when** each copied skill has a reviewed access-change diff against
its pinned source, and representative reading, writing, and compilation
tasks run end to end through the CLI. Check artifact fidelity, provenance,
and required history against the source skill's contract. Reuse the paper's
task and grading procedures where applicable. Measure performance again
after inserting the CLI; the original results do not establish equivalence.

## How we will know it works

Every phase has the deterministic tests listed above, and they run in CI. They
show that the commands are correct and fast. They do not show that agents
become cheaper or better, which is the actual goal. That question needs agent
experiments, which the research note designs. In short, they compare
file-based agents with CLI-using agents on:

- the ARA paper's 450-question understanding benchmark;
- a scaling curve, holding questions fixed while the ARA grows to 1,000+ nodes;
- a session-by-session replay of research-manager on the-ara-of-ara;
- several agents writing separate copies of one ARA and merging.

Use three reference conditions, with source skill revisions pinned as in
Phase 5:

| Condition | Skills and artifact interface | Purpose |
|---|---|---|
| Files | Unchanged ARA skills with their original file access | Baseline. |
| CLI | Copies of the same skills, with artifact access replaced by `ara` commands | Measure the interface change. |
| CLI + frontier + intentions | CLI-backed copies plus the explicit collective-research instructions | Measure the coordination extension. |

Reading, writing, and compilation experiments compare Files with CLI.
Collective-research experiments compare CLI with CLI + frontier + intentions.
Do not run every condition on every benchmark. Disable frontier views or
intentions individually only when a diagnostic comparison is needed.

Within each comparison, hold models, tasks, starting artifacts, grading,
and total compute budgets fixed. Pin and review every prompt difference;
keep research procedures unchanged in Files versus CLI. In the collective
comparison, record the coordination instructions as part of the intervention.
Measure held-out task quality, dollars and time, accidental duplicate
experiments excluding deliberate verification, and artifact integrity.
Repeat whole community runs; contributions within a run are not independent
samples. Fix quality margins and success criteria before running experiments.

Each experiment reports tokens, wall-clock time and answer quality. The
experiment harness lives outside this repo. This repo supplies the commands
and the timing tests.

## Open questions for the reviewer

| # | Question | My recommendation |
|---|---|---|
| Q1 | Should Phase 1 add the new layers (staging, sessions, heuristics) to the viewer's `Manifest`, or build a separate query-only model? | Add them to `Manifest` as optional fields. One model is simpler, and the viewer can show them later. The cost is a viewer bundle rebuild when the core changes (see `docs/stage-4-serve.md` on `viewer-embed-fresh`). |
| Q2 | Should commands find the ARA automatically, or always take a path? | Automatic, as described under "Finding the ARA", with `-C` to override. |
| Q3 | Should nodes created on the same date be grouped in the tree? | No. A child node means "builds on its parent", so grouping by date would invent false parent links. Use `ara ls --since` and session records instead. |
| Q4 | How strictly to enforce CLI-only writes? | Decide after Phase 5, using evidence of how often direct edits still happen. |
| Q5 | Who may write `logic/`: every agent (through the CLI, with conflicts reported at merge), or only research-manager? | Every agent through the CLI. The merge report makes conflicts visible, and research-manager stays the one that resolves them. |
| Q6 | Should `ara merge --git` call the `git` binary or use a Rust git library? | Call `git`. It is always present where merges happen. A library such as gix adds a large dependency for one feature. |

## How this relates to the open issues

- **#61** (claim falsification, proof and dependency rendering). Phase 1 needs
  its parser half: reading `Falsification criteria`, `Provenance` and `Tags`.
  The viewer half stays in #61.
- **#62** (node artifact pointers) and **#63** (node→concept links). These are
  format questions, so they go into the Phase 0 proposal. `ara show` returns
  them once they exist.
- **#40** (`.ara-check.toml`). Becomes relevant only if Q4 is answered with an
  `ara check` rule.
- **#60, #46, #31** (viewer). Not affected.

## Order of work

1. Review and approve this plan.
2. Phase 1: extend the read model first, then add the commands, about two PRs.
   At the same time, open the Phase 0 proposal in the protocol repo.
3. Phase 4 (search), in parallel with Phase 2 once Phase 1 is in.
4. Phase 2: single writes, then `ara apply`.
5. Phase 3: `ara merge` on directories, then `--git`.
6. Phase 5 in the protocol repo, after Phase 2 ships.
7. Rewrite this plan as `docs/agent-cli.md` and remove it from `plans/`.
