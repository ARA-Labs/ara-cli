# Agent CLI design and command reference

The agent interface reads and edits native ARA files offline. It uses one source
snapshot per command, preserves unrelated source bytes, and records imported
identities and mutable changes so later reads and merges can resolve them.
Scientific experiments, installed-agent execution, and human protocol approval
are separate acceptance gates. The delivery evidence records their status.

## Selecting an artifact and reading it

`-C <directory>` takes precedence over `ARA_DIR`. Otherwise discovery walks the
current directory's ancestors, checking each ancestor and its `ara/` child for
`trace/exploration_tree.yaml`. Explicit roots must be real directories. Reads
never create a lock, journal, or artifact structure. Explicit source-document
reads can inspect a partially initialized root.

Agents read the brief default text; programs add `--json`:

```sh
ara -C ./ara status
ara -C ./ara ls
ara -C ./ara ls logic/claims.md
ara -C ./ara ls --type question --under N01 --status open
ara -C ./ara find 'failure boundary' --context 2
ara -C ./ara show C01 'logic/solution/architecture.md#h/Architecture/A%2FB'
ara -C ./ara show --document logic/problem.md --source
ara -C ./ara show logic/problem.md --lines 120:200 --max-bytes 32768
ara -C ./ara path N12
ara -C ./ara refs C01
ara -C ./ara open

ara -C ./ara status --json
ara -C ./ara show N01 C01 --full --json
ara -C ./ara show --document logic/problem.md --source --full --json
ara -C ./ara show --document logic/solution/architecture.md \
  --heading Architecture --heading 'A/B' --full --json
ara -C ./ara find 'failure boundary' --limit 10 --full --json
```

| Command | Behavior | Brief default text |
|---|---|---|
| `status` | Layer counts, diagnostics and advisory next IDs | Counts and next IDs only when complete; error and warning counts with rule codes |
| `ls` | Source-order entries; intersecting type, subtree, date, status and provenance filters | No arguments: one line per knowledge document with entry counts by kind (or heading count) and line count, then the direct-file roots. `ls <path>`: that document's entries, or its heading addresses when it has none. Filters: matching entries |
| `show` | Entry projection, relations via `--with`, full body or bounded native document; `--lines A:B` windows and `--max-bytes` budgets | One labeled block per selection: native source with its `source_digest`, or a projection without a digest; 16 KiB budget, paged at whole lines |
| `path` | Root-to-node nesting, with cross-edges kept distinct | Root-to-node IDs, indented by depth |
| `refs` | Typed references with source spans, separately reported possible prose mentions; classified by the writer's citation rules ([Citation repair](#citation-repair-for-restructures)) | Referencing ID, field, `source:line`, literal; prose mentions labeled as possible |
| `open` | Unfinished questions/experiments, unpromoted observations and active continuity; observation rows add measured inactivity ([below](#observation-inactivity-in-open)) | Address, kind, reasons, title; observation rows end with `turns=… days=… last_reference=… history=…` |
| `find` | Stateless keyword ranking over loaded knowledge | Ranked addresses with one-based source lines; `--context N` adds merged context |
| `resolve` | Resolve a qualified imported identity through the portable identity records | The resolved ID |

With `--json`, results use command-specific `ara.<command>/v1` JSON formats.
Success goes to stdout; a JSON error goes to stderr with `code`, `message`,
and applicable ID, line or details. Exit 0 means completion, exit 1 means a
semantic rejection or unresolved merge, and exit 2 means setup, argument, I/O
or lock failure. `--fields` projects supported fields while retaining
transaction/result identity. JSON excerpts are bounded to 160 Unicode
characters; `--full` retains source content. Incomplete source representation
must not be reported as success.

### Observation inactivity in `open`

Every observation row of `ara.open/v1` adds these fields to its existing ones
(plan 19 D1). The envelope and the other fields do not change.

| Field | Meaning |
|---|---|
| `turns_since_reference` | Fully logged turns strictly after the latest attributable reference, through the latest provably ordered turn; `null` when unknown |
| `session_days_since_reference` | Distinct later written dates with at least one logged turn; `null` when unknown |
| `last_reference_turn` | `session#turn` of that reference, when turn order proves which one is latest |
| `last_reference_date` | Written date the day count starts after |
| `reference_basis` | `structured`, `literal`, `staging_timestamp`, or `null` |
| `evidence_sources` | Every matched occurrence: `source`, `line`, `field`, `session`, `turn`, `date`, `basis`, `literal`, `target`, `status` (`attributed`, `unresolved`, or `before_staging` for a reference stamped before the staging instant), optional `resolved_via` |
| `history_status` | `complete` (both counts known), `missing` or `ambiguous` |
| `history_diagnostics` | Why a count is `null`: `code`, `status`, `message`, `count` (occurrences; a range of unstamped turns counts every turn), up to three `examples` |

A reference is the observation's exact ID, or the exact ID of a node in its
`bound_to`, either as a typed `id`/`entry` field of a turn row (`structured`)
or as an exact token in caller-written turn text (`literal`). `O011`, `XO01`
or a topic word are not references. The CLI does not decide whether a topic
was semantically revisited; the five-turn topic-abandonment rule stays the
caller's judgment.

The timeline comes from the session records, the per-turn stamps (a session's
`last_turn` for its latest turn and the archived `session_metadata` in
`trace/pm_reasoning_log.yaml` for each turn) and, for imported sessions,
`trace/aliases.yaml`. It never uses numeric IDs, file times, index totals or
lexical session order. Turn text is the turn's typed rows (`events_logged`,
`ai_actions`, `claims_touched`, `logic_revisions` `entry`/`note`,
`key_context`), the summary and new `open_threads`/`ai_suggestions_pending`
items that turn wrote (the first turn also owns the summary the session was
started with), and its reasoning notes. Generated counters, the session index,
unchanged archived copies, revision `before`/`after` values and
`observation.mark_stale` evidence records are not references.

- **Turn order.** Within a session the turn number orders turns and the stamps
  must agree. Sessions are ordered only when their turn intervals are separated
  by instant. Overlapping intervals, equal timestamps across sessions, a turn
  without a stamp, a legacy record without turn identities, a session named in
  the reasoning log or index without a record, contradictory metadata
  (including a reasoning entry naming a turn beyond the session's
  `turn_count`, or a session that archives its turns but claims more than
  its archive and `last_turn` reach), or an
  unresolved imported literal make the affected turn count `null`. Overlap
  entirely before the latest reference does not.
- **Starting point.** The latest attributable reference. The structured
  staging event (`events_logged` row with the O ID) is itself one. A
  reference whose turn is stamped strictly before the staging `timestamp`
  predates the observation: it is listed as `before_staging` and counts for
  neither turns nor days; one at the same instant is the staging turn. With a
  date-only staging timestamp, a reference on an earlier date is
  `before_staging`, and a latest reference on the staging date itself makes
  the turn count `null` (`history.staging_position`); days still count after
  the staging date. Without any
  reference the staging `timestamp` is used (`staging_timestamp`): only turns
  provably after that instant count, and a turn with the same instant or a
  date-only timestamp makes the turn count `null`. No reference and no
  timestamp make both counts `null` (`history.creation_evidence_missing`).
- **Days.** Counted separately from turns: the later of the latest reference
  date and the staging date, then each later date with a logged turn. Empty
  sessions and calendar dates without a logged turn do not count. Overlapping
  sessions (concurrent agents, see plan 14) leave the day count known. It is
  `null` only when a logged turn has no date, a reference has no attributable
  date, an unresolved literal may fall after the start, or a session's
  `turn_count` is unreadable (whether it logged a turn is unknown).
- **Imported literals.** In a session that `trace/aliases.yaml` records as
  imported, an ID-shaped token is attributed through the import's alias for
  that original ID (`resolved_via: "alias:<source>:<original>"`). Without an
  alias it is `unresolved`; it never silently matches a local ID.

`stale_observation` stays in `reasons` whenever the stored flag is `true`,
whatever the current history proves, and is added when a known day count is 3
or more. `open` never promotes, discards or marks anything stale. The history is
computed only when the artifact has observations, from the sources this read
already loaded plus the raw bytes of `trace/aliases.yaml` (an undecodable
ledger is `history.alias_invalid`, not a read failure). A session's turns are
held as a count plus the stamps actually recorded, so a huge `turn_count`
costs nothing, and every turn total uses checked arithmetic (overflow is
`history.contradictory`, never a saturated count). It takes no lock and opens no write
state. Codes: `history.invalid_source`, `history.alias_invalid`,
`history.legacy_session`, `history.turn_stamp_missing`, `history.contradictory`,
`history.session_missing`, `history.overlap`, `history.undated_turn`,
`history.reference_undated`, `history.reference_turn_unknown`,
`history.unresolved_reference`, `history.creation_evidence_missing`,
`history.staging_position`.

### Brief text output

Without `--json`, `status`, `ls`, `show`, `path`, `refs`, `open` and `find`
print address-led text ([`brief/`](../crates/ara-cli/src/brief)). Each item line
starts with an address `show` accepts: a native ID (`C04`), a heading address
or a document path. A key that another loaded entry shares (claim `C01` and
concept `C01`) is qualified by its source so it reads back that one entry:
the section's cited form (`logic/claims.md#C01`, `logic/concepts.md#C01`, or
the canonical heading address when the key needs escaping) for a
heading-backed entry, and `path#ID` (`trace/exploration_tree.yaml#N02`) for
any other. A key no other entry has stays bare. This applies to `ls`, `find`,
`open`, `path`, `refs` (the target and each referencing entry), a `show`
projection header and miss candidates; `--json` rows keep their `id`/`key`.
Data only the text needs is computed only without
`--json` (or, for `show`, with a JSON bound; see below). `--fields` keeps its row meaning: with it, reads print the projected
rows in the previous row text (tab-separated rows; `show` rows as JSON) and
the same once-per-command diagnostics summary as unprojected reads.
Write commands keep their previous text.

`show` prints one block per selection. The block starts with `== <address>
[<label>]`. A native source selection (a document, a heading, or an entry that
heads exactly one Markdown section, such as `C04` or `H18`) then prints the
`heading:` path, a metadata line and the exact selected bytes, in full when
they fit the [read budget](#bounded-and-resumable-reads):

```text
== logic/claims.md#C04 [claim C04]
heading: Claims > C04: Universal Ingestor produces lossless transformations
source_digest=sha256:09f6c8af… scope=heading_body selector: --document logic/claims.md --heading Claims --heading 'C04: Universal Ingestor produces lossless transformations'
- **Statement**: The LLM-based Ingestor faithfully transforms PDF papers …
```

A section that is the only one headed by a loaded entry of its document is
cited in the short form `path#ID` (`logic/claims.md#C04`,
`logic/solution/heuristics.md#H01`), which `show` resolves to the same
section; other sections use their canonical heading address.
`source_digest` keeps the meaning of the JSON `digest`: SHA-256 of the full
selected source. For a heading, that is the section body that `document.replace`
with the printed `--document`/`--heading` selection replaces (title and
ancestors excluded). For a document, it is the whole file (`scope=whole_document`).
Use it as `expected` for that selection with `document.replace`, `logic.revise`
Body or `paper.edit`. `entry.rename` and `entry.remove` guard a different span,
the entry's heading line plus its body up to the next heading of the same or a
higher level, and no `show` line prints that digest; the skills' access page
(`references/cli-access.md`) reads the span as a `--source --lines` window and
hashes its JSON `content`. The line says `selector: none` when
the full heading vector also matches another section, or when the document is
read-only for `document.replace` (`PAPER.md`, `trace/`, `staging/` and any
path that is neither a mutable logic document nor registered in
`knowledge_paths`), or for a heading in a `logic/claims.md` whose stray
leading `---` reads recover (writes reject that heading with
`write.frontmatter`). The reason follows `none`, and the digest is still
printed. A value starting with `-` prints as `--heading=<value>`. The display adds one newline when the source lacks a
final one. `--source --json` remains the exact-byte path. Entries without a
native section (nodes, observations, sessions) print a labeled projection with
no digest. The projection ends with the exact source read, such as
`ara show --document trace/exploration_tree.yaml --source`. A session
projection omits the raw `body` (its whole YAML file, which the fields
already show); JSON keeps it. Multi-line list items print as `- |` blocks,
and a list whose items contain `,`, `[` or `]` prints one `- item` per line.

`find` keeps BM25 order, tokenization and filters. Each result prints
`<address> [<kind>] <source>`. Under it are the one-based source lines of the
result's native span (its section, YAML entry without nested entries, session
file, or a document's indexed text) that contain one of the result's matched
query terms, compared case-insensitively. A result with no such line prints
its excerpt as `excerpt:`. At most 20 lines print per result; a final line
counts the rest. `--context N` (long form only; `-C` selects the artifact)
adds up to N lines on each side within the span. Overlapping or touching
ranges merge, `--` separates the others, and hit lines use `N:` while context
lines use `N-`. `--context 0` adds nothing.

Load diagnostics print once on stderr, after the output, as one line. It gives
the error and warning counts with each rule code once (`ARA219×3`) and suggests
`ara check`. It names at most 12 distinct codes per severity and then counts
the rest (`… 5 more codes`); the error and warning counts stay exact and
`--json` keeps every diagnostic at its severity. `status` prints its codes on stdout instead. Text errors print
`error [code]: message`, then the `hint`, up to 10 ranked `candidates`, the
`blocking` codes, `file_access` roots and `unrepresented` documents from
`details`. A refusal (`invalid_artifact`, `incomplete_artifact`) replaces the
full validation report in its message with error and warning counts and rule
codes; the JSON `message` keeps the report. When a next ID cannot be
allocated, `status` prints `X=unavailable` and a `next_ids_unavailable` line
with the reason from `next_id_errors`.

Native documents are `PAPER.md`, `logic/**/*.md`, `trace/**/*.yaml`,
`staging/observations.yaml` and paths registered in `PAPER.md` `knowledge_paths`.
`rubric/`, `evidence/` and `src/` are file-access roots: agents read and search
them with their own file tools. `show --document` on one of them, on a bare
root such as `rubric`, or on any path outside the native set, fails with
`invalid_document` and a `details` object:
`hint` (plain text), `native_documents`, `registered_documents` and
`file_access` (`["rubric/", "evidence/", "src/"]`). The check runs on the path
alone, so a registry entry cannot serve a file-access path. `find` and `ls` do
not index these roots, and rubric headings such as `R84` are not entry IDs.

Native nodes, claims, heuristics, observations, sessions, experiment plans,
concepts and typed documents keep their own namespaces. A concept named `C05`
is a concept name, not an instruction to relocate claim C05. Unknown fields
and complete source bodies remain available in full reads. `--source` returns
original UTF-8 source rather than regenerated normalized YAML/Markdown. Source
digests refer to the selected exact bytes; replacement preconditions must use
the corresponding source selection. Pure parser and native source spans are
both bounded; see [deep-tree-parsing.md](deep-tree-parsing.md).

### Bounded and resumable reads

`show` accepts two read bounds ([`agent/window.rs`](../crates/ara-cli/src/agent/window.rs)):

- `--lines A:B` keeps one-based inclusive lines of each selected native
  source: a document, a heading body, or an entry's native section. `A:` and
  `:B` leave one end open; one end is required. A line ends after its `\n`,
  so `\r\n` stays whole and a final newline belongs to the last line
  (`a\nb\n` has two lines). An end beyond EOF clamps and the reported range
  shows the actual end. A start beyond EOF rejects with `line_out_of_range`
  (`id`, `details.start`, `details.total`); `--lines :B` on an empty selection
  succeeds as empty. Zero, negative, signed, reversed, nonnumeric and
  overflowing values reject with `invalid_lines`. Each selection of a
  multi-address read gets its own window. An entry without a native section
  (node, observation, session) rejects with `lines_unavailable`, whose hint
  names the exact source read (`ara show --document trace/exploration_tree.yaml
  --source --lines A:B`).
- `--max-bytes N` is the budget of the whole stdout response, counting
  headers, digest lines, relations and range lines, not only source bytes.
  Brief `show` uses 16 KiB (16384 bytes) without it. `--json` reads have no
  budget unless `--max-bytes` is given, and brief `--source --full` is the
  explicit unbounded read. `0`, negative, signed, nonnumeric and overflowing
  values reject with `invalid_max_bytes`.

Argument errors exit 2 and reject before any source is read. `--fields`
cannot be combined with either bound, so it keeps its row meaning; the
argument parser rejects the combination with exit 2 (with `--json` the
error code is `argument_error`; brief text prints the parser's usage error).
Every rejection writes nothing to stdout. `line_out_of_range`,
`lines_unavailable` and a single-selection `output_limit_too_small` name the
selection in `id`, using its cited form (`logic/claims.md#C04`) when it has
one.

When a response does not fit:

- A single native selection pages. The page holds as many whole lines from
  the start of the window as fit; a UTF-8 code point or a line is never
  split. Its block ends with the range and the next window, which keeps the
  original upper bound (an open bound stays open):

  ```text
  == logic/claims.md [document]
  source_digest=sha256:2ef90ab0… scope=whole_document selector: --document logic/claims.md
  …
  lines: 18-31 of 84; truncated; next: --lines 32:
  ```

  Rerun the same selection with `--lines 32:` and the same budget. In JSON,
  concatenating the pages' `content` of an unchanged source gives the
  selection's exact bytes. Brief text shows the same lines, except that the
  display adds one newline after a final line that has none, as for any
  brief block. Each page prints the `source_digest` of the full selection,
  not of the page; when it changes between pages, the source changed and
  the caller restarts. Use a page for reading. Before a `document.replace`,
  read the full selection and use its digest: a digest does not authorize
  replacing content the caller has not seen.
- When the next line cannot fit even alone, the read fails with
  `output_limit_too_small`. `details.required` is the smallest budget that
  returns that line with its metadata, `details.line` is the line and
  `details.max_bytes` the budget in force; the hint says to rerun with that
  `--max-bytes`. No empty page is returned and no line is skipped, so a tiny
  budget cannot loop without progress.
- Several selections fit together or not at all: `output_limit_too_small`
  names the aggregate `required` and advises reading each address
  separately to page through it. No selection is dropped or truncated, the
  request order is kept, and overlapping selections stay separate items.
- A single entry projection has no line mapping, so it fails with
  `output_limit_too_small` and a hint naming the exact source read.

A brief block prints its range line (`lines: S-E of N`, or `lines: none of 0`)
only when `--lines` is given or the page is truncated, so a read that fits
prints as before. In JSON, `--lines` returns each native selection as a
`source_document` row, as brief reads do: `show C04 --lines 1:5 --json`
returns the C04 section's lines, not the claim projection. `--max-bytes`
alone keeps JSON entry projections. With either bound, source rows carry
their full exact `content` (or the page's slice) even without `--full`, keep
`digest` as the SHA-256 of the full selection, and gain a `display` object:
`scope`, `selector` (`{document, heading}`, or null with `no_selector`),
`cited` (the address brief headers print: `path#ID` for a section that alone
heads a loaded entry, otherwise the canonical heading address), `lines` (`{start, end, total}`,
with `end = start - 1` when empty), `truncated` and `next` (the next
`--lines` value, or null). JSON is never cut at a byte boundary: the
envelope, diagnostics included, fits the budget or the read rejects.

An empty brief result prints `no results` (`find`), `no entries` (`ls`) or
`no open items` (`open`) on stdout; JSON keeps the empty array.
Other brief commands are compact but not byte-bounded. Use `find --limit`,
filters and `ls <path>` to narrow them.

### Headings and canonical addresses

Repeated `--heading` values form an exact vector: `['A/B']` names one heading
containing a slash and differs from `['A','B']`. A heading read tries these
tiers in order and stops at the first tier that matches anything; if that
tier matches more than one section, the read rejects instead of trying a
weaker tier:

1. the exact full heading vector;
2. an exact suffix of the vector, then the native-ID shorthand (`C04` selects
   `C04: Title`);
3. a recorded rename or merge redirect, when the artifact has identity records
   (`trace/logic_mutations.yaml`, `trace/merge_log.yaml`, `trace/aliases.yaml`)
   and the read is not `--source`;
4. tolerant suffix matches on segments trimmed of surrounding whitespace and
   lowercased with Unicode lowercase (locale-independent; no accent stripping
   or Unicode normalization): equality, then a prefix of the source heading,
   then a source heading ending in a literal `...` whose nonempty stem starts
   a longer request. An exact heading containing `...` matches at tier 1 or 2.

A canonical address names one section of the current source. A document is
its path. A heading is `path#h/<segment>/<segment>` with the full original
heading vector. Path components and segments escape every byte outside
`A-Z a-z 0-9 - . _ ~` as uppercase `%XX`, so `path#h/A%2FB` and `path#h/A/B`
differ and a literal `;` is `%3B`. When the full vector repeats, the address
ends in `;occurrence=N`, the one-based source-order position among those
sections; an address without it rejects for a repeated vector. An occurrence
is a read locator for one source snapshot, not a durable identity after
edits, and no write selector accepts it.

`ara show <address>` accepts canonical addresses alongside entry IDs, bare
native IDs (`C04`), `trace:N09` and `logic/claims.md#C04`. Canonical addresses
resolve exactly, with no redirect or tolerant tier. The input is decoded once
and then passes the normal document boundary: malformed escapes, invalid
UTF-8, absolute paths and traversal reject (`invalid_address` or
`invalid_document`). A positional whole-document path is decoded the same
way; its raw spelling still reads when no decoded path matches. A solution
document (`logic/solution/<name>.md`, other than `heuristics.md`) reads as its
`solution` entry under either spelling, so `logic/solution/my%20notes.md` as
printed by `ls` and `find` returns the same entry as the raw path. A legacy
`path#Method/Step 3` is tried only when no entry has that selector, and
resolves only when exactly one section has a heading suffix spelled that way
with `/` joins; each segment may use its native-ID shorthand
(`logic/problem.md#O1`). If recorded identities call the selector ambiguous,
it stays ambiguous even when one current section matches.

Source-document rows keep `heading` (the requested or redirected vector) and
add `heading_path` (the section's full source vector) and `address`. The
`digest` still covers exactly the selected bytes.

A miss returns `unknown_id`; more than one match, whether sections or
entries, returns `ambiguous_heading`. Both carry `details.candidates`, at most
40 addresses from loaded knowledge that `show` accepts (a section that alone
heads a loaded entry is cited as `path#ID` and ranked by that ID; other
sections use their canonical address; an entry whose key another entry
shares is qualified as in [brief text](#brief-text-output)), and `details.capped`, which is
true when more existed. When a heading read finds several sections, the
candidates are those sections in source order. Otherwise they are the
document's headings (for a document selector) or the artifact's entries,
ranked by edit distance to the request and then by source order. When
identity records cannot be indexed (for example a corrupt alias file), a read
that reaches them fails with `identity_lookup_failed` instead of an ordinary
miss.
Selection errors from `show`, `path`, `refs` and `ls --under` never carry
internal `merge.*` codes; `ara resolve` and `merge` keep theirs.

### Invalid artifacts, stray fences and claim spellings

Structural reads (`find`, `ls`, `show`, `open`, `refs`, `path`) need a
complete representation, not a valid artifact. Each result carries
`diagnostics` with `errors` and `warnings` at their original severities. A
validation error is read through only when its code is in this allowlist
([`agent/validity.rs`](../crates/ara-cli/src/agent/validity.rs)):

| Code | Read | Why the artifact is still fully represented |
|---|---|---|
| `ARA107` unknown-evidence-claim | admit | The node keeps its `evidence` source; only the binding is absent, as with no `claims.md` (`ARA207`) |
| `ARA108` unknown-dependency-node | admit | The node keeps its `also_depends_on` source; no edge is created |
| `ARA109` unknown-claim-dependency | admit | The claim keeps the ID verbatim in `deps` |
| `ARA100`–`ARA103`, `ARA113` | refuse | The tree or identity history cannot be loaded |
| `ARA104`–`ARA106` | refuse | Entries are dropped or identities are duplicated |
| `ARA110` | refuse | A cycle; graph reads would have no root order |
| `ARA111`, `ARA112` | refuse | A node's place in the hierarchy is unknown or contradictory |
| any other error code | refuse | Unclassified rules refuse until added here |

Admitted errors also need every typed native document (`claims`,
`solution/heuristics`, `concepts`, `related_work`, `experiments`) to be fully
represented: no `ARA229` fence hides one, no `logic/claims.md` heading that
starts like a claim ID (`## C01—Speedup`) fails to parse, no code fence left
open at EOF in `logic/claims.md` hides a claim-like heading (whether or not
any reference names it; a closed fence holding an example heading is fine),
and no line of
`logic/claims.md` starting with `#` names a dangling claim ID as a whole token
(ignoring case, spaces, code fences and heading level, so `### C01: A`,
`## c01 — A`, `## **C01**: A`, a heading swallowed by an unclosed code
fence, and conservatively a claim title such as `## C02: Extends C01` all
count). Dangling dependency IDs come from the parsed claims; an `ARA107` or
`ARA109` whose claim ID cannot be identified also refuses. A dropped claim produces the same `ARA107` as a dangling
reference, so this parse loss still refuses with `invalid_artifact` and `details.unrepresented` (`path:line:
reason`). `ARA217`, `ARA218`, `ARA222` and `ARA226` warnings still refuse with
`incomplete_artifact`, and I/O issues still fail as I/O errors. A refusal
(`invalid_artifact` or `incomplete_artifact`) has `details.blocking` (the
refusing rule codes) and `details.hint` (run `ara check`; source reads still
work). Relationship reads never invent a missing target: `show --with
claims,depends_on` omits it and `refs`/`show` on the missing ID return
`unknown_id`. `status` is unchanged: `complete` is false, and counts and next
IDs are null, whenever any error exists. `check` and `validate` still fail on
these errors. Write commands do not use this tolerance; `session log` refuses
any validation error. Explicit `--source` reads keep
`artifact_validation: "not_run"`, which does not mean the artifact is valid.

An unclosed leading `---` in a Markdown document hides the rest of it as
frontmatter, and loads warn `ARA229` at `path:line` (for `PAPER.md`, the
knowledge-registry warning `ARA217` reports it instead). A `logic/claims.md`
whose opener is followed only by blank lines, the exact title `# Claims`, a
claim heading and a known claim field, with no code fence left open at EOF
hiding a claim-like heading, is the one recovered case: reads skip
that line and warn `ARA228` (rules in
[stage-1](stage-1-core-parse-validate.md#validation-severity)). Writes never
recover it. A write, including a dry run, fails with `write.frontmatter` and
the opener's line when it would leave a changed document behind an unclosed
fence, or when its selector names a heading the fence hides; other selector
misses stay `write.selector`. A guarded whole-document `document.replace`
that removes the fence is accepted, but claim retention counts the recovered
claims: a repair that drops one fails with `write.claim_retention` like any
other edit that retires a canonical claim.

Claim headings accept `:` and a spaced `-`, U+2013 or U+2014 separator
(`## C04 — Title`). `show C04`, `--heading C04`, `refs` and merge titles
resolve them, source bytes keep the original spelling, and `ara check` does
not report them (`ARA004` covers only unspaced dashes).

## Authoring commands and source inputs

Convenience commands cover `add node`, `add edge`, `edit`, `claim add/set`,
`heuristic add/set`, `stage`, `promote`, `session start/log`, and `link --same-as`.
`session log` without `--session` asks the writer to select or create the
session and therefore requires `--summary`; omitted `--timestamp`/`--started`
values come from the writer's locked clock (see
[One clock value per batch](#one-clock-value-per-batch)).
`apply` exposes the complete typed operation contract in
[`write/mod.rs`](../crates/ara-core/src/write/mod.rs).

```sh
ara -C ./ara add node --type question --parent N01 --title 'Boundary behavior' \
  --set description=@description.txt --provenance ai-suggested --json
ara -C ./ara add edge N02 --depends-on N01 --json
ara -C ./ara apply /tmp/request.jsonl --dry-run --json
ara -C ./ara apply /tmp/request.jsonl --json
```

CLI text values accept `@file` or `@-` for complete external UTF-8 input; stdin
can be consumed once. `@@text` escapes a literal leading `@`. These expansions
belong to CLI inputs. JSONL values are literal JSON and do not expand `@file`.
For `--set`, only an explicit JSON array starting with `[` is decoded as a list;
ordinary scalar text, including numbers and booleans, stays exact text.

The five creation kinds and required source payloads are question (`description`),
decision (`choice`, `alternatives`), experiment (`result`), dead_end (`hypothesis`,
`failure_mode`, `lesson`) and pivot (`from`, `to`, `trigger`). Explicit replay IDs
satisfy the same validation as allocated IDs. IDs do not authorize unsupported
kinds or missing source content. Legacy `observation` nodes and legacy pivot
payload names cannot be silently rewritten into this creation contract.

| Operations | Purpose |
|---|---|
| `node.add`, `edge.add`, `node.link_same_as` | Append a typed node or dependency/equivalence pointer |
| `claim.add`, `heuristic.add`, `entry.edit` | Create or edit allowed native logic/metadata fields |
| `observation.stage`, `observation.promote`, `observation.mark_stale` | Stage, atomically crystallize, or audit stale observations |
| `session.start`, `session.log` | Create sessions and append complete turns/history |
| `document.create`, `document.replace`, `paper.edit` | Create bounded documents or replace exact mutable source selections |
| `record.append`, `entry.taste_append`, `entry.annotate` | Append typed records or permitted annotations |
| `logic.revise`, `entry.rename`, `entry.remove` | Audit mutable changes and structural identity transitions |
| `artifact.init` | Initialize the `compiler` or `research-manager` seed profile |

New fields preserve native scalar/list forms.

A block created by `claim.add` or `heuristic.add` (including a promotion to a
claim or heuristic) lists only the supplied fields, in a fixed schema order,
whatever the input order or the layout of nearby entries:

| Entry | Field order |
|---|---|
| Claim | Statement, Conditions, Sources, Status, Provenance, Falsification, Proof, Evidence basis, Dependencies, Tags |
| Heuristic | Rationale, Source, Sources, Status, Provenance, Sensitivity, Bounds, Code ref, Tags |

`Falsification criteria` is accepted as input, and a new block writes the
schema label `Falsification`. Existing blocks, including their label spellings
and list styles, are not reformatted. `Last revised` and `Merged into` belong
to `logic.revise` and are rejected at creation.

A single-line value with no leading or trailing whitespace is written inline
(`- **Status**: supported`) when the field reader returns exactly that text. Empty,
multiline (LF or CRLF) and padded values use the indented continuation form,
which keeps every caller byte, including a final newline. When the target file
uses CRLF, the blank-line separator, the heading line and inline field lines end
with CRLF; a continuation-form field keeps LF structure, the only form the
continuation reader decodes, around the exact caller bytes. A new or LF file
gets LF throughout. Bytes before the block do not change. Dependencies are
written as `[]` or `[C03, C04]`. Other lists (Proof, Sources, Tags, Code ref)
stay compact JSON arrays such as `["x"]`: the readers keep these fields as text,
so a comma-joined list would not decode back to the same list. A scalar is
written as given, so `Tags=evaluation, experimental-design` stays one exact
string. IDs inside a list value still count as references: `Proof=["E01","E03"]`
projects `proof` as `[E01, E03]`, and a dangling ID inside Proof or Sources
rejects the write.

Named concepts must already exist or resolve through an authenticated alias.
Claims are retained in audited withdrawal/merge states or renamed canonically;
physical claim deletion is not allowed. Compiler heuristics retain singular
scalar `Source` and complete Bounds prose without inventing PM fields. Extra
knowledge documents are bounded by native registration/allowlists; arbitrary
filesystem writing is not an operation. `knowledge_paths` rejects `rubric/`,
`evidence/` and `src/` entries, and no operation creates, edits, renames or
removes a `rubric/` document or `R` entry. A compiler writes
`rubric/requirements.md` as a plain file.

Trace and staging content is immutable except for declared pointer/metadata
transitions. Terminal nodes cannot acquire children. `same_as` points from a
later node to a provably earlier existing node using recorded time or actual
creation order; numeric IDs and preorder do not establish chronology.

### Citation repair for restructures

Plan 19 C1 lets four restructures repair the current citations of the
entry they change, inside the same transaction and through the same audit
path as a caller-written edit:

| Restructure | Operation | Caller supplies |
|---|---|---|
| Rename | `entry.rename` with `rewrite_references: true` | `target`, `name`, entry-span `expected`, audit context |
| Claim merge | `logic.revise` with `rewrite_references: true` | `set` with `Status: withdrawn` and `Merged into: <live claim>` |
| Removal with replacement | `entry.remove` with `rewrite_references: true` | an explicit existing `redirect` (claims still cannot be removed) |
| Claim split | `logic.revise` with `action: "split"`, `split_into`, `references` | spin-offs created earlier, one row per current citing field |

`rewrite_references` and explicit `references` are mutually exclusive on one
operation (`write.reference_mode`). A merge may instead list explicit
`references` rows (see the row rule under **Split**); each must move the
source's citations in that field to the survivor.

**Shared rules.** `ara refs` and the writer classify citations with the same
rules (`ara_core::write::citation_rules` and the writer's citation
inventory): one table of reference-bearing Markdown fields, one set of
historical sources, and one rule for protected spans. For a claim,
heuristic, experiment plan or concept target, `refs` lists exactly the
Markdown citations the writer's inventory finds, so what `refs` shows in a
rewritable field is what a restructure repairs.

| Markdown field | Listed by `refs` | Rewritten by C1 |
|---|---|---|
| `Dependencies`, `Proof`, `Sources`, `Claims affected`, `Related` / `Related concepts`, `Merged into`, `Evidence output`, `Code ref` | yes | yes, where the entry's schema accepts the field |
| `Depends on`, `Deps`, `Promoted from`, `Last revised` (read-only aliases) | yes | no: a `read_only_field` mention |

Historical sources, read the same way by both: the exploration tree
(`parent`, `evidence`, `also_depends_on`, `same_as`, `concepts`,
`source_refs`, artifact `pointer`s, annotation `references`, child IDs),
observations (`bound_to`, `promoted_to`, `crystallized_via`), taste
`target`s, `trace/aliases.yaml` alias `target`s, reasoning-log `turn`s, the
session index, session rows (`events_logged` `id`/`target`,
`claims_touched` `id`, `logic_revisions` `entry`, `ai_actions`
`files_changed`) and the mutation ledger. Reasoning turns, the index, child
IDs and the ledger describe identity bookkeeping, not citations, so the
writer does not validate them as historical citations. A tree `concepts`
name cites a concept only when it resolves to exactly one concept heading
(leaf text or full heading path); `refs` lists nothing for an ambiguous name
and a restructure of one of its candidates refuses.

**What is rewritten.** The writer builds a typed inventory of the current
mutable Markdown: in each native entry it reads only the accepted reference
fields `Dependencies`, `Proof`, `Sources`, `Claims affected`, `Related`
(`Related concepts`), `Merged into`, `Evidence output` and `Code ref`, where
that entry's schema accepts the field. Inside a field it parses native ID
tokens (with the read model's boundaries), qualified locators
`document:ID`, `document#ID`, `document#<leaf heading>` or
`document#<full heading path>`, and in `Related` comma-separated concept
names. Tokens inside quotes, backticks, fenced code or HTML comments are
protected, as in merge rewriting: they are never rewritten or listed by
`refs`, and they count as `protected` mentions. A value that is exactly a
JSON array of strings is a native list, so its item quotes are list syntax,
not quotation (an item with an escape sequence stays protected). A token counts only when it resolves to exactly one heading of the
subject (or of its subtree), directly or through an authenticated claim
redirect (a retired alias). The longest spelling that ends at a delimiter
wins; a multi-word heading followed by more words is prose. Each rewrite
replaces only those token bytes and keeps quotes, prose, list delimiters and
the spelling style (bare ID, qualified ID, path with the same number of
segments, bare name). Afterwards every new spelling must resolve to exactly
its destination, or the operation fails with `write.reference_rewrite`.

**What is never rewritten.** PAPER frontmatter, other registered documents,
headings, prose, fields outside the list above, unknown fields, inline values
followed by unindented prose lines, and every historical record: tree
`evidence`, `parent`, `also_depends_on`, `same_as`, `concepts`,
`source_refs`, artifact pointers and annotations, observation `bound_to`,
content and promotion tuples, prior session rows, reasoning, taste, the
mutation and merge ledgers and archived before/after payloads. Possible
mentions are reported in the operation result as `skipped_references`, each
with `document`, `heading`, `field`, one-based `line`, `literal` and
`reason` (`prose`, `heading`, `untyped_field`, `unknown_field`,
`read_only_field`, `protected`, `unparsed`, `ambiguous`).

**Renames and removals.** The existing dangling-reference guard still
applies, now computed from the inventory: after the repairs, any remaining
non-heading mention of the old identity refuses the operation with
`write.dangling_reference` and lists every location in the error's
`details.locations`. A skipped citation is never permission to leave a
broken identity. One deliberate difference from the textual guard: a parsed
token in a reference field that resolves to a *different* entry is not a
mention. Renaming `Group A/Term` therefore leaves
`Sources: logic/concepts.md#Group B/Term` alone and is not blocked by it,
while the explicit-`references` path keeps the textual guard and still
counts the `Term` inside that locator. Typed historical citations of the old identity are recorded
and, at final validation, must resolve through the retained entry or the
appended authenticated `trace/logic_mutations.yaml` mapping, both by the
writer's redirect chain and by the read side's identity index that `show`
and `refs` consult (`write.history_unresolved` otherwise, reported on the
restructure's line even when a later operation broke the chain). The read
side checks each citation's literal spelling, not only a normalized
selector: a scalar locator must resolve the way `show` resolves it. The
result lists the checked citations as `historical_citations` (`source`,
`field`, `literal`). `show` follows an authenticated mapping for every
spelling of a retired heading: the canonical `path#h/...` address, a legacy
`path#A/B` joined-path locator (any suffix of the old vector) and the
mapping's own `path:A/B` origin all read the renamed section. The read-side index keys a two-heading concept path by its leaf
(`logic/concepts.md#Term`); when that key belongs to a different live entry,
a retired origin now keeps its exact heading vector instead of colliding with
it, and a retired suffix alias never shadows a live identity. A historical citation that is
ambiguous between the subject and another entry (for example
`logic/concepts.md#Term` with two `Term` leaves) refuses the restructure,
because changing one heading would silently re-point it. A removal needs a
`redirect`; a bare-ID citation cannot point at an unnumbered destination and
a concept name cannot leave `logic/concepts.md` (`write.reference_rewrite`).
An entry that would come to cite itself (the survivor or redirect already
cites the subject) is refused; revise it explicitly first.

**Merge.** The source claim is retained with its `Status: withdrawn` and
`Merged into` relation, so prose mentions stay valid and are only listed.
The survivor must be a different live claim (`write.merge_survivor`) that
is not already merged, directly or transitively, into the source
(`write.redirect_cycle`). Claims previously merged into the source move to
the survivor too.

**`expected` on a merge or split** keeps the meaning it has for a `Body`
revision: the digest of the selected heading body, which `show` prints as
`source_digest` (JSON `digest`). It is optional for a merge or split, required
for `Body`, and rejected for other field revisions. `entry.rename` and
`entry.remove` keep their separate entry-span digest.

**Split.** `action: "split"` and a nonempty `split_into` of exact selectors
come together (`write.split`). Destinations are distinct existing claims
other than the primary (`write.split_destination`); a spin-off created
earlier in the batch qualifies, a forward binding does not. Every current
citing field of the primary, spin-offs included, needs exactly one
`{target, field, before, after}` row with the exact current `before`
(`write.split_unclassified` lists the missing ones). A row's `after` must
equal its `before` except that each unprotected citation of the primary is
replaced; every other byte, including prose, quotes and other claims'
citations, stays (`write.reference_mapping`). Each replacement names the
primary or a declared spin-off (`write.split_destination`); keeping the
primary unchanged is allowed and writes no audit row. Only list-typed values
fan one citation out to several destinations, joined by the list separator:
`Dependencies`, or a value that is a JSON array of strings before and after.
In `Proof`, `Sources` prose and other mixed or scalar values each citation
becomes exactly one destination; `Merged into` stays one claim
(`write.reference_scalar`). Any other content change needs its own audited
revision. The CLI never chooses which proposition a citer
meant. The primary's own `set` must change it. `after` is literal text, so
name spin-offs by explicit IDs (`claim.add` with `id`). The primary's
`logic_revisions` rows carry `action: split` and `split_into`; this is an
additive, optional row shape.

**Audit.** Every changed citing field gets one `logic_revisions` row with its
exact decoded before and after, plus `Last revised`; the result lists them
as `rewritten_references`. Rename and removal mappings append to
`trace/logic_mutations.yaml` as before. Existing historical rows stay byte
exact; only files that receive a new row grow. A failure anywhere in the
batch, including the post-change spelling check and final validation, writes
nothing, and a dry run persists nothing.

## Batches, audit ownership and recovery

Each nonempty JSONL line is one operation. A creation ID such as `$question`
binds its allocated native identity for later structured references. BatchBinding handling
keeps each binding in its declared namespace: unknown, duplicate, forward and wrong-kind
uses reject. Ordinary prose and historical before/after text stay literal.
Dry-run identities and turns are tentative. The whole batch plans in one working
snapshot and validates the declared source delta before any source commit.
A later failure leaves source bytes unchanged and reports the failing line.

### One clock value per batch

A commit-mode writer reads the clock once, after it takes the artifact lock and
recovers any prepared transaction, and before it plans. A dry run reads one
tentative value without taking a lock; its time, IDs and turns are not reserved.
That value, `batch_time`, is a UTC `YYYY-MM-DDTHH:MM:SSZ` timestamp. Every
omitted value in the batch uses it:

| Omitted value | Default |
|---|---|
| `node.add` `fields.timestamp` (also promotion-created dead ends) | `batch_time` |
| `observation.stage` `timestamp` | `batch_time` |
| `session.log` `timestamp` | `batch_time` |
| `session.start` `started` / `date` | `batch_time` / the date written in `started` |
| `record.append` to `trace/taste_log.yaml`, `record.timestamp` | `batch_time` |
| `entry.taste_append` `record.date` | the UTC date of `batch_time` |
| Session created for an omitted-`session` log, `started` / `date` | the log's timestamp / its written date |

An explicit timestamp or date is kept exactly after validation. A log's
effective timestamp is its supplied value or `batch_time`, and session
selection uses the date written in that value, so `2026-10-04T23:30:00-05:00`
belongs to 2026-10-04. Chronology checks compare instants. An explicit past
session with an omitted timestamp therefore fails with a message that names
both dates; the writer does not swap the session or backdate the clock.

`add node`, `stage`, `promote`, `session start` and `session log` send omitted
values to this same locked path. The CLI never chooses an ID, session or
timestamp from an unlocked pre-read.

### Owner anchor for omitted audit context

A batch can leave audit context to its one `session.log`, the owner anchor:

| Operation | May omit | Caller still supplies |
|---|---|---|
| `logic.revise` | `session`, `turn` | target, complete change, `signal`, `provenance`, preconditions |
| `entry.rename`, audited `entry.remove` | `session`, `turn` | target, name, `expected`, `signal`, `provenance`, reference edits |
| `paper.edit.audit`, `observation.mark_stale.audit` | `session`, `turn` in the supplied `audit` | `signal`, `provenance`, note; stale `reason` |
| reasoning `record.append` | `record.turn` | complete `notes` |
| `session.log` | `session`, `timestamp` | a nonempty `summary` |

```jsonl
{"op":"session.log","summary":"Revised C01 after the ablation"}
{"op":"logic.revise","target":{"id":"C01"},"set":{"Statement":"..."},"signal":"empirical-resolution","provenance":"user"}
```

Rules:

- The batch must contain exactly one `session.log`, and it must carry a
  nonempty caller-written `summary`. With no log the first omitting line fails
  with `write.owner_required`; with several, `write.owner_ambiguous`. A missing
  summary fails at the log with `write.owner_summary`.
- The anchor must come before every operation that omits context
  (`write.owner_order` otherwise). Its session is resolved and its next turn
  reserved at its own line, from the locked snapshot and earlier operations
  only. An earlier `session.start` can bind `$s` for it; unknown, forward,
  duplicate and wrong-kind bindings still fail at their own line.
- An explicit `session` or `turn` on an omitting operation must equal the
  anchor's session and reserved turn (`write.owner_mismatch`). The writer never
  overrides it or attaches a change to an earlier turn.
- Revision rows, pending audits and operation-derived session rows (below) are
  attached after all ordered operations succeed, then the complete candidate
  is validated.
- No `session.log` is ever added because an operation needs a turn. Standalone
  writes without audits stay possible and create no session.

If the anchor omits `session`, the writer picks the session itself:

- One open session dated on the log's date: it is selected.
- None: a new session is created with the log's summary, timestamp and date.
- Several: `write.session_ambiguous`, listing their IDs.
- Closed sessions are never selected or reopened.

The common case is work that runs past midnight UTC. If yesterday's session is
still open, the writer creates today's session and leaves yesterday's open. The
report lists it in an additive `open_sessions` field so the caller can close it
on purpose. To continue yesterday's session, name it and give a timestamp on
its date. The `session.log` operation result reports `session_created: true`
when it created the session, and operations that took their owner from the
anchor report `session` and `turn`.

Fully explicit batches keep their order freedom: a revision may come before its
owning log when it names a valid new turn, and several logs are allowed when
every audited operation names its own session and turn. An operation with
omitted context in a multi-log batch is an error, never a guess based on line
proximity.

An empty batch writes nothing. A batch of no-op operations with no
`session.log` creates no turn or history. An explicit `session.log` is a
requested turn even if everything else is a no-op, and no-op mutations add no
revision rows.

### Operation-derived session rows

A batch that contains a `session.log` gets the mechanical rows of its turn
from the operations that succeeded. The caller no longer repeats them.

| Successful operation | `events_logged` row | `claims_touched` row |
|---|---|---|
| `node.add` | new N ID, the node type, `direct` | none |
| `observation.stage` | new O ID, `observation`, `staged` | none |
| `claim.add`, `heuristic.add` | new C/H ID, `claim`/`heuristic`, `direct` | `created` for a claim |
| `observation.promote` to claim, heuristic or dead end | new C/H/N ID, destination type, `crystallized` | `crystallized` for a claim |
| `observation.promote` to concept, constraint or architecture | source O ID, destination type, `crystallized`, `target` | none |
| `logic.revise`, or a rename's reference repair, that changes a claim | none | `revised` (also when a plain revision sets `Merged into`) |
| C1 merge (`rewrite_references` or `references` with `Merged into`) or split (`action: "split"`) | none | `merged` / `split` for the source or primary claim; repaired citers `revised` |

- **Summary and provenance.** A derived event's `summary` is the operation's
  `title`, or the observation's complete `content`, copied without
  truncation. Its `provenance` is the operation's own: `fields.provenance` on
  `node.add`, `Provenance` on claims and heuristics, `provenance` on staging,
  and the destination's supplied or inherited value on promotion. A creation
  with no valid provenance fails at its own line with
  `write.event_provenance`, unless the caller supplies its event row.
- **`target`.** A promotion to a named section has no numeric ID, so its event
  names the source observation and adds
  `target: {document: <canonical document>, heading: [<section>]}`. The field
  is additive and optional: older rows without it keep their meaning. A
  supplied `target` needs `crystallized` routing and must resolve to the exact
  entry the row names. For an observation, its type must be concept,
  constraint or architecture and the observation's `promoted_to` must point at
  that heading; otherwise `write.event_target`.
- **Which turn.** With one log, every eligible operation belongs to its turn,
  wherever the log appears and whether its session is explicit, selected or
  created. With several logs, a new entry belongs to the one log whose
  `events` or `claims_touched` row names it. No naming row, or rows in two
  logs, is `write.owner_ambiguous`. A claim change belongs to the turn the
  revision names. With no log, nothing is derived, and a no-op operation
  derives nothing.
- **Caller rows win on words, not facts.** Event identity is the turn plus
  `(id, routing, target)` after resolving `target`; an absent `target` matches
  a numeric-ID destination, and a supplied one must select the same entry. A
  caller row with the same identity as a derived row keeps its summary
  verbatim and suppresses the derived one, but must agree on type and
  provenance. Any caller row that names an entry this batch created or
  promoted must match one of that operation's facts, so it cannot relabel the
  routing or drop the target. These fail with `write.event_conflict`.
- **Duplicates.** Identical repeated rows are written once. Two different rows
  with one identity fail with `write.event_conflict` naming both inputs.
- **Claim judgments.** Touch identity is `(claim, action)`. `created` and
  `crystallized` must match the operation that made the claim; labeling a
  revised existing claim `created` also fails. A caller judgment (`revised`,
  `advanced`, `weakened`, `confirmed`, `refuted`, `withdrawn`, `merged` or
  `split`) for a claim changed in the turn replaces the generic `revised` row.
  Beyond the vocabulary, only two checks apply, both against explicit Status
  changes written in the same turn. First, when the turn changes that claim's
  Status, a judgment naming a terminal status must match one of the written
  values: `confirmed` needs a change to `supported`, `refuted` a change to
  `refuted`, and `withdrawn` or `merged` a change to `withdrawn`. `advanced`,
  `weakened`, `revised` and `split` are not compared with Status values.
  Without a Status change in the turn, only the vocabulary applies; the stored
  Status is never consulted. Second, `confirmed` and `refuted` on the same
  claim in one turn reject unless the turn has two distinct Status changes, one
  to `supported` and one to `refuted`. Both fail with
  `write.claim_touch_conflict`. A Status change to `supported` alone is only
  `revised`. Judgments on claims the batch does not change keep the existing
  vocabulary and reference checks.
- **Order.** Caller rows keep their order; derived rows follow in operation
  order. Rows are inserted inside their own turn when one batch owns several
  turns of a session. Earlier turns are never searched, replaced or deleted.

A fully explicit request still validates every supplied row and receives any
deterministic row it left out, so its meaning is kept, but its bytes, event
counts and index rows can differ from the old binary's output. A
whole-document `Body` revision of `logic/claims.md` derives no claim touch,
because it does not name one claim; supply the rows. A plain `logic.revise`
that sets `Merged into` derives `revised`; `merged` and `split` are derived
only by the explicit merge and split operations of
[Citation repair](#citation-repair-for-restructures). A caller judgment for
that claim still replaces the derived row and is checked as above.

Compatibility changes for requests that worked before 19c, all in batches
with a `session.log`:

- A `node.add` or `heuristic.add` without provenance needs its event row
  (`write.event_provenance`).
- A row for a promotion to a concept, constraint or architecture without
  `target`, or a row `{type: observation, id: O.., routing: crystallized}`
  naming a promotion's source observation, no longer stands in for the
  promotion. It names a promoted entry but matches none of its facts, so it
  rejects with `write.event_conflict`. Omit the row, or write the derived
  identity with its `target`.
- Any other caller row that disagrees with the operation it names, and two
  different rows for one identity, reject (`write.event_conflict`,
  `write.claim_touch_conflict`).
- With several logs, every new entry must be named in exactly one of them
  (`write.owner_ambiguous`).
- Identical repeated rows are written once, and missing deterministic rows are
  appended.

Errors that involve two inputs carry the other one as `related_line` (the same
physical-line numbering as `line`) and `related_field`. The CLI's
`session log --node` still builds rows for existing nodes from their titles; it
runs no operations, so nothing else is derived.

### Session history and transactions

Session logging retains complete events, actions, touched claims, revisions,
context, threads and suggestions. Mutable logic revisions carry exact
before/after source history, signal, provenance and the owning next session turn.
Stale transitions need at least three distinct actual logged session days after
last observation/bound-node use, a caller-supplied reason and an atomic owning
`session.log` turn. They do not infer a scientific rationale from elapsed time.

`observation.mark_stale` uses the same history as `open`
([Observation inactivity](#observation-inactivity-in-open)), without the stale
operation's own turn and its notes, and with days ending at the owning audit
date (plan 19 D2):

- **Omitted `session_days`.** The writer derives the sorted distinct eligible
  logged dates after the latest attributable reference, up to the audit date,
  and records that full list. Fewer than three fail with `write.observation`
  at `session_days`.
- **Supplied `session_days`.** A verified subset, kept exactly as written in
  the evidence record. A repeated day, an invalid date, a day not after the
  last reference, a day after the audit date, a day whose only turn is the
  stale decision's own (or with no logged turn), and fewer than three distinct
  days fail with `write.observation` at `session_days[i]` or `session_days`.
  The list is never replaced.
- **Unknown day evidence** (any reason a day count would be `null`) refuses the
  write with `write.stale_history_unknown` at `session_days`, naming the
  history diagnostics. Silence is never assumed.

The evidence record keeps its fields (`session_days`, `last_reference`,
`bound_to`, `signal`, `provenance`, `session_sources`, `audit`); validation
recomputes it from the final candidate and rejects any difference. A refused
write leaves every byte unchanged, and a dry run persists nothing. An existing
`stale: true` is never cleared or re-derived, and an already stale observation
is a no-op.
Promotion creates its complete destination and forward pointers together;
immutable original observation content and prior turns remain exact.

Commit-mode writes take a persistent artifact lock, recover any prior prepared
transaction, snapshot native inputs, plan/validate, recheck inputs, stage exact
candidate files, then commit through a durable private journal. The private
`.ara/transactions` payload namespace retains preimages, candidate identities,
digests and approved created directories. A prepared-to-committed rename marks
the commit point. Failure before that point rolls back original bytes and
original directory existence; recovery validates the private namespace before
using it. Reads and dry runs only probe that namespace and reject a prepared
transaction without creating a lock or reading mixed source state. Corrupt
private state is an I/O-class error, not an empty pending marker shortcut.
`ara check --fix` shares this lock/recovery/transaction path and retains its
per-rule no-new-errors and normalized-equivalence guards.

The lock coordinates cooperating CLI writers in one checkout. Raw file readers
and direct external writers do not acquire that lock. Portable identity/conflict
records live in native knowledge files; private transaction payloads are not
portable merge evidence.

## Directory and Git merge

```sh
ara -C ./ours merge --base ./base --theirs ./theirs --as peer --dry-run --json
ara -C ./ours merge --base ./base --theirs ./theirs --as peer --json
ara -C ./ours merge --git peer-branch --as peer --json
ara -C ./ours resolve 'peer:N02' --json
```

Directory roots must be nonoverlapping. The planner reads complete native layers,
including opaque files. It preserves ours, allocates incoming collisions within
the correct namespace, rewrites actual structured incoming references, and
records unchanged as well as relocated identities for byte-identical replay.
Exact heading vectors survive current and archived identity lookup. Qualified
references to descendants use the complete persisted token mapping. Closed
session histories compare in destination identity/occurrence space during replay;
actual frozen history edits still reject before mutation.

Files under `rubric/`, `evidence/` and `src/` merge as external read-only bytes.
Identical or unchanged incoming bytes need no decision. An incoming addition,
change or deletion that differs from the destination becomes an
`external_read_only` conflict: ours stays in place and `ours` is the only
allowed resolution, so acknowledging it never copies incoming bytes. These files
get one file-level identity (`layer: "external"`) and no entry identities.

Mutable conflicts preserve exact base/ours/theirs candidates, keep ours and
commit safe imports with portable unresolved records; exit 1 exposes unfinished
work. Protected violations block mutation and carry complete conflict evidence.
Unknown/opaque source changes are evaluated explicitly rather than dropped from
inventory or rewritten as regenerated normalized content. Ambiguous prose tokens
and quoted historical values remain opaque and visible for review.

`merge resolve <conflict> --take ours|theirs|base --session ... --signal ...
--provenance ...` records its decision in an owning session turn. Protected
conflicts use the separate audited `merge repair --conflict-file ...
--decision reject_incoming|restore_base --expected-current ... --session ...
--signal ... --provenance ... --reason ...` path with exact captured
candidates/current fingerprint. Generic entry edits and mutable resolution
cannot override immutable history.

Both commands are themselves the audit action, so they append their own turn
without a JSONL anchor. `--session` stays required: they never select or create
a session. Under the lock they resolve that open session and allocate
`turn_count + 1` with the same allocator as `apply`. `--turn` is optional; when
given it must equal that next turn. `--timestamp` defaults to the locked UTC
clock, and `--summary` defaults to keeping the rolling summary, so a session
from an earlier date needs an explicit timestamp on that date. The session
turn, exact resolution audit, ledger decision, indexes and selected content
commit as one transaction. Reports add the resolved `session` and `turn`. A
conflict choice never implies a confirmed or refuted claim.

Git mode resolves a local ref and merge base with the installed Git executable.
It captures exact trees and blobs into temporary snapshots without changing
branches, staging the worktree, invoking a shell command supplied by the user,
or requiring a remote. Blob captures are filter-neutral; user clean/smudge
filters must not regenerate source bytes. Reports retain the Git object/source
identity and phase timings. Git convenience setup is measured separately from
native merge planning/validation/commit phases.

## Keyword search and duplicate advice

Search tokenizes maximal Unicode alphanumeric runs, lowercases Unicode text,
and uses BM25 (`k1=1.2`, `b=0.75`). It indexes complete loaded knowledge once;
source/evidence bodies are excluded. There is no persistent index, model,
network or semantic judgment. Type filtering and result limits are explicit.
Duplicate advice uses weighted set Jaccard over title/substantive body with
threshold 0.8, at most 64 retrieval candidates and ten reported candidates.
Advice is nonblocking on additions and merge, can be disabled with
`--no-duplicate-check`, and never deduplicates or merges scientific claims.

The frozen source-heldout labels and criteria under
`crates/ara-core/tests/fixtures/agent-cli/search` support bounded functional verification.
Four singleton queries and one crafted duplicate per split cannot establish
population recall or natural/paraphrase duplicate quality. Automated criteria
review is not human protocol approval. Fixed gates must not be retuned after
measurement.

## Compatibility and delivery gates

Brief text migration: default output of the read commands is now brief text,
so scripts that parsed the old tab-separated or pretty-JSON text must add
`--json`. Successful JSON fields keep their meanings; the additive fields
are listed below. Some outcomes change:

- New error codes: `ambiguous_heading`, `identity_lookup_failed`,
  `invalid_address`, `line_out_of_range`, `lines_unavailable` and
  `output_limit_too_small`.
- Reads (`show`, `path`, `refs`, `ls --under`) no longer return internal
  `merge.*` codes; `ara resolve` and `merge` keep them.
- Reads of artifacts whose only errors are `ARA107`–`ARA109` now succeed and
  carry those errors in `diagnostics`; `status`, `check` and `validate` still
  report them.
- `invalid_document` errors and `invalid_artifact`/`incomplete_artifact`
  refusals gain `details` (hint, document lists, blocking codes,
  unrepresented documents).
- `rubric/` documents and `R` entries are no longer readable (see the rubric
  migration below), and file-access roots compare ASCII case-insensitively,
  so `Rubric/requirements.md` is refused too.

Additive fields: `find` results
gain `match_count` and, when a source line matches, `line` (the first hit) and
`matches` (`[{line, text}]`, at most 20). With `--context N` they also gain
`context` (`[{start, end, lines}]`). `ls <path>` is a new positional filter.
For a Markdown document without typed entries, it returns rows with
`kind: "heading"`, `address`, `heading_path`, `title` and `source`.
`show --lines` and `--max-bytes` are new opt-in options: with either, JSON
source rows gain `display` (see [bounded reads](#bounded-and-resumable-reads)),
and with `--lines` an entry with a native section returns that section's
`source_document` row. Reads without them keep their JSON unchanged.
Selection-error `details.candidates` now cite a section that alone heads a
loaded entry as `path#ID` (`logic/claims.md#C04`) instead of its `#h/`
address, and rank it by that ID; both forms resolve to the same section, and
row `address` fields keep the canonical form.

The JSON additions preserve missing optional fields and published experiment
`fields.status`; common non-experiment status is optional. Existing JSON callers
can ignore additive fields. Public Rust types/constructors and node-kind bodies
have changed, so source compatibility is distinct from JSON compatibility.
`NodeArtifact` retains arbitrary typed extra values and optional source fields;
Rust literals must supply its `extra` map. Its richer value domain does not
implement `Eq`. Legacy dotted bold-field labels still resolve without changing
their original Markdown spelling or bytes.

Rubric migration: `rubric/requirements.md` is no longer a native document. The
public `EntryKind::Requirement` variant, native `R` IDs and rubric write and
merge-entry handling are removed. Existing rubric records (session
`logic_revisions`, `trace/logic_mutations.yaml` renames and removals, and
`trace/aliases.yaml` imports of rubric entries, local or incoming) stay
byte-exact and readable as history. They no longer resolve or redirect, so
`show R04` fails like any unknown ID, and no replacement identity is invented.
A retained private journal that names a `rubric/` target still recovers: a
committed one is only verified and retired, and a prepared one rolls back to
its authenticated preimage bytes. New transactions cannot target `rubric/`.
Before this change, an artifact holding such a rename record failed
`show <id>` and `merge` with `merge.redirect_data`. A `PAPER.md` that registers
a `rubric/` path now has an invalid registry, as one that registers `evidence/`
already did.

Workspace version is 0.1.25 for this integration. The minor/major release
decision remains pending; no tag or release is implied by engineering checks.

Core behavior affects wasm even when the embedded-viewer source hash does not.
Delivery requires a manual `scripts/embed-viewer.sh` rebuild with pinned Rust
and compatible wasm-bindgen, then `--check`; the hash alone cannot prove the
new core behavior. Browser UI and installed-agent runtime smoke are owned by
the final integration owner. The final CLI PR must merge into `main` with a
merge commit, preserving feature-branch bookkeeping. The protocol repository
has its own commit and PR. Neither integration engineering nor the native
107-operation proof claims an E0–E6 scientific experiment. Historical paper
pinning remains unresolved: 465 available native questions versus 450 reported
published questions.

Final observed checks and remaining gates: [delivery verification](verification/agent-cli-2026-10-02/README.md).

## Agent skills

The skills that teach agents these commands live in [`skills/`](../skills); see
[agent-cli-skills.md](agent-cli-skills.md) for their layout, provenance and
change rules.
