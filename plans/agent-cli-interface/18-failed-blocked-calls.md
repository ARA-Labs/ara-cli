# PR 18: stop failed and blocked calls in CLI agent sessions
**Date:** 2026-10-04

Status: **approved design, 2026-10-04**, at the user's direction. Implementation, external-repository changes and measurement remain pending. Repository: `ARA-Labs/ara-cli` (binary and [skills](../../docs/agent-cli-skills.md)). Dependencies outside this repository: the `ara-eval` harness (H1, H2) and the `ara-paperbench` corpus (C1). Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md). This approval does not authorize implementation in this documentation task, a paid run, a release or upstream protocol adoption.

## TL;DR

The supplied preliminary `runs/e1-test` analysis attributes 3.25 model calls per CLI session to failed or blocked tools, against a 5.32-call difference from Files. This plan removes native rubric handling, gives agents bounded text reads and usable addresses, and permits structural reads when parsing is complete but validation fails. It keeps exact source reads and guarded-write digests separate from displayed excerpts, and continues to reject unsafe or incomplete reads. The proposed 120-point dev pilot is a screening exercise, not evidence that accuracy is non-inferior or that the call-reduction target has been met.

## Problem

### What does the supplied snapshot show?

The user-supplied snapshot is dated 2026-10-04 16:53 PDT: 580 CLI and 581 Files sessions with complete traces, about 52% of the run, with 6 CLI timeouts excluded. Its pins are agent `glm-5.3-flash`, `ara 0.1.23` (`4f70972`), skill `research-foresight-cli` at protocol `03f19c7`, and Pi 0.82.1. The [analysis document](https://claude.ai/code/artifact/70b438b7-c4a1-4a12-9899-d107136b03fe) and `runs/e1-test` are historical inputs supplied for this revision. These results were not rerun or independently verified here and are not the registered analysis.

| Per session (mean) | Files | CLI |
|---|---|---|
| Model calls | 8.65 | 13.97 |
| Calls spent on failed or blocked tool calls | 0 | 3.25 |
| Wall time | 166 s | 289 s |
| Cost (list price) | $0.0121 | $0.0280 |

The supplied attribution gives each tool in a model call with k tool calls a weight of 1/k. It reports 158 sequences of five or more bad tool calls. The earlier estimate of about 20 seconds and 39k repeated prompt tokens per failed model call describes this snapshot; it does not establish the savings from a fix.

### Which failures motivate the changes?

| # | Supplied cause | Model calls / session | Share |
|---|---|---|---|
| 1 | The agent does not know the harness permits only one `ara` command per shell call | 1.10 | 34% |
| 2 | The CLI serves the PaperBench rubric; `--heading` misses on it | 0.77 | 24% |
| 3 | Claim parsing leaves 8 artifacts invalid | 0.53 | 16% |
| 4 | Truncated output leads to attempts to read Pi's log | 0.38 | 12% |
| 5 | `show --document` targets `evidence/` or `src/` | 0.24 | 7% |
| | Other scattered errors | 0.23 | 7% |

The supplied harness analysis says `ara-eval/src/ara_eval/pi/guard.ts` permits one `ara` command per shell call and rejects unquoted `| > & ; * ? [ ] ( ) # ! ~ $`. Examples include piping JSON to `head` or using `cd` followed by `&&`. It attributes 43% of piped commands to `find`, whose median output was 2.7k characters. This restriction belongs to that harness, not to the `ara` executable or shells in general. The supplied registration pilot also reported 3.2 denied calls per session in `ara-eval/plans/registration-reading-wave.md`.

`rubric/requirements.md` is PaperBench grading material. The supplied compiler conversion flattens it into `R01` through `Rnn` headings shortened to 60 characters plus literal `...`. Category B questions direct agents to read it. The current native allowlist serves its document but does not make requirements searchable native entries. The supplied traces report successful whole-document reads, `unknown_id` for `show R84`, and `merge.unknown_identity` for `--heading R84`, although the harness allows direct `grep` and `read`.

| Rubric access per Category B CLI session (139 sessions) | Count |
|---|---|
| `ara show` that fails | 5.84 |
| `ara show` that succeeds | 3.22 |
| `ara show` blocked by the harness | 1.15 |
| Allowed `grep` or `read` tool | 0.07 |

The supplied analysis attributes 95% of heading misses to the rubric. It reports 9 invalid corpus artifacts: 7 have an unclosed leading `---` followed by `# Claims`; `nanogpt-speedrun` uses a U+2014 separator in claim headings; and `rebench-restricted_mlm` uses a different trace format that this plan does not change. The first 8 account for 23% of CLI sessions and 1.95 failed calls per affected session, with 6 to 54 unknown-claim errors per artifact. The reported removal of the stray first line in `pinn` restored `find`; that observation is accepted without rerunning it.

The same snapshot reports approximately 50 KB single-line `ls --json` output, truncation in 54% of those calls, and blocked attempts to read `/tmp/pi-bash-*.log`. It attributes wrong-document calls to `evidence/` in 77% of cases and `src/` in 22%. These observations motivate bounded text and better errors, not smaller or silently incomplete JSON.

## Constraints

The CLI must preserve the knowledge boundary, literal source bytes and write preconditions. It must not read through unsafe paths, hide parse loss, reinterpret duplicate identities as unique, or weaken recovery checks. Read tolerance applies only to fully represented artifacts with semantic validation errors. No read creates a lock, journal, background task or collaboration store.

This work changes access and deterministic presentation. It does not change research judgment, question generation, the excluded trace schema, scoring or the original Files skills. Rubric handling becomes direct file access and an external read-only merge source. Removal must not silently install incoming rubric files or rewrite old mutation records and aliases.

JSON compatibility is field-level, not byte-level. Existing successful `ara.*/v1` fields retain their meanings where applicable. Removed rubric operations, heading error codes, additive diagnostics and new opt-in address or bounds fields are observable changes and require migration notes. Default text is deliberately different. The approved design does not claim lower runtime cost or improved accuracy before measurement; normal commands must not pay for unused context, pagination or source hashing.

## Proposed approach

### Which binary contracts change?

| # | Change | Cause | Contract impact |
|---|---|---|---|
| B1 | Remove `rubric/requirements.md` from native reads and parsing, including registered-path bypasses. `show --document rubric/...` returns `invalid_document` with the B9 hint | 2 | Breaks native rubric readers |
| B2 | Remove `Requirement`, native `R` IDs, rubric write allowlists and native merge entry handling. Treat all `rubric/` paths as `external_read_only`, like `evidence/` and `src/` | 2 | Removes a public Rust variant and native write operations; no incoming rubric installation |
| B3 | Resolve headings exact-first, then use the bounded tolerant lookup below | 2, general | Additional unambiguous inputs match |
| B4 | Return `unknown_id` on a miss and `ambiguous_heading` on multiple matches, with at most 40 canonical candidate addresses | 2, general | Corrects error codes and adds candidate metadata |
| B5 | Recognize only the conservative stray-fence case below; retain protection for malformed metadata and report the opening line | 3 | Narrow parse recovery and a diagnostic |
| B6 | Allow structural reads of fully represented artifacts with validation errors; retain original diagnostic severities | 3 | Some semantic refusals become diagnostic-bearing read success |
| B7 | Default to brief text for agent reads; keep explicit `--json` for programs and exact `--source` for source access | 2, 4 | Changes default text, adds addresses and explicit display metadata |
| B8 | Add inclusive `--lines A:B`, a text-output byte budget and resumable pagination, including oversized lines | 4 | Additive read options; invalid combinations reject before output |
| B9 | Add an actionable `invalid_document` hint naming allowed native roots and direct file access for `rubric/`, `evidence/` and `src/` | 2, 5 | Additive error field |
| B10 | Parse claim headings separated by colon, spaced ASCII hyphen, U+2013 or U+2014 | 3 | Additional native claim spelling; source stays unchanged |

B2 must change both native inventories and the external-file conflict classifier. Identical or unchanged rubric bytes produce no new conflict. Incoming additions, modifications and deletions that differ from the destination produce the existing external read-only conflict, preserve destination bytes and permit only the existing external acknowledgment policy. Acknowledgment never copies incoming content. Existing rubric-related historical records remain exact and readable as history; the migration must not invent replacement native identities.

B4 translates only heading-selection failures into read-facing codes. It must not turn a corrupt alias index, unsafe path or failed recovery check into an ordinary missing heading. Those failures retain a truthful read/setup diagnostic without leaking a `merge.*` implementation code. Candidate generation uses loaded knowledge only, ranks deterministically, and reports whether the candidate list was capped.

### How do headings and addresses resolve?

Repeated `--heading` arguments remain an exact vector. `['A/B']` names a literal slash in one heading and differs from `['A', 'B']`. Resolution first tries the exact full vector, then the existing unique exact suffix and native-ID shorthand. Each tier must reject multiple matches before considering a weaker tier. Only when exact matching finds none may the reader trim surrounding whitespace and compare case-insensitively, first for equality and then for a unique prefix. The normalization must be locale-independent and documented; use Unicode lowercase without accent stripping or Unicode normalization. A real trailing literal `...` may match a longer request with the same nonempty prefix, but only in the final tolerant tier. An exact heading containing `...` wins first.

Canonical addresses encode the original heading identity. Keep bare native IDs (`C04`), `trace:N09` and `logic/claims.md#C04` as convenient unambiguous forms. General headings use `path#h/<segment>/<segment>` with the full original heading vector. Encode each segment's UTF-8 bytes using uppercase percent escapes for everything except URI unreserved characters. A literal slash becomes `%2F`, so `path#h/A%2FB` and `path#h/A/B` stay distinct. Encode path components the same way while keeping path separators; decode once and then apply the normal boundary checks. Malformed escapes, invalid UTF-8, absolute paths and traversal reject.

For repeated identical full vectors, append `;occurrence=N`, a one-based source-order occurrence among that vector's matches. Literal semicolons in headings are percent-encoded. Generated addresses always include the occurrence when needed; an unqualified duplicate rejects. Canonical addresses resolve exactly, without tolerant fallback, and JSON retains the original heading array alongside any additive address. They round-trip within the same source snapshot; an ordinal is not a durable identity after edits. `--document` and repeated `--heading` remain supported structured selectors rather than lossy aliases. Legacy `path#Method/Step 3` input may resolve only if its literal-vector and split-vector interpretations identify one section; otherwise it rejects with canonical candidates.

### What does brief text contain?

The supplied Files navigation analysis covers a separate 781-session cohort. Its inclusion list and relationship to the 581-session snapshot above must be frozen before comparison; the two denominators must not be pooled. It reports the following behavior, which motivates the read format without proving that the format reduces calls.

| Step | Supplied Files behavior | Per session | Reported CLI limitation |
|---|---|---|---|
| Orient | List the artifact and `logic/` names | 2.9 calls, 169 characters total | Entry-heavy `ls`; verbose `status` |
| Search | Case-insensitive in 66%; context in 54% | 3.6 calls | Excerpts lack source match locations |
| Read | Whole documents or ranges found by search; 35% use a range | 6.1 calls | Exact headings or whole-document reads |
| Cite | `logic/claims.md#C04` form in 99% of answers | About 10 citations | Cited addresses are not accepted uniformly |

| Command | Default brief |
|---|---|
| `ls` | One line per knowledge document, with kind and counts, plus a direct-file boundary note |
| `ls <path>` | One canonical entry or heading address and title per item; existing filters retain their meaning |
| `find <query>` | Ranked addresses with one-based source line numbers and actual matching source lines |
| `find ... --context N` | Context around matched lines, merged where ranges overlap; `N=0` means no extra lines |
| `show <address>...` | A labeled block for each selection, native source when selected, and explicit projection or truncation metadata |
| `status` | Counts when complete, original error/warning counts and codes, and next IDs only when safe to compute |
| `path`, `refs`, `open` | Address-led items retaining relation type and other command-specific meaning |

`--context` has no `-C` short form because global `-C` selects the artifact root. Search keeps BM25 ordering, existing tokenization and filter semantics; it adds source-mapped lexical hit locations instead of substituting an unrelated grep engine. A result without a literal source hit must label its existing excerpt as an excerpt and must not invent a matching line. Matching is case-insensitive under the documented search rules. Candidate and display ordering must be deterministic.

Text diagnostics appear once per command on stderr, with separate error and warning counts, codes and a suggestion to run `ara check`. Deduplicate the display, not the underlying report. JSON continues to carry structured diagnostics on the command's existing channel; it does not acquire text noise. Tests assert addresses, source spans, digest correspondence, bounds and diagnostic semantics, not prose wording or complete layout snapshots.

### Which bytes does a digest identify?

A displayed projection, a selected native source body and a displayed page are different objects. For document/heading reads, retain the existing `digest` meaning: SHA-256 of the full exact selected source, before excerpts or pagination. For headings that is the native body range used by the corresponding replacement operation, not the title, ancestors or rendered body. Whole-document reads hash the entire original document. Entry projections must not label regenerated text with a source digest.

Brief headers label a guarded-write digest as `source_digest` and include its exact native selector and selection scope. If an entry projection has no single matching write source selection, omit that digest and direct the caller to the exact document/heading `--source` read. JSON's existing document `digest` field is retained; additive display metadata distinguishes the selector, displayed range, `truncated` and continuation. No digest of a truncated page is presented as the digest for replacement. Computing an optional page hash would need a separate name and is not required.

`--source` preserves original UTF-8 bytes, unknown fields, line endings and selection rules. It does not normalize dash headings or strip metadata. Unbounded `--source --full --json` remains the exact-byte path in the existing envelope. Explicit paging of source content returns exact slices plus metadata; callers must fetch the full selection and use its matching digest before replacing it. A digest alone does not authorize overwriting unseen content. Duplicate-heading occurrence addresses are read locators only unless the existing write selector can express that exact selection; no new write bypass is introduced.

### How are reads bounded and resumed?

Brief `show` defaults to a 16 KiB stdout budget; `--max-bytes N` changes that budget. The budget includes headers, separators and continuation metadata, not only body text. It does not apply implicitly to existing `--json` or to explicit unbounded `--source --full` reads. JSON is never cut at a byte boundary: an explicit bound must return a complete valid envelope with display metadata or reject if the envelope cannot fit. `--fields` and write inputs retain their current semantics. Other brief commands are compact but not guaranteed byte-bounded: use existing `find --limit`, filters and document-scoped `ls`. Measure large document maps and search output before claiming that the plan eliminates every truncation case.

`--lines A:B` uses one-based inclusive lines within each full selected native source, with open endpoints allowed (`A:` or `:B`) and at least one endpoint required. It applies independently to each selected address; the byte budget applies to the whole response. A final newline belongs to the preceding line, not an extra empty line. Reject zero, negative, reversed, nonnumeric or overflowing bounds and an explicit start beyond EOF. An end beyond EOF clamps to EOF and reports the actual range. An empty selection without an explicit out-of-range start succeeds as empty. Line bounds on a projection without a native source range reject and suggest the source selector.

For a single native selection, stop at a complete line and include the exact next `--lines A:B` window, retaining the original upper bound. Never split a UTF-8 code point or line. If the next whole line cannot fit even in an otherwise empty page, return `output_limit_too_small` with the minimum budget needed for that line and its metadata, and advise a larger `--max-bytes`. The error must not pretend that an empty page is progress or silently skip the oversized line. No byte-offset or cursor API is introduced. Every native-source page includes the full selection's source digest; the caller must restart if that digest changes between pages. An oversized entry projection without a native line mapping returns the same limit error and suggests an exact source read instead of inventing a continuation.

For multi-address reads, preserve request order, resolve every address before output, and apply line bounds independently to each source selection. The byte budget covers the aggregate response. Return all requested windows if they fit; otherwise return an actionable limit error before writing stdout, naming the required budget and advising separate single-address reads for pagination. Do not return only the first address or silently truncate later selections. Overlapping explicitly requested selections remain separate items. This makes multi-address behavior predictable without adding cross-item cursor state.

Reject `--max-bytes 0`, negative, nonnumeric and overflowing values. A budget too small for required metadata and the next complete line returns the actionable limit error before writing stdout. Long addresses and metadata count toward the required minimum, so tiny budgets cannot create a zero-progress loop. Bound text diagnostic summaries separately and report omitted counts without modifying structured severities. Tests cover one-line megabyte inputs, multibyte code points, CRLF, no trailing newline, empty input, changed source digests, multiple addresses and exact no-gap/no-overlap reassembly on unchanged source. No performance or maximum-output claim is accepted without these behavior tests.

### When can parsing recover or reads continue?

B5 must not infer “not YAML” from the absence of `key: value`. Keep generic `frontmatter_range` conservative: closed front matter is unchanged, and an unclosed leading fence hides the remainder unless a native-document-specific recognizer proves the supported stray-fence case. Initially that exception is only `logic/claims.md`: after optional blank lines/BOM, the opener must be followed by the exact top-level `# Claims`, then blanks and a recognized level-two claim heading, then a canonical claim field/body sufficient for the existing claim parser. The sequence cannot contain unrelated metadata, a second title, YAML directives or other nonblank material before that first claim. The full candidate claim document must parse without dropped entries. A heading-only or comment-only block is insufficient evidence.

This deliberately leaves uncertain malformed front matter hidden. Test unclosed mappings with empty values or malformed colons, quoted and explicit keys, sequences, flow collections, anchors, aliases, tags, directives, block scalars, indentation, comments followed by metadata and comment headings without claim bodies. None may expose metadata headings. Add the reported stray-opener fixture, BOM/CRLF variants and closed-frontmatter regressions. If a reported corpus document does not meet the recognizer, C1 repairs it explicitly; do not widen the heuristic merely to pass the corpus. Emit one diagnostic naming the opener's file and line for each unclosed leading fence, distinguishing recovered source from protected metadata. The recovered stray-fence case is a warning; any associated parse/validation errors keep their original severity.

B10 accepts only a well-formed native claim ID followed by `:` or a dash separator with surrounding whitespace. This prevents a hyphen within an ID or prose from creating a claim. Colons remain valid; all four separators preserve exact source bytes, and duplicate claims remain errors.

B6 separates parse completeness from semantic validity. `find`, `ls`, `show`, `open`, `refs` and `path` may return data for a fully parsed dangling reference, with the original validation report. They still refuse I/O failure, malformed YAML or Markdown that loses entries, missing required structure, duplicate ambiguous IDs, unsafe root or symlink traversal, and unresolved recovery state. Relationship reads report unresolved references rather than inventing targets. Explicit source-document reads keep their existing partially initialized-root access and `artifact_validation: not_run` meaning; this is not evidence that the artifact passed validation.

`status` already reports invalid artifacts. Preserve its `complete` semantics and null counts/next IDs when that existing completeness condition fails; do not turn invalid into healthy by moving errors into `warnings`. Any additive `parse_complete` field must remain separate from validity. `check` and `validate` continue to fail on errors and show original severities; a warning-only stray fence follows their existing warning exit policy. Writes remain strict, including guarded replacements and dry runs. No read-tolerance path is shared with write admission.

### What changes in skills and other repositories?

| # | Change | Ownership |
|---|---|---|
| S1 | Teach one quoted `ara` invocation per shell tool call, no pipes, redirects or `&&`, and native bounds as a safe recipe across harnesses. Explain that harnesses may impose this rule and other shells may permit composition | Local CLI skill copies |
| S2 | Route `rubric/`, `evidence/` and `src/` to file tools and name the native document boundary | `research-foresight-cli` |
| S3 | Write the rubric as a plain file using the same verbatim compiler conversion; remove the fixed native rubric exception from schema/access guidance | `compiler-cli`, all three `cli-access.md` copies |
| S4 | Orient, search, read and cite using returned canonical addresses; choose candidates after a miss | All CLI skills |
| S5 | Use brief text for routine reads; retain JSON where a structured contract is needed and exact source reads before guarded replacement | All CLI skills |
| H1 | Give denial messages the harness rule and permitted alternatives, only for new runs | `ara-eval` |
| H2 | Permit direct compiler writes to the rubric within the compile-condition guard's explicit boundary | `ara-eval` |
| C1 | Remove the seven reported stray openers in a reviewed new corpus revision | `ara-paperbench` |

Local CLI skills own these access changes; original upstream baseline skills remain untouched. Do not remove `--json` mechanically from write or structured-consumer examples. Keep the three shared access copies consistent and preserve research procedure, required source content and mutation ownership. H2 authorizes the rubric path only, not arbitrary knowledge-layer bypasses. C1 and new skill pins affect future conditions only; the vendored `e1-test` inputs stay frozen.

## Alternatives considered

Indexing rubric requirements as native `R` entries would expand a benchmark-specific boundary and still duplicate direct file access, so this plan removes native handling. Shrinking existing `ls --json` fields would break consumers that need those fields; the default text path supplies the bounded agent view instead. A third `--brief` format would leave the default agent path unchanged and is unnecessary.

Treating every unclosed fence as a horizontal rule would expose metadata as headings. Keeping every such file unreadable would preserve the reported failure, so B5 uses a narrow positive native-claims recognizer and explicit corpus repair. Broad read leniency would conceal parse loss; B6 permits only fully represented invalid artifacts. Prefix-only heading lookup and slash-flattened addresses are rejected because they can select the wrong source.

## Tradeoffs

Canonical percent-escaped addresses are less readable than short IDs, but they distinguish literal slashes and duplicate headings. Simple IDs remain available when unambiguous. Line-boundary pagination avoids a new cursor protocol; an oversized single line requires a larger explicit budget, and an oversized multi-address response requires separate reads. Compact listings can still exceed a harness limit on unusually large artifacts, so their size remains a measurement requirement.

The conservative frontmatter recognizer may leave other broken files unreadable. That is an accepted safety limit, with diagnostics and explicit corpus repair as the remedy. Semantic read tolerance lets agents inspect a dangling reference without claiming that the artifact is valid. Rubric removal breaks native consumers and a public Rust variant, so release notes and consumer migration are prerequisites to publishing it.

## Migration

Functional sub-PRs target `feat/agent-cli-interface` and squash-merge there. Each bumps the workspace patch version, updates all local workspace versions in `Cargo.lock`, adds the repository-required changelog entry, and updates affected docs, CLI skills and behavior tests. Refresh the lockfile with the documented non-locked workspace command before final locked checks. Intermediate patch versions are bookkeeping; the final integration PR to `main` uses a merge commit and the [parent release policy](../agent-cli-interface.md#order-of-work). The pending minor/major release decision must explicitly account for removed Rust/API behavior and changed default text.

Before landing B1/B2, inventory rubric consumers in CLI/core/viewer/WASM, public exports, fixtures, local skill copies, harness guards and compiler paths. Remove active native callers in the same cutover, but preserve historical serialized facts byte-for-byte. Add regressions that old history can still be inspected and that attempting a removed native rubric operation returns a clear error. If historical resolution requires a removed type, keep a private history representation rather than a public write alias or silent data rewrite. External changes land in their owning repositories and record adopted commits in the CLI stage PR.

Keep successful JSON v1 field semantics unless an explicit change is listed here. Document additive canonical addresses, candidate metadata and display/pagination metadata, B4's corrected errors, B6's changed read admission, and B1/B2's removals. Existing unbounded JSON consumers should not receive the new default text limit. Update `docs/agent-cli.md` and typed consumers together; do not require byte-identical serialization or hide an incompatible schema change behind the same format marker. A required field/type change discovered during implementation requires explicit contract review before shipping.

Protocol PR #38 remains draft. Future experimental conditions use the protocol's `feat/agent-cli-interface` branch through an exact external-harness submodule commit, not a moving branch name. Preserve the original skill archives, existing run manifests, historical corpus pins and scored-registration requirements. Plan approval is not upstream protocol approval, and protocol merge is not a prerequisite for a separately approved experimental run.

The inspected implementation anchors are [`agent.rs`](../../crates/ara-cli/src/agent.rs) (`Artifact::load`, `status`, `knowledge_document`, `show_document`, `source_output`), [`markdown.rs`](../../crates/ara-core/src/markdown.rs) (`frontmatter_range` and its metadata-protection tests), [`merge/mod.rs`](../../crates/ara-core/src/merge/mod.rs) (external-file conflict classification), and [`agent_reads.rs`](../../crates/ara-cli/tests/agent_reads.rs) (actual-command source and relation contracts). The [command reference](../../docs/agent-cli.md#selecting-an-artifact-and-reading-it) documents global `-C`, exact heading vectors and source digest scope. These repository facts ground the proposed changes; the historical trace measurements remain separately attributed inputs.

## Goals and acceptance

Engineering acceptance requires exact address round-trips, truthful diagnostics, external read-only rubric behavior, preserved source/guard semantics and bounded, resumable output. Add reproducing fixtures before each implementation change and behavioral regressions for the cases named above. Do not add tests that inspect plan prose, skill wording or source-text layout; executable command/argument fixtures and typed contract checks cover skills without freezing prose.

The research targets remain below 1.0 failed/blocked-call weight per CLI session, no native rubric calls in Category B, and no five-call bad sequence in more than 2% of sessions. Accuracy is compared with the registered 0.03 non-inferiority margin overall and for Category B against Files and the unchanged CLI condition. These are screening targets for the pilot, not implementation acceptance promises or proof of non-inferiority.

The proposed pilot remains 120 points: 60 dev questions × Files and the revised CLI condition × one repetition. The supplied unchanged-CLI comparison is `runs/pilot-glm53flash`, a 360-point historical pilot with the reported e1-test binary/skill pins. It is not a randomized contemporaneous third arm. The supplied standard deviation of 4.7 implies an approximate standard error of 0.61 for 60 independent CLI observations, before accounting for question/paper clustering or changed variance. That uncertainty cannot establish a mean below 1.0 merely because the point estimate crosses it.

Freeze the inclusion list, trace-script revision, attribution rules, timeout/missing-trace policy, paired question comparisons and uncertainty method before inspecting new outcomes. Report means, distributions and intervals, including uncertainty for the rare five-call-sequence rate. Report the historical CLI arm separately with its pins and exclusions; do not pool repetitions as independent comparable sessions. Historical model/service drift and simultaneous binary/skill changes prevent causal attribution to an individual fix. Accuracy differences and their uncertainty are descriptive; formal non-inferiority requires the separately registered analysis and its planned three repetitions.

The proposed dev split covers 4 papers, 40 Category A and 20 Category B questions and 2 artifacts with stray fences. It lacks Category C, RE-Bench and large artifacts, so engineering fixtures must cover truncation and malformed source beyond the pilot. Use contemporaneous Files only for time/cost ratios: the supplied historical timing comparison was 119 versus 162 seconds while calls were 8.9 versus 8.7, distinct from the 166-second snapshot table. The earlier throughput estimate of 82 sessions/hour and about 1.5 hours for the pilot is historical scheduling input, not a guarantee. Report root-cause and Category B access tables for all three labeled arms; screened misses trigger review rather than silent target changes.

## Next Steps

1. Freeze the supplied analysis in `ara-eval`: move `events.py`, `calls.py` and `roots.py` from scratch space into versioned analysis code, capture the snapshot inclusion lists and separate 781-session navigation cohort, and retain original run pins. Do not rerun a paid experiment under this approval.
2. Implement PR 18a for B1/B2/B9 and S2/S3. Reproduce native rubric acceptance, then prove removal, preserved history and external read-only merge behavior for identical, added, changed and deleted files. Coordinate H2 before testing a new compiler condition.
3. Implement PR 18b for B3/B4 and address resolution. Test exact precedence, normalized collisions, literal slash vectors, escaping, duplicate occurrence selectors, capped candidates and truthful non-merge read errors.
4. Implement PR 18c for B5/B6/B10 and remaining B9 errors. Test conservative metadata protection, the reported claim spellings, complete-but-invalid reads, unchanged status/check validity and every retained refusal boundary.
5. Implement PR 18d for B7/B8 and S1/S4/S5. Test actual command behavior, source digests and guard correspondence, UTF-8-safe pagination, aggregate bounds, context without `-C` collision and JSON field compatibility. Measure output size on pinned corpus artifacts and ordinary-read overhead against the prior binary; record results without assuming a speedup.
6. Land H1/H2/C1 in their owning repositories for new conditions only, after e1-test completes. Run the shared engineering checks once each sub-PR is complete, including workspace tests, pinned formatting/lint checks, viewer freshness when affected and relevant source/merge fixtures.
7. Obtain separate run authorization and freeze the pilot analysis before collection. Publish the screening results and limitations without declaring non-inferiority. Registered collection and any follow-on scope require their own approval.
8. After implementation and its evidence are complete, move the design record to `docs/agent-cli-interface/` and retire this plan. Until then, approval records the decisions above while implementation, external adoption, release compatibility and measurement remain open delivery requirements.
