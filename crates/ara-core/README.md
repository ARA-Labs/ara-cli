# ara-core

Shared core of the [ARA viewer runtime](https://github.com/ARA-Labs/ara-cli): parsing,
normalization, binding resolution, and layered DAG layout. Compiled to both native
and `wasm32-unknown-unknown` so the server and browser client share one
implementation.

Parses `trace/exploration_tree.yaml` (+ optional `logic/claims.md`) into one
normalized `Manifest { nodes, links, bindings, claims }`. `parse_sources` is pure
and wasm-safe; `parse_dir` (native feature) reads an artifact directory. DAG
layout lands in a later stage.

```rust
let (manifest, report) = ara_core::parse_sources(tree_yaml, Some(claims_md))?;
```

License: MPL-2.0
