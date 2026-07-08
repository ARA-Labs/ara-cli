# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
- Bumped workspace version `0.0.0 → 0.0.1`.
