# PR 19: deterministic bookkeeping in the CLI
**Date:** 2026-10-04

Status: **approved 2026-10-04** at the user's direction; **revised and re-approved 2026-10-05** after a code review against 0.1.24 (`aeb5e0d`). See [the 2026-10-05 revision](#what-changed-in-the-2026-10-05-revision). Approval covers this design, not implementation, measured improvements, upstream protocol adoption, or completed verification. Repository: `ARA-Labs/ara-cli`, including its [CLI-backed skills](../../docs/agent-cli-skills.md). Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks and integration policy: [PR index](README.md). Related read and skill changes: [plan 18](18-failed-blocked-calls.md).

## TL;DR

`ara` will derive clock values, owned turn numbers, operation-backed event rows, and claim-touch rows when a caller explicitly logs a turn. It will also repair permitted mutable references, expose evidence-backed idle counts, and render new claims and heuristics in a fixed schema order. The caller still supplies research content, summaries, provenance, signals, judgments, and any ambiguous reference mapping. Existing history remains unchanged, and missing or conflicting ownership fails before commit.

## Problem

The initial 2026-10-04 audit reported deterministic work in six CLI-backed skills that the binary could perform. It also reported a `claim add` layout bug. These are preliminary historical inputs retained from the draft. This approval revision did not repeat the audit or reproduce the bug. The implementation PRs must attach runnable evidence for their changes.

The current writer already allocates IDs, stamps session-row turns, maintains session counters and indexes, records exact revision endpoints, computes `Last revised`, and validates promotion pointers. The remaining work is uneven: JSONL needs values that convenience commands calculate, and some skill instructions still request duplicate history or fields that the writer rejects. Counting explicit references is mechanical; deciding whether a topic was abandoned is not.

### Which audit items are in scope?

The skill paths in this table are under `skills/research-manager-cli/`. Core paths beginning with `write/` are under `crates/ara-core/src/`; CLI source paths are under `crates/ara-cli/src/`. Labels A through F retain the original audit's grouping so implementation evidence can account for every item.

| Item | Reported chore | Current source boundary | Approved disposition |
|---|---|---|---|
| A1 | Next turn for revisions, audits, reasoning, and `merge --turn` | `write/mod.rs` requires concrete revision ownership; `write/batch.rs` rejects forward bindings and couples revisions to a new logged turn | Derive omitted ownership from one explicit `session.log`; handle `merge resolve` and `merge repair` separately |
| A2 | Node, observation, session, and taste timestamps | `write/node.rs` reads `fields.timestamp`; staging, sessions, and taste validate caller values; CLI `write.rs` defaults some values before locking | Capture one clock value inside the writer boundary and preserve valid explicit values |
| A3 | Today's session ID or session creation | CLI `session log` selects a date-matching session; core `session.log` requires its ID | Select one open session, or create one only for an explicit log with a summary |
| A4 | Duplicate `events_logged` descriptions | CLI `session log --node` builds node events; core `SessionLog.events` writes `events_logged` | Derive operation facts, retain caller summaries, reject conflicting facts |
| A5 | Duplicate `claims_touched` rows | `write/sessions.rs` validates supplied action rows but does not derive them | Derive structural actions; preserve explicit scientific judgments |
| B1 | Set `Last revised` | `SKILL.md` edit procedure; `write/logic.rs::revise` rejects caller values and computes the pointer | Delete the duplicate instruction |
| B2 | Copy revision before/after into `logic_revisions` | `SKILL.md` edit procedure; `write/logic.rs::revise` appends those values | Explain the CLI-owned audit instead of asking for a second copy |
| B3 | Set observation promotion pointers | `SKILL.md` crystallization procedure; `write/staging.rs` sets the final tuple atomically | Require `observation.promote`, not manual pointer edits |
| B4 | Add claim `Crystallized via` and `From staging` fields | Early skill instructions contradict its current-snapshot rule; `write/fields.rs` does not accept those claim fields | Keep provenance in the observation and session history; remove the rejected writes |
| C1 | Rewrite citations after rename, merge, or split | `write/logic.rs::structural` accepts exact caller reference edits; current `refs` also finds immutable references | Rewrite only allowed mutable fields; retain or authenticate historical identities |
| D1 | Count turns without an observation/topic reference, using k = 5 | `SKILL.md` topic-abandonment rule; CLI `agent.rs::SessionHistory` currently scans session bodies by day | Expose explicit-reference counts with evidence and unknown states; leave semantic topic matching to the caller |
| D2 | Construct `observation.mark_stale.session_days` | `write/staging.rs::stale_evidence` verifies the supplied days against session history | Derive the canonical evidence list when omitted; still validate supplied evidence |

### What was reported about claim layout?

The draft reported this command on a copy of `crates/ara-core/tests/fixtures/agent-cli`:

```sh
ara claim add --title "Order probe" --set "Statement=S text" --set "Conditions=C text" \
  --set "Status=supported" --set "Falsification criteria=F text" --set 'Proof=[]' \
  --set 'Dependencies=[]' --set "Provenance=user" --set 'Tags=["x"]'
```

The 2026-10-05 review reproduced it on 0.1.24 (`aeb5e0d`). The new `C17` block lists fields alphabetically (Conditions, Dependencies, Falsification, Proof, Provenance, Statement, Status, Tags). Every value is on its own indented continuation line, and `Falsification criteria` became `Falsification`. The `C16` block just above it uses inline values in Statement-first order, with `Dependencies: [C03, C04, C05, C06]` and `Tags: evaluation, experimental-design`. Current `write/fields.rs` confirms that validation returns a `BTreeMap`, aliases both falsification spellings, and renders all values as continuation lines. The fix must address ordering and readability without treating valid JSON list encoding as data loss or promising to imitate every existing layout.

## Constraints

This plan does not decide scientific status, upgrade provenance, infer closure signals, discard observations, or turn an arbitrary edit into a research event. Summary text, near-miss reasoning, confirmed taste, and `ai_actions` remain caller-authored. Standalone writes without a logged turn remain possible where they are possible today; they do not silently create a session. Operations that already require an audit still require one.

The existing [knowledge boundary, transaction, and history contracts](../../docs/agent-cli.md#batches-audit-ownership-and-recovery) apply. Unknown fields, ambiguous selectors, invalid ownership, malformed evidence, and new dangling references fail closed. Source preconditions use digests of the exact selected source, never a brief view, excerpt, or truncated projection. Document selectors retain their complete literal `heading` arrays, including headings containing `/`, `#`, or duplicate leaf names under different parents.

Bookkeeping runs only when its operation or read view needs it. Ordinary reads do not open write state, acquire a writer lock, capture snapshots, or calculate session history for unrelated commands. Reuse the working snapshot, typed inventories, and existing transaction. This feature needs no background service, persistent cache, or new dependency. Implementation must measure the affected paths; this approval makes no performance claim.

## Proposed approach

### A0 and A2: capture one clock value without replacing explicit values

In commit mode, capture `batch_time` once after acquiring the artifact lock and recovering any prepared transaction, before planning from the current snapshot. Pass the captured value through core planners using an injectable clock at the execution boundary. Native defaults use an explicit UTC `YYYY-MM-DDTHH:MM:SSZ` timestamp and its UTC calendar date. Core code must not read the clock independently for each operation, and CLI convenience commands must not choose an ID or timestamp from an unlocked pre-read.

Today the only clock read is `crates/ara-cli/src/write.rs::now()`, which runs in the CLI adapter before the lock is taken. Replace it with a core entry point that takes a clock function and calls it once after lock and recovery. The shipped binary passes the system clock. Do not add a test-only environment variable or hidden flag to override time in the released binary. Exact-time cases (midnight, offsets, date and session mismatch) are core tests with a fixed clock. CLI integration tests assert the format, check that every omitted value in one batch is identical, and check that the value falls between wall-clock reads taken before and after the command.

| Authored value | Omission behavior |
|---|---|
| `node.add.fields.timestamp` | Stamp the new node with `batch_time`; promotion-created dead-end nodes use the same rule |
| `observation.stage.timestamp` | Use `batch_time` |
| `session.log.timestamp` | Use `batch_time` |
| `record.append` to `trace/taste_log.yaml`, `record.timestamp` | Use `batch_time` |
| `entry.taste_append.record.date` | Use the UTC date of `batch_time`; this operation has a date, not a timestamp |
| Session created by A3, `started` and `date` | Use the owning log's effective timestamp and its written calendar date |

An explicit timestamp or date remains exact after validation. A log's effective timestamp is its supplied value or `batch_time`; session selection uses that value's written date, so an explicit offset is not silently converted to a different day. Keep the accepted legacy timestamp grammar, date/session matching, closed-session checks, and instant-based monotonicity checks. An explicit past session with an omitted timestamp can therefore fail a date or chronology check; the writer must explain the mismatch rather than replace the supplied session or backdate the clock. Explicit IDs remain replay inputs subject to the existing uniqueness and namespace checks.

An empty batch writes nothing. A batch whose operations are all no-ops and which contains no `session.log` creates no turn or history. An explicit `session.log` is a requested new turn even if the other operations make no change; it must not be suppressed by a no-op heuristic. No-op mutations produce no derived events, claim touches, or revision rows. Dry runs capture one tentative clock value without creating a lock or session on disk; their time, IDs, and turns are not reserved. Failure before the transaction commit point rolls back all source changes, allocations, derived rows, indexes, and created directories under the existing recovery protocol.

### A1 and A3: require an explicit owner for omitted context

A `session.log` operation is the only batch anchor that authorizes automatic session selection or creation. In the omission workflow it must carry a nonempty caller-written `summary`, and there must be exactly one such log in the batch. No `session.log` is synthesized because a revision happens to need a turn. If `session` is omitted, select the single validated open session on the effective log date; create a session with the supplied summary if there are none. More than one open candidate is an error listing their IDs. Closed sessions are never reopened or selected. This closes Q1.

Work that runs past midnight UTC is the common edge case. Suppose a session from the previous date is still open and the log's effective date is today. The writer then creates today's session and leaves yesterday's open, without closing, merging, or selecting it. The report gives the other open session's ID in an additive `open_sessions` diagnostic, so the caller can close it on purpose. A caller who wants to continue yesterday's session names it and supplies a timestamp with a matching date.

| Operation or context | Fields that may be omitted with the sole log anchor | Fields still supplied by the caller |
|---|---|---|
| `logic.revise` | `session`, `turn` | Target, complete requested change, `signal`, `provenance`, and required source preconditions |
| `entry.rename`, audited `entry.remove` | `session`, `turn` | Target, destination or name, `expected`, `signal`, `provenance`, and any explicit reference edits |
| `paper.edit.audit`, `observation.mark_stale.audit` | `session`, `turn` inside an explicitly supplied audit object | `signal`, `provenance`, optional note; stale reason remains required |
| Reasoning `record.append` | `record.turn` | Complete `notes` |
| `session.log` | `session`, `timestamp`; row-level turn stamps remain CLI-owned | Nonempty `summary` when using omitted ownership or automatic session selection/creation |

The new workflow puts the owner anchor before operations that omit audit context. Resolve its session and reserve the next turn in normal JSONL order, using only the locked snapshot and earlier operations. An earlier explicit `session.start` can supply `$s` to that anchor. Reservation does not make a later `$name` available earlier: unknown, forward, duplicate, and wrong-kind bindings still reject at their original line. Finalize the owner's derived rows and pending audits after the ordered operations have succeeded, then validate the complete candidate. Deferring finalization does not permit forward bindings in caller-authored logs or reference fields.

Fully explicit batches retain their existing order flexibility, including a revision before its owning log when it names a concrete valid new turn. Multiple logs are allowed only with explicit ownership: each audited operation must name exactly one new turn, and any operation-derived event or claim touch must be attributable to exactly one log through its explicit row. A creation missing that attribution in a multi-log batch is an ambiguity error, not a guess based on line proximity. The sole-log workflow attributes its eligible operations to that log regardless of where their creation appears. Omitted audit context with zero or multiple logs fails. Explicit `session` or `turn` values must match the owning log and its allocated next turn; the writer never silently overrides them or attaches a new revision to historical turns.

The convenience `stage`, `add node`, `promote`, taste, and `session log` commands use this same locked defaulting path. Only an explicit `session log` request may trigger A3. Existing fully explicit logging without a new summary keeps its current rolling-summary behavior; the stricter summary requirement applies when asking the writer to derive ownership or choose/create a session.

### A1 also covers the separate merge audit commands

The audit's `merge --turn` shorthand means `merge resolve` and `merge repair`, not the directory/Git import command. They do not pass through `apply`. CLI `merge.rs` takes its own lock, while core `merge/mod.rs::audit` currently verifies an explicit next turn and appends a session log using the session's last timestamp. Changing only `write/batch.rs` would leave this chore unresolved.

Make `--turn` optional on both audit commands. Keep `--session` explicit; these commands do not select or create a session. Under their existing lock and recovery boundary, resolve the named open session and allocate `turn_count + 1` with the same owner allocator as `apply`. A supplied `--turn` must equal that value. Retain required signal, provenance, conflict choice, and protected-repair reason/precondition. Add optional `--timestamp` and `--summary`: omission of timestamp uses the locked clock, while omission of summary preserves the current rolling summary. A historical session therefore needs an explicit date-compatible timestamp. The command itself is the explicit audit action, so it may create its internal log without a separate user JSONL anchor.

The session append, exact resolution audit, conflict ledger decision, indexes, and selected content form one transaction. Preserve the dedicated protected-history repair checks; ordinary resolution gains no permission to edit immutable history. Include resolved `session` and `turn` in the command report as additive fields. Do not infer confirmed/refuted claims or other scientific actions from a conflict choice. This work is in PR 19b, not a deferred follow-up.

### A4 and A5: derive facts, preserve authored judgment

Collect operation facts from successful, non-no-op results. `SessionLog.events` remains the input spelling and writes native `events_logged`. For an automatically derived event, copy the operation's explicit title, or an observation's complete content, as its default summary; do not summarize with a model or truncate it. A supplied matching row may carry a different caller-written summary. Routing comes only from the explicit operation, and provenance comes from the validated new entry or promotion result. Missing provenance cannot be guessed; require a supplied valid value or row before deriving an event.

| Successful operation | Derived event | Derived claim touch |
|---|---|---|
| `node.add` | Allocated N ID, supplied node type, `direct` routing | None |
| `observation.stage` | Allocated O ID, `observation`, `staged` | None |
| `claim.add`, `heuristic.add` in an owned turn | Allocated C/H ID, `claim`/`heuristic`, `direct` | `created` for a claim |
| `observation.promote` | Destination C/H/N ID where available, destination type, `crystallized` | `crystallized` for a claim destination |
| Promotion to concept, constraint, or architecture | Source O ID, destination type, `crystallized`, plus exact destination selector in additive `target` | None |
| `logic.revise` or reference repair changing a claim | None | `revised` unless the caller supplies a valid explicit judgment row |
| Explicit claim merge or split described in C1 | None | `merged` or `split` for the source claim; separately created spin-offs remain `created` |

Promotions carry the source observation's existing provenance rules into the validated destination; the bookkeeping collector does not upgrade it. For the named destinations without numeric IDs, `events[].target` is an optional typed `EntrySelector`, not a generated display ID. Extend the strict event validator, reader, and skill examples together. It must resolve the exact destination and agree with the promotion tuple. Existing rows without `target` retain their meaning; this is an additive row-schema change requiring documentation and protocol review, not a claim that upstream has adopted it.

Deduplicate only within the new owned turn. Event identity is `(session, turn, id, routing, target)` after exact typed identity resolution; absent `target` is distinct except when the corresponding numeric-ID promotion proves the same destination. An explicit row matching that identity must agree on type, routing, provenance, and destination. Preserve its summary verbatim and suppress the derived duplicate. Repeated explicit rows collapse only when the complete row is identical; differing summaries or facts for the same identity are an error with both input locations. A row naming a newly created or promoted entry cannot evade this check by supplying a different routing or target: it must match a supported operation fact for that entry. Nonmatching explicit events for other entries remain subject to the existing reference and type checks. Earlier turns are never searched for a row to replace or delete.

Claim-touch identity is `(session, turn, claim ID, action)`. Repeated identical rows collapse, and distinct valid actions can coexist when several explicit actions occur in one turn. `created` and `crystallized` must match their operation; a conflicting attempt to relabel creation rejects. A supplied judgment such as `advanced`, `weakened`, `confirmed`, `refuted`, or `withdrawn` replaces the generic derived `revised` row for that claim, after vocabulary and explicit Status-change consistency checks. A Status change to `supported` does not itself prove `confirmed` or `advanced`; without the caller's judgment the row is simply `revised`. Contradictory judgments such as confirmed and refuted without distinct supporting explicit transitions reject. Never drop caller actions, context, threads, suggestions, or reasoning because a mechanical row exists.

Keep explicit row order and append any remaining derived rows in operation order. The collector emits one promotion event and suppresses the direct-creation event from its internal helper. Fully specified old requests still validate their supplied facts and receive any missing deterministic rows. Their meaning and old source history are preserved, but newly authored session bytes, counters, reports, and timestamps need not be byte-identical to the old binary's output. Matching complete explicit rows yield no duplicate. Document new ambiguity/conflict errors rather than promise blanket compatibility.

### B: remove duplicate instructions without removing research decisions

Update the local `research-manager-cli` skill and its schema/access references after the associated code lands. Delete B1 through B4's duplicate or rejected writes, replace manual counters with the owner-anchor workflow, and show which row fields the CLI derives. Keep instructions for deciding whether to log a turn, scientific status, closure signals, provenance, explanations, explicit session selection when ambiguous, and standalone audit ownership. Change all bundled copies of shared CLI access instructions together under [the skill ownership contract](../../docs/agent-cli-skills.md).

The original baseline skills and historical-run pins stay unchanged. The early crystallization/current-snapshot contradiction is documented in the existing CLI access contract as a source-skill issue; changing a local access instruction does not settle its upstream disposition or establish an interface-only experimental comparison. Affected cross-repository skill/protocol changes require their owning repository's review and exact adopted revision. Plan 18 owns the broader read-workflow rewrite.

### C1: rewrite mutable references and preserve historical identities

Move or reuse the existing typed-reference classification used by `agent/references.rs`, `write/logic.rs`, and merge rewriting. The current `refs` inventory includes historical references and token-shaped prose, so each match still needs classification before it can be edited. Resolve full selectors and exact reference tokens through the existing identity machinery, then intersect them with the allowed mutable fields below. Do not use whole-document string replacement.

| Location | Automatic rewrite permission |
|---|---|
| Mutable native Markdown entries | Existing accepted `Dependencies`, `Proof`, `Sources`, `Claims affected`, `Related`/`Related concepts`, `Merged into`, `Evidence output`, and `Code ref` fields, only where that entry's schema accepts the field and the typed reference parser identifies the target |
| Mixed prose/reference fields such as `Proof` or `Sources` | Rewrite only an unambiguous parsed reference token; retain quotes, prose, delimiters, and non-reference bytes exactly |
| PAPER frontmatter or arbitrary document bodies | No automatic C1 setter; use their existing explicitly audited operations if a caller decides to change them |
| Trace node `evidence`, `parent`, `also_depends_on`, `same_as`, `concepts`, `source_refs`, artifact pointers, and annotations | Preserve old bytes; resolve citations through a valid retained identity or authenticated redirect |
| Observation `bound_to`, original content, promotion tuple, and prior annotations | Preserve old bytes; promotion's declared pointer transition remains a separate operation |
| Prior session rows, reasoning, taste, mutation/merge ledgers, source evidence, and archived before/after payloads | Never rewrite historical rows; append only the new audit/identity records authorized by this operation |

Unknown fields and possible prose mentions are listed with source locations and left untouched. If leaving a current mention would violate the existing dangling-reference guard, reject with the unresolved locations and require an explicit audited edit. Reporting a skipped citation is not permission to leave a broken identity. Automatic rewriting cannot broaden an allowlist, reinterpret a display address, or use a digest from shortened output.

| Restructure | Concrete operation and caller responsibility |
|---|---|
| Rename | Extend `entry.rename` with optional `rewrite_references: true`. Retain `target`, `name`, exact `expected`, and the audit context. Generate the permitted `ReferenceEdit` rows internally and append the existing authenticated mutation mappings |
| Claim merge | Extend `logic.revise` with `rewrite_references: true` for an explicit `set` containing `Status: withdrawn` and `Merged into: <existing claim ID>`. Retain the source claim and repair its eligible current citers to the chosen survivor in the same transaction. The caller chooses the survivor and content; the CLI does not infer duplicate claims |
| Eligible non-claim removal with replacement | Extend `entry.remove` with the same flag only when an explicit `redirect` is supplied. Keep the existing source archive, identity mapping, and claim-deletion prohibition |
| Claim split | Keep the original ID for the primary claim, create spin-offs earlier in one `apply` batch, then use `logic.revise` on the primary with a new optional `references` array of the existing `{target, field, before, after}` shape. Every current mutable citing field must be classified by an explicit row, including unchanged rows that deliberately retain the primary. `before` is exact field source; `after` supplies the caller's complete mapping to the primary, one spin-off, or several destinations |

For a split, the primary `logic.revise` also supplies `action: "split"` and a nonempty `split_into` array of exact destination selectors. These two optional fields are valid only together, require distinct existing claim destinations, and identify the split in its appended audit. Every rewritten destination must be the retained primary or a declared spin-off. One-to-many replacements are accepted only in fields whose existing type permits a list; a scalar citation requiring several destinations needs a caller-authored content revision. The CLI never chooses which proposition a citer intended. New provisional destinations must be created before the reference operation; forward bindings still fail.

For rename, merge, and removal, explicit `references` and automatic generation are mutually exclusive for the same operation. Each generated row uses the same exact before/after audit path as a caller-written edit. Preserve existing dependency-cycle checks, canonical namespace checks, source digest guards, no-reuse rules, and claim retention. A mapping cannot create a redirect cycle or repurpose an old identity. Claim merge retains the withdrawn source and its `Merged into` relation; split retains the primary, so neither needs a one-to-many historical alias.

Historical citations must remain resolvable through the retained source or the existing authenticated `trace/logic_mutations.yaml` mappings and claim redirects. Check every affected historical typed citation against that mechanism before commit; if a namespace or selector cannot be represented safely, refuse the restructure. Extend resolver consumers where necessary to honor existing authenticated mappings, without inventing a second alias format or rewriting trace evidence and `bound_to`. Existing historical record bytes stay exact; files that receive a new audit or mapping can grow. That is the preservation guarantee, not byte-identical whole historical files.

### D1 and D2: report measured inactivity with its limits

Add nullable `turns_since_reference` and `session_days_since_reference` to observation rows in `open`, with `last_reference_turn`, `reference_basis`, `evidence_sources`, and a `history_status` of `complete`, `missing`, or `ambiguous`. Preserve the `ara.open/v1` envelope and existing item fields. Report an unknown count as `null` with a diagnostic, never zero or an invented large count. Existing stored `stale: true` remains visible even when current history is incomplete.

Build the timeline from validated session records, per-turn stamps, and archived session metadata, not numeric IDs, filesystem times, index totals alone, or lexical session order. Within a session, turn number defines order and timestamp chronology must agree. Between sessions, order provably separated timestamp intervals by instant, retaining the session's written date for day counts. Overlapping sessions, equal cross-session timestamps, missing turn timestamps, missing source sessions, contradictory metadata, and unresolved imported turn identities make affected turn counts unknown. Do not pick a lexical tie-breaker and call it chronology.

Day counts are computed separately from turn counts. A day count needs only each logged turn's written date, plus the date of the latest attributable reference. Overlapping session intervals therefore leave `session_days_since_reference` known while `turns_since_reference` becomes `null`. This matters for [plan 14](14-shared-frontier-intentions.md): concurrent agents produce overlapping sessions by design, and D2's stale check, which counts days, must keep working for them. A day count is unknown only when a relevant turn has no date, or the reference itself has no attributable date.

An explicit reference is an exact observation or bound-node ID/locator in a typed turn field or an exact token in caller-authored turn text. Label those bases separately as `structured` or `literal`; do not treat substring or topic-word similarity as an ID reference. Exclude generated counters, session indexes, archived metadata copies, derived before/after audits, and the stale-evidence record itself from new reference activity. Attribute an old literal through an authenticated redirect when available; otherwise mark the evidence unresolved. Show the matched source paths and turn identities so the caller can inspect what was counted.

Count fully logged turns strictly after the most recent attributable reference through the latest provably ordered logged turn. With complete history and no later reference, use the attributable staging turn as the starting point. If staging has only a timestamp, include only turns provably after that instant; equal or ambiguous positions make the turn count unknown. Missing observation creation evidence also produces `null`. Day counts count distinct subsequent dates with actual logged turns, after the latest attributable observation/bound-node reference date. Empty sessions and elapsed calendar dates without a logged turn do not count.

The five-turn topic-abandonment rule remains a caller judgment: check bound-node activity, topic wording, and current `open_threads` before deciding whether a closure signal exists. A count of five explicit-reference-free turns is not proof of semantic abandonment. Neither `open` nor `mark_stale` promotes, discards, changes scientific status, or infers a reason. Shared evidence extraction for the read and write paths must also stop the current day-based body scan from treating copied audit text as a new event.

For `observation.mark_stale`, make `session_days` optional. Derive the sorted distinct eligible logged dates after the latest attributable reference, up to the owning audit date, using complete validated source evidence. Require at least three eligible days, a caller-written reason, explicit signal/provenance, and the newly owned turn. A supplied list remains a verified evidence subset: reject duplicates, nonexistent days, days outside the cutoff, or fewer than three proven days after last use; do not silently replace it. Omission records the full canonical list, while a valid supplied subset stays exact in the new evidence record. Exclude the stale operation's own log/evidence from proving its prior inactivity. Unknown day evidence refuses the write rather than assuming that silence is proven.

### New-entry rendering: close Q2 with a fixed schema

For newly appended `claim.add` and `heuristic.add` blocks, including promotion-created blocks, use the registry order below regardless of surrounding entries. Do not infer a majority style, sort field labels alphabetically, or reformat existing blocks. Retain accepted aliases at input; canonical new output uses the schema's `Falsification` label, while an existing `Falsification criteria` label remains untouched unless that field is explicitly edited. This closes Q2 without changing the claim schema to match one fixture.

| Entry kind | Order of supplied fields |
|---|---|
| Claim | Statement; Conditions; Sources; Status; Provenance; Falsification; Proof; Evidence basis; Dependencies; Tags; Merged into; Last revised |
| Heuristic | Rationale; Source; Sources; Status; Provenance; Sensitivity; Bounds; Code ref; Tags; Last revised |

Only supplied, permitted fields are emitted; revision-owned fields are not added to a creation. This order applies only to newly created blocks. Where `logic.revise` inserts a field into an existing block, such as `Last revised` or `Merged into`, that placement is out of scope and stays as it is today. Keep the existing accepted types: claim Dependencies are a typed C-ID list, Merged into is a C-ID scalar, and the schema's scalar-or-list fields remain scalar-or-list inputs. Heuristic singular scalar `Source`, optional plural `Sources`, complete `Bounds`, and compiler content must survive without fabricated PM fields. Scalar `Tags` containing commas remains one exact scalar, not an inferred list.

Render ordinary single-line scalar values inline as `- **Label**: value`. Use the existing lossless indented continuation form for multiline values, leading/trailing whitespace, or structural content that cannot round-trip inline. Preserve every caller newline, including the final empty line. Structural line separators follow the file's supported newline convention; source value bytes and all pre-existing document bytes remain unchanged.

Render Dependencies inline as `[]` or `[C03, C04]`. This bracketed list of canonical IDs is the form the reader already parses and that existing claims use; it is unambiguous because the elements are typed IDs. For flexible string-list fields such as Proof, Sources, Tags, and Code ref, the plain form is a comma-separated inline list (`Tags: evaluation, experimental-design`). Use it only when every element is nonempty, has no leading or trailing whitespace, and contains no comma, bracket, quote, backslash, newline, or literal `none`. Even then, use it only if a test shows that the field's reader decodes the plain form back to the identical list. Any other list keeps lossless JSON array encoding rather than comma-joining elements. This preserves commas, brackets, quotes, backslashes, empty strings, literal `none`, Unicode, and embedded newlines. Preserve exact scalar content separately from list encoding in the authoring codec; never reinterpret a scalar that resembles `[]`, `none`, or JSON as a list. Existing read-model fields and source-field text keep their documented representations; tests must cover both decoded source and typed reference projections. Lossless data takes precedence over making every list resemble a hand-written fixture.

## Alternatives considered

Implicitly creating a session whenever any write needs one would hide the decision to log a research turn and leaves no source for the summary. The explicit-anchor rule gives that decision one owner. Requiring `session.start` every day would preserve more manual work without resolving ambiguity better; A3 instead permits creation only after an explicit summarized log request.

Replacing every old ID in every source would corrupt quotations and historical evidence. Typed mutable edits plus authenticated identities preserve the audit trail. Selecting one destination automatically for a split would invent a scientific mapping, so the split remains a caller-authored mapping executed atomically. Keeping all staleness work in the agent avoids timeline logic in the CLI but repeats counting and makes evidence hard to inspect.

Matching the most common field layout would make identical writes depend on unrelated entries and could damage list values. Fixed schema order with lossless list encoding produces a predictable new block without changing old source. Compiler and collective bookkeeping remain separately scoped follow-ups rather than being presented as solved by these changes.

## Tradeoffs

The ordinary single-turn workflow supplies less duplicate data, but multi-log batches and ambiguous histories require explicit ownership or refusal. A caller can still record a scientific action that the CLI cannot prove; validation checks the explicit operation and schema, not the truth of the research judgment. Added rows and timestamps change newly authored bytes, and optional fields alter public Rust request types even where JSON remains additive.

Reference and history scans can cost more than a simple append. Run them only for the relevant restructure or stale query, reuse one snapshot, and attach measurements to the affected implementation PR. Missing chronology may yield more `null` counts than the current loose body scan. That is preferable to a precise-looking count that silently assumes an order.

## Migration

Implement each functional sub-PR against `feat/agent-cli-interface` and squash-merge it under the [parent rollout policy](../agent-cli-interface.md#order-of-work). Each sub-PR includes its patch bump in `Cargo.toml`, corresponding local crate versions in `Cargo.lock`, an `Unreleased` changelog entry, command/skill documentation, and behavior tests. After a version bump, refresh the lockfile with a non-locked workspace command before the final locked checks. Apply the shared viewer freshness rule when core behavior affects embedded output; a manifest-only freshness hash is not evidence that changed core code reached the bundle.

| Sub-PR | Implementation scope and evidence |
|---|---|
| 19a | Reproduce the reported creation-layout case in `crates/ara-cli/tests/agent_writes.rs`; fix `write/fields.rs` and creation renderers. Test all claim/heuristic fields, accepted aliases, compiler scalar Source/Bounds, empty and delimiter-containing lists, multiline/Unicode/CRLF values, and byte preservation outside the new block. Exercise the real `claim add`, `heuristic add`, promotion, and `show` paths |
| 19b | Implement A0 through A3 in core batch/session/record/node planners and CLI adapters, including both merge audit commands. Test with injected UTC clocks: midnight and offsets, past-midnight open sessions, explicit timestamp and session mismatch, zero, one, and multiple open sessions, anchor order, every omission scope, multi-log ownership, overflow, closed sessions, and forward or wrong-kind bindings |
| 19c | Implement A4 and A5 on top of 19b's owner anchor, then the B skill changes. Test row identity, caller summaries, provenance and judgments, every promotion destination kind including the additive `target`, no-op behavior, and explicit and omitted requests that choose equivalent defaults. Update the skill and access copies in the same PR so they never describe rows the binary doesn't yet derive |
| 19d | Share explicit-reference evidence for `open` and stale writes. Test known turn and day counts, staging boundaries, bound-node use, copied audit text, semantic-only mentions, multiple sessions, overlapping sessions (turn count `null`, day count known), missing timestamps, imported histories, empty sessions, invalid evidence lists, and unchanged stored stale flags. Confirm `null` with a diagnostic for unknown chronology, and no automatic promotion |
| 19e | Implement C1 through typed inventories, mutation identities, and exact audit edits. Test rename, retained claim merge, eligible non-claim redirect, and mapped split. Include immutable tree evidence and `bound_to`, aliases, duplicate leaf headings, delimiter-containing headings, prose ambiguity, unknown fields, cycle prevention, source-digest conflicts, and refusal where history cannot resolve. Assert existing historical spans are exact and new audits identify every actual mutation |

Land them in this order. 19a has no dependencies and fixes a visible bug, so it goes first. 19c needs 19b's owner anchor. 19d and 19e are independent of each other, but 19e has the widest blast radius (it rewrites existing source) and the least frequent use, so it goes last and gets reviewed on its own.

Every sub-PR must include invariant and failure-atomicity tests, not only successful examples. Inject a failure after tentative allocation or reference planning and verify unchanged source bytes, indexes, history, and directory existence. Exercise lock contention and allocation under cooperating writers, rollback/recovery, and dry-run non-persistence for affected transaction paths. Match error code, physical JSONL line, field, and CLI exit behavior. Use semantic parser and actual CLI tests; source-text searches or assertions that a function contains a string are not evidence of behavior.

Use the shared workspace test, formatting, Clippy, and relevant fixture/smoke gates after each implementation is complete. Attach actual CLI command/output evidence on temporary artifacts, including fully specified requests, new omitted-field requests, refusal cases, and a write/read round trip. Freeze historical baselines and original run pins; a development smoke or improved bookkeeping does not validate scientific accuracy, token savings, an unchanged historical replay, or an experimental comparison.

### What counts as acceptance?

1. A summarized, explicitly logged single turn can omit session choice, turn arithmetic, supported clock fields, and operation-derived rows. Every A1 audit context and both separate merge audit commands have an implemented ownership rule.
2. B1 through B4 no longer ask the local CLI skill to author a value the writer already owns or rejects. The skill still requests judgment, summaries, explicit ambiguity resolution, and standalone audit ownership where required.
3. C1 updates every eligible mutable citation or refuses atomically with the unresolved locations. Historical citations remain resolvable without changing old trace, staging, or audit payloads; splits use caller mappings.
4. D1/D2 report inspectable explicit-reference evidence, distinguish unknown from zero, retain day-count validation, and never infer semantic abandonment or promotion.
5. New claims and heuristics have canonical field order and lossless values. Existing source bytes remain unchanged outside intended appends and explicitly requested edits.
6. Repeat the six-skill audit with a recorded prompt, exact skill/CLI revisions, and runnable examples. Account for every A through D item as implemented derivation, preserved research judgment, or a documented explicit ownership requirement. Do not claim that compiler/collective follow-ups or all research bookkeeping are solved.

## Follow-ups outside this approval

| Group | Work retained from the original draft |
|---|---|
| E, compiler bookkeeping | Verify cited quote bytes at `path:line`; consider Seal L1 structural checks as an `ara check` profile; generate PAPER Layer Index and evidence-index rows/counts; review compiler-mode creation APIs without PM-only fields; consider automatic `knowledge_paths` registration with explicit access intent |
| F, collective bookkeeping | Specify a `knowledge_revision` digest and frontier facts with digests; correct any skill claim that an unimplemented field already exists through its own reviewed contract |

Existing compiler field fidelity is a constraint on 19a, even though additional compiler commands remain in E. Upstream protocol/skill adoption and any new experiment registration stay pending in their owning repositories. These follow-ups are not a reason to weaken A through D's accepted scope.

## What changed in the 2026-10-05 revision

The review compared the 2026-10-04 approval against the code at `aeb5e0d`, version 0.1.24. The design's scope and boundaries hold. These are the changes:

1. **Layout bug reproduced.** The `claim add` report is now confirmed against the real binary, not just carried over from the draft audit.
2. **Dependencies use the existing bracket form.** The draft's `none` / comma form didn't match how existing claims or the reader write this field. Plain comma lists for string-list fields are allowed only when a reader round-trip test proves them lossless.
3. **Clock injection is specified.** There is no injectable clock today; `write.rs::now()` reads the time before the lock is taken. The clock moves behind the lock as a core parameter, with no test-only override in the shipped binary.
4. **Past-midnight sessions are defined.** Previously this case was implied but never stated. An open session from the previous day is left open and reported, not reused or closed.
5. **Day counts survive overlapping sessions.** The draft let any interval overlap null out both counts, which would have disabled stale detection for plan 14's concurrent agents.
6. **Placement of fields that revisions insert is out of scope.** The fixed order covers new blocks only.
7. **19b is split, and C1 lands last.** Ownership and clock (19b) are now separate from derived rows and skills (19c), staleness is 19d, and reference rewriting is 19e. The old 19b mixed two separately reviewable risks, and the skill text would have run ahead of the binary.

## Next Steps

1. Implement 19a through 19e in order with their evidence and shared checks; keep this approval distinct from feature completion.
2. Update local commands, all affected CLI skill/access copies, schemas, and release notes alongside each shipped behavior. Record exact revisions for any separately reviewed upstream adoption.
3. Attach the closing audit and actual CLI verification. After every acceptance item is met, move the completed design record into `docs/agent-cli-interface/` and remove this plan under the repository's planning policy.
