# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Deterministic layered DAG **node** layout in `ara-core` via `dagre-dgl-rs`;
  positions + bounds added to `Manifest`; `ara layout <dir> --json` and
  `ara validate --layout`. Edge routing deferred to the client; geometry wire
  shape frozen, logical model kept additive.
- Real-ARA no-panic regression coverage: vendored `ara-paperbench` subset under
  `crates/ara-core/tests/fixtures/corpus/` with an always-on test asserting the
  parser never panics and always produces a `ParseReport`; opt-in submodule
  full-sweep test (`RUN_CORPUS_SWEEP=1`) over all 34 real artifacts (#3).
- `ara-core` YAML parser (`serde-saphyr`) with dual-dialect (`tree:`/`root:`)
  normalization to a `Manifest { nodes, links, bindings, claims }`, source-order
  preservation, cycle detection, Markdown claim parsing + binding resolution, and
  tolerant unknown-field capture. Pure `parse_sources` (wasm-safe) plus a native
  `parse_dir`.
- `ara validate <dir>` CLI with `--json` and `--strict`.
- Pinned fixtures copied from the two official ARA examples, plus synthetic and
  broken error-path fixtures and `insta` JSON snapshots of both manifests.

- Cargo workspace scaffold with crates `ara-core`, `ara-cli` (binary `ara`),
  `ara-wasm`, and `ara-viewer`.
- Reserved crate names on crates.io at `0.0.0`: the working `ara-*` crates and
  the defensive `bara-*` placeholders (`bara-core`, `bara-cli`, `bara-wasm`,
  `bara-viewer`).
- Root README documenting the workspace layout and install path.
- CI workflow (`fmt`, `clippy`, `test`, `wasm-build`) on GitHub Actions with a
  pinned Rust toolchain (`1.94.1`), SHA-pinned actions, `--locked` builds, and
  Dependabot for `cargo` + `github-actions`.
- `rust-toolchain.toml`, `rustfmt.toml`, and a `CONTRIBUTING.md` documenting the
  local pre-PR checks that mirror CI.
- `TODOS.md` tracking deferred work (MSRV job, wasm-target clippy, `docs/`).

### Changed
- Dropped the unverified `rust-version = "1.85"` MSRV declaration until it is
  tested at the `0.1.0` publish.
- Bumped workspace version `0.0.0 → 0.0.1`, then `0.0.1 → 0.0.2` (Stage 1),
  then `0.0.2 → 0.0.3` (issue #3), then `0.0.3 → 0.0.4` (Stage 2).
