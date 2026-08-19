# published-fields-wrong-kind fixture — source & attribution

A **synthetic** matrix, not an upstream copy: each scoped body field
(`experiment`: `result`/`exploration`/`outcome`/`status`; `decision`:
`choice`/`alternatives`/`rationale`; `dead_end`:
`hypothesis`/`failure_mode`/`why_failed`; `pivot`:
`prior_direction`/`new_direction`/`reason`; plus `lesson`, which is scoped to
`dead_end`/`pivot`) is placed on a node kind that does not project it. Every
node must produce exactly one drop warning ("field `<field>` dropped for type
`<kind>`") and no lint diagnostics — nothing here matches a fixable alias rule.

Field placement follows the published-fields alignment plan's matrix (step 8)
and mirrors the parse.rs unit-test table.

- **Upstream repo:** https://github.com/ARA-Labs/Agent-Native-Research-Artifact
- **Pinned commit:** `e26f5882be4cfc1079573dfe463e75b0dc83dba0` (pin kept
  aligned with the sibling `published-fields/` fixtures; this fixture has no
  upstream counterpart — it is authored against the ara-cli node model)
- **License:** MIT (fixture authored in this repo; pin records the contract
  revision it tests against)
