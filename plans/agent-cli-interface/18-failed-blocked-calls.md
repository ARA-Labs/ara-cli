# PR 18: stop failed and blocked calls in CLI agent sessions
**Date:** 2026-10-04

Status: **draft, pending review**. Repository: `ARA-Labs/ara-cli` (binary and
[skills](../../docs/agent-cli-skills.md)). Dependencies outside this repository: the
`ara-eval` harness (H1, H2) and the `ara-paperbench` corpus (C1). Evidence: preliminary
`runs/e1-test` ([analysis doc](https://claude.ai/code/artifact/70b438b7-c4a1-4a12-9899-d107136b03fe)).
Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

A CLI session spends **3.25 model calls on tool calls that fail or are blocked**,
out of 5.3 more calls than a Files session. Each wasted call costs about 20 s and
re-sends about 39k prompt tokens. Five causes account for 93% of them. The largest
is the skill never telling the agent that bash runs only single `ara` commands.
The second is the CLI serving `rubric/requirements.md`, a PaperBench grading file
that is not research knowledge: agents make about 10 `ara` calls per Category B
session to read a file one `grep` answers. The fix is to stop handling the rubric
in the CLI, not to make it handle the rubric better. The rest are an unclosed `---`
that hides a whole document, output larger than the agent's tool shows, and errors
that do not say what to do next. Each fix starts with a reproducing test, then a
dev-split pilot.

## Problem

### Evidence

Snapshot 2026-10-04 16:53 PDT: 580 CLI and 581 Files sessions with complete
traces (about 52% of the run; 6 CLI timeouts excluded). Agent `glm-5.3-flash`,
`ara 0.1.23` (`4f70972`), skill `research-foresight-cli` at protocol `03f19c7`,
Pi 0.82.1. Preliminary and descriptive, not the registered analysis.

| Per session (mean) | Files | CLI |
|---|---|---|
| Model calls | 8.65 | 13.97 |
| Calls spent on failed or blocked tool calls | 0 | 3.25 |
| Wall time | 166 s | 289 s |
| Cost (list price) | $0.0121 | $0.0280 |

A model call that issues k tool calls counts 1/k toward each of their causes.
158 times, a session ran five or more bad tool calls in a row.

### Root causes

| # | Cause | Model calls / session | Share |
|---|---|---|---|
| 1 | The agent doesn't know bash runs only one `ara` command | 1.10 | 34% |
| 2 | The CLI serves the PaperBench rubric; `--heading` misses on it | 0.77 | 24% |
| 3 | Claims that fail to parse make 8 artifacts invalid | 0.53 | 16% |
| 4 | Truncated output, then reading Pi's log | 0.38 | 12% |
| 5 | `show --document` on `evidence/` or `src/` | 0.24 | 7% |
| | Other (scattered errors) | 0.23 | 7% |

**1. Shell habits.** The harness guard (`ara-eval/src/ara_eval/pi/guard.ts`) allows
one `ara` command per bash call and rejects unquoted `| > & ; * ? [ ] ( ) # ! ~ $`.
Neither the harness nor the skill says so; the agent learns from denials. Typical:
`ara -C … find '…' --json 2>/dev/null | head -c 6000`, `cd … && ara …`. 43% of the
piped commands pipe `find`, whose output has a median of 2.7k characters, so most
of this is habit, not a reaction to long output. Real harnesses usually allow
pipes, so this cause is partly specific to the experiment, but it counts in its
results. The registration's dev pilot recorded the same pattern
(`ara-eval/plans/registration-reading-wave.md`, "3.2 denied calls per session").

**2. The rubric.** `rubric/requirements.md` is PaperBench's expert-written
reproduction rubric, flattened by `generate_rubric_requirements_md.py` into
`R01`…`Rnn` with headings cut to 60 characters plus a literal `...`. It is grading
material, present only in artifacts compiled for the benchmark. The eval uses it as
reading material: Category B questions are generated from it and tell the agent to
open it. The CLI handles it because `compiler-cli` needed an allowlisted write path
for it (`compiler-cli/references/cli-access.md`, "the fixed compiler allowlisted
case"); read access followed. The handling is partial: `show --document` returns the
whole file, but `show R84` fails with `unknown_id`, `--heading R84` fails with a
`merge.unknown_identity` code, and `find`/`ls` do not index requirements. Meanwhile
the guard lets CLI agents `grep` and `read` the file directly, as Files agents do.

| Rubric access per Category B CLI session (139 sessions) | Count |
|---|---|
| `ara show` that fails | 5.84 |
| `ara show` that succeeds | 3.22 |
| `ara show` blocked by the harness | 1.15 |
| `grep`/`read` tool (allowed) | 0.07 |

95% of all `--heading` misses are on this file.

**3. Claims that fail to parse.** 9 corpus artifacts fail validation. In 7, line 1
of `logic/claims.md` is `---` and line 2 is `# Claims`: no YAML and no closing fence,
most likely a compiler-written opener or horizontal rule. CommonMark renders it as
a horizontal rule, but `frontmatter_range` (`ara-core/src/markdown.rs:36`) treats any
first non-blank `---` as a front matter fence and, when unclosed, extends it to end
of file on purpose, so metadata such as `## metadata only` in a broken block never
becomes a selectable heading (test at `markdown.rs:407`). The whole file is hidden
and no claims parse. In `nanogpt-speedrun` the front matter is valid and closed, but
claim headings read `## C01 — Title`, and `parse_claims` requires `C01: Title`. The
ninth, `rebench-restricted_mlm`, has a different trace format and is out of scope.
Either way every evidence link fails with "unknown claim" (6–54 errors per
artifact), no diagnostic names the cause, and `Artifact::load` (`agent.rs:64`)
refuses `find`, `ls`, `status`, `open` and `refs`. The 8 artifacts are 23% of CLI
sessions and waste 1.95 calls each; Files agents read the files and never notice.
Deleting the stray line makes `find` work on `pinn`.

**4. Truncation.** `ls --json` prints about 50 KB on one line (`body`,
`source_fields` and per-kind fields even without `--full`). Pi keeps the last 50 KB
of output and saves the full text to `/tmp/pi-bash-*.log`, so the agent sees the
broken tail of a JSON document. 54% of `ls --json` calls are truncated; the agent
then tries to read the log, and the guard blocks it.

**5. Wrong access path.** The agent runs `show --document` on `evidence/` (77%) or
`src/` (22%). These are source files read with file tools, but the error says only
"Document outside the knowledge boundary".

## Goals and acceptance

On a dev-split pilot against the unchanged CLI condition and Files:

1. Model calls spent on failed or blocked tool calls fall from 3.25 to below 1.0 per
   CLI session.
2. Category B CLI sessions reach the rubric with file tools, with no `ara` calls on it.
3. No run of five or more consecutive bad tool calls in more than 2% of sessions.
4. Accuracy stays within the registered non-inferiority margin (0.03) of Files and
   of the unchanged CLI condition, overall and on Category B.

The text output format (B7, S5) is in scope because oversized JSON causes the
truncation in cause 4; its token savings come with it. Other token reductions that do
not remove calls (smaller skills, the required `status`/`ls` opening) get their own
plan after this one is measured.

## Proposed changes

### Binary

| # | Change | Cause | Contract impact |
|---|---|---|---|
| B1 | Remove `rubric/requirements.md` from the read boundary (`knowledge_document`, `agent.rs:980`) and from parsing (`parse.rs:444`). `show --document rubric/…` returns `invalid_document` with the B8 hint | 2 | Breaking for `show --document rubric/…` callers |
| B2 | Remove rubric handling from writes and merge: the `Requirement` entry kind and `R` ids (`write/fields.rs`, `write/logic.rs`), the document and transaction allowlists (`write/documents.rs`, `write/transaction.rs`, `write/source.rs`), and the merge special cases (`merge/identity.rs`, `merge/markdown.rs`). `rubric/` becomes a plain source directory, merged like `evidence/` | 2 | Breaking: removes `EntryKind::Requirement` from `ara-core`'s public API |
| B3 | `--heading` ignores case and surrounding whitespace, treats a trailing `...` on the real heading as a truncation marker, and accepts a unique prefix | 2 (general) | More inputs match; ambiguity still refuses |
| B4 | A missed or ambiguous `--heading` returns `unknown_id` with `candidates`: the document's heading paths, closest first, capped at 40. Read paths never surface `merge.*` codes | 2 (general) | Additive error field; error code changes |
| B5 | `frontmatter_range` treats an unclosed opening `---` as front matter only when the next non-blank line looks like YAML (`key: value`); otherwise it is a horizontal rule and the document parses. Either way `check`/`validate` warn about the unclosed fence, naming the file and line | 3 | Documents with a stray leading `---` now parse; new warning code |
| B6 | Read-only commands (`find`, `ls`, `show`, `status`, `open`, `refs`, `path`) run on artifacts with validation errors and return the diagnostics in `warnings`; `incomplete_artifact` and writes still refuse | 3 | Refusal becomes success with warnings |
| B7 | Text is the agent format: every read command prints compact text by default ([Agent text output](#agent-text-output)); `--json` keeps the complete `ara.*/v1` contracts for programs | 4 | Text output of `status` and `show <ids>` changes from JSON to text; JSON unchanged |
| B8 | `show --document … --max-bytes N` cuts at a line boundary and reports `truncated` and `next_line`; `--from-line N` continues | 4 | Additive flags and fields |
| B9 | `invalid_document` errors add a `hint`: the allowed `show --document` roots, and that `rubric/`, `evidence/` and `src/` are read directly | 2, 5 | Additive error field |
| B10 | `parse_claims` accepts `C01 — Title`, `C01 – Title` and `C01 - Title` as well as `C01: Title` | 3 | More claim headings parse |

### Agent text output

Agents read text; programs read JSON. Text carries what an agent acts on, once, and
nothing it would only re-read. Measured on `bam`, today's JSON is 50.2k characters
for `ls` against 6.0k for its existing text form.

| Command | Default text output |
|---|---|
| `ls` | One line per entry: `ID<TAB>kind<TAB>title` (exists today) |
| `find` | One line per hit: `ID<TAB>kind<TAB>title<TAB>excerpt`, the excerpt at most 160 characters around the match, not the document's opening |
| `show <ids>` | Per entry, a header line `== C01 claim logic/claims.md sha256:<digest>` then its Markdown body once; requested relations as `parents: N01, N02` lines |
| `show --document` | A header line `== <path>[#heading] sha256:<digest>` then the content; with `--max-bytes`, a footer `… truncated; continue with --from-line N` |
| `status` | Counts per kind on one line, `errors: N, warnings: N (codes)`, and next free ids |
| `path`, `refs`, `open` | One line per item |
| Any command | Validation warnings once on stderr as a count with codes and `run ara check`; errors as `error[code]: message`, then `hint:` and `candidates:` lines |

The digest stays in the header in full, since write skills pass it as `expected` to
guarded edits. Text layouts get snapshot tests and a section in `docs/agent-cli.md`,
but they are not a versioned JSON contract. `--json` output is unchanged, so no
`ara.*/v1` format version changes.

### Skills

| # | Change | Cause |
|---|---|---|
| S1 | A short "Running ara" section in `cli-access.md` and `SKILL.md`: one `ara` command per call, no pipes, redirects or `&&`, errors arrive without `2>&1`, bound output with `--heading`, `--max-bytes` and `--from-line` | 1, 4 |
| S2 | `research-foresight-cli`: list `rubric/`, `evidence/` and `src/` as files read with file tools (`grep`, `read`), and say which paths go through `show --document` | 2, 5 |
| S3 | `compiler-cli`: write `rubric/requirements.md` as a plain file (the same verbatim conversion as `generate_rubric_requirements_md.py`); remove "the fixed compiler allowlisted case" from `ara-schema.md` and all three `cli-access.md` copies | 2 |
| S4 | On a heading miss, retry with a listed `candidates` entry instead of guessing | 2 |
| S5 | All CLI skills drop `--json` from read commands and take the digest from the text header; write inputs (`apply` JSONL) are unchanged | 4 |

### Outside this repository

- **H1 (`ara-eval`):** the guard's denial message names the rule and the
  alternative ("run one `ara` command with no pipes or redirects; bound output with
  `--max-bytes`"). The denial is when the agent learns. Applies only to new runs.
- **H2 (`ara-eval`):** any compile-condition guard allows the compiler to write
  `rubric/requirements.md` directly, since `ara` no longer writes it.
- **C1 (`ara-paperbench`):** remove the stray `---` from the 7 `claims.md` files, so
  the corpus is clean for any `ara` version. B5 already makes them parse. Applies
  only to new runs; e1-test stays on the vendored corpus.

## Implementation steps

Per the bug process, each PR starts with tests that reproduce the failure on the
current binary.

1. **Measurement (`ara-eval`).** Move the trace scripts (`events.py`, `calls.py`,
   `roots.py`, now in the session scratchpad) into `ara-eval/src/analysis/` with a
   frozen snapshot list, so every comparison uses the same root-cause attribution.
2. **PR 18a — stop handling the rubric (B1, B2, B9 for `rubric/`, S2, S3).**
   Reproducers: `show --document rubric/requirements.md` on a fixture currently
   succeeds and must return `invalid_document` with a hint; a directory merge of two
   artifacts with identical and with differing `rubric/` must behave like `evidence/`.
   Remove the rubric cases from `ara-core` and update the four core test files that
   cover them (`parse_fixtures.rs`, `write_logic_documents.rs`, `merge_identity.rs`,
   `merge_markdown_layers.rs`). Check the `ara-core` public API diff, since
   `EntryKind::Requirement` goes away.
3. **PR 18b — heading selection (B3, B4, S4).** Reproducers in
   `crates/ara-cli/tests/agent_reads.rs`: the full text of a heading stored with a
   trailing `...`; a case-different heading; a guessed name that returns `candidates`;
   an ambiguous prefix that still refuses; a fallback miss that returns `unknown_id`,
   not `merge.unknown_identity`. Change `heading_matches`, `source_output` and the
   `show_document` fallback in `agent.rs`, and the error shape in `output.rs`.
4. **PR 18c — errors and invalid artifacts (B5, B6, B9).** Reproducers: a
   `claims.md` that starts `---` then `# Claims` parses all its claims and `check`
   warns once, naming line 1; `---` then `title: Broken` with no closing fence still
   hides `## metadata only` (the existing `markdown.rs:407` case); closed front matter
   is unchanged. A fixture with a dangling claim reference still answers `find`, `ls`
   and `show` with `warnings`; writes and `incomplete_artifact` still refuse;
   `show --document evidence/x.md` returns the `hint`; a `claims.md` with
   `## C01 — Title` headings parses (B10). Change `frontmatter_range` and add the
   warning in `ara-core`; split `Artifact::load` into strict and lenient loading.
5. **PR 18d — agent text output and bounds (B7, B8, S1, S5).** Reproducers:
   `status` and `show <ids>` without `--json` print JSON today; warnings repeat on
   every command. Snapshot tests for each command's text layout; `--json` output
   byte-identical before and after; the text digest equals the JSON digest;
   `--max-bytes` cuts at a line boundary; `--from-line` resumes with no gap or overlap.
   Measure text against JSON size on the corpus artifacts.
6. Each PR bumps the patch version, adds a `CHANGELOG.md` entry, updates
   `docs/agent-cli.md`, and updates the skills in the same change; `tests/skills.rs`
   checks that every command the skills name exists. 18a changes a public API
   (`EntryKind::Requirement`) and 18d changes default text output; record both for
   the pending minor/major release decision.
7. **H1, H2 and C1** as separate changes in their repositories, after e1-test finishes.
8. **Pilot** on the dev split with the new `ara-cli` commit (binary + skills), the
   same model, Pi version and repetitions. Report the root-cause table and the
   Category B rubric-access table above, next to accuracy against Files with the 0.03
   margin.
9. After the pilot, rewrite this plan as a design record in
   `docs/agent-cli-interface/` and remove it from `plans/`.

## Decisions

- **2026-10-04:** the CLI stops handling `rubric/requirements.md` for reads, writes
  and merge (B1, B2). It is benchmark grading material, the harness already allows
  direct reads, and CLI access made Category B sessions slower. Indexing requirements
  as entries and special-casing `R` ids in `--heading` were rejected.
- **2026-10-04:** an unclosed opening `---` counts as front matter only when the next
  non-blank line looks like YAML, with a warning either way (B5). A diagnostic alone
  leaves the documents unreadable; treating every unclosed fence as a horizontal rule
  would expose broken metadata as headings.

- **2026-10-04:** `parse_claims` accepts dash separators in claim headings (B10).
- **2026-10-04:** agents read text, not JSON (B7, S5). Every read command gets a
  complete compact text form and the skills stop passing `--json`; JSON stays the
  unchanged program contract. This replaces both shrinking default `ls --json` (a
  breaking contract change) and adding a `--brief` flag (a third format).

## Open questions

- **Q1.** Pilot size: the same 3 repetitions, or fewer to save budget?
