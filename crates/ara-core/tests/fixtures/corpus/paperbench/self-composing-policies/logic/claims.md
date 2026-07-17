# Claims

## C01: Linear Parameter Growth
- **Statement**: The total number of parameters in CompoNet grows as O(n) with respect to the number of tasks n, because each self-composing policy module has a constant number of parameters independent of n (six linear transformations and a feed-forward block whose sizes depend only on fixed hyperparameters d_enc, d_model, |A|).
- **Status**: supported
- **Falsification criteria**: Demonstrating that adding a new module requires increasing the size of any weight matrix as a function of n, or that total parameter count follows a super-linear curve.
- **Proof**: [E04]
- **Dependencies**: none
- **Tags**: scalability, parameter-growth, memory

## C02: Superior Performance and Forward Transfer
- **Statement**: CompoNet achieves strictly higher or equal average performance P(T) and forward transfer FTr on all three task sequences (Meta-World, SpaceInvaders, Freeway) compared to all five baselines (Baseline, FT-1, FT-N, ProgressiveNet, PackNet), as reported in Table 1.
- **Status**: supported
- **Falsification criteria**: Any baseline method achieving higher mean performance or forward transfer than CompoNet on any of the three sequences with 10 random seeds.
- **Proof**: [E01, E02, E03]
- **Dependencies**: C03
- **Tags**: performance, forward-transfer, benchmark

## C03: Three-Scenario Robustness
- **Statement**: CompoNet correctly handles all three task-relatedness scenarios: (i) when a previous policy solves the current task, the output attention head assigns high attention to it; (ii) when a function over previous policies can help, the module learns that function; (iii) when no previous policy is relevant, the internal policy overwrites the attention output and learns from scratch without interference.
- **Status**: supported
- **Falsification criteria**: In scenario (i), the output attention head failing to assign dominant attention to the relevant prior module; in scenario (iii), CompoNet performing significantly worse than a baseline trained from scratch.
- **Proof**: [E03]
- **Dependencies**: none
- **Tags**: knowledge-reuse, interference-robustness, attention

## C04: Empirical Inference Scalability
- **Statement**: Although CompoNet's theoretical inference complexity is O(n²), its empirical inference time grows substantially slower than ProgressiveNet up to 300 tasks tested, because CompoNet computes attention keys and values in parallel over previous module outputs rather than through sequential layer-wise lateral connections.
- **Status**: supported
- **Falsification criteria**: CompoNet's empirical inference time curve matching or exceeding ProgressiveNet's quadratic growth rate at any point in a 300-task sequence.
- **Proof**: [E04]
- **Dependencies**: C01
- **Tags**: scalability, inference-time, computational-cost

## C05: Knowledge Composition Beyond Fine-Tuning
- **Statement**: CompoNet's forward transfer consistently exceeds the Reference Transfer (RT) in all three task sequences, demonstrating that it leverages compositional knowledge from multiple previous tasks simultaneously, beyond what fine-tuning on the single best previous task can achieve.
- **Status**: supported
- **Falsification criteria**: CompoNet's average FTr falling below or equal to RT on any task sequence.
- **Proof**: [E01, E02, E03]
- **Dependencies**: C02
- **Tags**: forward-transfer, knowledge-composition, RT
