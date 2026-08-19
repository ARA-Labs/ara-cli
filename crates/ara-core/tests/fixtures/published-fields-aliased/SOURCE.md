# published-fields-aliased fixture — source & attribution

A **reduced slice** of the canonical `the-ara-of-ara` example with the pivot
node's body keys deliberately rolled back to the pre-canonical aliases
(`from`/`to`/`trigger` in place of
`prior_direction`/`new_direction`/`reason`). ARA005/ARA006/ARA007 must fire on
it, and `ara check --fix` must recover the values into the canonical keys; a
second `--fix` run is a no-op.

- **Upstream repo:** https://github.com/ARA-Labs/Agent-Native-Research-Artifact
- **Pinned commit:** `e26f5882be4cfc1079573dfe463e75b0dc83dba0`
- **License:** MIT (see the upstream `LICENSE`)

## Source map

| Fixture file | Reduced from (upstream path) |
|--------------|------------------------------|
| `trace/exploration_tree.yaml` | `examples/the-ara-of-ara/trace/exploration_tree.yaml` — node N01 reduces upstream pivot N88, values trimmed to one line each, with the canonical keys rewritten to their pre-rename aliases `from:`/`to:`/`trigger:` |
