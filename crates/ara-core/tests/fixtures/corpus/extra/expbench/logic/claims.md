# Claims

## C01: Individual phase scores for AI agents on EXP-Bench reach 20–35% but do not reflect end-to-end competence
- **Statement**: On EXP-Bench, leading AI agents achieve design correctness (D) of 6.4–20.6%, implementation correctness (I) of 10.0–35.0%, and conclusion correctness (C) of 0.0–14.9%, but these scores conceal near-zero end-to-end performance.
- **Status**: supported
- **Falsification criteria**: If any agent achieves All✓ > 10% while also achieving D, I, C individually above 20%, then individual scores would not mask end-to-end failure.
- **Proof**: [E01]
- **Dependencies**: none
- **Tags**: benchmarking, partial scoring, agent evaluation, design, implementation, conclusion

## C02: The complete end-to-end experiment success rate (All·E✓) for the best AI agent is 0.5%
- **Statement**: OpenHands + o3-mini, the top-ranked agent, achieves All·E✓ = 0.5% across 461 EXP-Bench tasks, where All·E✓ requires correct design, implementation, conclusion, and executable code.
- **Status**: supported
- **Falsification criteria**: If a re-evaluation of OpenHands + o3-mini on the same 461 tasks produces All·E✓ > 5%, then this claim would be refuted.
- **Proof**: [E01]
- **Dependencies**: C01
- **Tags**: end-to-end, executability, benchmark, o3-mini, OpenHands

## C03: Applying conjunctive evaluation metrics progressively reduces average agent score from 20.6% to 0.2%
- **Statement**: Among the execution-verified subset of tasks, average score decreases monotonically: M alone = 20.6%, M·C·D = 3.7%, M·C·D·I = 0.4%, M·C·D·I·E = 0.2%.
- **Status**: supported
- **Falsification criteria**: If the score at any conjunction step is not strictly lower than the previous step for the majority of agents, this claim would be refuted.
- **Proof**: [E02]
- **Dependencies**: C01, C02
- **Tags**: conjunctive metrics, evaluation, progressive scoring, brittleness

## C04: Missing essential implementation components is the most prevalent failure mode (39.71%)
- **Statement**: Among all identified implementation failures across all agent-task pairs, 39.71% are classified as "Missing Essential Implementation Components," making it the single largest failure category.
- **Status**: supported
- **Falsification criteria**: If a re-analysis of the same 3,238 raw failure insights yields a different failure type with prevalence ≥ 39.71%, this claim would be refuted.
- **Proof**: [E03]
- **Dependencies**: none
- **Tags**: failure analysis, implementation, missing components, bottleneck

## C05: Execution failures are dominated by environment/dependency misconfiguration (29.38%) and script errors (23.84%)
- **Statement**: Among execution phase failures, environment/dependency configuration errors account for 29.38% and execution script/file errors account for 23.84%, together constituting over half of all execution failures.
- **Status**: supported
- **Falsification criteria**: If either prevalence figure changes by more than 5 percentage points in a re-analysis of the same failure logs, this claim would be refuted.
- **Proof**: [E03]
- **Dependencies**: C02
- **Tags**: failure analysis, execution, environment, dependencies, reproducibility

## C06: Conjunctive metrics (C·D, I·E) substantially reduce score variance compared to individual metrics
- **Statement**: Individual metrics C and E exhibit high variance across agent-task pairs, while conjunctive forms C·D and I·E produce more stable, lower-variance evaluation signals that better discriminate agent capability.
- **Status**: supported
- **Falsification criteria**: If the standard deviation of C·D scores across tasks is not smaller than the standard deviation of C scores alone for at least 5 of 7 agent configurations, this claim would be refuted.
- **Proof**: [E04]
- **Dependencies**: C03
- **Tags**: metric stability, variance, conjunctive scoring, evaluation reliability
