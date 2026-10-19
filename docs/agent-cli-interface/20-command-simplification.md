# Command simplification and task-oriented skills
**Date:** 2026-10-07

## TL;DR

The binary and six live skills now use nine agent command routes, with complete typed `apply` batches replacing removed authoring wrappers. Consolidated reads preserve ancestry, citation, unfinished-work, and identity payloads without compatibility aliases. Required engineering checks pass; historical reproduction, upstream approvals, and model-driven studies remain separate rollout gates.

Plan 20's approved design is implemented as one coordinated binary, live-skill, and acceptance-runner cutover in workspace version **0.1.27**. The implementation PR targets `feat/agent-cli-interface`; it does not merge that branch, release a binary, adopt an upstream protocol revision, or authorize experiments.

The [command reference](../agent-cli.md) specifies the shipped options, typed operations, and [old-to-new input/output mappings](../agent-cli.md#command-simplification-migration). [Verification](../verification/plan-20-command-simplification/README.md) records actual execution and the remaining rollout gates separately.

## Problem

The earlier surface duplicated core operations behind many top-level command adapters. Removing those adapters reduces command-route choice without replacing typed writes with arbitrary document patches or delegating scientific judgment to the CLI.

Keep nine agent routes: `status`, `ls`, `show`, `find`, `edit`, `claim set`, `heuristic set`, `apply`, and `merge`. `validate`, `check`, `layout`, `serve`, and existing tooling remain unchanged. Remove top-level `add`, `stage`, `promote`, `session`, `link`, `path`, `refs`, `open`, and `resolve`, plus `claim add` and `heuristic add`. No deprecated routes or compatibility aliases remain.

Retain all typed writer operations and merge resolution/repair capabilities. Teach task-oriented batches in the six live CLI skills; do not add new research-action verbs. Command count alone is not evidence of better accuracy, lower token cost, or faster research.

## Constraints

Keep the user-requested setters, `merge`, existing tooling, and all native history, identity, promotion, and transaction protections. Research decisions and protected-repair authorization remain in role procedures. Do not add a daemon, persistent cache, dependency, research-action interpreter, or experiment intervention.

## Proposed approach

### Ancestry and incoming references

`show NODE --with path` returns exact root-to-selected steps in `entries[i].relations.path`. Immediate parents and dependency cross-edges remain distinct existing relations.

`show SELECTOR --with refs` returns `relations.refs` with the previous `target`, `structured`, and `prose` shapes. Structured citations retain fields, source spans, and the writer's mutability/history classification; uncertain prose mentions remain separate. Both a positional whole-document selector and `--document` without a heading can request document refs. Matching uses the existing complete document scope, not newly invented short aliases.

A request constructs one citation inventory and classifies Markdown fields once for all requested logic targets. The native-only core `markdown_citations_many` query shares the writer's resolver and token classifications, retaining the previous read-only fallback for recovered headings and writer-inventory failures. Ordinary reads do not initialize the citation inventory. The core writer remains the authority for citation protection and historical identity. Native Rust callers replace the old single-target citation API with a one-element or multi-target request.

Selectors and relation applicability are validated as a whole. Source/heading selections do not silently drop relations. Node-only path requests reject incompatible targets, including a mixed multi-target request. Repeated/comma-separated relations deduplicate; selected entry order remains caller order.

### Unfinished selection

`ls --unfinished` exposes the previous open-work rows through the ordinary `ara.ls/v1` `entries` array. Reasons, measured inactivity, evidence sources, explicit-reference counts, limits, and null values for unknown history are retained. Filters intersect the reason-based result but never shrink the history used to calculate inactivity. An empty filtered selection remains empty rather than falling back to document listings.

Ordinary brief `ls` still lists documents; ordinary JSON `ls` retains its entries. Ordinary listing does not read history merely because unfinished mode exists.

### Identity-only inspection

`show --identity QUALIFIED` uses the existing resolver snapshot, not the current content model. It can resolve a retained identity even when its current body is not representable. Ambiguous, corrupt, and unknown mappings keep the resolver's precise failures.

The mode requires exactly one positional argument and rejects document, heading, source, native line, and relation options. Address-count errors are `argument_error`, exit 2, before artifact discovery or prepared-transaction inspection. A successful row always retains `kind: "identity"`, `requested_address`, and the complete `resolved_target`, including under projection.

### Projection and byte limits

`--fields` selects top-level row fields. Selecting `relations` keeps the complete nested citation payload; it is not a nested-field language. The recognized field registry covers empty unfinished selections, including `source_refs`.

Projected text reads of a whole document preserve selected `relations` in the complete printed row, for both positional and `--document` selectors. Rows without selected relations still print exact raw content. PR #117 review comment 4210650271 identified the earlier content-only shortcut, which discarded refs after projection. Actual-binary regressions cover both selector forms and raw-content preservation.

Bounds apply to the complete serialized response. Requested relations never truncate, disappear, or paginate independently. Native Markdown content pages contain whole lines and repeat the complete requested relations on every page; an insufficient metadata/line budget reports actionable required bytes. A relation request never falls back to relation-free source content. Multi-target and identity responses are complete or error. JSON remains unbounded unless an explicit maximum is supplied.

### Authoring cutover

Complete typed `apply` requests replace authoring wrappers. Preserve authored text, scalar/list types, explicit replay IDs, provisional bindings, dry-run semantics, failing physical-line reports, transaction rollback, promotion atomicity, and audit ownership.

JSON values are literal: `@file`, `@-`, and `@@text` are not expansion instructions inside JSONL. Callers resolve wrapper expansion before constructing JSON, preserve omitted/default fields intentionally, and consume `ara.apply/v1` per-operation results and `bindings` rather than old wrapper envelopes. Scratch requests stay outside knowledge-layer files.

Promotion returns `operations[i].target`; only numeric claim, heuristic, or dead-end destinations also return `id`. Named concept, constraint, and architecture destinations do not. The promotion tuple belongs to the observation and is inspected through `show`, not copied from a nonexistent operation-result tuple. A later failed operation leaves both source observation and destination unchanged.

Retained `edit`, `claim set`, and `heuristic set` preserve their existing field validation, `@` input expansion, and rejection boundaries. A standalone setter does not invent a session or claim an audit record. Audited current-knowledge revision/restructure uses the existing complete typed operation and explicit owning turn. Original history and exact audit endpoints remain protected.

Directory and local-Git merges retain collision remapping, portable sources, replay, conflict evidence, ordinary `merge resolve`, and separately authorized protected `merge repair`. Identity inspection consumes `show --identity`; equivalence annotation uses `node.link_same_as` and does not collapse records.

### Live consumers and research permissions

All six live skills use retained routes and complete typed batches. Three `cli-access.md` copies and the collective shared page are synchronized. Eleven shared task subsections cover finding/reading/citing, creating and revising records, full-turn recording, observations/promotions, structural edits, integration, and session closure. Role procedures link those subsections rather than reconstructing wrappers.

Reader access remains read-only. Compiler validation still requires format checks and the full Seal procedure. Closure signals, contradiction handling, provenance, taste-target confirmation, and protected-repair permission remain research decisions; mechanical bookkeeping does not infer them. The installed skill bundle is paired with the built binary during packaging smoke checks.

The current acceptance runner consumes the new nested refs envelope and distinguishes `show`, `show.path`, `show.refs`, and `ls.unfinished` scenario IDs, avoiding overwritten evidence rows. Fixed 100 ms real-read and 1000 ms generated-read/10k-merge budgets remain unchanged; 100k merge measurements remain report-only.

## Alternatives considered

Keeping the adapters would preserve old callers but leave command selection unchanged. Removing the retained setters would violate the requested boundary. Arbitrary document patches or new research-action verbs would replace established typed protections or introduce a second workflow interface.

## Tradeoffs

Fewer routes still require callers to learn typed operation schemas. Complete task examples and explicit output mappings make that migration inspectable; they do not establish improved research accuracy or lower token cost. Frozen callers remain historical evidence, not compatibility tests for the new surface.

## Migration

This is a public command-interface break. The intermediate workspace patch bump satisfies integration-PR bookkeeping; it does not declare backward compatibility. The final integration release must explicitly resolve the repository's major-version rule before publication.

Binary, live skills, migration guide, and current runner ship together. Frozen reports, historical run inputs, archived baseline skills, and downstream vendor pins are not rewritten to appear compatible. A future experimental condition must pin matching binary/skill revisions and migrate external harnesses in their owning repositories. Historical reproduction, upstream approval, model-driven studies, and paid execution remain separately gated.

The approved plan is retired into this design record after its engineering acceptance. The [rollout index](../../plans/agent-cli-interface/README.md) tracks unrelated pending work and scientific/upstream gates.

## Next Steps

1. Review the coordinated implementation PR targeting `feat/agent-cli-interface`; do not merge or release without the corresponding authorization.
2. Resolve the separate historical, upstream, and study gates before claiming wider rollout acceptance or running a newly pinned experimental condition.
