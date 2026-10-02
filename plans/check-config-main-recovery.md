# Land the existing #40 checker configuration on main
**Date:** 2026-10-02

## TL;DR
Issue [#40](https://github.com/ARA-Labs/ara-cli/issues/40) already has an implementation in [PR #93](https://github.com/ARA-Labs/ara-cli/pull/93). GitHub merged that PR into `feat/43-check-rule-codes`, after [PR #92](https://github.com/ARA-Labs/ara-cli/pull/92) had landed on `main`. The config commit is therefore absent from `main`. Recover that commit in a PR targeting `main`, preserving its documented behavior and tests.

## Problem

The current `main` is `598361ca94aac22b0f907c9954477d4a8b8e5758`, with workspace version `0.1.18`. Its `ara check --help` lists `--fix`, `--strict`, and `--json`, but no config flags. PR #93's merge commit is `21aaa21099d76b46eb3fd678873127fc618a7975`, on the still-existing remote branch `feat/43-check-rule-codes`. Issue #40 remains open because the implementation did not reach the default branch.

## Constraints

Reuse PR #93 instead of designing another config format. Keep the rule registry and `ara validate` output from #92 unchanged. Preserve the existing fix-safety guards, and apply configuration before choosing fixes or calculating report counts and exit status. This plan adds no rules, inline suppressions, or new configuration keys.

## Proposed approach

Create `fix/40-land-check-config` from current `main` and cherry-pick `21aaa21099d76b46eb3fd678873127fc618a7975`. That is the config-only commit; do not merge the whole stale stack base, which also contains the pre-squash #43 history. Resolve any conflicts against the current rule registry without changing PR #93's contracts.

The preserved configuration keys are `select`, `ignore`, `fixable`, `unfixable`, and `[severity]`. Selectors are exact rule codes or prefixes. The longest matching selector wins, and the negative list wins a tie. Discovery checks the artifact directory, then its ancestors through the git root, choosing the nearest file without merging files. Outside a git repository it checks only the artifact directory. `--config` reads one explicit file; `--no-config` bypasses discovery. Invalid files, selectors, or unknown keys exit `2` and name the file. Disabled findings do not affect output, fixes, counts, or exit status. Severity controls failure, with `--strict` failing on any remaining finding. Without config, behavior remains unchanged.

The affected files are PR #93's twelve files: `CHANGELOG.md`, `Cargo.lock`, root and CLI `Cargo.toml`, `README.md`, CLI `check.rs`, new `check_config.rs`, CLI `main.rs`, new CLI `tests/check_config.rs`, core `fix.rs` and `lib.rs`, and `docs/stage-5-check.md`. The commit already bumps the workspace to `0.1.19`. Keep that bump if it is still the next patch when the PR is created.

## Alternatives considered

| Option | Benefit | Cost |
| --- | --- | --- |
| Cherry-pick the config-only commit | Reuses the implementation and retains a focused diff against main | Requires fresh verification after recovery |
| Merge the old stack base | Retains all branch ancestry | Includes superseded #43 commit history after its squash merge |
| Rewrite configuration | Allows a different design | Duplicates implemented scope and invalidates prior verification |

## Tradeoffs

The recovery PR must explain why an already-merged PR needs another landing PR. Its body links #93 and uses `Closes #40`. An existing merged PR is evidence of implemented scope, not evidence that a recovered branch passes current checks.

## Migration

1. Recover the config-only commit on a branch based on `main`.
2. Run the existing config unit tests, CLI integration tests, and core fix filtering tests, followed by workspace tests, formatting, and Clippy. Check embedded viewer freshness; no frontend behavior changes in this PR.
3. Run the actual CLI on temporary artifact copies: no config, nearest discovered config, explicit config, `--no-config`, ignored rules, severity promotion/demotion, unfixable rules under `--fix`, and invalid config. Inspect stdout, JSON, exit codes, and changed file bytes. Include the current ARA005 through ARA007 rules.
4. Compare no-config behavior with the baseline binary built from `main`, including output, exit status, and post-fix source bytes. Keep `ara validate` unchanged with config present.
5. Review the recovered diff, open the PR against `main`, and confirm the issue closes only when the implementation reaches the default branch.

## Next Steps

Approve recovery of #93's existing behavior. Land this PR first; the figure and math PRs may stack on it, with each functional PR taking the next patch version and adding its own changelog entry. Once this recovery is implemented, remove this temporary plan; the permanent configuration design remains in `docs/stage-5-check.md`.
