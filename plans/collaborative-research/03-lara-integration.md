# 03: Lara numerical and argument checks for contributions
**Date:** 2026-10-03 (content from the 2026-10-02 draft)

Status: **approved** by the human developer on 2026-10-03 after design review. Implementation pending. This plan remains staged in `ara-cli` until the upstream route is decided (README decision D1). Target repositories: `ara-eval` (invocation adapter and verdict views), `Agent-Native-Research-Artifact` (the ARA-to-Lara binding contract), and `Lara` (documentation only). Parent: [collaborative research plan series](README.md). Lara is a parallel track that joins at phase 5; it does not gate core phases 3 and 4. No additional `ara-cli` change beyond the other plans in this series. Upstream contract adoption and scored-study registration remain separate gates.

## TL;DR

For selected claims in a contribution, an argument producer writes a Lara argument plus bindings to exact native sources. The runner runs the pinned `lara check`, archives its inputs and outputs, and builds composite `.laramap` views only from compatible members. Separate review attestations establish who audited each formalization; a checker verdict cannot establish that. Every verdict names an immutable contribution or map revision and its coverage. Verdicts never change ARA claim status or stand in for reproduction.

## Sources

Lara's interfaces at the inspected revision `a31299f`:
- [specification](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/docs/spec.md)
- [multi-artifact composition contract](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/docs/multi-artifact-composition-decision.md)
- [ordered comparison](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/src/Lara/Strict/Ord.hs)
- [relative drop](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/src/Lara/Strict/RA.hs)
- [comparison expansion](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/src/Lara/Elaborate/Comparison.hs)

These links pin what was inspected, not a chosen dependency. Freeze the actual source and executable digests at implementation and registration. [Lara issue #6](https://github.com/ARA-Labs/Lara/issues/6) asks for clearer README documentation of the shipped numerical checks and their measurement-assurance boundary; it asks for no new behavior.

## What Lara checks

| Surface | Checks | Does not establish |
|---|---|---|
| `ord@1` | `<` and `<=` in exact rational arithmetic; operands bound to cited premise cells; reports consulted dependencies. | How the cells were measured. |
| `ra@1` | Recomputes relative drop; checks the witness fraction, claimed drop, and threshold; rejects a zero full cell. | Evaluator correctness or experiment comparability. |
| `comparison` | Generates a certified numerical step and a defeasible empirical bridge; checks metric polarity and system, metric, dataset, and setup bindings. | The comparison setup, which stays attackable declared evidence. |
| `.lara` | Support terms, declared obligations, certificates, typed attacks; scoped argument status. | Prose-to-formal correspondence, omitted evidence, or policy adequacy. |
| `.laramap` | Rechecks independently written members under one shared contract; generates cross-member attacks from declared contraries. | Vocabulary reconciliation, evidence independence, or member bytes. |

Measurement execution, source-byte verification, and independent reproduction stay with [02](02-contribution-workflow.md#what-verification-must-establish).

## How arguments enter the workflow

1. **Freeze.** The runner runs `ara snapshot` before any argument is written, so arguments target a fixed native revision.
2. **Write the argument.** The argument producer reads the frozen ARA through complete CLI source reads and permitted evidence access.
3. **Write the binding record.** For each selected claim, map every Lara claim, leaf, argument, and attack to its source key, native revision, selector, and evidence object digest. Record the formalization author, rationale, assumptions, and declared audit status. The declared status is untrusted; authoritative review follows the attestation rules below.
4. **Treat the producer as untrusted.** An LLM-assisted producer is untrusted and spends the normal research budget. The checker can't verify prose-to-formal correspondence or transcription from a cited CSV.
5. **Store outside the snapshot.** Argument and binding files live outside the native snapshot root, so their header can name the snapshot's fingerprint without a self-referential hash.
6. **Bind into the payload.** An initial payload includes the snapshot plus exact argument, binding, policy, executable, and verdict bytes. These initial files refer to the native revision, not their enclosing contribution ID, which is computed later under [02](02-contribution-workflow.md#what-identifies-a-contribution).
7. **Map aliases.** After publication, map aliases to full contribution IDs, argument/check record IDs, and source identities in a separately published map record. Neither aliases nor the author-declared `artifact` digest authenticate a publisher.
8. **Survive renumbering.** Native integration can renumber entries, so keep the original source-qualified bindings plus the merge identity mapping. When the supporting claim or evidence changes, write a new argument against the new snapshot.

Raw findings can be published without a Lara argument. The envelope records coverage and, per check, `not-performed`, `accepted`, `rejected`, or `unavailable`:
- An accepted file can still report `gap`, `defeated`, or `contested`.
- An absent check supplies no status.
- Admission-blocked diagnostics and map refusals are kept separately.
- A policy condition that requires Lara fails explicitly if the checker is unavailable.

## Who may mark a formalization reviewed?

The run's role policy names authorized reviewers. The producer cannot approve its own formalization for audited coverage. An authorized reviewer publishes an immutable attestation binding reviewer identity, target contribution or initial payload inputs, native revision, argument and binding digests, policy/vocabulary revision, reviewed selectors, disposition, and rationale. The coordinator checks actor authority and exact input identities; Lara's own `reviewed` annotation is not sufficient.

Review covers whether the formal statement, numeric cells, assumptions, and source references faithfully represent the selected evidence. It does not prove measurement correctness or reviewer independence. A changed covered input invalidates the attestation for the new version. Corrections and disputes are new records; they retain the previous attestation and explain the affected scope.

Individual checks may run on unreviewed or disputed bindings, but their views must display audit state beside checker status. Audited community maps include only members whose selected bindings have authorized review and no unresolved dispute. Excluded members remain visible with their reason and never count toward audited coverage. An explicitly labeled exploratory map may include other members, under a distinct scope and coverage policy; it cannot supply an audited-community verdict.

## How do members agree on experimental settings?

The protocol defines a versioned vocabulary and setting registry before L2. A setting descriptor binds the dataset and split digests, evaluator revision and configuration, metric definition and polarity, experimental controls, and the registered treatment of seeds and other replication variables. System and baseline identifiers bind their actual code/configuration inputs separately. Evidence values and contribution IDs do not define setting identity, so two replications can refer to the same setting despite different outcomes.

The runner derives registry identifiers from canonical descriptors, using the encoding and schema-specific hash domains in [02](02-contribution-workflow.md#what-identifies-a-contribution). The argument producer uses those identifiers rather than inventing local synonyms. Authorized reviewers check that source bindings justify the descriptors. A common dataset name alone does not establish the same setting, and matching formal symbols alone does not establish comparable experiments.

Lara receives the pinned registry's mapping to formal symbols and the shared policy/signature. Descriptor changes create new identifiers. Vocabulary additions create a new vocabulary/policy revision and map scope; members must be checked under that exact contract or remain excluded with a reason. Old verdicts remain attached to their original scope. This is an external binding rule, not a new Lara ontology-matching feature.

## How are later checks and map revisions identified?

Raw contributions may publish before their first argument exists. A later argument, checker run, review, or dispute becomes an immutable attachment record targeting the original contribution ID. Reuse [02](02-contribution-workflow.md#what-identifies-a-contribution)'s record hashing, actor/request binding, publication, and recovery rules. Each attachment has its own inventory and record ID; the original payload and envelope stay unchanged. An initial inline check can be referenced by contribution ID and its inventory path after publication.

A map revision binds its exact manifest, ordered alias mapping, member contribution and argument revisions, bindings, policy/signature, vocabulary registry, and checker executable/backends/theories. It also binds the intended population at a visibility sequence, coverage policy, and exclusions. Compute its identity from these inputs, excluding outputs. A check record separately binds the map revision to exit status and verdict bytes. A stable display name may group map revisions but cannot identify a verdict's scope.

Adding or removing a member, changing formalization or policy, or changing the intended population creates a new map revision. The frontier retains verdicts from different scopes rather than letting the newest small map erase a prior contest. Supersession applies only to records with the same verifier, exact target/map revision, method/configuration, and scope; an explicit superseding record and coordinator sequence select the current result. No map claims whole-community coverage merely because all included members passed.

## How the runner invokes Lara

The runner materializes immutable argument files, runs `lara check`, and archives the exit status, the output, and every actual checker input. Composite checks name:
- the exact member roster and map manifest;
- the policy and signature;
- the backend and theory selection;
- every input digest, immutable map revision, and the coverage population and policy.

Two pitfalls:
- **Map digests aren't verified.** Lara maps read current local paths and don't check their declared artifact digests; package pinning supplies the reproducibility.
- **Stale `--out` files.** Check the exit status before reading output, because a failed `--out` run can leave an older verdict file in place.

## Composite views and their limits

Build a composite map only from members that share policy structure, proposition vocabulary, theories, and backends. Lara map v1 imposes several limits:
- It is flat.
- It rejects members that need unsupported admission or group pruning.
- It has no cross-member support imports and no handwritten cross-member undercuts.

So a native ARA may cite a peer's contribution, but its Lara argument must start self-contained, carrying that evidence's original pinned provenance.

Reporting rules:
- List excluded or incompatible members and reasons, including absent arguments, unreviewed or disputed bindings, and vocabulary mismatch. Show the intended population and visibility sequence alongside included members. Never present a partial map as the whole community.
- Extending map composition needs a separate Lara contract review, done in Lara rather than as runner changes to its rules.

Individual and composite verdicts keep their scopes:
- A claim can be justified alone and contested in a map, if another contribution declares a contrary under the same setting.
- Different settings aren't a disagreement just because their prose sounds opposed.

Three things stay separate:
- Lara argument status;
- ARA research maturity;
- experimental reproduction verdicts.

Numerical acceptance, claim promotion, and canonical integration are different events.

The inspected S4 example checks a numerical ordering while defeating the broader improvement claim through an attack on the comparison setup. S5 checks a lower-is-better metric and its bridge. These are illustrative checker examples. They don't validate real experiments or show that every empirical claim can be represented.

## Failures

| Failure | Required behavior |
|---|---|
| Certificate or formalization rejects | Keep the raw contribution, bindings, and diagnostic. Don't mark the argument accepted, and don't infer that the experiment failed. |
| Composite map refuses or misses required coverage | Expose the unavailable or incomplete scope with member reasons. Never reuse a prior verdict as the current result. |
| Argument-to-snapshot hash mismatch | Reject the argument package; require a new argument against the new snapshot. |
| Checker unavailable | Record `unavailable`; conditions that require Lara fail explicitly. |
| Producer labels its own binding reviewed | Retain the declared annotation as untrusted; it supplies no authorized attestation or audited coverage. |
| Review input or setting descriptor changes | Require a new binding/review or scope as applicable; do not carry forward the previous approval or map verdict. |

## Engineering checks

Run real Lara checks on:
- same-setting contrary claims;
- different settings;
- metric polarity;
- wrong cells and false comparisons;
- relative-drop witness and threshold failures.

Test:
- stale argument-to-ARA bindings;
- altered policy or map bytes;
- incompatible contracts;
- refused maps;
- stale `--out` results;
- self-review, unauthorized review, and disputed bindings excluded from audited coverage while raw findings remain visible;
- same-setting replications with different outcomes, and distinct evaluator/configuration settings sharing a dataset name;
- a first argument/check attached after raw publication without changing the original contribution ID;
- a changing map roster or population creating a different scope, without erasing earlier contested verdicts;
- vocabulary extensions requiring rechecks or explicit exclusions rather than silent symbol reuse.

Assert that argument acceptance never upgrades research maturity or stands in for a reproduction receipt. The integrated smoke reads back exact argument inputs and both individual and composite outcomes through the frontier's declared scope. Reuse Lara's own backend conformance cases.

## Alternatives and tradeoffs

- **A custom numerical or argument checker** would duplicate Lara's certificate, polarity, bridge, and attack rules. Reuse Lara as a pinned external process instead.
- **Replacing native knowledge with `.lara`** would lose research continuity and require formalizing every raw observation. Attach arguments to selected claims instead.
- **Equating Lara `justified` with ARA `supported`** would erase the difference between checked consequences of declared evidence and independently verified measurements.
- **Cost.** Writing arguments and reviewing bindings costs research budget. A valid certificate can sit on a misleading formalization or an inappropriate policy, so the runner keeps the audited mapping and the policy identity.
- **Coverage.** Lara's shared-contract requirement limits which contributions compose. Show that coverage boundary, and keep unsupported domains and raw findings visible.
- **Evaluation.** Adding Lara to the evaluated collective stack needs a reviewed `ara-eval` update covering stack definition, prompts, dependency locks, coverage policy, and registration, all before collection. Argument production, formalization review, checker runs, and composite views are charged to the collective budget.

## Next Steps

1. Freeze the binding, review-attestation, setting-registry, attachment, and map-scope schemas in L1 with their protocol owners. Select the actual Lara source and executable pins.
2. Implement and exercise the adapter and scoped views in L2. Record numerical checks, formalization review, research maturity, and reproduction separately.
3. Join the complete worker-loop smoke at phase 5. Update and approve the `ara-eval` stack registration before any scored collection; this plan approval does not authorize paid runs.
