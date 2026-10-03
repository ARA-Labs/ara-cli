# 03: Lara numerical and argument checks for contributions
**Date:** 2026-10-03 (content from the 2026-10-02 draft)

Status: draft for human review, staged in `ara-cli` until the upstream route is decided (README decision D1). Target repositories: `ara-eval` (invocation adapter and verdict views), `Agent-Native-Research-Artifact` (the ARA-to-Lara binding contract), and `Lara` (documentation only). Parent: [collaborative research plan series](README.md). The user requested Lara's inclusion; whether it gates the core phases is README decision D3. No `ara-cli` change.

## TL;DR

For selected claims in a contribution, an argument producer writes a Lara argument plus a binding record that ties each element to exact native sources. The runner runs the pinned `lara check` and archives every input and output, and builds composite `.laramap` views only from compatible members. Verdicts are reported with their scope (one contribution or a named map). They never change ARA claim status, and they never stand in for reproduction.

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
3. **Write the binding record.** For each selected claim, it maps every Lara claim, leaf, argument, and attack to the original source, native revision, selector, and evidence object. It also records the formalization author, rationale, audit status, and assumptions.
4. **Treat the producer as untrusted.** An LLM-assisted producer is untrusted and spends the normal research budget. The checker can't verify prose-to-formal correspondence or transcription from a cited CSV.
5. **Store outside the snapshot.** Argument and binding files live outside the native snapshot root, so their header can name the snapshot's fingerprint without a self-referential hash.
6. **Bind into the package.** The package binds the snapshot plus the exact argument, binding, policy, executable, and verdict bytes.
7. **Map aliases.** `.laramap` aliases map to full contribution and source identities. Neither aliases nor the author-declared `artifact` digest authenticate a publisher.
8. **Survive renumbering.** Native integration can renumber entries, so keep the original source-qualified bindings plus the merge identity mapping. When the supporting claim or evidence changes, write a new argument against the new snapshot.

Raw findings can be published without a Lara argument. The envelope records coverage and, per check, `not-performed`, `accepted`, `rejected`, or `unavailable`:
- An accepted file can still report `gap`, `defeated`, or `contested`.
- An absent check supplies no status.
- Admission-blocked diagnostics and map refusals are kept separately.
- A policy condition that requires Lara fails explicitly if the checker is unavailable.

## How the runner invokes Lara

The runner materializes immutable argument files, runs `lara check`, and archives the exit status, the output, and every actual checker input. Composite checks name:
- the exact member roster and map manifest;
- the policy and signature;
- the backend and theory selection;
- every input digest.

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
- List excluded or incompatible members and the reason for each. Never present a partial map as the whole community.
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
- stale `--out` results.

Assert that argument acceptance never upgrades research maturity or stands in for a reproduction receipt. The integrated smoke reads back exact argument inputs and both individual and composite outcomes through the frontier's declared scope. Reuse Lara's own backend conformance cases.

## Alternatives and tradeoffs

- **A custom numerical or argument checker** would duplicate Lara's certificate, polarity, bridge, and attack rules. Reuse Lara as a pinned external process instead.
- **Replacing native knowledge with `.lara`** would lose research continuity and require formalizing every raw observation. Attach arguments to selected claims instead.
- **Equating Lara `justified` with ARA `supported`** would erase the difference between checked consequences of declared evidence and independently verified measurements.
- **Cost.** Writing arguments and reviewing bindings costs research budget. A valid certificate can sit on a misleading formalization or an inappropriate policy, so the runner keeps the audited mapping and the policy identity.
- **Coverage.** Lara's shared-contract requirement limits which contributions compose. Show that coverage boundary, and keep unsupported domains and raw findings visible.
- **Evaluation.** Adding Lara to the evaluated collective stack needs a reviewed `ara-eval` update covering stack definition, prompts, dependency locks, coverage policy, and registration, all before collection. Argument production, formalization review, checker runs, and composite views are charged to the collective budget.
