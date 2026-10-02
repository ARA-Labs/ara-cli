# PR 05: stage observations, promote entries, and retain session history
**Date:** 2026-10-01

Status: **approved** 2026-10-01. Repository: `ARA-Labs/ara-cli`. Parent: [agent CLI interface](../agent-cli-interface.md). Shared rollout and verification: [PR index](README.md). Depends on [PR 04](04-logic-editing.md), PR 03's transaction engine, and approved F3/F4 plus the session-metadata decision in [PR 00](00-protocol-contracts.md).

## TL;DR

Add typed observation staging, promotion, session creation, and turn logging. Promotion creates the target and records the observation's forward pointers in one transaction. Session logging retains events, actions, revision before/after values, context, and continuity lists instead of reducing a turn to node IDs. Update the session record and its index together under the checkout lock. Mutable session metadata and stale observations remain protocol blockers until the upstream rules explicitly allow their transitions.

## Problem

The parent command list has `session log --node`, but the current manager's session schema also carries `ai_actions`, `claims_touched`, `logic_revisions`, `key_context`, `open_threads`, and `ai_suggestions_pending`. Its `last_turn`, `turn_count`, rolling `summary`, and index counts change after creation. Recording only `events_logged` would discard research history needed by the copied skill, including the only preserved before/after text for mutable logic revisions.

The parent F4 recommendation lists forward pointers and `events_logged`, not these rolling metadata fields. The current skill also sets `stale: true` despite describing staging as immutable apart from forward pointers. F3/F4 must resolve those tensions, the append authorization for current-day turns, and the distinction between a current session and historical sessions. Current source places the index at `trace/sessions/session_index.yaml`; use the approved pinned path rather than silently creating a second root-level index.

## Constraints

All writes use PR 03's whole-source guards, one exclusive lock, and multi-file preimage rollback. A promoted observation remains in staging with its original content, context, provenance, timestamp, and `bound_to`. CLI code does not decide maturity, infer closure, upgrade provenance, fabricate a session summary, or judge evidence. The caller supplies those decisions and any complete target fields. Stage `potential_type` vocabulary and promotion target grammar follow approved protocol, not a guessed enum inferred from one fixture.

This PR's direct promotion targets are claim and heuristic, matching the parent command list. The manager also needs concepts, constraints, architecture, and refuted observations that become dead ends; PR 06 must cover those pinned requirements before CLI-backed skill integration. Do not call direct claim/heuristic promotion complete manager coverage. This PR does not add merge, reasoning/taste log commands, artifact initialization, or arbitrary historical session editing.

## Proposed approach

Add `crates/ara-core/src/write/staging.rs` and `sessions.rs` (new, native-only). Extend the operation enum, source policy, and adapters introduced in PR 03/04. Extend PR 01's session/staging readers in `agent_layers.rs` so readback preserves every supported record. Update existing `main.rs`, the new CLI write adapter, and session query consumers from PR 02. Proposed payloads use the following tags; optional creation `id` uses PR 03/06's replay/binding rules.

```json
{"op":"observation.stage","content":"Observed behavior","potential_type":"claim","context":"What happened","provenance":"ai-executed","timestamp":"2026-10-01T10:00","bound_to":["N12"]}
{"op":"observation.promote","observation":"O12","to":"claim","id":"C06","title":"Finding","fields":{"Statement":"Complete conclusion","Conditions":"Its boundary","Status":"hypothesis","Provenance":"ai-executed"},"signal":"empirical-resolution"}
{"op":"session.start","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Research resumed"}
{"op":"session.log","session":"2026-10-01_001","timestamp":"2026-10-01T10:05","summary":"Recorded an experiment","events":[{"type":"experiment","id":"N12","routing":"direct","provenance":"ai-executed","summary":"Measured the boundary"}],"ai_actions":[{"action":"Ran the experiment","provenance":"ai-executed","files_changed":["src/run.rs"]}],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":["Explain the boundary"],"ai_suggestions_pending":[]}
```

A `session.log` payload represents one whole turn. Assign its `turn` under the lock as previous `turn_count + 1` and stamp that turn onto the supplied record arrays. Absent rolling lists mean preserve the existing list; an explicit empty list means clear it, subject to approved metadata permissions. Caller-supplied summaries replace the rolling summary only when allowed. A `logic_revisions` record contains entry locator, field, exact `before`, exact `after`, signal, provenance, and optional note. PR 06 couples these records to actual edits and verifies them against staged source values; standalone logging preserves the caller's historical record and must not claim it proved an edit.

The proposed CLI adds `ara session log --session <ID> --record @turn.json` for a full typed turn. `--node N12` is a convenience event append: it requires the event summary/type/provenance or resolves only facts already present on the node. It does not replace full-turn history. Repeated `--node` values describe one turn. `ara session start` allocates the next date sequence under the lock; logging without `--session` requires an unambiguous current-day session and otherwise rejects. The current, unpinned manager skill groups turns by day; this proposal resumes an existing current-day session and permits a new same-day sequence only through explicit `start`, subject to the pinned policy and protocol review. Inject a clock in the library and use an explicit date in deterministic tests.

1. Obtain upstream approval for F3 pointer transitions, F4 current-session append permissions, `last_turn`/count/summary/index updates, rolling continuity lists, and stale flags. Decide whether stale is a permitted monotone field or a derived read property. Name these decisions in the protocol contract; do not broaden the whitelist locally. Resolve whether crystallization metadata belongs only in staging/session history or also in logic, since the current skill's procedure and later current-state rule differ.
2. Implement staging with required caller content and provenance plus approved optional fields. Allocate `O` IDs from staging only; `O1` labels in `logic/problem.md` are a separate namespace. Render all string content exactly using PR 03's source engine. Reject unknown bound nodes, duplicate IDs, malformed payloads, and unsupported potential types. Existing unknown fields and observation text remain unchanged.
3. Plan claim/heuristic creation using PR 04, then stage `promoted: true`, `promoted_to: "logic/claims.md:C06"` or the heuristic destination, and `crystallized_via` under the same operation intent. Validate that the destination exists in the candidate and refers to the newly created target. An already-promoted observation rejects without allocating or creating another target. Explicit provenance changes require caller fields; omission preserves the observation's provenance and never silently upgrades it. Include the source observation and created target in session history when a full turn accompanies the promotion in PR 06.
4. Create the session record and append its index row in one transaction. Reject mismatched filename/session ID/date, duplicate dates/sequences that break the approved selection rule, dangling index entries, and index paths outside the approved location. Preserve existing records and unknown fields. Initialize arrays and metadata only as prescribed by the approved schema; do not invent research content from the date.
5. Append complete turn arrays without modifying earlier entries. Update only approved rolling fields and derive index `turn_count`, `events_count`, touched-claim union, and open-thread count from the complete candidate record, not a truncated manifest. Keep `started` and previous timestamps immutable, validate chronological updates, and prohibit modifying a historical closed session. The current-session/closed-session boundary needs a reviewed rule, not file-mtime inference.
6. Add real-binary adapters for `ara stage`, `ara promote`, `ara session start`, and `ara session log`, including full record input and file/stdin long text. Output proposed `ara.stage/v1`, `ara.promote/v1`, and `ara.session/v1` JSON with assigned/target IDs, turn when applicable, changed paths, diagnostics, and committed status. Keep PR 02 error handling and surface rollback/recovery errors with exit 2.
7. Add implementation tests, the then-current patch bump in `Cargo.toml`, non-locked workspace check to refresh `Cargo.lock` before locked gates, `CHANGELOG.md`, and `docs/agent-cli.md`. Apply the index's Rust 1.94.1/native/wasm gates. Check affected wire literals/consumers and rebuild the embed if changed core behavior affects its wasm, not merely because the version changed.

## Alternatives considered

A session record containing only node IDs is simpler but loses actions, decisive context, revision history, and unresolved work. A generic replacement of the entire session YAML would retain caller content but permit rewriting prior turns. Use typed append records plus narrowly reviewed rolling metadata updates.

Creating the target first and marking the observation second can leave an unmarked target or a pointer to a missing target. Both edits belong in one transaction. Re-running promotion to repair a partial legacy state must be a separately reviewed repair operation, not a hidden duplicate-creation path.

## Tradeoffs

Per-file atomic rename does not give global filesystem atomicity. Any reader can see partial files during the rename sequence, and a process crash in that window can leave a promoted pointer without its target until PR 08's durable journal lands; `ara check` reports the dangling pointer and Git restores it. Rollback can fail, in which case the command returns exit 2 with the affected paths and does not claim the artifact is unchanged.

Full session payloads are larger than `--node`, but they preserve the source skill's continuity record. The CLI validates types, source references, turn consistency, and approved mutations. It does not verify whether a quoted user statement occurred or whether an empirical conclusion is warranted. Those remain the skill's responsibilities.

## Migration

No existing observation or session is rewritten merely to adopt the commands. The reader distinguishes absent optional logs from unreadable or malformed logs, and writers reject ambiguous target records. Do not merge two index locations automatically; if a pinned artifact uses a different approved location, record a reviewed migration or compatibility decision first. Keep the full original provenance and revision values when importing fixtures. PR 06 and PR 13 cannot claim fidelity until the metadata and stale decisions are resolved.

## Verification and acceptance

Add proposed tests in the new native staging/session modules and existing `crates/ara-cli/tests/cli.rs`. Cover exact multiline staging, separate problem/staging `O` namespaces, malformed dates, duplicate IDs, dangling `bound_to`, missing optional files, and preserved unknown content. For promotion, test target creation and all forward pointers together, duplicate promotion rejection, explicit versus inherited provenance, unsupported target kinds, invalid signal/payload, and failure after each affected-file rename with complete source restoration.

For sessions, test concurrent date-sequence allocation, same-day resumption, day rollover, no implicit selection when ambiguous, and rejection of historical edits. Append two turns with full events/actions/context/revisions and assert earlier records remain byte-identical. Check precise turn numbers, supplied before/after values, rolling lists' absent-versus-empty distinction, touched-claim union, and index counts derived from the record. Unauthorized metadata/stale changes must reject under the unapproved policy; approved behavior gets its own fixtures once PR 00 lands. An injected failure after the first rename must roll back so that no promoted pointer exists without its target and no index entry exists without its committed record.

The proposed real-binary smoke uses a disposable approved fixture with `N12` and a prepared full `turn.json`. Set `SMOKE_ARA` to that copy. Create `observation.txt` with multiline content and choose a fixture where the next observation is `O12` and the next claim is `C06`, then run after implementation:

```sh
ara stage -C "$SMOKE_ARA" --content @observation.txt --potential-type claim --provenance ai-executed --bound-to N12 --json
ara promote O12 -C "$SMOKE_ARA" --to claim --title Finding --signal empirical-resolution --set Statement='Complete conclusion' --set Conditions='Its boundary' --set Status=hypothesis --json
ara session start -C "$SMOKE_ARA" --date 2026-10-01 --started 2026-10-01T10:00 --summary 'Research resumed' --json
ara session log -C "$SMOKE_ARA" --session 2026-10-01_001 --record @turn.json --json
ara show O12 C06 2026-10-01_001 -C "$SMOKE_ARA" --full --json
ara promote O12 -C "$SMOKE_ARA" --to claim --title Duplicate --signal empirical-resolution --json
```

Expect preserved observation text/provenance, one committed claim and its complete promotion pointers, complete turn history, consistent index counts, and exit 1 with unchanged files on duplicate promotion. Adjust target required fields in the smoke to the approved schema rather than relying on incomplete fixtures. Inspect actual staging/session/index source files. This documentation task runs no commands or checks.

## Next Steps

Approve the session metadata, current-session boundary, stale behavior, index path, and crystallization-history decisions in PR 00 before shipping writes. Review full-turn and convenience CLI payloads. [PR 06](06-batch-apply.md) then adds atomic cross-operation turns and every remaining knowledge-layer operation required by the pinned skills.
