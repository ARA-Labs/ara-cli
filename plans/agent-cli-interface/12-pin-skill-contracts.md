# PR 12: pin baseline skills and inventory every artifact operation
**Date:** 2026-10-01

Status: **approved** 2026-10-01. Target repository: `ARA-Labs/Agent-Native-Research-Artifact`. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies: none. This inventory can start before CLI commands ship and supplies [06-batch-apply.md](06-batch-apply.md), [13-cli-backed-skills.md](13-cli-backed-skills.md), and [15-experiment-harness.md](15-experiment-harness.md).

## TL;DR

Pin the exact skills, reference pages and task mappings used by the corresponding paper experiment. Preserve their complete bytes as the Files baseline and make a machine-readable inventory of every required artifact operation. Pin research-manager, the live project manager (PM), and compiler contracts separately where the paper does not provide them. Missing CLI coverage blocks the affected integration; it must not be filled from the locally installed skill. Pin and publish the reader rows first, because PR 13a and the first read experiments depend only on them. If the paper's historical reader revision cannot be verified, pin the current reader as a live pin and let PR 15 run E1 as "not a reproduction" instead of blocking it.

## Problem

An interface comparison is invalid if Files and CLI use different reasoning instructions. The protocol checkout currently has compiler, research-manager and research-foresight sources, but a current checkout or installed skill does not establish which files generated the paper's results. The accessible `../ara-paperbench/README.md` describes a collection of 32 artifacts; it does not identify the 450-question harness, task-to-skill mapping or historical skill revision.

The operation list must cover whole documents and full history, not only nodes and claims. The inspected compiler requires arbitrary `logic/solution/` prose, problem sections, concepts, related-work entries, experiment plans, root manifest content, source-grounded evidence and coverage repair. Research-manager requires initialization, complete sessions, logic revisions, reasoning records, taste comments and staleness handling.

## Constraints

This PR changes baseline packaging and evaluation contracts in the protocol repository. It implements no CLI commands, coordination intervention, enforcement rule or experiment results. Preserve every required procedure, role, closure rule, provenance tag, stopping criterion and evidence requirement. Follow the [series shared gates](README.md); no ara-cli version bump is needed for this documentation-only plan.

Current accessible source paths are `../Agent-Native-Research-Artifact/skills/compiler/`, `skills/research-manager/`, and `skills/research-foresight/`, including their referenced pages. These confirm source locations, not a historical benchmark pin. At implementation, verify full remote commit identities and the files at those commits through the protocol remote, and verify the paper experiment's release/configuration source separately. The parent cites `e52a925` as a checked protocol revision, not as the paper's skill revision; do not reuse that short hash as the benchmark pin without evidence.

## Proposed approach

Create `evaluation/agent-cli/baselines/` (new) in the protocol repository, with unchanged skill trees grouped by an immutable pin identifier. Create `evaluation/agent-cli/source-lock.json`, `task-skill-map.json`, `operation-coverage.json`, and `baseline-contracts.md` (all new). The lock holds remote URL, full commit, source path, content digest, included reference-page closure, license, experiment provenance and whether the pin is paper-corresponding or newly selected for a live task. The task map holds each experiment task's native identifier, associated pin, entrypoint, loaded references and grading source. It must include all paper question IDs once their source is verified, rather than mapping every task to the currently available reader by assumption.

The proposed operation artifact has `format: ara.skill-operations/v1`. It is consumed directly by PR 06 and PR 13 and is versioned with the source lock. Each row includes `operation_id`, `skill_pin`, `source_clause`, `layer`, `native_selector`, `access`, `required`, `payload_contract`, `history_contract`, `provenance_contract`, `role`, `proposed_cli_operation`, `proposed_batch_tag`, `coverage_status`, `proof_reference`, and `blocked_reason`. Allowed coverage states are proposed, covered and blocked. A command name alone does not count as coverage; covered requires a pinned CLI revision and an end-to-end proof that preserves the source contract. Operations on source/evidence bodies are marked direct-access-allowed instead of being mistaken for missing CLI operations.

| Contract family | Operations to inventory | Fidelity that must survive |
|---|---|---|
| Reader | Full DAG/relation queries; native claim/concept/section reads; grounding checks; full prose; evidence/code follow-up. | Original refs and complete cited bodies, uncertainty and output-only contradiction reports; reader remains read-only. |
| PM routing and promotion | Node/edge creation, staging, closure-based promotion, contradiction handling, stale marking and reader-report adjudication. | Raw observations, provenance, bindings, chosen and rejected signals, terminal-state restrictions and prior trace content. |
| PM reconciliation | Claim/heuristic/concept revision, split, merge, generalization, dependency repair and taste attachments. | Current logic plus verbatim full before/after history, revision pointers and conservative transition rules. |
| PM continuity | Initialization; session metadata/index updates; every session list; reasoning and taste log appends; fresh-session briefing. | `ai_actions`, `claims_touched`, `logic_revisions`, `key_context`, `open_threads`, `ai_suggestions_pending`, and all counts/summary/turn identity. |
| Compiler | Initialize root/core; author and revise every mandatory logic file and arbitrary solution body; create DAG with source support; coverage repair and validation. | Complete content, equations, source quotes, concepts/RW native identities, full citation footprint, evidence and appendix coverage. |
| Allowed direct access | Input acquisition; code/config/environment bodies; tables, figures, screenshots, run logs and source evidence. | The compiler's complete evidence ledger and extraction discipline remain unchanged. |

`PAPER.md` is a root document outside the parent's three named knowledge directories, yet both compiler and PM require it. Inventory its initialization and later content update explicitly. PR 06 must obtain approval for the bounded root-document operation or remain blocked. The same rule applies to any additional knowledge document the pinned skill generates. Never turn these required operations into direct file writes inside CLI-only skills.

Implementation steps for the future PR:

1. Locate the paper experiment's authoritative question, prompt, grading and skill configuration. Verify remote commits and retrieve complete skill/reference trees from those revisions. If they are unavailable, mark the historical pin missing, pin the current sources as live pins, and record that E1 runs as "not a reproduction".
2. Select and review live PM/compiler pins where no corresponding historical paper experiment exists. Record why each pin is selected and keep it distinct from any paper pin.
3. Copy source skill trees unchanged into baselines and record hashes for every loaded page and template. Include transitive reference pages, assets and validation instructions actually supplied to agents.
4. Create task mappings from verified task configurations. Record unresolved mappings without inventing an entrypoint.
5. Enumerate source clauses into operation rows, including arbitrary bodies, unknown fields, complete sessions and initialization. Send every missing capability to PR 06 and each protocol ambiguity to [00-protocol-contracts.md](00-protocol-contracts.md).
6. Review contradictions in the pins, such as mutable stale flags versus immutable staging, session rolling fields, and promotion instructions that request `From staging`/`Crystallized via` in logic while later declaring those fields trace-only. Choose an explicit documented disposition before variant integration; do not silently repair one condition's procedure.
7. Publish the immutable pin set, task mapping and operation inventory for PR 13 and the external harness.

## Alternatives considered

Copying installed skills would be convenient but could change the experiment's source procedures without a recorded reason. Using only current protocol sources is appropriate for a newly declared live experiment, not for reproduction of a historical paper condition. A prose-only operation checklist would not give PR 06 and PR 13 the same consumable list of missing operations.

## Tradeoffs

Archiving complete reference trees duplicates files, but preserves the actual instructions supplied to agents. Separate paper and live pin sets require more configuration and avoid falsely claiming historical equivalence. The inventory exposes source-contract contradictions before a CLI copy silently drops a step.

## Migration

Leave `skills/` originals intact. Baseline copies are immutable and identified by digest; a new source revision creates a new pin set rather than overwriting the old one. PR 13 derives CLI variants from a named pin. Existing artifact collections remain unchanged, including all evidence and source files.

Correcting a source contract after review requires a new lock and a clear declaration of whether the paper reproduction still uses its historical contract. Both conditions in any new interface comparison use the same selected contract; a historical result cannot be treated as the quality measurement for the new CLI condition.

## Verification and acceptance

This drafting task runs no checks. In the future PR, compare archived bytes and digests with the verified remote source, resolve every task mapping, and review every required clause against the machine-readable inventory. Proposed new verification targets are baseline byte fidelity, reference closure, task-map completeness and inventory completeness; they belong under `evaluation/agent-cli/` and must be labeled new in the implementation plan. No semantic rewriting is allowed in the baseline archive.

This packaging PR ships no binary behavior and has no CLI smoke prerequisite. Its own smoke scenario is to load a mapped baseline task from the archive in the existing agent runtime and observe that it loads the pinned references, retains read-only or PM/compiler roles, and emits the same required record shape. At the later integration gate, use the actual pinned `ara` binary to retrieve one full concept/section and perform a representative PM revision; absence of a covered operation leaves the variant blocked. These are future verification scenarios, not reported executions.

Acceptance requires a verified source lock, unchanged baseline trees, an auditable task-to-skill map, and operation rows for every required clause. The paper experiment cannot be declared reproducible while its authoritative revision/question/grading source remains missing. A compiler or PM integration cannot be declared complete while a required operation remains blocked.

## Next Steps

1. Verify the paper experiment's authoritative historical sources and obtain review of the live PM/compiler pins.
2. Approve the proposed archive and inventory paths and publish the missing-capability list to PR 06.
3. Resolve source-contract ambiguities in PR 00, then derive CLI-only copies in PR 13.
