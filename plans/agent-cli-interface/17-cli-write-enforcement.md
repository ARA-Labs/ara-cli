# PR 17: decide write enforcement from observed skill violations
**Date:** 2026-10-01

Status: conditional draft for review, not scheduled. Repository: protocol repo or `ARA-Labs/ara-cli`, depending on the reviewed choice. Depends on [PR 13](13-cli-backed-skills.md) and evidence from [PR 15](15-experiment-harness.md). Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

Keep CLI-only access as a skill instruction until experiments establish whether agents still edit knowledge files directly. If violations matter, choose either scoped tool restrictions in the experiment/protocol configuration or a separately designed `ara check` rule. Do not claim existing file content proves which tool wrote it. Keep this follow-up conditional on Q4 and separate from the baseline CLI interface rollout.

## Problem

Phase 2 explicitly adds no enforcement. The parent leaves Q4 open because direct-write frequency must be measured after skill integration. A syntax-valid artifact produced by Write/Edit can be indistinguishable from one produced by the CLI, so a linter cannot reliably detect authorship from file bytes alone.

## Constraints

Restrictions must still permit direct edits to `src/` and evidence bodies, which are outside the CLI's knowledge-layer scope. Preserve source skill research procedures and report tool restrictions as an additional experimental intervention. Do not alter the unchanged Files baseline or pretend the enforced condition measures only the CLI interface.

## Proposed approach

Treat this file as a decision-gated PR slot. Before implementation, PR 15 must report attempted and completed direct writes by path and operation, command-coverage failures, and artifact-integrity outcomes. Review whether violations come from missing CLI capabilities or instruction failure; fix capability gaps through their owning PRs instead of blocking access to required operations.

1. Agree a threshold for intervention from observed violations and their consequences. If instruction-only integration is sufficient, close this PR slot without code changes.
2. Prefer path-scoped Write/Edit restrictions when the actual harness supports them. Inspect the real harness configuration and tool-permission semantics before naming a file or API. Permit required code/evidence edits and CLI transaction paths; test symbolic links and path normalization against the chosen tool policy.
3. If review chooses an `ara check` rule, require a complete authorship/provenance contract and approved configuration design first. Issue #40 is a related per-rule configuration prerequisite, not proof that it already exists. A new provenance mechanism is additional protocol scope and needs its own approval; a checksum alone cannot prove who wrote a file.
4. Specify the exact target repo and files after the choice, then update this engineering plan before coding. Document expected rejection messages and recovery instructions without fake attribution or silently disabling checks.
5. Run a separate enforced comparison with prompts, budgets, tool permissions, and outcomes pinned. Keep the original CLI-only experiment unchanged for causal interpretation.

## Alternatives considered

Instruction-only access is the parent default and remains valid when violations are rare or harmless. A blanket ban on file edits breaks the stated code/evidence non-goals. Inferring writer identity from formatting or comments gives false assurance and is excluded.

## Tradeoffs

Tool restrictions can prevent mistakes but change the agent's action space. Provenance-based enforcement adds format and trust assumptions that the current plan does not establish. Neither approach should hide incomplete CLI coverage.

## Migration

No enforcement or metadata ships in earlier PRs. A protocol/harness-only change follows its target repo's review rules; shipped CLI behavior would require the shared patch, lockfile, docs, and changelog steps. This planning-only file does not create a dependency on issue #40 or a new protocol format.

## Verification and acceptance

For scoped tool restrictions, exercise a knowledge-layer direct-write attempt and confirm rejection, then complete the same operation through `ara`. Also exercise allowed edits to code and evidence bodies and confirm they still work. Check artifact fidelity and session history after recovery. For a linter design, add positive and negative provenance fixtures and explicitly test its stated trust limits before claiming authorship detection.

Acceptance requires an approved enforcement choice with actual target paths, demonstrated behavior in the real harness or binary, and separate evaluation from the baseline interface condition. If no evidence gate fires, record that result and do not open an implementation PR.

## Next Steps

Leave Q4 open until PR 15 reports direct-write behavior. Any selected enforcement mechanism must receive a concrete target-repo plan review before implementation.
