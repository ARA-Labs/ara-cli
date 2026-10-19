---
name: research-foresight-cli
description: >
  ARA World Model — read-only reasoning engine over ONE Agent-Native Research Artifact (ARA), run
  LOCALLY with the coding agent itself as the LLM (no SDK, no API key). Given an ARA directory and a
  free-text query, it answers any question about the ARA — a forward "what if I change X", but
  equally why-did-this-work, what-should-I-try, is-this-sound, how-do-these-compare, or anything
  else — by retrieving precedent through ara from the ARA's native files (references/RETRIEVE.md) and answering
  as the Predictor (references/PREDICT.md): a bold, grounded, falsifiable Answer shaped to what the
  question actually calls for.
  TRIGGERS: ask the world model, wm predict, predict with the world model, what if I change X,
  forecast the loss curve, will this help, why did this work, what should I try next,
  is this claim sound, compare these, retrieve precedent, what precedent surfaces.
allowed-tools: Read, Grep, Glob, Bash(ara *)
metadata:
  author: ara-commons
  category: research-tooling
  version: "1.1.0"
  tags: [research, world-model, retrieval, prediction, grounded-answering]
---

# research-foresight — the ARA World Model

You (the coding agent) are the LLM that runs the engine — no SDK, no API key, no network call. The
engine is three reference contracts under this skill's `references/` directory (quote every path;
it may contain spaces):

- `references/CONTRACT.md` — the foundation both contracts bind to; if documents disagree, it wins.
- `references/RETRIEVE.md` — the Retriever: agentic search + semantic rank over the ARA's native files.
- `references/PREDICT.md` — the Predictor: grounded, honest answering of the question asked.
- `references/cli-access.md` — the full `ara` guide; read it only when an `ara` error message does
  not tell you the fix.
- `templates/reader-report.md` — load directly if a standalone report shape is needed.

## Access

Read `PAPER.md`, `logic/`, `trace/`, `staging/` and the paths `PAPER.md` registers in
`knowledge_paths` only through `ara`. Read and search `rubric/`, `evidence/`, `src/` and this
skill's pages with the Read, Grep and Glob tools. Run one quoted `ara` command per shell call
with nothing else on the line: no pipes, redirects, `cd`, `&&`, `;` or globs. Send independent
calls as separate shell calls in the same response.

```sh
ara -C <ara_dir> ls
ara -C <ara_dir> find '<terms>' --context 2
ara -C <ara_dir> find '<terms>' --limit 5
ara -C <ara_dir> show '<address>' '<address>' '<address>'
ara -C <ara_dir> show --document <source> --source --lines A:B
ara -C <ara_dir> show '<address>' --max-bytes 32768
```

Use only addresses that `ls`, `find` or an error printed. `find` prints line numbers of the whole
source document, so read around a hit with the `--document` window; `show <address> --lines`
counts inside that address. A truncated block ends with `next:`; rerun with that `--lines`. On
`unknown_id` or `ambiguous_heading`, pick from the printed `candidates:`. A line
`incomplete: <path> (<codes>)` means that one file was not fully read; the rest of the output is
valid even though the command exits 1. Report any other `ara` error as it is; never fall back to
file tools on knowledge paths.

## Inputs

From the user's message (or `$ARGUMENTS`): an **`<ara_dir>`** (the ARA in scope) and a free-text
**query**. If `<ara_dir>` turns out not to be an ARA (a plain paper, repo, or notes folder),
compile it into one first with `/compiler <path>`, then rerun this skill.

## Procedure

Read `references/RETRIEVE.md` and `references/PREDICT.md` together, in one response, before you
start.

1. **Retrieve — adopt `references/RETRIEVE.md`.** Follow it exactly against
   `<ara_dir>`.
2. **Answer — adopt `references/PREDICT.md`.** Follow it exactly, consuming the
   retrieval from Step 1.

Render the `answer` prominently, then the honesty envelope (`grounded_inference` /
`speculative_leap` / `basis` / `reasoning` / `confidence` / `confidence_reason` / `falsifiable`).

The engine is read-only: read nothing outside `<ara_dir>` and this skill's `references/`; write
nothing anywhere.
