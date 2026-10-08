# PR 21: fewer model round trips for the CLI reader
**Date:** 2026-10-08

Status: **approved 2026-10-08** at the researcher's direction: build on 0.1.27, adopt the Files grounding rule (S2), and decide B1 after the V3 screen. No skill, binary or test change is implemented yet. Repository: `ARA-Labs/ara-cli`, branch `feat/reader-round-trips` from `feat/agent-cli-interface` at `5bcf376` (0.1.27). The measurement design lives in `ara-eval` at `plans/cli-reading-round-trips.md`; this plan covers only the changes that ship from this repository. Approving it does not authorize a paid run, a release or upstream protocol adoption.

## TL;DR

The local 0.1.26 reading pilot found that the shipped CLI reader cost 1.65 times as much as Files and took 1.34 times as long, at similar gold-relative accuracy. The binary took 4.26 seconds across 661 calls; the cost came from 48.6% more model requests, each carrying 37.6% more input. This plan proposes four skill changes, each one testable on its own: a reader-only access guide, the Files arm's grounding rule, explicit batching guidance, and a conditional binary option that returns search hits with their full bodies. The `ara-eval` screens decide which ones ship.

## Problem

The [`ara-eval` result report](https://github.com/ARA-Labs/ara-eval/blob/main/docs/local-cli-026-reading-results.md) compares Files and CLI sessions on 60 development questions with `glm-5.3-flash`. Native execution is negligible. The model requests are the cost.

| Measure, 60 sessions per arm | Files | CLI 0.1.26 |
|---|---:|---:|
| Model requests | 570 | 847 |
| Mean input tokens per request | 20,606 | 28,362 |
| Requests classified as discovery-only | 16 | 194 |
| Requests with more than one tool action | 417 (73%) | 473 (56%) |
| Mean instruction text returned per session, characters | 41,470 | 78,562 |
| Native `ara` seconds, total | 0 | 4.26 |

Three causes are visible in the traces and the skill text.

1. **The reader loads the whole writer guide.** `references/cli-access.md` was read in 59 of 60 sessions. At 0.1.26, 9.6k of its 35.6k characters cover reading; the rest are JSONL transactions, PM recording, compiler generation and merge repair. Plan 20 grew the shared file to 44.3k characters at 0.1.27, and `SKILL.md` now links two of its task subsections. Every later request in the session carries that text again.
2. **The CLI grounding rule is stricter than the Files rule.** Upstream Files `RETRIEVE.md` says to read the surrounding section when a grep hit is too terse to judge. CLI `RETRIEVE.md` (the paragraph beginning "A find hit line or excerpt is never grounding proof") requires `show` on every cited body even when the excerpt is sufficient. This adds `find`-then-`show` round trips that Files does not have, and it makes the comparison mix interface with method.
3. **Search and read are serialized.** The traces separate inventory, one keyword per `find`, address selection, `show`, negative-evidence search and trace reads into successive requests. `find` is BM25 over all query terms, so `find 'FARE APGD LLaVA'` already ranks hits for all three terms, and `show` already accepts several addresses. Files did the equivalent with `FARE|APGD|LLaVA` in one grep. The skill teaches neither, and its "one `ara` command per shell call" rule is easy to misread as one command per model request.

Tool errors are a smaller factor: the 23 pairs where neither arm had a tool error still had a 1.65 cost ratio.

## Proposed changes

Each change is a separate commit, so the `ara-eval` screens can pin each cumulative step to an exact `ara-cli` revision.

### S1: reader-only access guide

Split `cli-access.md` into two shared files.

| File | Contents | Copied into |
|---|---|---|
| `references/cli-access.md` | Boundary and failures, shell calls, reading (orient, search, read, cite), and the read-only task subsections: ancestry/citations/identities and unfinished work | `research-foresight-cli`, `research-manager-cli`, `compiler-cli`, byte-identical as today |
| `references/cli-write.md` | Structured output and exact source before writes, JSONL transactions, the write task subsections, PM complete turn, compiler generation, review and proof | Writer skills only: `research-manager-cli` and `compiler-cli` |

`research-foresight-cli` ships only `cli-access.md`. Update its `SKILL.md` links to the moved anchors. Writer skills link `cli-write.md` wherever they now link a write subsection. No rule is deleted; the writer guidance moves to a file the reader never loads.

Estimated effect, which the screen must confirm: the reader's guide shrinks from about 44k to about 12k characters. This is the input-per-request factor.

### S2: the Files grounding rule

Replace the CLI `RETRIEVE.md` paragraph that requires verifying every cited body with the Files rule, keeping the CLI mechanics: when a `find` hit is too terse to judge, `show` the surrounding section or window the source document around the hit, and cite the address `ara` printed. Keep "rank on the real prose, not a keyword coincidence". Remove the duplicate full-read requirement from `cli-access.md` "Reading" step 3 for reading; keep the full-read rule before guarded writes in `cli-write.md`, where digests need it.

This is a method change, not an access change, approved as `ara-eval` decision D2. It makes the two arms follow the same grounding rule.

### S3: batching guidance

Add a short "Fewer requests" paragraph to `RETRIEVE.md` Step 2 and to `cli-access.md` "Reading":

- Put every keyword for one information need into a single `find` query; `find` ranks all terms together.
- `show` several addresses in one call: `ara -C <artifact> show 'logic/claims.md#C03' E01 trace:N04`.
- When independent `ara` commands are needed, issue them as separate shell tool calls in the same response; "one command per shell call" limits the shell line, not the response.
- Skip `ls` when the question already names the entities to search for.

The skill tests already check that every documented command line parses against the binary.

### B1 (conditional): search hits with full bodies

Add `ara find <query> --show N` (name to settle in review): print the complete brief `show` block for the top `N` hits after the ranked list, under the normal `--max-bytes` budget and paging rules. It collapses the find-then-show round trip into one call without changing what the agent reads. It reuses the existing show renderer, so the output stays the same as `show`.

Implement B1 only if the S1–S3 screens leave CLI above the Files request count. It changes shipped behavior, so it needs a patch bump, a `CHANGELOG.md` entry, `docs/agent-cli.md` updates and unit tests: hit order matches plain `find`, bodies equal standalone `show` output, the byte budget pages correctly, and `--show 0` matches plain `find`.

## Tests

- `crates/ara-cli/tests/skills.rs`: keep `cli_access_copies_are_identical`; add the same check for `cli-write.md` copies; assert the reader skill has no `cli-write.md`; point the write-recipe tests (`cli_access_digest_recipes_match_each_guard_scope` and the `cli_access()` helper) at `cli-write.md` where the moved text lives.
- Existing tests that documented commands and long flags exist cover S3's new examples.
- `cargo test --workspace` and the installed-reader packaging smoke from plan 13.

## Implementation steps

1. Commit S1 with test updates. Record its revision for `ara-eval` screen V1.
2. Commit S2. Record revision for V2.
3. Commit S3. Record revision for V3.
4. After the V1–V3 screens, decide B1. If it is approved, implement it with the version bump and record the revision for V4.
5. Rewrite this plan as `docs/agent-cli-interface/21-reader-round-trips.md` with the measured outcome and update the plan index.

## Risks

- **Overfitting the development set.** Screens use the same four artifacts as the 0.1.26 pilot. The `ara-eval` plan keeps the 375-question test split untouched by this tuning as the out-of-sample check.
- **Grounding quality.** S2 may lower accuracy where full reads caught details. The screens report J1 accuracy for every step, and the per-question answers stay available for review.
- **Write skills.** S1 moves text that writers rely on. The packaging smoke and the guard-scope tests must pass for every writer skill before the split merges.
