# Agent CLI interface: PR rollout
**Date:** 2026-10-01

Status: draft for review. These are engineering plans, not approved implementation work. Parent: [agent CLI interface](../agent-cli-interface.md).

## TL;DR

Split the parent into 16 planned PRs and two evidence-gated follow-ups. The first three tracks are independent: existing-format reads, protocol decisions, and pinned skill inventories. Search can proceed alongside writes after the read commands ship; merge follows transactional batch support. CLI-only skills and collective coordination remain separate interventions, with experiment code outside this repository.

## Problem

The parent covers format changes, a new CLI interface, merge semantics, skill copies, and research experiments. A phase-sized PR would mix protocol review with unrelated parser, filesystem, and prompt changes. The table below gives each implementation PR a review boundary and acceptance plan while preserving every parent phase.

## Constraints

PR numbers below are local rollout identifiers, not GitHub numbers or release versions. Read each numbered plan before implementation. Dependencies mean merged prerequisites unless a row names a protocol approval; document drafting and fixture review can happen earlier. Protocol proposals, output choices, and conditional extensions remain proposals until reviewed.

No current code, version, lockfile, changelog, or installed skill changes are part of this planning task. No commits or remote PRs are authorized. The parent remains the scope and motivation document; completed PR plans become design records rather than leaving duplicate permanent planning text.

## Proposed approach

Use the table as the implementation queue. Each linked file contains its own background, file-level changes, implementation steps, failure behavior, verification, and review gates.

| PR | Target repo | Deliverable | Prerequisites |
|---|---|---|---|
| [00](00-protocol-contracts.md) | Protocol | One format proposal covering F1-F7, mutability, merge identity/conflict records, and #61/#62/#63 | None; can run with 01 and 12 |
| [01](01-read-model.md) | ara-cli | Optional staging/session/heuristic/experiment/taste model and complete claim fields | No protocol-format dependency |
| [02](02-read-commands.md) | ara-cli | `status`, `ls`, `show`, `path`, `refs`, `open`; discovery, JSON/errors, shared scanner, corpus/timing gates | 01 |
| [03](03-guarded-node-writes.md) | ara-cli | Lock and guarded source-text transaction engine; `add node`, `add edge` | 02; approved 00 F1-F4 |
| [04](04-logic-editing.md) | ara-cli | Claim/heuristic add/set and protocol-whitelisted generic edits | 03 |
| [05](05-staging-and-sessions.md) | ara-cli | `stage`, atomic `promote`, session creation/logging and required history | 04; approved promotion/session mutability |
| [06](06-batch-apply.md) | ara-cli | All-or-none JSONL `apply`, provisional IDs, dry run, required compiler/manager coverage | 05 and 12 coverage inventory |
| [07](07-same-as-links.md) | ara-cli | Explicit `link --same-as` and read/ref support | 03; approved 00 F5 |
| [08](08-directory-merge.md) | ara-cli | Complete directory three-way merge, renumbering, references, aliases/resolve, conflict report and repeat-merge semantics | 06; approved 00 merge contracts |
| [09](09-git-merge.md) | ara-cli | `merge --git` using a local git subprocess and temporary snapshots | 08 |
| [10](10-keyword-search.md) | ara-cli | In-memory BM25 `find`, relevance set and speed gates | 02 |
| [11](11-duplicate-warnings.md) | ara-cli | Advisory duplicate candidates for node additions and merge | 03, 08, 10 |
| [12](12-pin-skill-contracts.md) | Protocol | Pinned unchanged skill baselines, reference pages, task mapping, operation inventory | None; prepare before 06 |
| [13](13-cli-backed-skills.md) | Protocol | Reviewed CLI-only reader/writer/compiler copies with complete operation coverage | 02, 06, 10, 12; relevant 00 contracts |
| [14](14-shared-frontier-intentions.md) | Protocol | Separate collective extension with cross-fork intention publication and refresh | 08, 13; reviewed coordination contract |
| [15](15-experiment-harness.md) | External harness | Pinned Files/CLI/collective experiments, fidelity and quality/cost/time grading | 13 for Files/CLI; 14 for collective runs |
| [16](16-local-semantic-search.md) | ara-cli, conditional | Local embeddings behind an off-by-default feature | 10; 15 evidence of consequential keyword misses |
| [17](17-cli-write-enforcement.md) | Protocol/harness or ara-cli, conditional | Q4 enforcement choice, only if observed direct writes justify it | 13 and 15; reviewed mechanism |

Protocol means `ARA-Labs/Agent-Native-Research-Artifact`. External harness paths and source revisions must be verified in their owning repositories before implementation; this checkout does not establish their current layout. PR 07 does not block batches or merge. PR 09 and PR 11 do not block baseline skill integration. PR 14 changes coordination rules only in its separate condition.

### Which tracks can proceed together

1. Start 00, 01, and 12 after their plans are reviewed. Format decisions do not delay existing-format readers.
2. After 01, merge 02. Then run 10 alongside the 03-to-06 write chain once 00's required decisions are accepted. Complete 12 before finalizing 06's operation set.
3. Merge 07 when F5 is accepted, independently of the write chain. After 06, implement the full 08 merge; do not expose a command that silently skips unsupported layers.
4. After 08, run 09 and, if 10 is merged, 11. After 06 and 10, integrate 13 against 12's pinned coverage inventory.
5. Run Files-versus-CLI experiments once 13 meets fidelity gates. Review and implement 14 separately before collective runs. Schedule 16 or 17 only if their evidence gates fire.

### Which parent requirements belong to which PR

| Parent requirement | Plan owner |
|---|---|
| Phase 0 F1-F7 and #61/#62/#63 format coordination | 00; existing claim parser half in 01 |
| All existing reference kinds, optional layers, native/wasm compatibility | 01; lookup/scanner in 02; aliases in 08 |
| Six reads, automatic discovery, projected/full JSON, exit codes | 02; all later commands reuse the contract |
| Source-preserving writes, ID allocation, same-checkout locking | 03; all later writes reuse its engine |
| Mutable logic and immutable trace/staging pointer rules | 00, 04, 05 |
| Long text through `key=@file` / `key=@-` | 03 input contract, 04 field edits, 06 batch values |
| Staging, promotion, session history, compiler/manager missing operations | 05, 06; completeness inventory in 12 |
| F5 equivalence pointers | 07 |
| Complete layer-aware merge, reference rewrites, aliases, `resolve`, repeat import, conflict review | 08 |
| Git convenience mode | 09 |
| Search and relevance evaluation | 10 |
| Nonblocking duplicate warnings on add and merge | 11 |
| Unchanged pinned baselines and CLI-only research procedures | 12, 13 |
| Frontier plus shared intentions as a separate collective condition | 14 |
| 450-question benchmark, scaling, manager replay, multi-fork community runs | 15 |
| Unit/golden/JSON, corpus, concurrency, batch rollback, node replay, merge properties, timing | Per owning feature PR; shared gates below |
| Conditional local embeddings and direct-write enforcement | 16 and 17; neither is baseline scope |

### Which review gates must stay visible

- F1's rename log alone does not identify unchanged imports, fork revisions, or reused source labels. PRs 00 and 08 must agree durable import identity and repeat-merge semantics.
- F3's `true`-wins rule does not resolve two different promotion destinations. F4's short whitelist does not yet settle session counters/summaries, revision history, stale flags, or F5 additions. Resolve these protocol choices before affected writers ship.
- Existing normalization drops unknown fields and some full prose. PR 03's intended-change guard must compare a complete source representation, not only `Manifest` equality.
- `fix.rs` currently writes with `std::fs::write`. It supplies guard and text-edit patterns, not an existing atomic rename, locking, or multi-file transaction implementation. PR 03 owns that new mechanism and its stated crash/reader-visibility limits.
- CLI-only compiler and research-manager integration requires every operation in the pinned source skills. PR 06 cannot silently omit reasoning logs, taste records, initialization, logic bodies, or full revision history required by PR 12's inventory. Additional public operation syntax needs review, not direct-file fallback.
- Short prose references remain uncertain. PR 02's scanner and PR 08's rewriting must distinguish exact tokens from possible short forms; our-side text and arbitrary code/evidence bodies remain untouched.
- A new field can be additive on the wire but still require constructor updates and a new compiled viewer bundle. Current scripts do not hash core sources. Pure version bumps alone do not force a viewer rebuild.

## Alternatives considered

Two Phase 1 PRs follow the parent's proposed read-model/read-command split. Separate write PRs keep source-text safety, mutable logic, staged state transitions, and batches independently reviewable. Directory merge ships as one complete semantic feature because a partial layer union can lose or misbind research history. Semantic search and enforcement stay conditional instead of entering the baseline queue.

## Tradeoffs

More PR boundaries require maintaining a shared output and operation contract. The plans name one owner for each contract and make later PRs reuse it. A larger directory-merge PR is justified by cross-layer references and atomicity; splitting its implementation internally is allowed, but releasing a narrowed merge command is not.

## Migration

For each functional ara-cli PR, bump the then-current workspace patch once and add a Keep a Changelog entry under Unreleased. Refresh `Cargo.lock` with a non-locked `cargo check --workspace` immediately after the bump, before final locked gates. The four local package versions currently needing refresh are ara-cli, ara-core, ara-viewer, and ara-wasm; review dependency changes against that PR's actual scope. This sequencing follows `memory://root` guidance and is supported by the current shared workspace version and crate manifests; never reserve fixed future versions in these plans.

Update `docs/agent-cli.md` incrementally as commands ship and `docs/manifest-schema.md` when wire fields change. After a PR is implemented and verified, fold its decisions and actual behavior into the design record and retire its plan. Keep the parent and index until all required rollout work has an owning design record; conditional slots can close without implementation when their gates do not fire. Protocol and external-harness PRs follow their own repository release rules, not ara-cli version bumps.

## Verification and acceptance

These checks are implementation requirements, not claims that they ran during this documentation task. Use the toolchain pinned by `rust-toolchain.toml`, currently 1.94.1. A functional PR runs its changed-path unit/integration tests and a real binary smoke scenario specified in its plan, then the final workspace gates:

```bash
cargo check --workspace
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p ara-core --no-default-features --target wasm32-unknown-unknown --locked
```

The first command also refreshes the lockfile after the bump; subsequent verification uses locked dependency resolution. Check exported-symbol references before API changes and update all affected constructors/callers. Existing fixture tests use `assert_cmd`, `tempfile`, and `insta`; keep new behavior tests deterministic and offline. New fixtures require source revision and license attribution. The 32-artifact paperbench sweep accepts diagnostic failures as defined outcomes, not as clean parses, and must run in its documented opt-in job rather than force network access in ordinary unit tests.

If core changes alter compiled viewer behavior, run `scripts/embed-viewer.sh` and include the rebuilt bundle. Run `scripts/embed-viewer.sh --check` for freshness, but do not treat its core-blind hash as proof of rebuild. Existing documentation references that imply all workspace version changes require regeneration are superseded by the current script's normalized-version hash behavior.

Enforce the parent's process-inclusive read budget using prebuilt release binaries: under 100 ms on the pinned real artifact and under 1 s on a 10,000-node generated artifact. Include 100/1,000-node cases, and merge measurements through 100,000 nodes. Document the performance runner and fixed measurement procedure; failures block acceptance. Unit tests alone do not replace changed-path smoke output or demonstrate agent quality gains.

For this planning change, acceptance is 18 numbered plans, a valid dependency graph, complete phase/command/test ownership, working local links, and clean prose style. No Rust test or build is necessary because shipped behavior is unchanged.

## Next Steps

Review PR 00's protocol decisions, PR 01's optional wire names, PR 02's output contract, and PR 12's baseline inventory first. Approve each affected PR plan before implementation; this decomposition does not authorize feature work.
