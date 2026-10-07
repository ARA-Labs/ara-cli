# Plan 20: simplify agent commands and teach workflows in skills
**Date:** 2026-10-06

Status: approved with revisions on 2026-10-06 at the user's request. Planning branch: `docs/agent-cli-command-simplification`, created from `feat/agent-cli-interface`. Approval covers the design below; implementation is pending. This review does not authorize implementation work, commits, PR creation, merging, release, upstream protocol adoption, or experiments. Related work: [rollout index](README.md), [command reference](../../docs/agent-cli.md), [skill maintenance](../../docs/agent-cli-skills.md), and [deterministic bookkeeping](19-deterministic-bookkeeping.md).

## TL;DR

Keep `edit`, `claim set`, `heuristic set`, and `merge`, as the user requested. Keep a small read interface and `apply` for atomic changes, while removing redundant authoring wrappers and separate auxiliary read commands. Teach research actions in skill subsections using the existing typed operations; do not add a command verb for each action. Format checking and visualization commands remain unchanged, and the native data, audit, and history protections remain enforced by the binary.

## Problem

The current command enum exposes 22 top-level commands, excluding generated `help`. Four are existing tooling: `validate`, `check`, `layout`, and `serve`. The other 18 expose the agent interface, including multiple authoring adapters and separate read views. The inventory is visible in [main.rs](../../crates/ara-cli/src/main.rs) and was checked with local `ara --version` and `ara --help` calls on 0.1.26 while preparing this draft.

Most authoring wrappers submit one typed operation to the writer that `apply` uses. The user requested keeping `edit`, `claim set`, and `heuristic set` for the research-manager, even though they share the `EntryEdit` primitive in [write.rs](../../crates/ara-cli/src/write.rs). Skills can teach the other research actions through existing operations. The cutover must preserve those actions and their protections against unrestricted file editing.

## Constraints

The user-directed boundaries are fixed: retain the three editing forms and `merge`; leave format checking and visualization outside this change; put research semantics in skill subsections. The removal set, read-option spellings, and output mappings below are approved for implementation. Retain `merge resolve` and `merge repair`, including their different permission boundaries. This plan does not add a research-specific verb, a natural-language command interpreter, an automatic research-manager, or a new workflow engine.

The binary continues to enforce typed fields, ID allocation, source guards, provenance, immutable trace/staging content, promotion pointers, citation repair, and atomic audit/session/index updates. The skill decides what the research means: whether a topic is abandoned, which closure signal applies, whether evidence supports a claim, and what a confirmed user reaction says. Moving instructions into a skill does not make these protections optional.

Preserve brief text for agent reads and explicit `--json` for structured consumers. Reuse the current query, citation, identity, history, and transaction machinery. Content reads share one loaded artifact; identity-only lookup keeps the resolver's own snapshot path. Auxiliary computations run only when requested; ordinary `ls` and `show` must not start calculating session history or scanning every citation. No dependency, daemon, persistent cache, paid run, or experiment-condition change is part of this plan.

## Proposed approach

### Which commands remain?

The approved agent surface has nine top-level commands. `claim` and `heuristic` keep their `set` subcommand; creation moves to `apply`. Generated `help` and the four tooling commands do not count toward this agent inventory.

| Retained command | Responsibility |
|---|---|
| `status` | Artifact counts, completeness, and diagnostics |
| `ls` | Documents, entries, structural filters, and the unfinished-work view |
| `show` | Native content, source selections, relations, and identity inspection |
| `find` | Keyword retrieval with source locations and bounded context |
| `edit` | Existing permitted field edits through native selectors |
| `claim set` | Existing claim field setter |
| `heuristic set` | Existing heuristic field setter |
| `apply` | Complete typed JSONL operations and atomic multi-operation changes |
| `merge` | Directory/Git integration, mutable conflict resolution, and protected repair |

`validate`, `check`, `layout`, and `serve` keep their commands, arguments, output contracts, and behavior. The compiler skill still calls `check` where its existing procedure requires it. This boundary does not remove those calls or change the original scientific/visual verification checklist.

### How do removed commands map to retained capabilities?

Remove these command routes only in the same cutover that supplies and documents their replacements. Keep all typed writer operations and core query capabilities. The read-option names in this table are approved syntax, not supported by the current binary.

| Command to remove | Replacement | Capability that must survive |
|---|---|---|
| `add node` | `apply` with `node.add` | Required payloads, native allocation, parent and terminal-node rules |
| `add edge` | `apply` with `edge.add` | Validated dependency cross-edges |
| `claim add` | `apply` with `claim.add` | Complete claim creation and source field/list preservation |
| `heuristic add` | `apply` with `heuristic.add` | Complete heuristic creation in its source dialect |
| `stage` | `apply` with `observation.stage` | Original observation content, context, provenance, and timestamp |
| `promote` | `apply` with `observation.promote` | Atomic destination creation and promotion pointers; original observation retained |
| `session start` | `apply` with `session.start` when explicit setup is needed | Explicit session identity/date and replay behavior |
| `session log` | `apply` with `session.log` | Complete turn, index, ownership, and derived mechanical rows |
| `link --same-as` | `apply` with `node.link_same_as` | Separate retained records, proven chronology, and cycle checks |
| `path` | `show <node> --with path` | Root-to-selected-node nesting, separate from dependency cross-edges |
| `refs` | `show <selector> --with refs` | Entry and whole-document targets, typed references, source spans, possible prose mentions, and citation classification |
| `open` | `ls --unfinished` | Existing reason predicates and measured observation inactivity with evidence/unknown states |
| Standalone `resolve` | `show --identity <qualified-address>` | Exact imported/retained identity lookup, without requiring a readable current body |

The cutover removes nine top-level agent commands and two creation subcommands. It retains the nine agent commands above and all four tooling commands. Do not leave hidden aliases, deprecated wrappers, old dispatch paths, or obsolete command-specific renderers after the cutover. These removals are breaking CLI changes and require the migration steps below.

### How do the read replacements work?

`show --with path` uses the existing ancestry query, including the selected node as the last step. Keep immediate `parents`, `children`, and `depends_on` relations distinct. `show --with refs` uses the existing reference inventory and the writer's citation rules. It accepts every target supported by the old `refs`, including whole knowledge documents such as `trace/exploration_tree.yaml`. A document target keeps its existing document-wide reference matching; it does not become an arbitrary heading target. Share helpers over the loaded artifact without invoking another command or repeating its load. Build shared citation inventories once per request, not once per selected target.

`ls --unfinished` reuses the existing `open` predicates and timeline measurement. It returns source-order entry rows with `reasons` and the existing observation inactivity fields. Unknown chronology remains `null`, with its diagnostics and evidence. Additional type, subtree, date, status, provenance, or document filters intersect this selection. Filters select result rows; they never filter the history used to calculate inactivity. Preserve existing filter meanings, including `--under` selecting descendant nodes rather than observations bound to them. An empty intersection returns `entries: []`, without falling back to document headings or inventory.

Without `--unfinished`, preserve both current defaults: unfiltered brief `ls` lists documents, while unfiltered `ls --json` lists entries. Reading never changes stale or promoted state, and a stored stale flag remains visible under the existing rules. Build the timeline once when observation inactivity is needed, and leave ordinary list reads on their existing path.

`show --identity` is a boolean mode flag requiring exactly one positional address. It uses the current standalone resolver and returns the exact resolved target, including retained identities for which normal content selection is unavailable. Dispatch it before normal content selection and use one identity snapshot. Do not require an `Artifact::load` or a readable current body. Preserve the resolver's existing address domain, source checks, error codes, and exit classes. Normal `show` continues to accept qualified addresses without a separate lookup first. Unknown, ambiguous, and corrupt identity records remain explicit errors, not guessed local matches.

| Combination | Approved behavior |
|---|---|
| `show N02 --with path,refs --with parents` | Existing comma-separated and repeated `--with` syntax; combine the requested relations, emitting duplicate relation names only once |
| Multiple positional selectors with relations | Keep request order and attach relations to each row; reject the whole request if any selection cannot supply a requested relation |
| `--with path` on a non-node | Reject explicitly; never return an empty path as a substitute |
| `--with refs` on a document | Accept a positional document path or `--document <path>` without `--heading` or `--source`; preserve old document-target reference behavior |
| Relations on `--source`, heading-only selections, or another unsupported selector | Reject with `invalid_selector`; never silently discard a requested relation |
| `--identity` with `--document`, `--heading`, `--source`, `--lines`, or `--with` | Reject with an argument error before loading; zero or multiple positional addresses also reject |
| `--identity` with output options | Accept `--json`, `--fields`, `--full`, and `--max-bytes`; `--full` does not change the exact mapping, and existing bounds/projection conflicts still apply |

The approved JSON shapes are below. Content-read diagnostics stay in the retained command's outer envelope. Identity-only mode reports the resolver's actual failures; it must not claim an extra content-validation pass. Brief output prints the selected content followed by labeled relation sections, unfinished rows with their reasons and inactivity evidence, or the requested-to-resolved identity mapping.

| Mode | Approved result shape | Mapping from old output |
|---|---|---|
| `show --with path` | `ara.show/v1`, each selected row's `relations.path` is an ordered entry array | Old `steps` become that array, with existing row fields preserved |
| `show --with refs` | `ara.show/v1`, each row's `relations.refs` contains `target`, `structured`, and `prose` | Preserve the old reference rows, certainty, ranges, and brief source-line annotations |
| `ls --unfinished` | `ara.ls/v1`, `entries` contains reason-selected rows | Old `items` become `entries`; keep `reasons` and every inactivity/evidence field |
| `show --identity` | `ara.show/v1`, one entry with `kind: "identity"`, `requested_address`, and `resolved_target` | Old `address` becomes `requested_address`; old `id` becomes `resolved_target`, without altering its string |

Old `ara.path/v1`, `ara.refs/v1`, `ara.open/v1`, and `ara.resolve/v1` command outputs disappear with their routes. Existing modes of retained commands keep their field meanings and output bounds. Ordinary selection keeps the reader's candidate/error handling; identity-only mode retains the old resolver's precise error codes and exit classes. Document these mappings and migrate every live consumer together.

`--fields` keeps the retained commands' top-level row projection rules; it does not gain a nested query language. Request `relations` to keep the complete relation object, such as `show C01 --with refs --fields relations --json`. Old `refs --fields field,literal` callers must migrate to this shape rather than projecting fields from the selected entry by mistake. Register all unfinished-row fields for projection, even when the result is empty. Identity rows always retain `kind`, `requested_address`, and `resolved_target` when projected. Update `output.rs` alongside the command and brief renderers.

The whole `show` response, including relations and metadata, remains subject to the existing byte budget. A native content page may shrink at whole-line boundaries, but each requested relation is complete on every page; relation arrays never paginate, truncate, or disappear. If relations plus required metadata and the next content line cannot fit, return `output_limit_too_small` with the required budget and a hint to raise `--max-bytes`. A source-only fallback cannot satisfy a relation request. Multi-selection responses and identity mappings fit in full or reject with an actionable required budget. JSON remains unbounded unless the caller supplies `--max-bytes`. Do not add cursor state or relation-limit flags.

### How do authoring callers migrate to `apply`?

Each removed wrapper has a one-operation JSONL equivalent, and related operations can share one atomic request. Keep explicit replay IDs, complete scalar/list payloads, dry-run behavior, source preconditions, and error/rollback guarantees. JSONL string values are literal: wrapper inputs such as `@file`, `@-`, and `@@text` must become the intended text in JSON, not be copied as strings expecting expansion. The retained setters keep their existing CLI text-input expansion.

Migrated writes return `ara.apply/v1`, not the removed command's envelope. Callers consume the committed report's operation results and bindings, preserve the existing failing-line/error handling, and treat dry-run IDs, timestamps, and turns as tentative. Document each removed wrapper's argument-to-operation and result-field mapping in the migration guide. Scratch JSONL may be written outside the artifact with the caller's file tool; this does not permit direct knowledge-file edits or a shell pipeline.

### What belongs in the skill subsections?

Add task-oriented subsections to the shared access reference and link them from the relevant skill procedure. Keep the three `references/cli-access.md` copies identical, as [the current maintenance rules](../../docs/agent-cli-skills.md#changing-a-skill) require. Reader skills use only the read subsections; the research-manager and compiler use the write subsections within their existing role permissions. Update the collective skills' shared pages as well, since their frontier reads currently call `open` and `refs`.

| Skill subsection | Instructions to teach |
|---|---|
| Find and read relevant knowledge | Orient with `status`/`ls`, search with `find`, read complete sources with `show`, and cite native addresses |
| Initialize or extend an artifact | Use `artifact.init` with caller-supplied PAPER content, registered document operations, and audited root edits; retain the source/evidence boundary |
| Inspect ancestry, citations, and imported identities | Use the retained read options; distinguish nesting, cross-edges, typed references, possible mentions, and aliases |
| Review unfinished work | Use `ls --unfinished`; judge inactivity under the unchanged skill rules, then use `observation.mark_stale` with the required reason/audit when warranted |
| Record a research turn | Start one request with a summarized `session.log`, then add typed events and reasoning in the same `apply` batch |
| Stage or crystallize an observation | Choose the signal under the unchanged research procedure; use `observation.stage` or `observation.promote`, never manual pointer edits |
| Create claims and heuristics | Supply complete source-grounded fields through their typed creation operations; consume returned IDs/bindings |
| Edit current knowledge | Explain direct setters versus audited revisions, full-source guards, and coupled history |
| Rename, merge, or split knowledge entries | Use existing audited structural operations and citation repair; preserve original history and identities |
| Record annotations and confirmed user reactions | Use `entry.annotate`, `entry.taste_append`, or `record.append` under the existing confirmation and history rules |
| Integrate another artifact | Use `merge`, inspect candidates, and keep ordinary resolution separate from protected repair |

These subsections describe operation selection and interpretation. Do not introduce additional JSONL operation names, a custom skill scripting language, shell pipelines, or semantic mode flags such as `--confirm-claim`. Reuse the documented operation schemas, provisional bindings, exit codes, source/evidence boundary, and one quoted `ara` invocation per shell call.

### How do the retained setters fit audited research work?

The current setters submit `EntryEdit`, which changes permitted fields but does not record a complete research-manager turn. Keep their syntax and semantics. Put this distinction next to the skill's setter examples: an audited research-manager revision uses `logic.revise` and an owning `session.log` to capture exact before/after history and `Last revised`. A plain setter must not imply confirmation, invent provenance, or silently create a session.

For work requiring coupled audit history, put the revision and summarized log in one `apply` request. Separate setter and session calls cannot provide the same atomicity. The approved design adds no audit flag family or automatic session to standalone setters.

The following current-schema example records a question and its reasoning. Save it outside the artifact as `turn.jsonl`, then submit the whole request once. The tool allocates the node, session, timestamp, and turn; the skill supplies the question, provenance, summary, and reasoning.

```jsonl
{"op":"session.log","summary":"Recorded a measurement-boundary question"}
{"op":"node.add","id":"$question","type":"question","parent":"root","title":"Does the claim depend on this setup?","fields":{"description":"Check whether the claimed result holds under the declared measurement setup.","provenance":"ai-suggested"}}
{"op":"record.append","document":"trace/pm_reasoning_log.yaml","record":{"notes":["Recorded the question without changing any claim status."]}}
```

```sh
ara -C <artifact> apply <scratch>/turn.jsonl --json
```

During draft preparation, this example was exercised with `ara 0.1.26` on a temporary artifact initialized through `artifact.init` with caller-supplied PAPER content. The batch committed, bound `$question` to `N01`, and `show N01 --full --json` returned the authored question. The approved read options and command removals have not been implemented or exercised.

The 2026-10-06 design review separately exercised current `ls` brief/JSON defaults, `path`, entry and document `refs`, `open`, existing relations, a document/relation combination, and a bounded-output rejection on temporary artifacts with `ara 0.1.26`. Document `refs` returned a citation to a node within that document. Current `show --document ... --with parents` returned content but dropped the relation request, which the approved selector rules above forbid. These are current-binary observations, not evidence that the cutover is implemented.

## Alternatives considered

Keeping all commands and only shortening the skills avoids compatibility work, but leaves the binary surface unchanged. Keeping only `apply` for writes would remove the setters the user explicitly wants. Adding verbs such as `record-turn`, `crystallize`, or `review-stale` would keep expanding the command inventory even though the skills can teach those workflows through the existing operations.

Replacing typed operations with arbitrary document patches would reduce schema choices at the cost of promotion, identity, and audit protections. This plan retains those operations. Read consolidation needs a few options because ancestry, citation classification, and measured inactivity are computed facts that a prose subsection cannot safely recreate.

## Tradeoffs

Agents will learn fewer command routes, but `apply` still has a typed operation schema. Task-specific skill examples must make the common paths usable without copying the whole operation table into every procedure. Preserve the scientific procedures, closure signals, confirmation rules, and role permissions while changing access instructions. Command count alone does not establish better agent accuracy, lower cost, or faster research; this plan makes no such claim and does not authorize a pilot.

Removal breaks scripts and skills pinned to the old commands. Freeze historical inputs and results; do not rewrite old experiment fixtures to look compatible. New conditions must pin a matching binary and skill revision. For normal consumers, ship the migration guide, binary, and updated live skills together, with explicit old-to-new output mappings and no compatibility aliases.

## Migration

### What changes in the implementation PR?

Use one coordinated functional PR targeting `feat/agent-cli-interface`, squash-merged under the existing rollout policy. The following steps are implementation order within that PR. Design approval does not request implementation, commits, or PR creation in this documentation review.

1. Use the approved read-option and JSON contracts above. Capture old behavior on isolated fixtures so consumer-visible results can be compared without preserving the old routes in shipped code. Account for every row in the replacement table, wrapper input/output mappings, and all existing typed operations used by the skills.
2. Factor ancestry, reference, and unfinished-work queries into helpers over the loaded sources/indexes. Keep identity-only lookup on the existing resolver snapshot path. Add retained read modes in `crates/ara-cli/src/agent.rs`, its `agent/` helpers, `output.rs`, and `brief/{mod,show,lists}.rs`, including selector validation, projection, and byte-budget behavior. Keep the writer's citation and history rules as the single authority. Run language-server references before changing any exported Rust symbol.
3. Remove the obsolete command variants and dispatch in `crates/ara-cli/src/main.rs`, convenience-only argument types/adapters in `write.rs`, standalone identity adapter routing in `merge.rs`, and dead brief-output routes. Retain setter adapters, `apply` input handling and transaction behavior, all core writer operations, and merge conflict commands. Migrate tests that currently construct removed adapter types.
4. Update all six live CLI skills, including prose references outside fenced examples, and keep shared copies synchronized. Change the live acceptance runner's invocations and output consumption in `scripts/agent-cli-acceptance.py` and its tests. Update the command reference and implementation records whose current instructions name removed routes. Archived verification reports and completed run inputs remain historical evidence.
5. Bump the workspace patch version in `Cargo.toml`, refresh local crate versions in `Cargo.lock` with a non-locked workspace command, and add the breaking command migration under `CHANGELOG.md`'s Unreleased section. Command removal is a public-interface break; the final integration release must explicitly resolve the repository's major-version rule before release. An intermediate patch bump is not a declaration that this is backward compatible.
6. Update consumer regressions and run the focused CLI read/write/identity/skill suites. Keep behavior assertions for protected state and errors; remove incidental wording or implementation snapshots rather than pinning new wording. Perform the actual-binary workflow checks listed below, then run the locked final workspace gates once after integration.

Primary regression files are `crates/ara-cli/tests/{agent_reads,agent_brief,agent_addresses,agent_citation_reads,agent_merge_identity,agent_writes,skills}.rs` and the existing core write/history tests. The implementation must also find remaining callers across the repository and live downstream consumers; this list is not permission to leave other affected callsites unchanged. Downstream harness changes stay in their owning repository and are required before a newly pinned condition can use this surface.

### What proves that the cutover is complete?

| Scenario | Required observed result |
|---|---|
| Read ancestry and dependency relations | Exact root-to-selected path, immediate parents, and cross-edges remain distinguishable |
| Combine selectors, relations, and output bounds | Multi-target order and combined relations survive; whole-document refs retain old matching; unsupported combinations reject without stdout; oversized relations give an actionable budget error and never truncate |
| Project migrated results | Nested refs survive `--fields relations`; empty unfinished selections accept their documented fields; projected identity rows retain the exact mapping |
| Inspect references after rename/import | Typed citations retain fields/spans and mutability classification; possible prose mentions remain separate; unknown or malformed identities fail explicitly |
| Review an inactive observation | Unfinished reasons, explicit-reference counts, evidence sources, and unknown-history diagnostics survive; the read writes nothing |
| Apply filters to unfinished work | Filters intersect the reason-based selection without changing the full-history calculation or making unknown counts zero; empty results stay empty; ordinary brief/JSON `ls` defaults remain unchanged |
| Resolve imported or retained identities | Identity-only lookup returns the same exact resolved target and preserves the old resolver's loading/error boundary without requiring current content; ambiguous or corrupt mappings reject |
| Record a complete research turn | One batch writes the authored events/reasoning and owned session/index; returned identities and derived mechanical rows agree |
| Replace authoring wrappers | Complete text/list inputs, literal leading `@`, explicit replay IDs, provisional bindings, dry runs, and failing-line reports survive through `apply`; no source commit occurs after a later failure |
| Promote an observation | Destination and promotion tuple commit together; original content survives; a failed later operation leaves both unchanged |
| Revise or restructure current knowledge | Exact audit endpoints and revision pointers commit with the owning turn; typed citation repair preserves historical identities |
| Use the retained setters | Existing allowed edits and rejection rules survive; standalone setters do not claim a session audit they did not create |
| Merge directories or local Git refs | Existing collision mapping, replay, conflict evidence, ordinary resolution, and protected repair regressions pass |
| Execute skill examples | Current read/write examples run on the actual binary with no direct knowledge-file fallback, unsupported route, or stale flag |
| Run existing tooling | `validate`, `check`, `layout`, and `serve` retain their current contracts; exercise existing format checks and viewer surface |
| Keep unrequested reads cheap | Ordinary `ls` and `show` perform no new history/citation scan; combined relation reads share inventories, and identity-only lookup does not also load the content model |

Add or extend deterministic unit and CLI integration tests for these behavior boundaries. Run the actual built executable on temporary artifacts for each changed read mode and representative reader, research-manager, compiler, and merge workflows. Exercise malformed history, ambiguous identity, and failed-batch cases as well as successful calls. Static command/flag checks remain useful but do not replace these executions.

Final engineering checks include the pinned Rust 1.94.1 formatter, locked workspace tests, all-target Clippy, the applicable wasm/viewer synchronization checks, and the current acceptance runner's complete in-scope sections. Reuse fixed performance budgets for affected read/merge paths and record timings; do not infer a speedup from fewer command names. Installed-skill packaging checks must pair the new live skills with the built binary. Model-driven experiments and paid execution require separate authorization.

## Next Steps

1. Design approved with the revisions recorded above on 2026-10-06. The read-option spellings, output mappings, and standalone-setter audit boundary are settled in this plan.
2. When implementation is requested, execute the coordinated cutover with its skill migration and evidence. This review changes no Rust source, skill content, binary version, or release artifact.
3. Once every acceptance item is met, move the completed design into `docs/agent-cli-interface/`, update the rollout index, and remove this plan under the repository's planning policy. Suggest a commit message for human review; do not commit, open a PR, merge, or release without the corresponding authorization.
