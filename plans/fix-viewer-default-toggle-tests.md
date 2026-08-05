# Fix stale viewer default-toggle browser tests

## Problem background

CI has failed on `main` and on the current Dependabot pull requests in the
`viewer-web-test` job. The headless-Chrome suite reports 52 passing tests and
two failures:

- `layout_toggle_flips_active_segment` expects `stack` to be initially active.
- `display_toggle_flips_active_segment` expects `graph` to be initially active.

PR #67 intentionally changed the viewer defaults from `stack` + `graph` to
`split` + `tree`. It updated the native `LayoutMode` and `DisplayMode` tests,
but it did not update these two wasm browser tests. The shipped behavior is
correct; the browser-test expectations and nearby documentation are stale.
The same two failures occur on unrelated dependency-update PRs, which confirms
the dependency changes are not the cause.

## Reproduction

The existing browser tests are the regression cases. On commit
`b4f01d9acc41268ce3de0f50b4d0810e47200c1f`, run:

```bash
wasm-pack test crates/ara-viewer --headless --chrome --locked
```

Expected current result: `layout_toggle_flips_active_segment` and
`display_toggle_flips_active_segment` fail because their initial-state
assertions encode the pre-#67 defaults.

The GitHub Actions run that demonstrates the failure is
<https://github.com/ARA-Labs/ara-cli/actions/runs/31036993915>.

## Proposed solution

Keep the intended `split` + `tree` product defaults. Update the two browser
tests so each asserts the current default, then clicks the non-default segment
and verifies the signal, CSS class, and `aria-pressed` state all move together.
This preserves both initial-state coverage and toggle-interaction coverage.

Also update stale comments and the Stage 3 viewer design document so they no
longer describe `stack` + `graph` as the defaults.

This is a test-and-documentation correction only. It does not change shipped
binary behavior, public API, or dependencies, so it requires no version bump or
changelog entry under the repository's versioning rules.

## Implementation steps

1. Change `layout_toggle_flips_active_segment` to assert `split` initially and
   verify clicking `stack` activates `LayoutMode::Stack`.
2. Change `display_toggle_flips_active_segment` to assert `tree` initially and
   verify clicking `graph` activates `DisplayMode::Graph`.
3. Update stale default descriptions in `toolbar.rs` and
   `docs/stage-3-viewer.md`.
4. Run the focused wasm headless-Chrome suite.
5. Run formatting and the relevant native workspace checks.
6. Push the branch and open a draft pull request that links the failed Actions
   run and explains why no version bump is needed.

## Acceptance criteria

- All viewer browser tests pass under headless Chrome.
- The two toggle tests cover the `split` + `tree` initial state and transitions
  to `stack` + `graph`.
- Native viewer tests, formatting, and clippy remain green.
- Documentation consistently names `split` and `tree` as the defaults.
