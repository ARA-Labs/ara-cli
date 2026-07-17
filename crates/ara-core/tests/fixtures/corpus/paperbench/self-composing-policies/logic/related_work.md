# Related Work

## RW01: Rusu et al., 2016 — Progressive Neural Networks
- **DOI**: arXiv:1606.04671
- **Type**: baseline
- **Delta**:
  - What changed: CompoNet composes previous modules at the output (policy) level via attention; ProgressiveNet connects all hidden layers of all previous columns to each new column via lateral connections.
  - Why: Output-level composition yields linear O(n) parameter growth vs quadratic O(n²) in ProgressiveNet; avoids the hidden-layer coupling that causes quadratic growth.
- **Claims affected**: C01, C04
- **Adopted elements**: Freeze-and-grow paradigm (freeze previous module parameters; add new module per task); separate module per task.

## RW02: Mallya & Lazebnik, 2018 — PackNet
- **DOI**: Proceedings of CVPR 2018, pp. 7765–7773
- **Type**: baseline
- **Delta**:
  - What changed: CompoNet grows the network; PackNet stores all task solutions in a single fixed-size network by iterative pruning and masking.
  - Why: PackNet's approach limits plasticity as tasks accumulate (trainable parameters decrease with each task until none remain); CompoNet maintains full plasticity.
- **Claims affected**: C02, C03
- **Adopted elements**: None directly; serves as ablation showing the plasticity–memory tradeoff.

## RW03: Haarnoja et al., 2018 — SAC
- **DOI**: Proceedings of ICML 2018, pp. 1861–1870
- **Type**: imports
- **Delta**:
  - What changed: CompoNet uses SAC as the off-policy RL algorithm for continuous control (Meta-World); SAC's entropy regularization and replay buffer are unchanged.
  - Why: SAC is state-of-the-art for continuous control and is standard in the Meta-World CRL literature (Wolczyk et al., 2021).
- **Claims affected**: C02
- **Adopted elements**: Entire SAC training loop with entropy auto-tuning; actor-critic architecture.

## RW04: Schulman et al., 2017 — PPO
- **DOI**: arXiv:1707.06347
- **Type**: imports
- **Delta**:
  - What changed: CompoNet uses PPO as the on-policy RL algorithm for discrete visual control (ALE environments); PPO's clipping objective and GAE are unchanged.
  - Why: PPO is standard for discrete action ALE tasks and does not require a replay buffer (avoiding its interaction with continual learning).
- **Claims affected**: C02
- **Adopted elements**: Entire PPO training loop including GAE, advantage normalization, value clipping.

## RW05: Wolczyk et al., 2021 — Continual World (CW20 Benchmark)
- **DOI**: NeurIPS 2021, volume 34, pp. 28496–28510
- **Type**: bounds
- **Delta**:
  - What changed: CompoNet uses CW20's task sequence (10 Meta-World tasks × 2) but with v2 environments; adopts the same forward transfer and average performance metrics.
  - Why: CW20 provides a standardized benchmark enabling fair comparison with prior CRL methods.
- **Claims affected**: C02, C05
- **Adopted elements**: CW20 task ordering, FTr metric definition, RT concept, success rate metric, baseline comparison methodology.

## RW06: Vaswani et al., 2017 — Attention Is All You Need
- **DOI**: NeurIPS 2017, volume 30
- **Type**: imports
- **Delta**:
  - What changed: CompoNet uses single-head scaled dot-product attention with cosine positional encoding (not multi-head, not learned positional encodings) applied to policy output vectors rather than token sequences.
  - Why: Attention provides a principled, differentiable mechanism for soft selection among previous policy outputs conditioned on the current state.
- **Claims affected**: C03
- **Adopted elements**: Scaled dot-product attention formula; cosine positional encoding.

## RW07: Rosenbaum et al., 2019 — Routing Networks Challenges
- **DOI**: arXiv:1904.12774
- **Type**: refutes
- **Delta**:
  - What changed: CompoNet eliminates the need for a dedicated routing/composing network; modules learn to compose themselves autonomously.
  - Why: Rosenbaum et al. (2019) showed that jointly training the composing strategy and the modules being composed is non-stationary and unstable; CompoNet avoids this by using attention within each module.
- **Claims affected**: C03
- **Adopted elements**: Problem framing of neural composition for multi-task RL.

## RW08: Wolczyk et al., 2022 — Disentangling Transfer in CRL
- **DOI**: NeurIPS 2022, volume 35, pp. 6304–6317
- **Type**: extends
- **Delta**:
  - What changed: CompoNet extends the disentangled transfer analysis framework with new metrics and sequences; adopts the forgetting metric and the FTr matrix analysis.
  - Why: Provides rigorous evaluation methodology for CRL that disentangles forward transfer from forgetting.
- **Claims affected**: C02, C05
- **Adopted elements**: Forgetting metric $F_i$; forward transfer matrix computation; methodology for computing RT.

## RW09: Gaya et al., 2023 — Subspace of Policies
- **DOI**: ICLR 2023
- **Type**: baseline
- **Delta**:
  - What changed: CompoNet uses output-level attention-based composition with per-task modules; Gaya et al. build a policy subspace in a shared parameter space.
  - Why: CompoNet explicitly avoids catastrophic forgetting via module freezing, while subspace approaches may still require careful parameter management.
- **Claims affected**: C01, C02
- **Adopted elements**: Conceptual motivation for scalable continual policy learning.
