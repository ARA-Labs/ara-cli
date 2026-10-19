# How an agent reads an ARA through `ara`
**Date:** 2026-10-14

## TL;DR

An agent that answers questions about a research artifact reads its knowledge files (`PAPER.md`, `logic/`, `trace/`, `staging/`) only through `ara`, and reads `rubric/`, `evidence/` and `src/` with ordinary file tools. The reader skill teaches this in one short access block with a six-line command card, and the binary is built so that one round of parallel `find` calls and one multi-address `show` cover most needs. Measured in `ara-eval` against agents that read the files directly, this design (0.1.32) is as accurate and 6% cheaper, but 7% slower, because the agent writes longer responses. The most useful next change is to let `show` accept a pasted `find` hit line, which is the most common remaining failure.

## Problem

A reader agent pays for every model request, because each one resends the whole conversation. The first measured CLI reader (0.1.26) made 14.1 requests per question where an agent reading files directly made 9.5, and it cost 1.65 times as much. Running `ara` took almost no time: 661 calls took 4.26 seconds in total. The cost came from how the skill told the agent to use the binary, and from output that pushed the agent into extra requests.

Records [21](agent-cli-interface/21-reader-round-trips.md) and [22](agent-cli-interface/22-reader-turn-overhead.md) traced the extra requests to five causes:

| Cause | Example from the traces |
|---|---|
| Too much instruction text | The reader loaded the whole writer guide, `cli-access.md`, about 37k extra characters per session |
| Startup and orientation as separate requests | `ara ls` could not run until the guide was read, so listing became its own request |
| One search or read per request | Searches and reads were not batched |
| Sections with no address | A stray `---` at the top of three `logic/` documents in 7 of 23 corpus artifacts hid their entries, so every `show` guess failed |
| Expensive failures and large output | One bad address discarded the good ones in a multi-address `show`; one `find` returned 35k characters |

This document describes the reader interface that resulted, as of 0.1.32. Records 21 and 22 keep the step-by-step history and measurements.

## Constraints

The reader is measured against a Files reader in a controlled comparison, and that sets most of the rules:

- The CLI skill may describe how to call `ara`. It must not add research-method advice that the Files skill lacks, or the comparison would measure the advice instead of the interface. Every skill text change is reviewed against the Files skill before a run.
- Knowledge paths are read only through `ara`. In `ara-eval`, a guard refuses Read, Grep and shell access to them, and it allows only one plain `ara` command per shell call (no pipes, redirects, `cd`, `&&`, `;` or globs).
- `rubric/`, `evidence/` and `src/` belong to file tools, and `ara` refuses them with `invalid_document`.
- Command output carries no hints that the Files reader lacks. A line such as "now run `show` on these" would help only the CLI arm, so `find` prints hits and nothing else.
- A failed address must make the command exit non-zero, so the harness can count failures.

## Proposed approach

The design has two parts that change together and ship in one `ara-cli` commit: the reader skill (`skills/research-foresight-cli`) and the read commands.

### The reader skill

`SKILL.md` opens with one access block. It names which paths go through `ara` and which go through file tools, gives the shell rules, and ends with the command card:

```sh
ara -C <ara_dir> ls
ara -C <ara_dir> find '<terms>' --context 2
ara -C <ara_dir> find '<terms>' --limit 5
ara -C <ara_dir> show '<address>' '<address>' '<address>'
ara -C <ara_dir> show --document <source> --source --lines A:B
ara -C <ara_dir> show '<address>' --max-bytes 32768
```

A test runs every line of this card against the matching binary, so the card cannot drift from the commands. The agent reads `RETRIEVE.md` and `PREDICT.md` together in its first response, as the Files reader does. `cli-access.md`, the full guide, is read only after an `ara` error that does not explain its own fix. The retrieval steps tell the agent to send `ara ls` with its first searches, send one `find` per thing it needs as parallel calls, and then read everything it chose with one `show`. When a search must cover the whole artifact, the agent sends `ara find` together with a Grep of `rubric/`, `evidence/` and `src/` in the same response, which matches what a single root Grep gives the Files reader.

### `find`: ranked hits with addresses, bounded

`find` ranks knowledge entries with offline keyword search (BM25). Each hit starts with its address, kind and source document, followed by matched lines numbered by their line in the source document:

```text
E05 [experiment_plan] logic/experiments.md
  99:   - System: MLM baseline: randomly mask words in ground-truth text, ...
  102:   2. Train BBOX-ADAPTER with MLM loss: generate text chunks from ground-truth, ...
logic/solution/constraints.md [solution] logic/solution/constraints.md
  6: - **Condition**: The black-box LLM must expose a text generation API ...
  +2 more matching lines
```

The brief output is bounded so that batching many terms cannot flood the conversation:

| Option | Default | Effect |
|---|---|---|
| `--lines-per-hit N` | 3 (brief), 20 (JSON) | Keeps the first N matched lines in source order and prints `+K more matching lines`; `0` keeps all |
| `--max-bytes N` | 16 KiB (brief) | Drops whole hits from the end and closes with `N more hits; narrow with --type, --document or more specific terms` |
| `--document <path>` | none | Keeps only hits from that knowledge document; repeatable |
| `--all` | off | Keeps only hits containing every query term |

When a hit is a whole document rather than an entry, each matched line under a new section is preceded by `in: <address>`, so every hit gives the agent something it can pass to `show`. Filters act inside the ranking, so filtered hits keep their unfiltered order.

### `show`: many addresses, partial success

`show` takes any number of addresses and prints one block per address, in order. An address can be an entry ID, a document path, a heading address (`path#heading`), or a bare entry ID such as `E01` when exactly one heading in the knowledge documents starts with it. When some addresses fail, `show` still prints the blocks that resolved, prints each failure in place with its candidates, and exits 1:

```text
== H01 [heuristic] logic/solution/heuristics.md [error]
error [unknown_id]: Unknown or ambiguous selector `H01 [heuristic] logic/solution/heuristics.md`
  candidates:
    logic/solution/heuristics.md
    logic/concepts.md
    ...
```

Only address errors (`unknown_id`, `ambiguous_heading`, `invalid_address`, `invalid_document`) are partial; any other failure rejects the whole call. A block that exceeds `--max-bytes` ends with `next:` and the `--lines` range to request.

### Recovering hidden sections

A document under `logic/` that opens with a stray `---` but contains no YAML metadata is read as Markdown, with a warning, instead of being hidden as an unclosed metadata block. This applies to the typed documents (`experiments.md`, `heuristics.md`, `problem.md`) and to other `logic/` documents that have a level-one title followed by a level-two section. Real metadata is never read as text: every protected case in the parser's tests stays protected.

## Alternatives considered

| Alternative | Why it was not chosen |
|---|---|
| `find --show N`, returning entry bodies with the hits | Search-then-read pairs were not a source of excess: 56 in the 0.1.28 CLI against 70 for Files |
| One `find` taking several queries | Parallel `find` calls already batch at the same rate as Files |
| `ara find` over `rubric/`, `evidence/` and `src/` | Crosses the access boundary; the skill pairs `find` with a Grep instead |
| Next-step hints in `find` output | Files' Grep output carries no hints, so this would favour the CLI arm |
| A `next:` continuation for truncated `find` | A hit list has no line range to resume; a narrower query is the intended next step |
| Keeping the full guide preloaded | It added about 10k instruction characters over the Files reader and pushed `PREDICT.md` into a later request of its own |
| Fixing the stray `---` lines in the corpus | Changes the corpus revision for both arms of every study; the parser fix needs no new corpus pin |
| Reporting hidden documents without recovering them | Stops guessing, but the entries would still have no address |

## Tradeoffs

Partial success and a non-zero exit pull in different directions. An agent that sees exit 1 may treat the whole call as failed and retry everything. Keeping exit 1 was chosen so that failures stay countable, and the error block sits next to the good blocks, so the agent can see what worked.

The per-hit line cap hides matching lines. A hit that matches on its tenth line shows only the first three and a count. The agent can raise the cap or narrow by `--document`, but that costs a request. In the 0.1.32 pilot, accuracy did not fall when the cap was introduced (55 of 60 in both arms), and `find` output fell by a third.

The byte budget drops hits rather than cutting them, so the response stays readable, but a low-ranked relevant hit can disappear. The closing line says how many were dropped.

The fairness constraint limits what the skill can say. The skill cannot, for example, tell the agent to keep its reasoning short, because the Files skill does not. This is the main reason the remaining time gap, which comes from response length, is outside this design's reach for now.

## Migration

Every change from 0.1.29 to 0.1.32 kept existing successful output byte-identical, with one exception. When every address resolves, `show` prints exactly what it printed before; multi-address failures that used to reject the whole call now return partial output with exit 1. In `find`, the old 20-line marker `… K more matching lines` became `+K more matching lines`, and brief output is now capped at 3 lines per hit and 16 KiB unless the caller passes `--lines-per-hit 0` or a larger `--max-bytes`. JSON output keeps its old 20-line cap and has no byte budget unless one is asked for.

The skills live in this repository and change with the binary, so a harness pins the reader by one `ara-cli` commit. `ara-eval` adds each measured commit to its approved list before freezing a study; older studies keep the binary they pinned.

## What this design does not do

It does not change the reader's research method: what to look for, how to ground claims and how to answer stay identical to the Files skill. It does not search or read `rubric/`, `evidence/` or `src/`. It does not try to shorten the model's responses. It does not parse pasted `find` hit lines in `show`; they fail as a single unknown address. All of these were left out on purpose: the first two to keep the comparison fair and the boundary clear, the last two because no evidence called for them until the 0.1.32 pilot.

## Next steps

1. Let `show` read the address from a pasted `find` hit line (`<address> [<kind>] <source>`). Most of the 22 failed calls in the 0.1.32 pilot's 60 CLI sessions were this mistake.
2. Measure where the reader's extra output goes before changing anything for it. CLI sessions write 12% more output than Files sessions, and that is now the whole time gap.
3. Look at the Grep calls on knowledge paths, about half of the 22 blocked calls. The access block already forbids them, so the fix may belong in `ara-eval`'s guard message rather than in the skill.
4. Run the 375 test questions before claiming the design generalizes. Every change so far was designed on the same 60 development questions.
