# ara-cli

The command-line runtime for the [ARA viewer](https://github.com/ARA-Labs/ara-cli).
Installs a binary named `ara`.

```bash
cargo install ara-cli
ara validate path/to/artifact            # parse + validate an ARA directory
ara validate path/to/artifact --json     # machine-readable diagnostics
ara validate path/to/artifact --strict   # treat warnings as failures
ara layout   path/to/artifact --json     # positioned manifest as JSON
ara serve    path/to/artifact            # live-reloading web viewer
```

`ara validate` parses `trace/exploration_tree.yaml` (+ optional
`logic/claims.md`) and reports errors/warnings, exiting non-zero on any error.

## `ara serve`

Serves an ARA directory with the web viewer and reloads it in place as you edit:

```bash
ara serve path/to/artifact               # http://127.0.0.1:8080
ara serve path/to/artifact --port 3000   # choose the port
ara serve path/to/artifact --assets dist # serve an on-disk viewer build instead of the embedded one
ara serve path/to/artifact --poll        # polling watcher (network / bind mounts)
```

- The viewer is **embedded in the binary**, so `cargo install ara-cli` is all you
  need. `--assets <dir>` serves a `trunk`-built `dist/` instead (dev / Docker),
  with precompressed brotli/gzip.
- `GET /api/manifest` returns the parsed manifest as JSON with a strong `ETag`
  (`304` on `If-None-Match`); `GET /api/figure/<path>` streams evidence figures
  (range requests, sandboxed to `<dir>/evidence`).
- Editing the artifact reparses it (debounced) and pushes over the `/api/live`
  WebSocket; the graph updates **without losing pan/zoom or selection**.

License: MPL-2.0
