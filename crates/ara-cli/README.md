# ara-cli

The command-line runtime for the [ARA viewer](https://github.com/EYH0602/bara).
Installs a binary named `ara`.

```bash
cargo install ara-cli
ara validate path/to/artifact            # parse + validate an ARA directory
ara validate path/to/artifact --json     # machine-readable diagnostics
ara validate path/to/artifact --strict   # treat warnings as failures
```

`ara validate` parses `trace/exploration_tree.yaml` (+ optional
`logic/claims.md`) and reports errors/warnings, exiting non-zero on any error.
`ara serve <dir>` lands in a later stage.

License: MPL-2.0
