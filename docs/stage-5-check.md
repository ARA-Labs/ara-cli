# Stage 5 — `ara check`: fixable linter + a reusable CI action

Design record for the linter/format-checker shipped in Stage 5 (PR #39, first
released in `0.1.10`). Where `ara validate` is the deterministic semantic gate,
`ara check` is the opinionated, CI-facing front-end: it reuses the same parse
engine, adds a small set of **auto-fixable** format rules, and ships a reusable
GitHub Action so a downstream repo can gate its ARA the way `ruff` gates a Python
repo.

Companion docs: [`stage-1-core-parse-validate.md`](stage-1-core-parse-validate.md)
(the `validate` layer this reuses) and
[`ara-format-feedback.md`](ara-format-feedback.md) (the format drift the fixable
rules canonicalize).

## Problem / background

Issue [#39](https://github.com/ARA-Labs/ara-cli/issues/39) asks for CI support:
drop a format/lint check into a repo that holds an ARA artifact so the ARA stays
clean automatically. The follow-up framed it as a linter with an autofix flag —
`ruff` as the **UX analogy**, not a dependency. `ara` stays self-contained and
does not shell out to `ruff` or any external tool.

`ara validate` already parses an ARA and emits errors/warnings, but it has no
autofix, and the common tolerated drift forms are handled today in three
different ways — none of which hands the author a fixable signal (see
[`ara-format-feedback.md`](ara-format-feedback.md) items 2, 4, 5, 7):

- **`root:` vs `tree:`** (item 2) — silently normalized. Both dialects parse
  (`root:` becomes a one-element list), so `validate` emits no diagnostic at all.
- **dead-end `reason:` / decision `justification:`** (items 5, 4) — not
  recognized as aliases. They fall into `extra`, surface as an `unknown field`
  **warning**, and the value is **dropped** (the canonical keys are `why_failed:`
  / `rationale:`).
- **em-dash / hyphen claim headers** (item 7) — `parse_header` requires a `:`
  separator, so `## C01 — Title` makes the **entire claim silently disappear**;
  it only surfaces indirectly as an "unknown claim" error if some node still
  references the dropped id.

`check` makes all four visible and, for each, offers a safe in-place fix.

## Design rationale — a new command, not `validate --fix`

`validate` is the deterministic semantic gate whose byte-stable output other
tooling and docs already lean on; keeping that output unchanged matters. `check`
is the opinionated, fixable, CI-facing front-end. They share the parse engine
but have distinct contracts. Accordingly the format-lint layer is a **new
`ara-core` module** (`check_dir`) that reads the raw source text and is **not**
wired into `parse_dir`, so `validate`'s output is unchanged.

## Command surface

```
ara check <dir> [--fix] [--strict] [--json]
```

| flag       | behavior |
| ---------- | -------- |
| (none)     | Parse, run the format-lint layer, print diagnostics; annotate auto-fixable ones with `[fixable]`. Exits non-zero if there are errors **or** unfixed fixable issues. |
| `--fix`    | Apply the fixable rules to the source files in place, re-check, and report what changed. Exit reflects the **post-fix** state. |
| `--strict` | Treat remaining warnings as failure (mirrors `validate --strict`). |
| `--json`   | Machine-readable composed report (for CI annotations). |

## The two diagnostic layers

`ara check` composes two sources into one report:

1. **Validate layer (reused):** the exact errors/warnings from the existing
   `parse_dir` pipeline — duplicate ids, unknown references, missing `id`/`type`,
   unknown fields, and so on. Reported as-is. Almost all need human judgment and
   are reported **not fixable**.
2. **Format-lint layer (new):** a small, closed set of rules (`check_dir`) that
   detect the *canonicalizable* drift and know how to rewrite it. These carry a
   rule id (`ARA001`..`ARA004`) and the `[fixable]` marker, and are the only
   thing `--fix` touches.

`ARA002` and `ARA003` correspond to drift `validate` already warns about (the
`unknown field` warnings) — `check` upgrades those specific warnings to
`[fixable]`, and the fix stops the value being dropped. `ARA001` and `ARA004`
cover drift `validate` is currently silent about, so the lint layer is what first
makes them visible.

## The four fixable rules

Each rule canonicalizes one documented drift from
[`ara-format-feedback.md`](ara-format-feedback.md).

| id       | detects | fix | drift doc |
| -------- | ------- | --- | --------- |
| `ARA001` root-dialect | top-level `root:` (a single node) | rewrite to `tree:` with a one-element list (re-indent the block) | item 2 (`tree:` vs `root:`) |
| `ARA002` dead-end-reason-alias | `reason:` on a `dead_end` node | rename the key to `why_failed:` **and recover the value** validate drops | item 5 (`why_failed` vs `reason`) |
| `ARA003` decision-rationale-alias | `justification:` on a `decision` node | rename the key to `rationale:` (recovering the dropped value) | item 4 (type-specific body fields) |
| `ARA004` claim-header-style | `## C01 — Title` / `## C01 - Title` in `logic/claims.md` | rewrite the separator to `## C01: Title` (recovering the otherwise-dropped claim) | item 7 (claims in Markdown) |

The rules split by kind: `ARA001` is **structural** (it re-indents a YAML block);
`ARA002`/`ARA003`/`ARA004` are **value-recovering** (they intentionally change
the manifest because they resurrect a value or claim validate currently drops).
That split drives the fix-safety guard below.

Fixes are surgical text edits only. `ara check --fix` is **not** a canonical
re-serializer: it never re-emits the YAML/Markdown from the parsed model, so
comments, key order, and author style are left untouched.

## The fix-safety guard (load-bearing correctness)

Every `--fix` edit is computed in memory and written only after a guard passes.
The guards evaluate normalized parse outcomes:

- **`ARA001` (structural) — clean semantic no-op.** Both the base and candidate
  parses must be clean normalized manifests, and the manifests must be
  identical. An error-bearing or fatal outcome rejects the edit.

- **`ARA002` / `ARA003` / `ARA004` (value-recovering) — targeted recovery.**
  These rules may operate on normalized error-bearing artifacts only when the
  complete candidate error multiset is a subset of the base error multiset.
  Error identity includes severity, logical path, and message, and containment
  preserves occurrence counts. A fatal base or candidate outcome rejects the
  edit without writing.

The normalized semantic delta must also be exact. `ARA002` and `ARA003` change
only the target alias field from `None` to `Some`. `ARA004` adds exactly one
intended claim, leaves nodes and links unchanged, and adds only bindings to that
recovered claim. Any rejected candidate leaves the source byte-identical;
accepted fixes retain the re-detection and idempotence backstop, so a second
`--fix` is a no-op.

These fixes rewrite source files only. They do not modify or regenerate Hub
output, static `trajectory.html`, or viewer assets.

## Exit codes

- `0` — clean: no errors and no unfixed fixable issues (and, under `--strict`, no
  warnings).
- `1` — errors present, **or** fixable issues remain unfixed. In non-`--fix` mode
  this is the "run `--fix`" signal (like `ruff check` without `--fix`).
- `2` — internal failure: target missing / not a directory / unreadable
  `trace/exploration_tree.yaml`, a JSON serialization error, or a failed `--fix`
  write.

## The reusable GitHub Action

[`.github/actions/check/action.yml`](../.github/actions/check/action.yml) is a
**composite** action a downstream repo references as
`ARA-Labs/ara-cli/.github/actions/check@v0`. It installs a released `ara` binary
via cargo-dist's `ara-installer.sh` (pinned by tag, or `latest` for the newest
release) and runs `ara check` on an input path.

| input     | required | default    | meaning |
| --------- | -------- | ---------- | ------- |
| `path`    | yes      | —          | ARA artifact directory to check (contains `trace/` and `logic/`). |
| `strict`  | no       | `false`    | Append `--strict` when truthy. |
| `version` | no       | `latest`   | Release tag to install (e.g. `v0.1.10`), or `latest`. |
| `args`    | no       | `""`       | Extra args appended verbatim to `ara check` (escape hatch). |

There is deliberately **no `fix` input** — CI checks, it does not mutate the
tree. Caller-supplied inputs are passed via `env:` and read as shell variables
(never interpolated with `${{ }}` into the script body) to avoid workflow script
injection.

Because the action installs a **published** release, `ara check` must exist in
the pinned release. It first ships in `0.1.10`, so downstream callers should pin
`version: v0.1.10` (or later) until it is the newest release.

### Downstream workflow snippet

```yaml
# .github/workflows/ara.yml (in your repo)
jobs:
  ara-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ARA-Labs/ara-cli/.github/actions/check@v0
        with:
          path: ./my-ara
          version: v0.1.10   # pin until check is in the newest release
```

This repo also runs an `ara-check` CI job that dogfoods the command **from
source** against the official fixtures (plus an inline fixable artifact for a
`--fix` round-trip), so the linter is exercised on every push in addition to the
release-binary path the action uses.

## What is deferred

- **A slim `ara`-only Docker image** as a CI fast-path (skip the install step,
  like `ruff`/`uv` ship). Planned, not yet built.
- **Per-rule config via `.ara-check.toml`** — tracked as issue
  [#40](https://github.com/ARA-Labs/ara-cli/issues/40). The rule set is hardcoded
  for v1.
