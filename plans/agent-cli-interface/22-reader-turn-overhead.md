# PR 22: remove the CLI reader's per-session turn overhead
**Date:** 2026-10-10

Status: **approved 2026-10-14**. All decisions are made. Part A is implemented in 0.1.29 (22a). Part B is next; C and D follow only if the A + B paired pilot calls for them (P4). Repository: `ARA-Labs/ara-cli`, branch `feat/reader-turn-overhead`, from `feat/reader-round-trips` at `de92588` (0.1.28). The motivation, measurement design and approval live in `ara-eval` at `plans/cli-reader-turn-overhead.md`, revised with the 0.1.29 paired pilot results. In 0.1.29 the CLI matched Files' accuracy (56/60 each) at 1.16 times the cost and 1.20 times the session time. Requests that only list, search or check status were 131 against Files' 19, and their extra cost exceeded the whole gap. That makes part B the first target. Approving this plan does not authorize a paid run, a release or upstream protocol adoption.

## TL;DR

0.1.28's reader still makes 10.67 model requests per question against 9.50 for Files. The 70 extra requests over the 60 development questions all come from fixed per-session overhead. Search and read work is already at parity (355 vs 363 requests), and so is batching (2.12 vs 2.20 tool calls per request).

| Plan part | Change | Main evidence | Estimated effect |
|---|---|---|---|
| A | Bug: recover the corpus documents hidden behind a stray `---` | 3 documents in 7 of 23 paperbench artifacts have no addresses; drives repeated `unknown_id` guesses | −10 to −15 failed requests |
| B | Skill: one-step startup, no separate orientation request, one boundary block, plan-then-batch search | +40 instruction-read and +35 orientation-only requests | −55 to −70 requests (≈ −1 per question); ≈ −5k instruction characters per request |
| C | CLI: partial success for multi-address `show`; addresses on every `find` hit; bare entry-ID selectors | 14 of 26 failed shows lost good addresses; 272 address-less hits | ≈ −10 retry requests; fewer guessed addresses |
| D | CLI: bounded `find` output and a document filter | 10k characters per `find`; 35k in one call | ≈ −10 to −15k characters per session (≈ −6% input per request) |

## Problem

The `ara-eval` trace analysis compared the V3 screen (the skills released as 0.1.28) with the archived Files arm on the same 60 questions.

| Request purpose | Files | 0.1.28 | Difference |
|---|---:|---:|---:|
| Read instruction files (alone or with other tools) | 129 | 169 | +40 |
| Orientation only (`ara ls` / `ara status` / `ls`) | 16 | 51 | +35 |
| All tools failed | 2 | 5 | +3 |
| Search and/or read | 363 | 355 | −8 |
| Final answer | 60 | 60 | 0 |
| **Total** | 570 | 640 | **+70** |

1. **Startup takes an extra request.** Agents load about two instruction pages per request. Files reads SKILL, then RETRIEVE + PREDICT (+ CONTRACT): 2.15 instruction requests per session. The CLI reader adds `cli-access.md`, which takes PREDICT's place in the second request. PREDICT then needs its own request later (29 sessions, vs 1 in Files): 2.82 instruction requests per session.
2. **Orientation is a separate turn.** RETRIEVE Step 2 says "Orient with `ara ls`, then `ls <path>`". `ara` cannot run until `cli-access.md` is read, so `ls` (22 times together with `status`) becomes its own request. `status` prints writer information (next IDs) that a reader never uses. Files orients inside its first or second request.
3. **Some sections have no address.** `logic/experiments.md`, `logic/problem.md` and `logic/solution/heuristics.md` in 7 of the 23 paperbench artifacts start with a stray `---` line. Two of the four development artifacts are affected, so 30 of the 60 questions. Under `markdown::frontmatter_range`, an unclosed leading fence hides the rest of the document as frontmatter: `ARA229` warns, `ls` prints `document headings=0`, and `ls logic/experiments.md` prints `no entries`. Because `## E01: …`, `## H01: …` and `### O1: …` have no address, every guess fails with `candidates: none`. Only `logic/claims.md` has a recovery exception (`stray_fence.rs`).
4. **Failures are expensive.** 0.1.28 sessions had 65 failed tool actions (Files: 9), and in 45 of them the next request retried the same kind of operation:
   - 24 `show` `unknown_id` failures. Causes include the hidden documents, the skill's own batching example `show … E01 …`, which primed `show E01`, and guessed percent-encoded heading addresses. In 14 of the 26 failed shows, one bad address discarded the other good ones.
   - 15 root-wide `Grep` denials. Files greps the whole artifact in 47% of its searches.
5. **`find` output grows with batching.** The median query went from 1 to 5 terms after the 0.1.28 batching guidance. `find` prints every line matching any term under every hit, so each call returns about 10k characters, and one returned 35k. 272 of about 1,660 hits were `source_document` hits that printed line numbers but no section address, which led to 35 `show --document --source --lines` windows.

## Proposed changes

### A. Hidden typed documents (bug)

Reproduction: `crates/ara-core/src/stray_fence.rs` `stray_opener_before_typed_entries_is_recovered` (it was added ignored, then enabled by the fix). It uses the corpus shape of all three documents.

Options (decision P1):

| Option | Change | Trade-off |
|---|---|---|
| **R1, recommended** | Extend the conservative claims exception to the typed Markdown documents `logic/experiments.md`, `logic/solution/heuristics.md` and `logic/problem.md`. The text after the opener must be one level-one title, then entry headings (`## E01: …`, `## H01: …`, `### O1: …` under `## Observations`/`## Gaps`), each opening with a non-empty `- **Field**: value` bullet. Every case in `uncertain_metadata_stays_protected` stays protected. A `- **Field**:` line cannot be YAML, because `*` starts an alias. Loads warn `ARA228`-style "recovered" instead of `ARA229` | Fixes all 21 corpus documents without changing the corpus pin; a narrow, typed rule like the existing claims rule |
| R2 | Remove the stray lines in `ara-paperbench` (21 files) | Simplest; changes the corpus revision for both arms, so new study pins and an inventory receipt |
| R3 | Keep the documents hidden, but make the read path actionable: `ls` prints `hidden by unclosed --- at line 1; read with show --document <path> --source`, and `show <path>#…` on a hidden document returns that hint in the error | Stops guessing, but entries still have no address, so reads cost a source window |

R1 is a patch bump. It needs tests for the three corpus shapes, protection of every existing metadata case, a docs update to `docs/stage-1-core-parse-validate.md`, `docs/stage-5-check.md` and `docs/agent-cli.md`, and a CHANGELOG entry. `only_claims_md_can_recover` becomes "only typed documents with their typed entries can recover".

### B. Reader skill restructure (`research-foresight-cli` only)

These change how the reader calls `ara`, not its method. RETRIEVE Steps 1 and 3–6, the honesty rails, PREDICT and CONTRACT stay unchanged. The shared `cli-access.md` stays byte-identical across the three skills and still ships with the reader as an on-error reference.

- **B1 One-step startup.** Replace the two boundary paragraphs (SKILL.md and RETRIEVE.md, about 3k characters each) with one "Access" block in SKILL.md of about 1.5k characters. It covers:
  - which paths go through `ara` and which through file tools
  - one quoted `ara` command per shell call
  - a six-line command card (`ls`, `find`, `show` with several addresses, `show --document … --source --lines`, `--limit`, `--max-bytes`)
  - the find/show line-number rule in one sentence
  - "report `ara` errors; never fall back"

  SKILL.md tells the agent to read RETRIEVE.md and PREDICT.md together in one response, as Files agents already do. `cli-access.md` becomes "read only when an `ara` error's message does not tell you the fix". The reader then loads the same pages as Files plus about 1.5k characters, instead of 10.4k more.
- **B2 No orientation-only turn.** Replace "Orient with `ara ls`, then `ls <path>`" with: "`ara ls` needs nothing else: send it in the same response as your first searches. Use `ls <path>` only to choose between candidates." Drop `status` from the reader's text; it remains documented in `cli-access.md`.
- **B3 Plan, then batch.** Open Step 2 with: "Before your first search, list the separate things the frame needs (usually two to five). Send one `find` per need, as separate shell calls in one response; after the hits return, `show` every address you will read in one call." Add one worked example with placeholder addresses taken from `ls`/`find` output. This replaces the `E01` example that primed bad addresses. End Step 2 with the one-sentence nudge from Anthropic's parallel-tool guidance: "Before each response, list what you still need, then request every item that does not depend on another's result in this one response."
- **B4 Negative evidence in one response.** "Root-wide Grep is refused. When a search must cover the whole artifact, send `ara find` and a Grep of `rubric/`, `evidence/` and `src/` in the same response." This describes mechanics the Files arm gets from a single root grep.
- **B5 Remove reader-irrelevant text.**
  - The writer-task links ("Use only the shared read and relation tasks…", twice).
  - The provenance and pin sentences ("Pending protocol review…", the live-pin paragraph).
  - The `ls --unfinished` inactivity-field paragraph in RETRIEVE.md.
  - The duplicated line-numbering explanation.

  Phrase the remaining rules positively.

The skill tests keep checking that every documented command line parses. Add a test that the reader's SKILL.md command card matches commands the binary accepts.

**Implemented (22b, 0.1.30).** B1–B5 as described. A fairness review against the Files skill asked for four wording changes, all applied: no LS tool in the reader's tool list, "Follow it exactly" instead of a second "Read it now", a mechanics-only batching sentence instead of a per-response planning step, and the `ls` call in the Step 2 example. Two items stay as approved: B4 names `rubric/` (Category B questions point there), and `cli-access.md` keeps its orientation text because the three copies stay identical; the reader opens it only after an unexplained error.

### C. Fewer failed and guessed addresses (binary)

- **C1 Partial-success multi-address `show`.** Print every resolved block in order. Print each unresolved address as an `error:` block with its `candidates:`. Exit 1 when any address failed, so failures stay visible to the harness. `--json` adds a per-address `errors` array. Tests:
  - All-good output is byte-identical to today.
  - One bad address among good ones returns the good blocks plus the error.
  - Byte budgets and paging are applied across the blocks as today.
- **C2 Every `find` hit has an address.** A `source_document` hit prints `in: <address of the innermost containing heading>` when the hit lies under a heading. Tests: the printed address round-trips through `show`, and hits outside any heading print no `in:` line.
- **C3 Bare entry-ID selectors.** `show E01` (also `H01`, `O3`, `G1`) resolves to the unique heading in a knowledge document whose text starts with `<ID>:` or `<ID> —`. Several matches return `ambiguous_heading` with candidates. An ID that is also a native node or claim ID keeps its current meaning. Tests: the unique, ambiguous and native-ID precedence cases.

### D. Bounded `find` (binary)

- **D1 Per-hit line cap.** Brief `find` prints at most 3 matching lines per hit (`--lines-per-hit N`; `0` for all), followed by `+K more matching lines`.
- **D2 `--max-bytes` on `find`.** The same budget and paging semantics as `show`, defaulting to the brief 16 KiB. When the budget cuts the output, it says "N more hits; narrow with --type, --document or more specific terms".
- **D3 `--document <path>`** (repeatable): restrict hits to those documents. The agent already tried this flag.
- **D4 `--all`**: require every query term (AND), for multi-term queries that would otherwise flood.

Tests: hit order is unchanged by D1 and D2; the cap and budget are respected; `--document` returns a subset of the unfiltered hits; `--all` returns hits containing every term.

### Not proposed now

| Idea | Why not now |
|---|---|
| `find --show N` | Search-then-read round trips: 56 in 0.1.28 vs 70 in Files; this is not a source of excess |
| Multi-query `find 'q1' 'q2'` | Batching is at parity; B3 asks for parallel `find` calls first |
| `ara find` over `rubric/`, `evidence/`, `src/` | Crosses the access boundary (those roots are file-tool territory); B4 covers it in the skill |
| Next-step hints in `find` output | Files' Grep output carries no such hint, so this would favour the CLI arm |

**Implemented (22a, 0.1.29).** R1 as described. `markdown_entries` reads the document's own sections, so recovered experiment plans and heuristics load as typed entries. In all 7 corpus artifacts, `ls` now lists `experiment_plan=N`, `heuristic=N` and the problem headings, and `show 'logic/experiments.md#E01'` resolves.

**Looser rule (P5, decided 2026-10-11).** The same 7 artifacts also opened `logic/concepts.md`, `logic/related_work.md` and the untyped `logic/solution/` documents with a stray `---`. Every other `logic/` Markdown document now recovers when:
- a level-one title and a level-two section follow the opener;
- no other level-one title follows and no code fence is left open at EOF;
- the text does not load as a YAML mapping (frontmatter metadata, which `PAPER.md` also requires to be a mapping) or null (a heading-only block of YAML comments).

Markdown bullet lists load as YAML lists, so they do not count as metadata. Across all 32 corpus artifacts, only the 7 affected ones changed, and none has an `ARA229` left.

## Decisions

- **P1:** decided 2026-10-10: R1.
- **P5:** decided 2026-10-11: the looser rule for the other `logic/` documents.
- **P2:** decided 2026-10-14: the reader drops the `cli-access.md` preload and reads it only when an `ara` error does not explain the fix.
- **P3:** decided 2026-10-14: exit 1 when any address failed.
- **P4:** decided 2026-10-14: A + B first. C and D are built only if the A + B paired pilot does not meet the decision rule.
- **Evaluation:** decided 2026-10-14: each revision is measured with a paired Files-and-CLI pilot in `ara-eval`, not a CLI-only screen.

## Implementation steps

Each functional sub-PR gets a patch bump and a CHANGELOG entry and targets `feat/reader-round-trips`. Each revision is recorded for an `ara-eval` screen.

1. 22a: part A with tests and docs (0.1.29).
2. 22b: part B (0.1.30). Run the A + B paired pilot in `ara-eval`.
3. 22c: part C (0.1.31).
4. 22d: part D (0.1.32). Run the A–D paired pilot in `ara-eval`. Steps 3 and 4 happen only if step 2's pilot calls for them.
5. Rewrite this plan as `docs/agent-cli-interface/22-reader-turn-overhead.md` with the measured outcome, and update both indexes.

## Risks

- **Overfitting the development set.** The changes target patterns counted on the same 60 questions. The 375 test questions remain the out-of-sample check (ara-eval D3).
- **Fairness.** B4 and the B1 command card describe mechanics only. The proposed B text must not add method guidance that the Files arm lacks; review it against the Files skill before screening.
- **Recovery rule.** R1 must never surface real metadata as Markdown. All existing protected cases stay as tests.
