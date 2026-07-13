# ara

[![CI](https://github.com/ARA-Labs/ara-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/ARA-Labs/ara-cli/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/ara-cli.svg)](https://crates.io/crates/ara-cli)
[![docs.rs](https://img.shields.io/docsrs/ara-core)](https://docs.rs/ara-core)
[![Downloads](https://img.shields.io/crates/d/ara-cli.svg)](https://crates.io/crates/ara-cli)
[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)
[![MSRV](https://img.shields.io/badge/MSRV-1.85%2B-blue.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

Blazing-fast **ARA viewer** — a Rust runtime that parses, serves, and renders
Agent-Native Research Artifacts (ARAs) as an interactive, drill-down DAG in the
browser.

`ara` is the **official ARA-Labs** runtime for the ARA format. Point it at any
ARA directory and it validates, lays out, and serves an interactive viewer
locally — no hub account, no build step, no LLM calls at view time.

> Status: released, `0.1.x`. `ara validate`, `ara layout`, and `ara serve`
> (live-reloading web viewer) all work. See `docs/` for the shipped-stage
> design write-ups.

## Why ara?

ARA-Labs hosts a hub where each artifact is published with its own pre-baked
viewer page. `ara` is the **local-first, deterministic viewer** you run
yourself — the same runtime is being prepared to power the hub. Rendering the
YAML directly instead of a baked page is a deliberate design decision:

- **Renders the YAML directly — never calls an LLM at view time.** Reference ARA
  viewers ship a static, upstream-baked HTML page per artifact (prose generated
  once, then frozen). `ara` reads `exploration_tree.yaml` + `claims.md` and
  renders them **deterministically** every load, so the view is byte-reproducible
  and always matches the source on disk. Missing upstream prose degrades
  gracefully to the structured fields — it is never faked at view time.
- **One shared Rust core, no parser drift.** `ara-core` compiles to **both**
  native and `wasm32`. The exact code that `ara validate` checks on the CLI is
  the code that lays out and renders in the browser — validation and view can't
  disagree.
- **Tolerant of the *real* corpus, not just the two published examples.** The
  parser ingests messy, hand-authored artifacts without panicking: unknown
  fields become warnings (not hard errors), `children` + `also_depends_on`
  cycles are detected, and source order is preserved. See
  `docs/ara-format-feedback.md`.
- **Small, fast, accessible client.** Leptos (CSR) + SVG ships a sub-megabyte
  wasm bundle with **selectable, searchable (Ctrl-F), ARIA-accessible** text and
  native browser zoom — things a canvas/WebGL viewer can't give you. Node kind
  is encoded by **glyph + label, not colour alone**, so the graph stays readable
  for colour-blind users; only dead-ends use a warning colour.
- **Deterministic layered DAG layout.** The Sugiyama-style layout runs inside
  `ara-core` and is byte-stable across native and wasm, which makes it
  snapshot-testable — the same artifact always draws the same graph.
- **Local-first, self-hostable, single binary.** `ara serve` watches the ARA
  directory and live-reloads on change (preserving pan/zoom/selection). The
  browser frontend is embedded into the `ara` binary, so there is one artifact to
  ship and no external services to run.
- **Open, no lock-in.** MPL-2.0, no telemetry, and format development happens in
  the open (`docs/ara-format-feedback.md`).

Use `ara` when you want to explore ARAs **on your own machine**, keep the
rendering faithful to the source, script it into CI/validation, or view
artifacts that never went through the hub.

## Workspace

| Crate         | Kind        | Role                                                         |
| ------------- | ----------- | ------------------------------------------------------------ |
| `ara-core`    | lib         | Shared parse + normalize + layout; builds native **and** wasm |
| `ara-cli`     | bin (`ara`) | Command-line runtime (`ara validate`, `ara serve`)            |
| `ara-wasm`    | cdylib/rlib | `wasm-bindgen` interop for the Leptos browser client         |
| `ara-viewer`  | bin         | Leptos/SVG browser frontend, embedded into `ara-cli` for `ara serve` |

## Install

```bash
cargo install ara-cli   # ships the `ara` binary
ara --help

ara validate path/to/ara-dir   # parse + validate an artifact directory
ara serve    path/to/ara-dir   # serve the live-reloading web viewer
```

## Run locally

From a clone of this repo, run the CLI through Cargo. The workspace has two
binaries (`ara` and `ara-viewer`), so pass `-p ara-cli` (or `--bin ara`) to tell
Cargo which one to run:

```bash
cargo run -p ara-cli -- validate path/to/ara-dir
cargo run -p ara-cli -- serve    path/to/ara-dir   # http://127.0.0.1:8080
```

### Example artifacts

A corpus of real ARAs is wired in as a git submodule
([`AmberLJC/ara-paperbench`](https://github.com/AmberLJC/ara-paperbench)). It is
**not** checked out by default — a fresh clone stays cheap and required CI does
not need it. Fetch it once when you want something to view:

```bash
git submodule update --init corpus-external/ara-paperbench
```

Then point `serve` at any artifact directory under it:

```bash
cargo run -p ara-cli -- serve corpus-external/ara-paperbench/artifacts/paperbench/pinn
```

Open http://127.0.0.1:8080 and the viewer live-reloads as you edit the artifact.
Any directory under `corpus-external/ara-paperbench/artifacts/` (e.g.
`paperbench/*`, `rebench/*`) is a valid ARA to serve.

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo run -p ara-cli
```

## Reserved crate names

The `ara-*` names above are the working crates. The `bara-*` names
(`bara-core`, `bara-cli`, `bara-wasm`, `bara-viewer`) are reserved defensively
and redirect here.

## License

[MPL-2.0](LICENSE).
