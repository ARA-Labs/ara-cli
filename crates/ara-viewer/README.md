# ara-viewer

The browser client for the [ARA viewer runtime](https://github.com/ARA-Labs/ara-cli)
— *bara*, the blazing-fast ARA viewer. A [Leptos](https://leptos.dev) CSR
(client-side-rendered) WebAssembly app that renders an Agent-Native Research
Artifact's exploration graph as an interactive, drill-down DAG, skinned to the
published ARA design.

It consumes the `Manifest` produced by `ara-core` (via `ara layout <dir> --json`)
and renders the DAG from Stage-2 layout positions.

## Develop

Built with [Trunk](https://trunkrs.dev):

```bash
cargo install trunk
cd crates/ara-viewer
trunk serve          # dev server with hot reload at http://127.0.0.1:3000
trunk build --release # optimized wasm bundle into dist/
```

The CLI tool (`ara`) ships separately from the `ara-cli` crate.

License: MPL-2.0
