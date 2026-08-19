# published-fields fixture — source & attribution

A **reduced, hand-trimmed slice** of the canonical `the-ara-of-ara` example from
the Agent-Native Research Artifact project, covering every published field added
for issue #75: `provenance`/`timestamp` on all nodes,
`exploration`/`outcome`/`status`/`result` on `experiment`, and
`prior_direction`/`new_direction`/`reason`/`lesson` on `pivot`. It must parse
and lint with **zero errors and zero warnings** (`ara check` PASS).

- **Upstream repo:** https://github.com/ARA-Labs/Agent-Native-Research-Artifact
- **Pinned commit:** `e26f5882be4cfc1079573dfe463e75b0dc83dba0` (upstream `HEAD`
  at fixture-creation time, 2026-08-19; the first pin where `the-ara-of-ara`
  carries the full published-fields contract)
- **License:** MIT (see the upstream `LICENSE`)

## Source map

| Fixture file | Reduced from (upstream path) |
|--------------|------------------------------|
| `trace/exploration_tree.yaml` | `examples/the-ara-of-ara/trace/exploration_tree.yaml` — node N02 reduces upstream experiment N95 (exploration/outcome trimmed; `status` added per the published-fields contract; `result` adapted from upstream N28), node N03 reduces upstream pivot N88 (`prior_direction`/`new_direction`/`reason`/`lesson` trimmed), node N01 is an authored question root so the slice is a single tree |
| `logic/claims.md` | `examples/the-ara-of-ara/logic/claims.md` — C01 reduces upstream C01, C02 reduces upstream C04, both trimmed to the `Statement`/`Status`/`Dependencies` bullets |
| `evidence/README.md` | `examples/the-ara-of-ara/evidence/README.md` — authored to the file-index contract (the upstream README carries prose result tables, not file-index rows); numbers in the `results/` row reduce the upstream understanding-evaluation table |
| `evidence/results/main_result.md` | reduces the overall row (n=450: ARA 93.7% vs baseline 72.4%) of the upstream understanding-evaluation table in `examples/the-ara-of-ara/evidence/README.md` |
| `evidence/proofs/lemma1.md` | authored proof sketch grounding C02; no upstream counterpart |

One deliberate deviation from upstream: upstream experiment nodes carry
`lesson:`, which this toolchain scopes to `dead_end`/`pivot` (a `lesson` on an
`experiment` would draw a wrong-kind drop warning). The reduced experiment node
omits it so the fixture stays warning-free.
