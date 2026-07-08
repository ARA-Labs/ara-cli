# Stage 0 — CI & Tooling Foundation

**PR target:** `stage0-ci-tooling` → `main`. **Depends on:** none.
**Version bump:** `0.0.0 → 0.0.1`.

## Problem background

The workspace is scaffolded and the crate names are reserved, but there is no
automated gate. Before any real code lands, we need reproducible checks so every
later stage PR is validated the same way: formatting, lints, tests, and a
verification that `ara-core` actually compiles to `wasm32-unknown-unknown` (the
core promise of the architecture — one crate, native + wasm). Catching a
wasm-incompatible dependency in Stage 1 by hand is expensive; CI should catch it.

## Proposed solution

A GitHub Actions workflow plus a pinned toolchain, giving a single green/red
signal for `fmt`, `clippy`, native tests, and a `wasm32` build check of
`ara-core`. No product code changes.

## Implementation steps

1. **Pin the toolchain.** Add `rust-toolchain.toml` at the repo root:
   ```toml
   [toolchain]
   channel = "1.94"
   components = ["rustfmt", "clippy"]
   targets = ["wasm32-unknown-unknown"]
   ```
2. **Formatting + lint config.** Add `rustfmt.toml` (max_width 100, edition 2024)
   and a workspace `[workspace.lints]` table in `Cargo.toml` denying
   `clippy::all` warnings where practical (start permissive, tighten later).
3. **CI workflow** `.github/workflows/ci.yml` with jobs (Ubuntu, cached via
   `Swatinem/rust-cache`):
   - `fmt`: `cargo fmt --all --check`.
   - `clippy`: `cargo clippy --workspace --all-targets -- -D warnings`.
   - `test`: `cargo test --workspace`.
   - `wasm`: `cargo build -p ara-core --target wasm32-unknown-unknown`
     (guards the native/wasm dual-build invariant).
4. **Dependabot** (optional, low-cost): `.github/dependabot.yml` for `cargo`
   weekly, so `serde-saphyr`/dagre pins get update PRs.
5. **CONTRIBUTING note** (short): document the local pre-PR commands mirrored by
   CI (`cargo fmt`, `cargo clippy`, `cargo test`).

## Tests / verification

- CI must pass on the scaffold as-is (it already builds + has one test).
- Locally run all four CI commands and confirm green, including the wasm build.
- Intentionally introduce a `std::fs` call in `ara-core` in a throwaway commit to
  confirm the `wasm` job fails, then revert (proves the guard works).

## Milestone / acceptance

Green CI on `main` with fmt + clippy + test + wasm-build jobs; toolchain pinned.
Every subsequent stage PR is gated by this workflow.

## CHANGELOG (Unreleased → Added)

- CI workflow (fmt, clippy, test, wasm-build) and pinned Rust toolchain.
