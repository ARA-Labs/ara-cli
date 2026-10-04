# `ara.snapshot/v1` contract

Status: frozen in phase 1a of the [collaborative research series](../../plans/collaborative-research/README.md) on 2026-10-03.
It implements the approved capture rules of [plan 01](../../plans/collaborative-research/01-ara-snapshot.md).
The optional version store and the `create`/`list`/`export` command shape still wait for the plan-01 spike and the developer's re-approval. Until then, the delivered command is `ara snapshot --output <dir>`.

## Command

```sh
ara -C <artifact> snapshot --output <new-dir> [--json]
```

- `-C`, `ARA_DIR`, and upward discovery select the source artifact, the same way every other agent command does.
- `--output` is required. The path must not exist, and the source root and the output must not contain each other after their existing parents are resolved.
- `--json` prints the result object below on stdout. Errors use the standard `{"format":"ara.snapshot/v1","error":{…}}` object on stderr.

## Package layout

```text
<new-dir>/
├── ara/            # every captured file with its captured bytes and mode
└── snapshot.json   # the canonical manifest bytes
```

## Manifest

`snapshot.json` holds exactly the canonical bytes that the capture ID hashes, followed by no trailing newline.

| Field | Type | Meaning |
|---|---|---|
| `format` | string | Always `ara.snapshot/v1`. |
| `fingerprint_scheme` | string | Always `ara.artifact/v1`. |
| `fingerprint` | string | `ara_core::merge::fingerprint` of the capture: lowercase hex SHA-256, no prefix. It is the value `ara merge` records for the same bytes. It does not cover modes. |
| `capture_id` | string | `sha256:<64 lowercase hex>`, defined below. |
| `root` | string | Always `ara`: the package directory that holds the files. |
| `files` | array | One entry per captured file, sorted by `path` in byte order. |
| `files[].path` | string | `/`-separated UTF-8 path relative to the artifact root. |
| `files[].digest` | string | `sha256:<64 lowercase hex>` of the file bytes. |
| `files[].size` | integer | Byte length. |
| `files[].mode` | string | Four octal digits of `st_mode & 0o7777`, for example `0644` or `0755`. |
| `excluded` | array | Always `[".ara/", ".git/", ".ara-write-<pid>-<nonce>"]`, in that order. It documents the private namespaces and is part of the hashed manifest. |
| `diagnostics.errors` | integer | Error count. |
| `diagnostics.warnings` | integer | Warning count. |
| `diagnostics.items` | array | Every error in report order, then every warning in report order. |
| `diagnostics.items[]` | object | `{"severity","code","path","message"}`. `code` is the rule code that `ara check` prints. |

`files` lists only existing, nonprivate, regular files. Absent canonical placeholders are never listed or materialized. Empty directories are not captured.

The diagnostics are those `ara status` reports (`parse_dir_detailed`), evaluated on the captured bytes after they are written to the private staging copy, never by rereading the live source.

### Canonical encoding

The manifest is encoded with [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) (JCS): object keys sorted, no insignificant whitespace, minimal string escaping, and integers only. The manifest contains no floating-point numbers. Array order is the order given above.

### Capture ID

```text
capture_id = "sha256:" || hex(SHA-256("ara.capture/v1\0" || JCS(manifest without "capture_id")))
```

The capture ID binds the native fingerprint, every path, digest, size, and full mode, the exclusion list, and the diagnostics. It excludes machine-local paths, timestamps, store identifiers, and per-invocation fields. Two captures with the same bytes but a different mode share a `fingerprint` and have different capture IDs.

## Result object

```json
{
  "format": "ara.snapshot/v1",
  "capture_id": "sha256:…",
  "fingerprint": "…",
  "file_count": 42,
  "diagnostics": {"errors": 0, "warnings": 2},
  "output": "/abs/path/to/new-dir"
}
```

`recorded` appears only when a version store was used. The directory-only command never prints it, because it records no history.

## Errors

| Exit | Code | When |
|---|---|---|
| 1 | `stale_snapshot_input` | The source inventory, bytes, or modes changed between capture and the final recheck. No output or temporary directory remains. |
| 1 | `write.path`, `write.encoding` | A nonprivate symlink, a special file, or a non-UTF-8 path. Same codes the merge loader uses. |
| 2 | `pending_transaction` | A prepared transaction exists in the source. Recovery stays with a guarded writer. |
| 2 | `output_exists` | `--output` exists, including when another process creates it before publication. That directory is never replaced. |
| 2 | `overlapping_roots` | The source and output contain each other, also through a symlinked parent. |
| 2 | `io_error` and lock failures | Unreadable files, lock failure, or output I/O failure. |
| 2 | `snapshot_unsupported_platform` | The platform cannot represent four-digit octal modes. |

Reserved for the optional store, not emitted by the directory-only command: `unknown_snapshot`, `snapshot_store_unavailable`, `snapshot_store_disabled`.

## Procedure

1. Resolve the source root and the output through its existing parent; reject an existing output or overlapping roots.
2. Take `ArtifactLock`, then reject a prepared transaction.
3. Capture with `ArtifactSnapshot::load_complete`, recording bytes, digests, and full modes.
4. Write the capture into `<output-parent>/.<name>.ara-snapshot-<pid>-<nonce>/ara/` and restore each mode.
5. Evaluate the diagnostics on the staging copy, build the manifest, and write `snapshot.json`. Sync every file and directory.
6. Reload the staging copy with `load_complete`; any difference in inventory, bytes, modes, fingerprint, or capture ID is an internal error.
7. Reload the source with `load_complete`; any difference in inventory, bytes, or modes is `stale_snapshot_input`.
8. Rename the staging directory to `--output` with an atomic no-replace rename (`renameat2(RENAME_NOREPLACE)` on Linux, `renamex_np(RENAME_EXCL)` on macOS), then sync the output parent.

A failure before step 8 removes the staging directory and leaves no output. A failure while syncing after step 8 reports `io_error`; the output may then exist and is complete and verified.

The lock serializes cooperating `ara` writers only. A point-in-time package also needs the caller to stop direct writers (code tools, background jobs) from step 2 through the end of the runner's package freeze. The double load in steps 3 and 7 detects observed changes; it does not prove that no direct writer interleaved.

## Privacy predicate

Capture and the fingerprint share one predicate, `ara_core::write::source::private_name`: a path component named `.ara` or `.git`, or a reserved write-temporary name `.ara-write-<pid>-<nonce>`. Every loader applies it before descending into a directory or reading a file, so a snapshot never inspects bytes that its fingerprint ignores. The fingerprint scheme `ara.artifact/v1` is unchanged.

## Zero-cost obligations

No command other than `snapshot` captures modes, writes a staging copy, or touches `.ara/vcs/`. The measurement policy that checks this is [zero-cost-measurement.md](zero-cost-measurement.md).
