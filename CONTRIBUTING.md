# Contributing

## Local checks before opening a PR

CI runs these exact commands; run them locally first to get a green PR on the
first try. The pinned toolchain (`rust-toolchain.toml`) and the
`wasm32-unknown-unknown` target are installed automatically by `rustup` on the
first `cargo` invocation.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked
```

Notes:

- **`--locked`** makes CI fail if `Cargo.lock` is out of date. After bumping a
  crate version or changing a dependency, run a plain `cargo build` to refresh
  `Cargo.lock` and commit it in the same PR.
- **Lints:** clippy warnings are errors in CI (`-D warnings`). Local `cargo
  clippy` without `-D warnings` only warns, so rely on the command above.
- **wasm:** `ara-core` and `ara-wasm` must build for `wasm32-unknown-unknown`
  (the browser path). Keep them free of OS-only APIs.

## Running the full corpus sweep

The always-on tests parse a small vendored subset of real ARA artifacts under
`crates/ara-core/tests/fixtures/corpus/` and assert the parser never panics. A
maintainer-run **opt-in sweep** exercises the *full* corpora (all 34 artifacts,
including the unlicensed `ARA-Demo` set that is never vendored) via git
submodules under `corpus-external/`.

The sweep is doubly gated — `#[ignore]` **and** `RUN_CORPUS_SWEEP=1` — so
`cargo test` never runs it and a fresh clone without submodules still passes.

```bash
git submodule update --init                       # fetch corpus-external/*
RUN_CORPUS_SWEEP=1 cargo test -p ara-core -- --ignored
```

Without `--ignored` the sweep test is skipped; with `--ignored` but no env var
(or no submodules) it skips cleanly, logging why. Required CI does **not** init
submodules and does not run the sweep.

## Versioning

Every PR bumps the workspace patch version in `Cargo.toml` and adds an entry to
`CHANGELOG.md` under `## [Unreleased]`. See `CLAUDE.md` for the full policy.
