# 21: fewer model round trips for the CLI reader

Shipped in workspace 0.1.28 as a skill-only change; the `ara` binary is unchanged from 0.1.27. The measurement design and run records live in `ara-eval` (`plans/cli-reading-round-trips.md`, study family `cli-reading-round-trips`).

## Problem

The local 0.1.26 reading pilot found that the CLI reader (`research-foresight-cli`) cost 1.65 times as much as the Files reader and took 1.34 times as long, at similar gold-relative accuracy. The binary took 4.26 seconds across 661 calls. The cost came from model requests: 48.6% more of them, each carrying 37.6% more input.

| Measure, 60 sessions per arm | Files | CLI 0.1.26 |
|---|---:|---:|
| Model requests | 570 | 847 |
| Mean input tokens per request | 20,606 | 28,362 |
| Requests classified as discovery-only | 16 | 194 |
| Requests with more than one tool action | 417 (73%) | 473 (56%) |
| Mean instruction text returned per session, characters | 41,470 | 78,562 |

The traces showed three causes:

1. The reader loaded the whole writer guide on every session.
2. The CLI grounding rule was stricter than the Files rule.
3. Searches and reads were issued one per model request.

## Design

### Reader-only access guide

`references/cli-access.md` now holds only what every CLI skill needs to read: the boundary and failure rules, shell calls, orient/search/read/cite, and the read-only task subsections (ancestry, citations, identities, unfinished work). The writer guidance moved to `references/cli-write.md`:

- structured output and exact source before writes
- JSONL transactions
- write task subsections
- the PM complete turn
- compiler generation
- review and proof

| File | Characters | Shipped with |
|---|---:|---|
| `references/cli-access.md` | 10,425 (was 44,340) | `research-foresight-cli`, `research-manager-cli`, `compiler-cli`, byte-identical |
| `references/cli-write.md` | 35,179 | `research-manager-cli`, `compiler-cli`, byte-identical |

No rule was deleted. Writer skills link `cli-write.md` wherever they used to link a write subsection. The full-read-before-write rule stays in `cli-write.md`, where guarded writes need source digests.

### Files grounding rule

The reader's `RETRIEVE.md` no longer requires `show` on every cited body. It follows the Files rule, using CLI mechanics: when a `find` hit is too terse to judge, `show` its section or window its source document around the hit, so the answer rests on the real prose and not a keyword coincidence. Both reading conditions now follow the same method and differ only in access.

### Fewer requests

`cli-access.md` "Reading" teaches fewer requests per information need:

- Put every keyword for one information need into a single `find` query, because `find` scores all terms together (BM25).
- `show` several addresses in one call.
- Issue independent `ara` commands as separate shell tool calls in the same response. "One command per shell call" limits the shell line, not the response.
- Skip `ls` when the question already names what to search for.

### Not shipped

The plan's conditional `find <query> --show N` (search hits with full bodies) was not implemented. The skill changes were adopted as the release, so the binary surface is unchanged.

## Measured outcome

The `ara-eval` screens ran the CLI arm only:

- 60 development questions, one repetition.
- `glm-5.3-flash`, with the frozen pilot seed, prompt, guard and price table.
- J1 graded the answers.

They were compared with the archived Files arm of the 0.1.26 pilot. The intermediate rows measure each change cumulatively.

| Skills | Agent cost ratio, CLI/Files | Requests per question | Input tokens per request | J1 correct | Discovery-only requests |
|---|---:|---:|---:|---:|---:|
| Files reference | 1.000 | 9.50 | 20,606 | 54/60 | 16 |
| 0.1.27 unchanged | 1.732 | 13.73 | 31,025 | 56/60 | 182 |
| + reader-only guide | 1.403 | 13.70 | 23,456 | 57/60 | 195 |
| + Files grounding rule | 1.357 | 13.27 | 22,950 | 55/60 | 197 |
| **0.1.28 (+ fewer requests)** | **1.233** | **10.67** | **23,205** | **57/60** | **117** |

Against unchanged 0.1.27 skills, 0.1.28 has:

- 29% lower agent cost
- 22% fewer requests per question
- 25% less input per request
- 71% of requests with more than one tool action, up from 62%

Other 0.1.28 measures:

- Mean bad-call weight per session was 0.46.
- No session had five consecutive errors.
- No session made a native `rubric/` call.

The reader still costs more than Files. The comparison is against an archived Files arm from an earlier day. A fresh paired Files/CLI pilot in `ara-eval` measures the remaining gap under the original decision rule.

## Tests

- `crates/ara-cli/tests/skills.rs` checks the following:
  - Every `cli-access.md` copy is byte-identical, and so is every `cli-write.md` copy.
  - The reader ships no `cli-write.md`.
  - The write-recipe and guard-scope tests read `cli-write.md`.
  - Every documented command line parses against the binary.
- `cargo test --workspace` passes.
