# Plan 19 bookkeeping: small CLI experiments

`run.sh` runs the release `ara` binary on throwaway copies of
`crates/ara-core/tests/fixtures/agent-cli` and prints every request and
response. `output.txt` is one run of it on 2026-10-06 (temporary paths
replaced with `$WORK`). Rerun with:

```sh
cargo build --release && docs/verification/plan-19-bookkeeping/run.sh
```

Each experiment starts from a fresh copy with `trace/sessions` removed. The
fixture's historical session index predates the writer's index validator, so
any `session.log` on the unmodified fixture fails with `write.session` on
field `date`. That happens on 0.1.24 too and is not part of plan 19.

| Step | Experiment | Result |
|---|---|---|
| 19a | The reported `claim add` with `Falsification criteria`, `Proof=[]`, `Dependencies=[]`, `Tags=["x"]` | New block in schema order (Statement, Conditions, Status, Provenance, Falsification, Proof, Dependencies, Tags), all values inline |
| 19b + 19c | One batch: summarized `session.log`, `node.add`, `observation.stage`, `claim.add`, `logic.revise` with no session or turn | Session created for today. `events_logged` gets N124/question/direct, O95/observation/staged, C17/claim/direct. `claims_touched` gets C17 `created` and C04 `revised`. `logic_revisions` has the C04 Status change |
| 19b | `logic.revise` omitting session/turn with no `session.log` | `write.owner_required`, line 1, exit 1 |
| 19b | Two open sessions today, then a `session.log` without `session` | `write.session_ambiguous` at physical line 2, naming both sessions |
| 19c | Caller `refuted` while the turn writes Status `weakened` | `write.claim_touch_conflict`; caller `weakened` is accepted and replaces the derived `revised` row |
| 19d | Observation staged on 10-01, then three unrelated turns on 10-02..10-04 | `open`: 3 turns, 3 days, last reference `2026-10-01_001#1` (structured), history `complete` |
| 19d | `observation.mark_stale` without `session_days` | Commits; evidence records `session_days: [2026-10-02, 2026-10-03, 2026-10-04]` and last reference 2026-10-01 |
| 19d | A later session summary naming O95 | Counts reset to 0 with a `literal` basis |
| 19d | The fixture's legacy reasoning log (names sessions that no longer exist) | O01: turn count `null`, day count 5, history `missing`, diagnostic `history.session_missing` |
| 19e | Merge C09 into C01 with `rewrite_references` | C09 kept as withdrawn with `Merged into: C01`; C11 and C12 Dependencies repaired to C01; `claims_touched` C09 `merged`, C11/C12 `revised`; `ara check` passes |
| 19e | Merge C13 into C14 when C14 depends on C13 | `write.reference_rewrite` with the C14 location; nothing written |

These are engineering smokes on one fixture. They don't measure token savings
or agent behaviour, and they are not the six-skill audit that plan 19's
acceptance item 6 still requires.
