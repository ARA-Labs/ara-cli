# PR 115 deterministic bookkeeping review fixes
**Date:** 2026-10-06

## TL;DR

The nine correctness findings in [PR 115](https://github.com/ARA-Labs/ara-cli/pull/115) now have behavioral regressions and real-CLI smoke coverage. Reference history distinguishes imported occurrences from native continuation and refuses stale writes when aliases or import origins cannot be established. Citation repair preserves protected examples, selected Body changes check Status judgments, and each distinct structural action receives its audit row. The patch version remains 0.1.25 because this PR already contains its required bump from 0.1.24.

## Problem

Review 5423733520 on commit 533a385 reported nine defects that the passing workspace suite did not cover. Three affected observation inactivity, and six affected citation classification, selector bindings, or claim audit history. The review's real-binary reproductions supplied the failure evidence; new tests encode the required behavior without rerunning those experiments just to confirm them.

## Constraints

The fixes restore the approved plan 19 contract without adding features. Historical source bytes remain unchanged, failed batches write nothing, and `open` and stale validation use the same timeline. Existing scalar locator behavior, caller-judgment precedence, literal repair text, and deliberately authored content changes remain supported.

The implementation does not infer scientific judgments, resolve topics semantically, rewrite archived text, or guess an imported field's origin from its enclosing session. It does not complete the separate six-skill closing audit required by the original plan.

## Proposed approach

The existing writer, selector walker, claim ledger, and shared citation rules remain the implementation points. The fixes are grouped below by the invariant they restore.

| Review comment | Implementation | Restored behavior |
|---|---|---|
| 4191480467 | `write/history/collect.rs`, `write/history/mod.rs` | Reference-bearing YAML aliases make both counts unknown with `history.reference_alias`; stale writes refuse that evidence. Scalar summaries, rolling lists, archives, and turn arrays receive the same treatment. |
| 4191480475 | `write/history/origins.rs`, timeline input callers | Strictly decoded merge revisions, frozen source files, occurrence mappings, and matching aliases establish field/token origin. Preserved imported originals follow aliases; relocated tokens and native continuation retain local identity. Missing/conflicting proof is `history.origin_unknown`. |
| 4191480480 | `write/citation_rules.rs` | Only outer JSON item quotes are list syntax. Inner quotes/backticks are protected using the existing span scanner; escaped items remain wholly protected. |
| 4191480485 | `write/logic.rs` | A selected claim's complete Body replacement compares its own decoded Status before/after and records a changed explicit Status for the judgment check. |
| 4191480490 | `write/history/collect.rs` | `query::scan_tokens` supplies complete-token boundaries; scoped IDs, paths, filenames, and padded near-matches do not name local observations. |
| 4191480495 | `write/batch.rs` | `split_into[]` and `references[].target` use the standard ordered selector-binding helper with indexed errors. Repair `before`/`after` remains literal. |
| 4191480500 | `write/bookkeeping.rs` | Every distinct structural `(claim, action)` is derived at its first operation, in order. Generic `revised` is used only without structural actions; caller judgments retain precedence. |
| 4191480504 | `write/logic/restructure.rs` | Explicit merge/split repair rows reject increased normalized self-citation counts. Existing self-citations and unchanged classification rows are retained; separate content revisions remain allowed. |
| 4191480509 | `write/logic/history_refs.rs` | Structured heading vectors match literal segments even with `/` in one segment. Scalar locator/concept forms retain joined-path matching. |

Internal review added coverage for multi-backtick code delimiters, aliased reasoning containers, malformed occurrence mappings, and fields containing both numeric and relocated session references. Captured rows now require matching original identity, layer, document, and destination. Origin proof uses the same complete scalar relocation as merge, with report generation disabled. Mapping and reasoning-ID indexes avoid repeated full scans of captured history; no scaling benchmark was run.

Follow-up review 4192277038 found that a valid second import was rejected by the superseded first capture of a mutable session summary. The capture pass now walks revisions newest-first and records mutable session metadata and rolling fields once per `(source_key, original session owner)`. Every revision still receives mapping authentication, every append-only occurrence remains captured, and competing source keys still require compatible proof. Selecting the latest entire owner snapshot also avoids reviving removed rolling fields from older revisions.

Review 4192392994 identified a missing boundary when the newest owner snapshot omits an optional rolling field but a committed mutable conflict retains the destination list. The selected owner's absent mutable fields now register imported prefixes without restoring older value captures. Surviving text with no current value proof remains unknown; it cannot fall through to native identity. Conflict resolution and the newest-summary precedence are unchanged.

## Alternatives considered

Resolving YAML aliases would require an anchor table, bounded traversal, and cycle guards across every reference-bearing path. The collector instead reports unknown evidence, which the review explicitly allowed and which prevents a silent stale write without introducing a second YAML expansion mechanism.

Session-wide aliases cannot distinguish imported rows from native rows appended later. Alias-target membership also cannot distinguish a preserved original from an already-relocated value. The origin reader therefore reuses the existing strict merge ledger decoder and its captured source/mapping evidence rather than introducing an unauthenticated heuristic or changing historical bytes.

## Tradeoffs

Alias-only legacy imports have unknown history. Imported scalar changes that cannot be proven as unchanged captured text or an exact supported token relocation also remain unknown. Missing proof cannot authorize a stale mutation.

A crate-private `merge::Record` re-export lets history reuse the strict decoder. The pure scalar relocation helper shares merge's complete rules without allocating audit facts for a read. Matching backtick delimiter runs strengthens protected-code handling in the shared scanner. The history constructor accepts merge-log bytes so the reader and writer use the same origin evidence; no compatibility wrapper remains.

## Migration

`Timeline::build` now receives history sources, optional alias bytes, and optional merge-log bytes. LSP references identified the two consumers (`write/staging.rs` and `ara-cli/src/agent.rs`) and the `write_history.rs` helper; all were migrated. The public command contract is documented in [agent-cli.md](../agent-cli.md), and the unreleased changelog records the fixes.

## Verification

Four regression targets compiled and exposed all nine findings before implementation: 19 behavioral failures and 11 passing controls across 30 tests. The initial history setup was corrected to avoid a pending journal attached to hand-edited source, resolve the actual imported observation ID, and remove scenarios that the YAML parser already rejects before `open`.

After the internal-review fixes, the review targets plus `write_history` passed 61 tests. The final locked workspace passed 1,116 tests with one ignored test. Clippy, formatting, the 16 acceptance-script unit tests, and embedded-viewer freshness all passed.

```sh
cargo test -p ara-core -p ara-cli --test '*review*' --test write_history --features ara-core/native --no-fail-fast
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
python3 -m unittest discover -s scripts -p 'test_agent_cli_acceptance.py'
scripts/embed-viewer.sh --check
```

A separate throwaway script exercised `target/debug/ara` on fresh artifacts through 15 scenarios. It used actual directory merge followed by a native log, both with and without colliding session identities, rather than a seeded ledger. The table records observed CLI output and persisted source state.

| CLI scenario | Observed result |
|---|---|
| Aliased recent excerpt | `history: ambiguous`, days `null`; stale rejects with `write.stale_history_unknown`. |
| Aliased reasoning entry, entries sequence, metadata, or after mapping | Both counts `null`; stale rejects with `write.stale_history_unknown`, or the owner log rejects the aliased entries collection with `write.unsupported_source`. Native bytes remain unchanged. |
| Wrong imported mapping layer or document | `history.origin_unknown`; stale rejects with `write.stale_history_unknown`. |
| `peer:O95` and `results/O95.csv` | Four inactive turns and three days remain; the otherwise-valid stale decision commits. |
| Actual merge then native `N01` reference | Local O95 resets at turn 2; imported observation remains at turn 1 with one later turn; local stale decision rejects with `write.observation`. |
| JSON list containing single/multiple-backtick code spans, quoted C09, and plain C09 | `refs` lists one structured citation; merge repairs only the plain item. |
| Selected Body supported to refuted | Caller `confirmed` rejects atomically with `write.claim_touch_conflict`; caller `refuted` commits. |
| Bound spin-off/citer, split then merge | Bindings allocate C18/C19; source touches are `created`, `split`, `merged`, in order. |
| Explicit repair to survivor's own ID | `write.reference_rewrite`; all native bytes unchanged. |
| Literal slash heading audit then nested rename | Nested heading renamed; literal slash heading and its prior revision remain intact. |

Permanent regressions are `ara-cli/tests/{history_review_regressions,agent_citation_review_regressions,agent_restructure_review}.rs` and `ara-core/tests/{write_bookkeeping_review,write_history}.rs`. The throwaway smoke script and fixtures are not shipped.

The follow-up regression performs two real directory merges with the same source key and the exact first peer snapshot as the second base. It covers an obsolete zero-token summary and a different one-token summary, both updated to `Revisited N01` and relocated to `Revisited N02`. Before the fix, the zero-token case reported `history.origin_unknown` despite two successful merges. After the fix, both cases keep local and peer history complete and allow the three-day stale decision. The focused history targets passed 34 tests.

A separate real-binary smoke passed both summary cases. It also confirmed that the earlier merge-ledger bytes remain intact after the second import. The temporary script and its fixtures were removed after verification.

The follow-up locked workspace passed 1,117 tests with one ignored test. Clippy, formatting, the 16 acceptance-script tests, and embedded-viewer freshness passed again.

The deleted-field regression imports protected `open_threads` text with a local/peer N01 collision, appends a token-free local note, and removes the peer field before the second merge. It uses a fresh native destination snapshot to avoid an old private recovery journal; alias and merge-ledger bytes are copied unchanged. Before the boundary fix, the second merge committed a mutable conflict and `open` credited the retained peer text to local N01 with complete history. After the fix, both counts are null with `history.origin_unknown` and a stale batch refuses atomically. The focused history targets passed 35 tests, including both latest-summary cases.

A separate actual-binary smoke confirmed initial peer attribution, the committed conflict, retained text, unknown local/peer counts, unchanged earlier ledger bytes, and atomic stale refusal. Its temporary script and fixtures were removed.

The deleted-field fix passed the locked workspace with 1,118 tests and one ignored test. Clippy, formatting, the 16 acceptance-script tests, and embedded-viewer freshness passed.

## Next Steps

The review loop replies to every original thread with the fix commit and exercised evidence, then monitors follow-up feedback until approval. The original plan's six-skill audit remains a separate release gate.
