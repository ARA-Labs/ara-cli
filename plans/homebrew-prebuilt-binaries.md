# Plan: Publish pre-built `ara` binaries via Homebrew (cargo-dist)

## Problem / background

Today `ara` is installable only via `cargo install ara-cli`, which requires the
user to have a Rust toolchain and to compile from source (minutes, plus the wasm
viewer bundle is already embedded so the build is heavy). GitHub Releases exist
(`v0.1.0`–`v0.1.4`) but carry **no binary assets** — they are source-only tags.

We want a one-liner install of a **pre-built** binary for the two platforms our
users actually run:

- `aarch64-apple-darwin` — macOS Apple Silicon
- `x86_64-unknown-linux-gnu` — Linux x86_64

delivered through **Homebrew** so users can `brew install ARA-Labs/tap/ara`.

### Why a custom tap (and why a second repo)

Homebrew **Core** builds every formula *from source* and won't host our own
pre-built binaries, so it does not meet the "ship pre-built binaries" goal.
Shipping our own binaries means a **custom tap**. Homebrew resolves
`brew install ARA-Labs/tap/ara` to a repo literally named
`github.com/ARA-Labs/homebrew-tap` (the `homebrew-` prefix is mandatory and is
dropped in the install command). There is no way to serve a tap formula out of
`ara-cli` itself with the clean `brew install` syntax — hence a small, separate,
**public** repo `ARA-Labs/homebrew-tap` is required. It will contain only
`Formula/ara.rb`, which is generated and updated automatically — not
hand-maintained.

## Proposed solution

Adopt **cargo-dist** (`dist`), the standard release tool for Rust binaries. On a
pushed `vX.Y.Z` tag it:

1. Cross-builds the configured targets on GitHub-hosted runners
   (macOS runner for the darwin arm64 build, Linux runner for the gnu build).
2. Produces per-target tarballs + a checksums file and attaches them to the
   GitHub Release on `ARA-Labs/ara-cli`.
3. Generates `Formula/ara.rb` (URLs + per-platform sha256) and **pushes it to
   `ARA-Labs/homebrew-tap`**.
4. Optionally also emits a `curl | sh` shell installer (nice-to-have; can enable
   later).

Result: tagging a release becomes the single action that yields both the binary
assets and an up-to-date Homebrew formula. No manual formula edits.

### Why cargo-dist over a hand-written workflow

- Correct sha256 + URL wiring into the formula is automated (easy to get wrong by
  hand, and wrong hashes break every user's install).
- Respects our pinned `rust-toolchain.toml` (1.94.1), preserving reproducibility.
- One config block in `Cargo.toml` + one generated workflow; upgrades are
  `dist init` re-runs.

## Key facts already verified in this repo

- Binary is `ara`, from crate `ara-cli`; workspace also has a second bin
  `ara-viewer` (in `crates/ara-viewer`) — dist must be told to ship **only the
  `ara` bin from `ara-cli`**, not `ara-viewer`.
- The viewer frontend is **pre-embedded** via `include_dir!` from the committed
  `crates/ara-cli/assets/viewer/`. Therefore building the `ara` binary needs
  **only the Rust toolchain** — no `trunk` / `wasm-pack` / wasm target at
  release-build time. Cross-compiling the release binaries is clean.
- License MPL-2.0; repo `ARA-Labs/ara-cli` is public.
- Existing `.github/workflows/ci.yml` is unaffected (separate `release.yml`).

## Status (2026-07-12)

Config + workflow are in place; the release itself is the remaining manual step.

- [x] Tap repo `ARA-Labs/homebrew-tap` created (public), README written.
- [x] cargo-dist 0.32.0 config in `Cargo.toml` (`[workspace.metadata.dist]`):
      targets = darwin-arm64 + linux-x64, installers = shell + homebrew,
      `tap = "ARA-Labs/homebrew-tap"`, `allow-dirty = ["ci"]`,
      **`publish-jobs` omitted** (autopush deferred).
- [x] `ara-viewer` excluded (`dist = false`); `ara-cli` shipped with
      `formula = "ara"` → `class Ara` / `brew install ARA-Labs/tap/ara`.
- [x] `.github/workflows/release.yml` generated; all `uses:` re-pinned to full
      commit SHAs to match `ci.yml`.
- [x] `dist plan` verified: only the `ara` bin ships, both targets, tarballs +
      checksums + `ara-installer.sh` + `ara.rb`.
- [x] Version bumped `0.1.4 → 0.1.5` (workspace + lockfile), CHANGELOG entry,
      README install section updated.
- [x] Autopush tracked in ARA-Labs/ara-cli#25.
- [ ] **Push tag `v0.1.5`** → release workflow builds + attaches artifacts.
- [ ] **Commit `ara.rb`** from the release asset into the tap (`Formula/ara.rb`).
- [ ] Verify `brew install ARA-Labs/tap/ara` on macOS arm64 + Linux x64.

## Implementation steps

1. **Create the tap repo** (human, in progress): public `ARA-Labs/homebrew-tap`,
   empty is fine.

2. **Provision a token for cross-repo push.** cargo-dist's release job needs to
   push the formula into `homebrew-tap`. Add a repo secret on `ara-cli`:
   - Preferred: a fine-grained PAT with **contents: read/write** scoped to
     `ARA-Labs/homebrew-tap` only, stored as a secret cargo-dist reads
     (dist documents the exact secret name during `init`; typically a
     `HOMEBREW_TAP` GitHub token). We will confirm the exact name from the
     `dist init` output and wire it.
   - Document this in `docs/` so re-provisioning is not tribal knowledge.

3. **Install and run cargo-dist locally.**
   ```bash
   cargo install cargo-dist --locked   # or: brew install cargo-dist
   dist init
   ```
   Answers during `init`:
   - CI backend: **GitHub**.
   - Targets: `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` (only these two).
   - Installers: **homebrew** (shell installer optional; default off for now).
   - Homebrew tap: `ARA-Labs/homebrew-tap`.

4. **Constrain dist to the `ara` bin only.** In the generated
   `[workspace.metadata.dist]`, ensure the shipped package is `ara-cli` and the
   `ara-viewer` bin is excluded (dist config: mark `ara-viewer` /
   `ara-core` / `ara-wasm` as `dist = false`, or set the `ara-cli` package as the
   only distable member). Verify `dist plan` lists exactly one artifact set for
   `ara`.

5. **Review generated files:**
   - `Cargo.toml` → `[workspace.metadata.dist]` block.
   - `.github/workflows/release.yml` → pinned action SHAs to match this repo's
     existing convention (ci.yml pins every `uses:` to a SHA). Re-pin any
     dist-generated `uses:` lines that come in as floating tags, to satisfy our
     supply-chain hygiene.
   - Confirm `rust-toolchain.toml` (1.94.1) is honored by the build job.

6. **Dry-run without publishing:**
   ```bash
   dist plan          # shows the artifact/target matrix
   dist build         # local build of the current host target as a sanity check
   ```

7. **First real release.** Follow the repo's versioning rules: this is a
   functional change (new distribution artifacts + release workflow). Bump patch
   `0.1.4 → 0.1.5` in `Cargo.toml`, add a CHANGELOG `Added` entry, then tag and
   push `v0.1.5`. The release job runs, attaches tarballs+checksums, and pushes
   `Formula/ara.rb` to the tap.

8. **Verify the install path end-to-end:**
   ```bash
   brew install ARA-Labs/tap/ara     # or: brew tap ARA-Labs/tap && brew install ara
   ara --help
   ara --version                     # should print 0.1.5
   ```
   Confirm on both a macOS arm64 machine and a Linux x86_64 box (or container).

9. **Docs + README.**
   - README `## Install` gains a Homebrew section as the recommended path,
     keeping `cargo install ara-cli` as the from-source alternative.
   - Add `docs/releasing.md` (or extend an existing deploy doc) describing: the
     tag-driven release flow, the tap repo, the token secret, and how to add a
     target later.

## Open questions / decisions for reviewer

- **Version to release under.** Plan assumes `v0.1.5`. Confirm, or whether you
  want to first cut `v0.1.4` release assets (v0.1.4 is already tagged as the
  current `Cargo.toml` version but has no binaries). Recommendation: go forward
  with a fresh `v0.1.5` so the "release now attaches binaries" change is itself
  captured in the changelog for that tag.
- **Add the `curl | sh` shell installer too?** Costs nothing extra in the
  workflow and gives non-Homebrew Linux users a one-liner. Default: enable it;
  easy to drop.
- **Add more targets later?** macOS x86_64 (Intel) and Linux arm64 were
  considered and deferred. Adding them later is a `dist init` re-run + a tag.

## Non-goals

- Homebrew Core submission (builds from source; not our goal here).
- Publishing to crates.io changes (unaffected; `cargo install` still works).
- Docker/hub image pipeline (separate, already exists in ci.yml `docker` job).
