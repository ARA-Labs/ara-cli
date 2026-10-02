# PR 02: ship structural read commands and the CLI contract
**Date:** 2026-10-01

Implementation record: [02-read-commands](../../docs/agent-cli-interface/02-read-commands.md). Remaining acceptance: The first pinned status timing sample misses 100 ms; the original fixed limit remains.

Observed proof: [delivery verification](../../docs/verification/agent-cli-2026-10-02/README.md).


Status: **approved** 2026-10-01. Repository: `ARA-Labs/ara-cli`. Depends on [PR 01](../../docs/agent-cli-interface/01-read-model.md). Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

Ship `status`, `ls`, `show`, `path`, `refs`, and `open` together over the extended manifest. Add one directory resolver and one versioned output contract for all new commands. Implement structural queries and reference scanning in pure `ara-core::query` code; keep argument parsing, file access, and rendering in the CLI. Prove the commands on the pinned real artifact, malformed corpus cases, and generated trees before later writes depend on them.

## Problem

The current command enum contains validate, layout, check, and serve. Agents cannot retrieve an entry or its ancestors without opening files. Discovery precedence, machine-readable errors, prose shortening, and relation semantics need one published contract before multiple command families grow independently.

`parse_dir` returns only a report on semantic errors. Status still needs useful diagnostic counts, and the real corpus includes broken graphs. Reads must report incomplete structural data explicitly instead of silently using a normalized model that dropped duplicate-ID subtrees.

## Constraints

Existing commands retain positional paths and their established output. New commands accept `-C`, then `ARA_DIR`, then upward discovery. At each ancestor, test the ancestor itself before its `ara/` child; stop at the nearest valid candidate. An explicit invalid path fails rather than falling through to a different artifact. Resolve relative environment paths against the invocation directory.

Successful JSON is on stdout and errors are on stderr, each as one parseable object. Preserve `format` even with `--fields`. New command exits are 0 for success, 1 for unknown IDs or artifact problems, and 2 for discovery or I/O failure. Clap argument failures also need JSON formatting when `--json` was requested; use fallible argument parsing rather than bypassing the shared error renderer. Do not modify old command error formats.

## Proposed approach

Add `crates/ara-core/src/query.rs` (new, public, wasm-safe) and export it through core's `lib.rs`. Build a borrowing `QueryIndex` once per invocation with ID lookup, Child parent/children adjacency, DependsOn adjacency, bindings, sessions, and structured reverse references. Add `crates/ara-cli/src/agent.rs`, `context.rs`, `output.rs`, and a thin `lib.rs` (all new); extend `main.rs` with the six commands. The CLI library exposes reusable native loading and rendering contracts without moving unrelated serve/check code. All names here are proposed APIs, not current symbols.

Proposed success envelopes contain `format` plus `entries` for ls/show, `steps` for path, `structured` and `prose` for refs, `items` for open, and `counts`, `next_ids`, `latest_session`, and `diagnostics` for status. Entry records contain `id` or an existing kind-specific `key`, `kind`, `title`, `source`, selected typed fields, and requested relations. Concepts use their existing term keys; solution bodies use scoped file identities. `E` and `T` entries get experiment-plan and taste kinds alongside the kinds explicitly listed in the parent. Expose related work and solution/problem document selectors needed by PR 12's inventory without minting protocol IDs.

1. Define discovery, error codes, JSON fields, relation names, and field projection in `docs/agent-cli.md`. A proposed error object is `{"format":"ara.show/v1","error":{"code":"unknown_id","message":"...","id":"N999"}}`. Keep command identity independent of diagnostic rule codes.
2. Implement source-order queries. `ls` combines all filters by intersection; `--under` includes descendants through Child edges, excludes the anchor itself, and never treats DependsOn as nesting. `--since` compares published calendar dates inclusively; undated entries do not match date filters. `--status` and `--provenance` omit entries without those fields. Unknown types, fields, and relation names fail explicitly.
3. Implement `show` in argument order. Resolve all selectors before emitting output so a missing ID cannot produce ambiguous partial success. `--with parents` means the immediate nesting parent, `children` immediate children, `claims` structured claim bindings, and `sessions` citing session records. `path` follows Child edges from root through the target, including both endpoints, not cross-dependencies.
4. Add a shared token scanner that emits exact byte ranges, source field, and certainty. Match case-sensitive complete IDs, including boundary checks against longer IDs, filenames, and scoped references. Do not collapse `N1` and `N01`. If a short-form mention such as `E2` is a possible reference to `E02`, return it as a possible match with its literal spelling; do not claim equivalence or rewrite it automatically. PR 08 must review ambiguous mentions separately.
5. Implement `refs`: typed fields first with certainty `certain`, then source prose with certainty `possible`. Retain source path, field, literal match, and bounded context. Deduplicate identical locations, not distinct mentions. Scan supported knowledge-layer text in one pass; do not scan or mutate arbitrary code/evidence bodies.
6. Implement `open` with explicit reason codes: childless questions, unpromoted observations, hypothesis claims, pending bindings, and stale observations. Stale means no reference for at least three subsequent distinct session-days, not three sequence files on one day. Creation counts as the initial reference. No read command sets stale flags or promotes entries.
7. Implement `status` counters and next-ID advice for each existing prefix. Allocate the numeric maximum plus one with minimum two-digit formatting; do not fill gaps. Report the latest session by date then sequence. On invalid artifacts, status returns diagnostic counts and explicit incomplete counts with exit 1; other queries return a structured artifact error rather than pretend completeness. Make I/O failure exit 2 distinct from malformed content exit 1.
8. Centralize output projection and one-line excerpts. Collapse whitespace and cap prose at a documented UTF-8-safe bound, proposed 160 characters; `--full` restores exact prose. The format/version key and identity remain present. Add snapshots for meaningful result contracts, not prose decoration.

## Alternatives considered

Putting queries in command handlers duplicates graph semantics and prevents wasm reuse. Use the pure core module. A regex search per requested ID misses source ranges needed for later merge rewriting; one scanner serves both reads and merge planning.

## Tradeoffs

Status can describe an invalid artifact, while general structural queries refuse incomplete results. This is less permissive than returning partial trees but prevents agents from treating dropped entries as absent. Exact IDs reduce false references; short-form aliases remain uncertain and require judgment.

## Migration

These are additive commands. Keep validate, layout, check, serve, and existing JSON shapes unchanged. Add the pinned real artifact under the core fixture tree with attribution. Use the shared patch bump, Cargo.lock refresh, docs/changelog, and core/wasm checks. Neither reads nor ID advice reserve IDs; only PR 03 writes allocate under lock.

## Verification and acceptance

Add unit fixtures for multiple roots, dependency-versus-child direction, filtering intersections, inclusive dates, missing IDs, multiple show arguments, all open reasons, three session-days versus multiple daily sessions, N1/N10/N01 boundaries, uncertain E2 mentions, and projection with `--full`. Add subprocess integration coverage in `crates/ara-cli/tests/agent_reads.rs` (new, proposed target) for path/env precedence, cwd discovery, stdout/stderr separation, all exit classes, and unchanged existing command contracts. Snapshot the six real-fixture JSON results with reviewed concrete IDs and relations.

Exercise the actual release binary with `status --json`, `ls --type dead_end --under N12 --json`, `show N62 --with parents,children,claims,sessions --json --full`, `path N85 --json`, `refs C05 --json`, and `open --json` on the pinned fixture after confirming those selectors exist at the pin. Compare result identities and edges with a reviewed fixture oracle. Invoke every command on the 32-artifact paperbench sweep and require a defined success/error outcome, never a crash or hang. Do not equate a corpus error with a clean parse.

Add deterministic synthetic artifacts with 100, 1,000, and 10,000 nodes, including broad and deep shapes. Time subprocess start through output consumption using the prebuilt release binary; compilation is outside timing. Enforce under 100 ms on the real fixture and under 1 s at 10,000 nodes for each command on the documented CI performance runner. Run timing separately from ordinary unit tests so debug builds cannot create false failures. Record runner, toolchain, artifact, invocation, and measurements; missing the limit blocks acceptance rather than silently weakening it.

## Next Steps

Review discovery tie-breaking, projection keys, excerpt bound, invalid-artifact status semantics, and scanner boundaries before publication. After merge, [PR 03](../../docs/agent-cli-interface/03-guarded-node-writes.md) and [PR 10](../../docs/agent-cli-interface/10-keyword-search.md) can proceed independently once their own gates are met.
