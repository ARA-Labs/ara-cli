# Release distribution — pre-built binaries, cargo-dist, and the Homebrew tap

Design record for the binary-release pipeline built for issue
[#25](https://github.com/ARA-Labs/ara-cli/issues/25) (#26, auto-push follow-up
#36) and first released as `0.1.6`. It covers what a tag produces, how the
Homebrew tap is fed, and the one step that is still manual.

## Problem / background

Before this, `ara` was installable only via `cargo install ara-cli`: a Rust
toolchain plus a multi-minute build (the wasm viewer bundle is embedded, so the
crate is heavy). Every tag up to `v0.1.5` was source-only — the GitHub
Releases carried no binary assets.

The goal was a one-liner install of a **pre-built** binary for the two platforms
our users actually run — `aarch64-apple-darwin` (macOS Apple Silicon) and
`x86_64-unknown-linux-gnu` — delivered through Homebrew.

Homebrew **Core** builds every formula from source and will not host our
binaries, so shipping pre-built ones means a **custom tap**. Homebrew resolves
`brew install ARA-Labs/tap/ara` to a repo literally named
`github.com/ARA-Labs/homebrew-tap` (the `homebrew-` prefix is mandatory and
dropped in the install command), so the tap has to be its own public repo — a
formula cannot be served out of `ara-cli` with that syntax.

## What ships

[cargo-dist](https://axodotdev.github.io/cargo-dist) (`dist`) 0.32.0 is
configured in `[workspace.metadata.dist]` in the root `Cargo.toml`:

- `targets` — `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`.
- `installers` — `shell` (a `curl | sh` one-liner) and `homebrew`.
- `tap` — `ARA-Labs/homebrew-tap`.
- `install-path = "CARGO_HOME"`, `install-updater = false`.
- `pr-run-mode = "plan"` — PRs run `dist plan` only, so the release workflow is
  checked on every PR without building or publishing anything.

**Only the `ara` binary ships.** The workspace defaults every member to
`dist = false`, and `crates/ara-cli/Cargo.toml` opts back in with `dist = true`
plus `formula = "ara"` (so the formula is `ara`, class `Ara`, and the install
command is `brew install ARA-Labs/tap/ara`, not `.../ara-cli`). `ara-viewer` is
a Leptos/wasm build entrypoint rather than a native tool, and `ara-core` /
`ara-wasm` are libraries.

The opt-out lives at the workspace level rather than in
`crates/ara-viewer/Cargo.toml` on purpose: that file's contents are hashed by
the `viewer-embed-fresh` gate (`scripts/embed-viewer.sh`), so editing it would
force a bundle regeneration for a packaging-only change.

Release builds need **only the Rust toolchain** — no `trunk`, `wasm-pack`, or
wasm target — because the viewer frontend is pre-embedded via `include_dir!`
from the committed `crates/ara-cli/assets/viewer/`. Cross-compiling is therefore
clean, and the pinned `rust-toolchain.toml` (1.94.1) is honoured by the build
jobs, preserving the repo's byte-determinism invariant.

## The tag-driven flow

`.github/workflows/release.yml` is dist-generated (every `uses:` re-pinned to a
full commit SHA to match `ci.yml`'s supply-chain convention) and triggers on a
pushed version tag. Its jobs are `plan` → `build-local-artifacts` →
`build-global-artifacts` → `host` → `announce`.

Pushing `vX.Y.Z` produces a GitHub Release on `ARA-Labs/ara-cli` carrying:

```
ara-cli-aarch64-apple-darwin.tar.xz            (+ .sha256)
ara-cli-x86_64-unknown-linux-gnu.tar.xz        (+ .sha256)
ara-cli-installer.sh
ara.rb
dist-manifest.json
sha256.sum
source.tar.gz                                  (+ .sha256)
```

`ara.rb` is the generated Homebrew formula: release-asset URLs plus a per-platform
SHA-256. It is generated, never hand-written — getting a hash wrong breaks every
user's install.

## Tap publishing is manual (current state)

Auto-pushing `ara.rb` into the tap needs a **cross-repo** write token
(`HOMEBREW_TAP_TOKEN`, fine-grained PAT with Contents:RW scoped to
`ARA-Labs/homebrew-tap` only). Provisioning it is gated on org-owner approval,
so the `publish-homebrew-formula` job has been removed and the formula is
published by hand. The history is #37 (disabled) → #44 (re-enabled) → #48
(disabled again, token setup deferred).

The `homebrew` **installer** stays enabled, so `ara.rb` is still generated and
attached to every release — only the push to the tap is manual:

```bash
gh release download vX.Y.Z --repo ARA-Labs/ara-cli --pattern ara.rb --dir /tmp
# in a checkout of ARA-Labs/homebrew-tap
cp /tmp/ara.rb Formula/ara.rb
git commit -am "ara X.Y.Z" && git push
```

Then verify the published path actually resolves:

```bash
brew update && brew install ARA-Labs/tap/ara
ara --version    # must print X.Y.Z
```

**To re-enable auto-push:** provision `HOMEBREW_TAP_TOKEN`, add
`publish-jobs = ["homebrew"]` back to `[workspace.metadata.dist]`, regenerate CI
with `dist generate`, and drop this section.

## Changing the pipeline

- **Add a target** (e.g. macOS x86_64, Linux arm64 — both considered and
  deferred): add the triple to `targets`, run `dist generate`, and cut a tag.
- **Upgrade dist:** bump `cargo-dist-version` and re-run `dist generate`. Note
  `allow-dirty = ["ci"]` is set, so dist will not clobber the SHA-pinned
  `uses:` lines in the generated workflow — re-pin any new ones by hand.
- **Dry runs:** `dist plan` prints the artifact/target matrix (it should list
  exactly one binary, `ara`); `dist build` builds the host target locally.

## Non-goals

- Homebrew Core submission — builds from source, which defeats the purpose.
- Changing crates.io publishing; `cargo install ara-cli` still works and remains
  the documented path for platforms outside the two shipped targets.
- The Docker image, which is a separate pipeline in `ci.yml`.
