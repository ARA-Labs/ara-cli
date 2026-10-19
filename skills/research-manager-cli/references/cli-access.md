# CLI-only native artifact access

This is an access contract, not a new research method. All variants retain their pinned source procedures, evidence standards, roles and stopping rules. The live pin is `e52a925e9d03b4ada3008653e72f99b04116fca2`, not a verified reproduction of the historical paper condition. Binary coverage and upstream protocol decisions are **proposed/pending review**, not accepted or measured. The variant lock records actual digests; final CLI revision and observed proof must be supplied after integration.

## Boundary and failures

Use `ara -C <artifact> <command>` (or the same explicit artifact via `ARA_DIR`). Every read/write of `PAPER.md`, `logic/`, `trace/`, `staging/` and registered additional knowledge documents goes through `ara`, including initialization, session indexes, reasoning and taste. Do not use Read/Grep/Glob/Edit/Write, Python or shell filesystem operations on those knowledge bodies. Reading the skill's supplied instruction/reference/template pages remains direct. Source inputs and actual `rubric/`, `src/`, `data/`, `evidence/` bodies are not `ara` documents: read, search and write them directly with file tools where the baseline permits them (`ara show --document` rejects `rubric/`, `src/` and `evidence/` paths with `invalid_document` and lists those roots in `error.details.file_access`); the reader still cannot execute source code or leave its single ARA. Never use a source/evidence exemption to open a knowledge path.

Exit 1 means rejected/unknown/invalid operation; exit 2 means setup/IO failure. Report the exact failure and apply the source skill's original stopping/repair rule. A read that prints its results with `incomplete` (`complete: false`; brief text: `incomplete: <path> (<codes>)`) also exits 1: use the printed results, and treat only the named optional-layer files as not fully represented. Never fall back to direct knowledge-file access or retry automatically with a changed scientific payload.

## Shell calls

Run each `ara` command as its own shell tool call: one invocation with quoted arguments and nothing else on the line. Shell calls are for `ara` only: list, search and read `rubric/`, `src/` and `evidence/` with the file tools (Read, Grep, Glob, LS), not with shell `ls`, `find`, `grep` or `cat`. Do not pipe (`|`), redirect (`>`, `2>&1`), chain (`&&`, `;`), `cd` first, or glob (`*`, `?`, `[...]`). Quote every argument that contains spaces, `#`, `$` or other shell characters, such as `'logic/claims.md#C04'` and `'failure boundary'`. Some harnesses reject an unquoted `#`, and in a POSIX shell a `#` after a space starts a comment that silently drops the rest of the line. Some harnesses enforce this rule and reject a composed command before it runs. Other shells may allow composition, but the single-call form works in all of them, so use it everywhere. The rule limits the shell line, not the response: issue independent `ara` commands as separate shell tool calls in the same response.

Bound output with `ara`'s own options instead of `head`, `tail`, `grep` or `jq`: `ls <path>` lists one document instead of the whole artifact; `find --limit N` caps results and `find --context N` adds surrounding lines (long form only, because `-C` selects the artifact); `show --lines A:B` selects a line window and `show --max-bytes N` sets the response budget. A paged `show` block ends with `lines: S-E of N; truncated; next: --lines X:`. Rerun the same selection with that `--lines` value (and the same `--max-bytes`, if you set one). Do not read harness logs or temporary files to recover cut-off output.

## Reading: orient, search, read, cite

```sh
ara -C <artifact> status
ara -C <artifact> ls
ara -C <artifact> ls logic/claims.md
ara -C <artifact> find '<keyword query>' --context 2
ara -C <artifact> find '<keyword query>' --type claim --limit 5
ara -C <artifact> show 'logic/claims.md#C04'
ara -C <artifact> show --document logic/claims.md --source --lines 26:40
ara -C <artifact> show logic/solution/method.md --lines 40:120
ara -C <artifact> show trace/exploration_tree.yaml --lines 300: --max-bytes 8192
ara -C <artifact> show 'logic/claims.md#C03' E01 trace:N04
ara -C <artifact> show N01 --with path
ara -C <artifact> show C01 --with refs
ara -C <artifact> ls --unfinished
```

1. **Orient.** `ls` prints one line per knowledge document with its entry counts by kind (or heading count) and line count, then the direct-file roots. `ls <path>` lists that document's entries, or its heading addresses when it has none. `status` gives layer counts, next IDs and diagnostic codes.
2. **Search.** `find` is case-insensitive keyword search. Each result prints `<address> [<kind>] <source>` and the matching source lines (`N:`); `--context 2` adds neighbouring lines (`N-`). Narrow with `--type` and `--limit`.
3. **Read.** `show` an address that `ls`, `find` or an error printed. A native section prints `== <address>`, its `heading:` path, a `source_digest=… selector: …` line and its exact source bytes; a node, observation or session prints a projection and names the exact source read. Read the section or document you need. Brief `show` stops at 16 KiB and pages at whole lines; follow `next:` until you have read every line you rely on.

   `--lines A:B` counts lines of the selection you `show`, and `find` and `show` count differently. `find` prints line numbers of the whole source document (`N:` under `<source>`). On a claim, entry or heading address, `--lines 1:` starts at the first body line under that heading. So read around a `find` hit with `show --document <source> --source --lines A:B`, using the hit's numbers (the example above windows `logic/claims.md` around a hit on line 30). Do not pass a hit's numbers to `show <address> --lines`: they select other text, or fail with `line_out_of_range` when the body is shorter. Use `--lines` on an address only with numbers counted inside that selection, such as a `next:` value or its `lines: S-E of N` trailer. A node, observation or session address is a projection and rejects every `--lines` with `lines_unavailable`. Window the source document it names instead (`show --document trace/exploration_tree.yaml --source --lines A:B`).
4. **Cite** the address the output printed: `logic/claims.md#C04`, `trace:N01`, a canonical heading address such as `logic/problem.md#h/Problem/Observations`, or a document path. Never cite a generated display ID or a heading spelling you guessed.

**Fewer requests.** Put every keyword for one information need in one `find` query: `find` scores all its terms together (BM25), so `find '<term A> <term B> <term C>'` replaces three searches. `show` takes several addresses in one call, as in the example above (`show 'logic/claims.md#C03' E01 trace:N04`). Issue independent `ara` commands as separate shell tool calls in the same response. Skip `ls` when the question already names what to search for.

A miss returns `unknown_id`; a selector that matches several sections or entries returns `ambiguous_heading`. Both print `candidates:` that `show` accepts. Choose the candidate whose address or heading path is the one you need, or run `ls <path>` to see the document's headings, instead of guessing another spelling. An ambiguous selector is an error, not permission to take the first candidate. `invalid_document` on a `rubric/`, `evidence/` or `src/` path means a file tool reads it.

`ls` enumerates structured entries; `status` identifies the selected artifact and knowledge revision. Do not infer that typed-entry enumeration exhausts arbitrary documents: inspect the native document inventory supplied by `ls`, the root Layer Index and registered `knowledge_paths`. Read all documents required by the source procedure. `find` is only a retrieval aid, never semantic ranking. When a hit is too terse to judge, `show` its section or window its source document around the hit, so you judge the real prose, not a keyword coincidence.

Brief `show` text prints the exact source bytes of the selection. A node, observation or session address prints a projection; window the source document it names (such as `ara show --document trace/exploration_tree.yaml --source --lines A:B`) when you need its exact bytes. Use the original native anchor in the answer (`trace:N01`, `logic/claims.md#C01`, concept names and path/heading addresses); never cite a generated display ID. A typed projection is not exhaustive: read the YAML source when source spelling, unknown fields, comments, raw observation context or full history matters.

`show --with path` recovers root-to-node nesting; `depends_on` remains a separate cross-edge relation. `show --with refs` separates structured references from possible prose mentions. `ls --unfinished` is local unresolved context, not proof of freshness or remote community progress. Its observation rows retain `turns_since_reference`, `session_days_since_reference`, `last_reference_turn`, `reference_basis`, `evidence_sources`, `history_status` and `history_diagnostics`. A `null` count is unknown, never zero. Filters select rows without filtering the history used to calculate inactivity. No count decides closure or maturity. Cite only text you read, and preserve the original evidence standards.

## Read-only tasks

### Inspect ancestry, citations and imported identities

```sh
ara -C <artifact> show N01 --with path,parents,depends_on --json
ara -C <artifact> show C01 --with refs --fields relations --json
ara -C <artifact> show trace/exploration_tree.yaml --with refs --json
ara -C <artifact> show --identity 'peer:N01' --json
```

Read `entries[].relations.path` as ordered ancestry and `entries[].relations.refs` as `{target,structured,prose}`. Whole-document reference targets retain document-wide matching. `--fields relations` keeps the complete relation object, not a nested field projection. Multiple selectors keep request order and all must support each relation; non-node paths, heading/source relations and incompatible identity options reject. Identity mode takes exactly one positional address and returns `kind: "identity"`, `requested_address` and exact `resolved_target`, without requiring a readable current body. Every requested relation remains complete under a byte budget; `output_limit_too_small` requires raising `--max-bytes`, never accepting partial citations.
### Review unfinished work

Use `ls --unfinished --json` and consume `entries`, including `reasons` and all measured/unknown inactivity fields. An empty intersection stays `entries: []`. Ordinary brief `ls` lists documents; ordinary unfiltered JSON `ls` lists entries. Reading changes no stale or promoted state.
