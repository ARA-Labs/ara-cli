# published-fields-unknown fixture — source & attribution

A byte-copy of the sibling [`../published-fields/`](../published-fields/)
fixture (same upstream pin and source map — see
[`../published-fields/SOURCE.md`](../published-fields/SOURCE.md)) with exactly
one injected defect: a `bogus_field:` key on node N02 in
`trace/exploration_tree.yaml`.

Purpose: the fail-closed check for the published-fields contract — the parse
layer must warn "unknown field `bogus_field`" (non-strict `ara check` still
passes), and `ara check --strict` must promote that warning to a failure.

- **Upstream repo:** https://github.com/ARA-Labs/Agent-Native-Research-Artifact
- **Pinned commit:** `e26f5882be4cfc1079573dfe463e75b0dc83dba0` (inherited from
  the sibling; the `bogus_field` key itself is synthetic, not upstream)
- **License:** MIT (see the upstream `LICENSE`)
