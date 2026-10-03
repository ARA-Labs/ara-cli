# PR 16: add local semantic search only if keyword search fails
**Date:** 2026-10-01

Current delivery status: **closed conditional, unimplemented**. Plan 15 is deferred, so the evidence gate has not fired; this is not an experiment outcome.


Status: **approved as conditional** 2026-10-01; not scheduled until its evidence gate fires. Repository: `ARA-Labs/ara-cli`. Depends on [PR 10](../../docs/agent-cli-interface/10-keyword-search.md) and evidence from [PR 15](15-experiment-harness.md). Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

Build this PR only if held-out experiments show consequential paraphrase misses in keyword search. Add local embeddings behind a Cargo feature that is off by default. Require pre-provisioned model files and preserve the no-network command promise. Review the actual model, artifact source, license, and performance budget before implementation; the parent's example model is not an approved choice.

## Problem

BM25 can miss entries whose wording differs from the query. The parent names a small bge-class model through fastembed as a possibility, not a requirement or evidence of improved quality. An embedding dependency also risks automatic model downloads, large native runtimes, and startup costs that do not fit the existing read budget.

## Constraints

No hosted embeddings and no download during an `ara` invocation. The default build, keyword search, wasm dependencies, and existing result contracts remain unchanged. Feature-disabled requests and missing or incompatible model files return explicit errors; never silently substitute another search method while reporting semantic results.

## Proposed approach

Add an optional CLI-only embedding dependency and `semantic_search.rs` (new) behind a proposed `semantic-search` feature in `crates/ara-cli/Cargo.toml`. Keep discovery and result rendering shared with `find`. Review an explicit search-mode flag and local-model path contract before adding them to clap. Model acquisition is a separate documented provisioning step performed by the operator, not an implicit command fallback.

1. Use PR 15's held-out failures to distinguish paraphrase misses from parser omissions, broken queries, or weak task grading. Record the evidence gate and a proposed quality improvement margin before choosing a model.
2. Benchmark candidate local models for licensed redistribution or operator provisioning, supported architectures, disk/memory footprint, offline runtime behavior, and startup-inclusive latency. Inspect dependencies for implicit download behavior. Approve exact model/runtime versions and checksum verification.
3. Implement deterministic entry text construction, local inference, cosine ranking, and stable tie ordering. If persistent vectors are necessary, key them by model checksum, tokenizer version, text construction version, and source content hashes. A cache remains disposable and must never accept stale vectors as current source.
4. Update command documentation with mode, model prerequisites, offline errors, and the actual measured resource limits. Changing the parent read-speed limits requires explicit review. If no candidate meets the approved limits, keep keyword-only search.
5. Extend the search relevance experiment with an explicitly separate semantic condition. Count provisioning and inference costs where applicable, and keep the Files-versus-CLI comparison's chosen mode pinned.

## Alternatives considered

Keep keyword-only search if labeled misses do not affect held-out task quality enough to justify local-model costs. Hosted APIs violate the no-network requirement and are excluded. A feature-enabled automatic download still violates that requirement during invocation.

## Tradeoffs

Local embeddings can improve paraphrase retrieval but add native dependencies and model provenance. Cold-start latency and model licensing may prevent acceptance. This is an evidence-gated optional extension, not a blocker for the baseline CLI interface.

## Migration

Default installations need no changes or model assets. A functional implementation gets its own patch bump, lockfile refresh, changelog, and docs. No embeddings enter `ara-core` or the default wasm graph. Model cache paths and invalidation rules become documented public behavior only after approval.

## Verification and acceptance

Feature-enabled tests cover missing models, incompatible checksums, offline execution, cache invalidation if introduced, deterministic ranking, and zero-vector handling. Feature-disabled tests verify a clear unsupported-mode error. Run the default build without the optional runtime and run the enabled binary in an environment that denies outbound traffic with a pre-provisioned model.

Smoke a held-out paraphrase query that keyword search missed, then inspect the retrieved entry through `show --full`. Compare task quality and ranking against the pinned keyword baseline; record startup-inclusive timing and memory. Acceptance requires the pre-agreed quality margin, approved resource budget, and demonstrated offline execution. If the evidence gate does not fire, do not open this implementation PR.

## Next Steps

Wait for PR 15's keyword-search results. Review a model/runtime selection and explicit mode contract only if those results justify this extension.
