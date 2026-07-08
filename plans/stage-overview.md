# ARA Viewer Runtime — Staged Implementation Overview

This decomposes `plans/ara-runtime-impl-plan.md` (the master plan) into six
PR-sized stages. Each stage has its own plan file and ships as one squash-merged
PR that bumps the workspace patch version and adds a `CHANGELOG.md` entry.

## Stage sequence

| Stage | Plan file | Delivers | Depends on |
| ----- | --------- | -------- | ---------- |
| 0 | `stage-0-ci-tooling.md` | CI (fmt/clippy/test), toolchain pin, wasm target check | — |
| 1 | `stage-1-core-parse-validate.md` | `ara-core` schema + parse + `Manifest` + `ara validate` | 0 |
| 2 | `stage-2-dag-layout.md` | Layered (dagre) DAG layout in `ara-core`, positions in `Manifest` | 1 |
| 3 | `stage-3-wasm-viewer.md` | Leptos CSR client: SVG DAG + drill-down from a static manifest | 2 |
| 4 | `stage-4-serve-live-reload.md` | `ara serve` (axum): assets, `/api/manifest`, `/api/figure`, live reload | 3 |
| 5 | `stage-5-hub-deploy.md` | musl Docker → distroless, hub cache, caching headers | 4 |

Stages are a linear chain: each builds on the previous. The manifest schema is
**frozen at the end of Stage 2** (parse + layout together define the wire type);
Stages 3–5 consume it and must not change it without a coordinated bump.

## Conventions (all stages)

- **Crate under development:** `ara-cli` (ships the `ara` binary). The umbrella
  `ara-viewer` name stays a placeholder until a release cut.
- **Versioning:** each dev stage PR bumps the workspace patch version
  (`0.0.0 → 0.0.1 → … → 0.0.5` across Stages 0–4). These are repo-only bumps;
  nothing is published to crates.io during dev, so the reserved `0.0.0` stays
  intact.
- **First published release = `0.1.0`, cut at the end of Stage 4.** Stages 1–4
  together are the first usable product (`ara validate` + `ara layout` +
  `ara serve` with a live viewer). Stages 1–3 alone are not shippable as a CLI
  release (no `ara serve` yet). The `0.1.0` cut is a dedicated release PR
  (`0.0.5 → 0.1.0`) that publishes `ara-core`, `ara-cli`, and `ara-wasm` to
  crates.io **in dependency order** (wire the `version =` fields on path deps
  first). Stage 5 (hub/Docker) is ops, not new public API, and ships as `0.1.1`.
- **Tests are mandatory** (per `CLAUDE.md`): every stage lands with unit +
  integration tests and `cargo test --workspace` green.
- **Docs:** after a stage merges, fold its plan into `docs/` and delete it from
  `plans/` (per `CLAUDE.md`).
- **Determinism:** parse and layout must be byte-deterministic (snapshot tests
  rely on it). Pin `serde-saphyr` and the dagre-port versions exactly.

## Per-stage PR checklist

1. Branch `stageN-<slug>` off `main`.
2. Implement per the stage plan; keep the diff scoped to that stage.
3. `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`,
   `cargo test --workspace` all green; wasm stages also `trunk build`.
4. Bump workspace patch version in `Cargo.toml`.
5. Add a `CHANGELOG.md` entry under `## [Unreleased]`.
6. Open PR; request human review. Do not self-merge.
