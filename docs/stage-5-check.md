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
  recognized as aliases. `justification:` falls into `extra` and surfaces as an
  `unknown field` **warning**; `reason:` names the pivot-canonical key, so on a
  `dead_end` it surfaces as a `field dropped for type` warning — either way the
  value is **dropped** (the canonical keys are `why_failed:` / `rationale:`).
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
ara check <dir> [--fix] [--strict] [--json] [--config <path> | --no-config]
```

| flag       | behavior |
| ---------- | -------- |
| (none)     | Parse, run the format-lint layer, print diagnostics; annotate auto-fixable ones with `[fixable]`. Exits non-zero if there are errors **or** unfixed fixable issues. |
| `--fix`    | Apply the fixable rules to the source files in place, re-check, and report what changed. Exit reflects the **post-fix** state. |
| `--strict` | Treat remaining warnings as failure (mirrors `validate --strict`). |
| `--json`   | Machine-readable composed report (for CI annotations). |
| `--config <path>` | Use this config file instead of discovering `.ara-check.toml` (see [Configuration](#configuration-ara-checktoml)). |
| `--no-config` | Ignore any `.ara-check.toml`; use the built-in rule settings. |

## The two diagnostic layers

`ara check` composes two sources into one report:

1. **Validate layer (reused):** the exact errors/warnings from the existing
   `parse_dir` pipeline — duplicate ids, unknown references, missing `id`/`type`,
   unknown fields, and so on. Each carries an `ARA1xx` (error) or `ARA2xx`
   (warning) rule code. All need human judgment and are reported **not fixable**.
2. **Format-lint layer (new):** a small, closed set of rules (`check_dir`) that
   detect the *canonicalizable* drift and know how to rewrite it. These carry an
   `ARA0xx` rule code and the `[fixable]` marker, and are the only thing `--fix`
   touches.

Every finding from either layer names its rule (see [Rule codes](#rule-codes)):

```
ARA105 error: nodes[N01]: duplicate node id
ARA201 warning: nodes[N01]: unknown field `bogus`
ARA002 [fixable]: trace/exploration_tree.yaml: `reason:` on a dead_end node is an alias; canonical key is `why_failed:`
```

The text after a validate code is exactly what `ara validate` prints for the
same diagnostic. In `--json`, every `validate.errors[]` / `validate.warnings[]`
entry has `rule`, `severity`, `path`, and `message`, and every `lint[]` entry has
`rule`, so CI can group findings from both layers by the same key.

`ARA002`, `ARA003`, and `ARA005`–`ARA007` correspond to drift `validate`
already warns about: the unmodeled spellings (`justification:`, `from:`,
`to:`, `trigger:`) fall into `extra` and surface as `unknown field` warnings,
while `reason:` on a `dead_end` names the pivot-canonical key and surfaces as
a `field dropped for type` warning — either way the value is dropped. `check`
upgrades that drift to `[fixable]`, and the fix stops the value being dropped.
`ARA001` and `ARA004` cover drift `validate` is currently silent
about, so the lint layer is what first makes them visible.

## The fixable rules

Each rule canonicalizes one documented drift from
[`ara-format-feedback.md`](ara-format-feedback.md). `ARA005`–`ARA007` were
added in `0.1.15` with the published-fields widening (issue #75).

| id       | detects | fix | drift doc |
| -------- | ------- | --- | --------- |
| `ARA001` root-dialect | top-level `root:` (a single node) | rewrite to `tree:` with a one-element list (re-indent the block) | item 2 (`tree:` vs `root:`) |
| `ARA002` dead-end-reason-alias | `reason:` on a `dead_end` node | rename the key to `why_failed:` **and recover the value** validate drops | item 5 (`why_failed` vs `reason`) |
| `ARA003` decision-rationale-alias | `justification:` on a `decision` node | rename the key to `rationale:` (recovering the dropped value) | item 4 (type-specific body fields) |
| `ARA004` claim-header-style | `## C01 — Title` / `## C01 - Title` in `logic/claims.md` | rewrite the separator to `## C01: Title` (recovering the otherwise-dropped claim) | item 7 (claims in Markdown) |
| `ARA005` pivot-from-alias | `from:` on a `pivot` node | rename the key to `prior_direction:` (recovering the dropped value) | item 13 (`from`/`to`/`trigger` on pivots) |
| `ARA006` pivot-to-alias | `to:` on a `pivot` node | rename the key to `new_direction:` (recovering the dropped value) | item 13 |
| `ARA007` pivot-trigger-alias | `trigger:` on a `pivot` node | rename the key to `reason:` (recovering the dropped value) | item 13 |

The rules split by kind: `ARA001` is **structural** (it re-indents a YAML block);
`ARA002`–`ARA007` are **value-recovering** (they intentionally change
the manifest because they resurrect a value or claim validate currently drops).
That split drives the fix-safety guard below. The alias rules are kind-scoped:
`reason:` is canonical on a `pivot` (ARA007's rename target) but an alias of
`why_failed:` on a `dead_end` (ARA002), and a key is only flagged when it sits
directly on a node of the matching kind.

Fixes are surgical text edits only. `ara check --fix` is **not** a canonical
re-serializer: it never re-emits the YAML/Markdown from the parsed model, so
comments, key order, and author style are left untouched.

## Rule codes

Every rule `ara check` can report has a stable code, defined once in
[`crates/ara-core/src/rules.rs`](../crates/ara-core/src/rules.rs) (`RuleCode`).
Codes are an API surface: once published they are never renumbered or reused (a
retired rule keeps its number). The code space is split by layer:

- `ARA0xx` — format/canonicalization drift (format-lint layer). All fixable; an
  unfixed one fails the run, so its default severity is `error`.
- `ARA1xx` — structural/reference errors (validate layer).
- `ARA2xx` — field/schema warnings (validate layer).

| code | name | meaning | severity | fixable |
| ---- | ---- | ------- | -------- | ------- |
| `ARA001` | root-dialect | top-level `root:` instead of a `tree:` list | error | yes |
| `ARA002` | dead-end-reason-alias | `reason:` on a `dead_end` node (canonical `why_failed:`) | error | yes |
| `ARA003` | decision-rationale-alias | `justification:` on a `decision` node (canonical `rationale:`) | error | yes |
| `ARA004` | claim-header-style | claim header uses a dash separator instead of `## <id>: <title>` | error | yes |
| `ARA005` | pivot-from-alias | `from:` on a `pivot` node (canonical `prior_direction:`) | error | yes |
| `ARA006` | pivot-to-alias | `to:` on a `pivot` node (canonical `new_direction:`) | error | yes |
| `ARA007` | pivot-trigger-alias | `trigger:` on a `pivot` node (canonical `reason:`) | error | yes |
| `ARA100` | malformed-tree | `trace/exploration_tree.yaml` fails to parse (invalid YAML, multi-document, non-mapping root, wrong field types) | error | no |
| `ARA101` | unreadable-tree | `trace/exploration_tree.yaml` cannot be read | error | no |
| `ARA102` | tree-and-root | both `tree:` and `root:` are present | error | no |
| `ARA103` | missing-tree | neither `tree:` nor `root:` is present | error | no |
| `ARA104` | missing-node-id | node is missing an `id` (node and subtree dropped) | error | no |
| `ARA105` | duplicate-node-id | two nodes share an id (the later node and its subtree are dropped) | error | no |
| `ARA106` | duplicate-claim-id | two claims in `logic/claims.md` share an id | error | no |
| `ARA107` | unknown-evidence-claim | node `evidence:` references a claim not in `logic/claims.md` | error | no |
| `ARA108` | unknown-dependency-node | `also_depends_on:` references a node that does not exist | error | no |
| `ARA109` | unknown-claim-dependency | claim `Dependencies:` references a claim that does not exist | error | no |
| `ARA110` | dependency-cycle | `children:` + `also_depends_on:` edges form a cycle | error | no |
| `ARA200` | unknown-document-field | unrecognized top-level key in `trace/exploration_tree.yaml` | warning | no |
| `ARA201` | unknown-node-field | unrecognized key on a node (value dropped) | warning | no |
| `ARA202` | empty-tree | `tree: []` yields an empty manifest | warning | no |
| `ARA203` | missing-node-type | node is missing a `type` | warning | no |
| `ARA204` | field-dropped-missing-type | body field dropped because the node has no `type` | warning | no |
| `ARA205` | field-dropped-unknown-type | body field dropped because the node's `type` is not recognized | warning | no |
| `ARA206` | field-dropped-for-type | body field not modeled for the node's `type` is dropped | warning | no |
| `ARA207` | unresolved-claim-reference | claim reference unresolved because `logic/claims.md` is absent | warning | no |
| `ARA208` | redundant-ancestor-dependency | `also_depends_on:` on an ancestor restates `children:` nesting (edge dropped) | warning | no |
| `ARA209` | duplicate-link | the same edge is declared more than once (duplicate dropped) | warning | no |
| `ARA210` | malformed-paper-frontmatter | `PAPER.md` frontmatter fails to parse (paper metadata dropped) | warning | no |
| `ARA211` | concept-missing-definition | a `logic/concepts.md` entry has no definition | warning | no |
| `ARA212` | related-work-missing-doi | a `logic/related_work.md` entry has no DOI | warning | no |
| `ARA213` | duplicate-exhibit-basename | the same exhibit basename appears under two `evidence/` categories | warning | no |
| `ARA214` | exhibit-missing-index-row | an `evidence/` body file has no row in `evidence/README.md` | warning | no |
| `ARA215` | index-row-missing-exhibit | an `evidence/README.md` row references a body file that does not exist | warning | no |

Some drift fires one rule in each layer: `reason:` on a `dead_end` is both
`ARA002` (fixable) and `ARA206`; `justification:` / pivot `from:` / `to:` /
`trigger:` are both their `ARA0xx` alias rule and `ARA201`. `--fix` resolves
both at once. `ARA101` is listed for completeness: `ara check` exits `2` before
parsing when the tree file is unreadable, so in practice only library callers of
`parse_dir` see it.

### How codes are attached

- A validate `Diagnostic` gets its code at its construction site:
  `ParseReport::error`/`warn` take a `RuleCode` (and debug-assert that its
  default severity matches). The code is `#[serde(skip)]` and not part of
  `Display`, so `ara validate`'s human and `--json` output stay byte-identical;
  `check` renders it explicitly.
- A format-lint diagnostic keeps its `LintRuleId` (used by the fixer);
  `LintRuleId::code()` maps it into the registry.
- The registry is generated from one macro table, so each rule's code, name,
  layer, default severity, fixability, and summary live on one line.
  `RuleCode::ALL` enumerates every rule in code order; `"ARA107".parse::<RuleCode>()`
  / `RuleCode::from_code` look one up. [Per-rule config](#configuration-ara-checktoml)
  keys off these.
- A test triggers every validate-layer diagnostic site and asserts the set of
  codes it sees equals every `Validate`-layer entry in `RuleCode::ALL`, so a new
  rule cannot ship without a case that fires it.

## The fix-safety guard (load-bearing correctness)

Every `--fix` edit is computed in memory and written only after a guard passes.
The guards evaluate normalized parse outcomes:

- **`ARA001` (structural) — clean semantic no-op.** Both the base and candidate
  parses must be clean normalized manifests, and the manifests must be
  identical. An error-bearing or fatal outcome rejects the edit.

- **`ARA002`–`ARA007` (value-recovering) — targeted recovery.**
  These rules may operate on normalized error-bearing artifacts only when the
  complete candidate error multiset is a subset of the base error multiset.
  Error identity includes severity, logical path, and message, and containment
  preserves occurrence counts. A fatal base or candidate outcome rejects the
  edit without writing.

The normalized semantic delta must also be exact. The alias rules (`ARA002`,
`ARA003`, `ARA005`–`ARA007`) change only the target alias field from `None` to
`Some`. `ARA004` adds exactly one
intended claim, leaves nodes and links unchanged, and adds only bindings to that
recovered claim. When the canonical key is already present alongside the alias,
the rename would change more than the recovered field, so the candidate is
skipped (reported as a skipped fix, never written). Any rejected candidate
leaves the source byte-identical; accepted fixes retain the re-detection and
idempotence backstop, so a second `--fix` is a no-op.

These fixes rewrite source files only. They do not modify or regenerate Hub
output, static `trajectory.html`, or viewer assets.

## Configuration (`.ara-check.toml`)

Added for issue [#40](https://github.com/ARA-Labs/ara-cli/issues/40). An
optional TOML file tunes which rules run, which fixes `--fix` may apply, and at
what severity each rule is reported. No file means the built-in behavior above,
byte-for-byte. The code lives in
[`crates/ara-cli/src/check_config.rs`](../crates/ara-cli/src/check_config.rs);
`ara validate` never reads it.

### Discovery

1. `--no-config` — no file is read.
2. `--config <path>` — exactly that file (it must exist).
3. Otherwise `ara check` looks for `.ara-check.toml` in the artifact directory,
   then each parent directory up to and including the git repository root (the
   first ancestor that contains `.git`). The nearest file wins; files are not
   merged. If the artifact is not inside a git repository, only the artifact
   directory itself is checked, so a stray file in a home or temp directory is
   never picked up.

A config entry that cannot be read or is invalid is an internal failure
(exit `2`), reported on stderr with the file path before any fixes. Discovery
does not skip dangling or cyclic symlinks, directories, or inaccessible entries
to fall back to an ancestor config or built-in settings.

### Keys

Every key names rules by **selector**: a full rule code (`ARA107`) or a code
prefix (`ARA` followed by fewer than three digits): `ARA1` matches every
`ARA1xx`, `ARA21` matches `ARA210`–`ARA215`, and `ARA` matches every rule. A
selector that is malformed or matches no rule (`ARA999`, `ARA3`, `ara107`) is an
error, and so is any unknown key, so a typo can never silently turn a check
off.

| key | type | default | meaning |
| --- | ---- | ------- | ------- |
| `select` | list of selectors | `["ARA"]` | rules to report |
| `ignore` | list of selectors | `[]` | rules not to report |
| `fixable` | list of selectors | `["ARA"]` | rules `--fix` may apply |
| `unfixable` | list of selectors | `[]` | rules `--fix` must not apply |
| `[severity]` | table: selector → `"error"` \| `"warning"` | each rule's default | reported severity |

How selectors combine (the `ruff` convention):

- **`select` / `ignore`.** For each rule, the longest matching selector across
  both lists decides. If the two lists tie, `ignore` wins. A rule with no
  matching `select` entry is off, so `select = []` disables everything.
  Example: `select = ["ARA", "ARA211"]` plus `ignore = ["ARA2"]` turns off
  every warning except `ARA211`.
- **`fixable` / `unfixable`.** The same rule. This only narrows the built-in
  set: a rule without a fix (`ARA1xx`, `ARA2xx`) never becomes fixable.
- **`[severity]`.** The longest matching key wins (`ARA2 = "error"` plus
  `ARA207 = "warning"` promotes every warning except `ARA207`).

### Effect on output and exit code

- **Disabled rule.** Its findings are dropped from the human output, from
  `--json`, and from the summary counts. They never affect the exit code, and
  `--fix` does not apply them.
- **Severity override.** A validate finding is reported at its new severity.
  It prints as `ARA201 error: …`, and in `--json` it moves between
  `validate.errors` and `validate.warnings` with an updated `severity` field.
  A format-lint finding prints its overridden severity
  (`ARA002 warning [fixable]: …`), and its `--json` entry gains a `severity`
  key. That key is absent when the rule keeps its default (`error`), so the
  no-config JSON is unchanged.
- **Unfixable rule.** It is still reported, but without `[fixable]`, with
  `"fixable": false` and `"fix": null` in `--json`. `--fix` leaves it in
  place and does not list it as `skipped`.
- **Exit code.** The run fails (exit `1`) when any reported finding has
  severity `error`, or under `--strict` when any finding remains at all.
  Format-lint rules default to `error`, which is why an unfixed fixable issue
  fails the run by default. Demoting one to `warning` lets the run pass unless
  `--strict` is set.
- **Summary counts.** `errors` / `warnings` count every non-fixable finding at
  that severity: validate findings plus any `unfixable` lint finding. `fixable`
  counts the lint findings `--fix` may still apply. When a config was loaded,
  the `--json` report also carries a top-level `config` key with its path.

### Example

```toml
# .ara-check.toml (at the repo root or in the ARA directory)
ignore    = ["ARA212"]       # related-work entries without a DOI are fine here
unfixable = ["ARA001"]       # report `root:` but let a human migrate it

[severity]
ARA2   = "error"             # treat every warning as an error...
ARA207 = "warning"           # ...except unresolved claim refs (no claims.md yet)
```

## Exit codes

- `0` — clean: no error-severity findings (and, under `--strict`, no warnings).
  With no config, this means no errors and no unfixed fixable issues.
- `1` — error-severity findings present. By default that is any validate error
  **or** any unfixed fixable issue. In non-`--fix` mode this is the "run `--fix`"
  signal (like `ruff check` without `--fix`). A [config](#configuration-ara-checktoml)
  can change which findings count as errors.
- `2` — internal failure: target missing / not a directory / unreadable
  `trace/exploration_tree.yaml`, an unreadable or invalid config file, a JSON
  serialization error, or a failed `--fix` write.

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
- **Inline suppression comments** (e.g. a per-line `# ara: noqa ARA201`).
  [`.ara-check.toml`](#configuration-ara-checktoml) works at the level of the
  whole project; per-finding suppression is not implemented.
