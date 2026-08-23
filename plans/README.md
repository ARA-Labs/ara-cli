# `plans/`

Working directory for in-flight design work. One markdown file per planned
change: the problem background, the proposed solution, and the implementation
steps, committed and reviewed **before** implementation starts (see
`CLAUDE.md`).

A plan is temporary. Once it is fully implemented, it is rewritten as a design
doc in [`docs/`](../docs) and removed from here — so an empty `plans/` means
nothing is mid-flight, not that the convention is unused.

Recently retired plans and where they landed:

| Plan | Shipped in | Design record |
|---|---|---|
| Homebrew pre-built binaries | #26, #36 (`0.1.6`) | [`docs/release-distribution.md`](../docs/release-distribution.md) |
| Rename "Recipes" panel to "Solution files" | #38 (`0.1.10`) | [`docs/hub-parity.md`](../docs/hub-parity.md) |
| Exhibit markdown rendering | #45 (`0.1.11`) | [`docs/exhibit-markdown-rendering.md`](../docs/exhibit-markdown-rendering.md) |
