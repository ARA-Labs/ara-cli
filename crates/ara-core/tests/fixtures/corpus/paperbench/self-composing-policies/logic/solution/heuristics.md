# Heuristics

## H01: Initialize New ALE Encoder from Previous Module's Encoder
- **Rationale**: Cold-starting a new CNN encoder on a new ALE task wastes the feature extraction knowledge learned on prior visually similar tasks. Warm-starting from the previous module's encoder accelerates early-stage learning.
- **Sensitivity**: medium — beneficial for visually similar task sequences (SpaceInvaders modes, Freeway modes); less important when tasks are visually dissimilar.
- **Bounds**: Applied whenever a new module is added for ALE tasks. Previous encoder must exist (not applicable to the first module).
- **Code ref**: [src/execution/encoder.py]
- **Source**: Appendix E.2 (CompoNet-specific implementation details)

## H02: Reset Critic at Each Task Boundary
- **Rationale**: The critic learns Q-values or state values specific to the current task's reward function $r^{(k)}$ and transition dynamics $p^{(k)}$. Carrying over a critic from a previous task introduces biased value estimates that destabilize actor training on the new task.
- **Sensitivity**: high — incorrect critic initialization can significantly harm early-task performance.
- **Bounds**: Applied universally to all methods in the experimental comparison (Baseline, FT-1, FT-N, ProgressiveNet, PackNet, CompoNet).
- **Code ref**: [src/execution/componet.py]
- **Source**: Appendix E (implementation details), common practice per Wolczyk et al. (2022)

## H03: Cosine Positional Encoding for Module Identity
- **Rationale**: The attention mechanism is permutation-invariant by default; without positional encoding, the model cannot distinguish which previous module produced which row of $\Phi^{k;s}$. Cosine positional encoding provides a stable, non-learned representation of module order.
- **Sensitivity**: low — any reasonable positional encoding scheme should work; cosine encoding is parameter-free and well-established.
- **Bounds**: Applied to keys matrices in both Output and Input Attention Heads ($E_{out}$, $E_{in}$). Matrix size matches $\Phi^{k;s}$.
- **Code ref**: [src/execution/componet.py]
- **Source**: Section 4.2; Vaswani et al. (2017)

## H04: Re-initialize Output Head at Each Task Boundary (FT-N, ProgressiveNet, PackNet)
- **Rationale**: Keeping an output head trained on task $k-1$ biases early exploration on task $k$ toward actions that solved the previous task, potentially increasing interference. Re-initializing the output head ensures the new task starts with an unbiased policy head while preserving learned features in earlier layers.
- **Sensitivity**: medium — following Wolczyk et al. (2021); primarily applicable to methods that share trunk parameters across tasks.
- **Bounds**: Applied to all methods in the comparison following established CRL practice. Not applicable to CompoNet (new module added entirely).
- **Code ref**: [src/execution/componet.py]
- **Source**: Appendix E.2 (FT-N, ProgressiveNet, PackNet descriptions); Wolczyk et al. (2021)

## H05: PackNet Retrain Budget = 20% of Task Timestep Budget
- **Rationale**: After pruning, the network must be retrained to recover performance before parameters are frozen. Using 20% of $\Delta = 1\text{M}$ timesteps (200K steps) provides enough retraining without reducing the available training budget for exploration on the main task.
- **Sensitivity**: medium — too few retraining steps leads to performance collapse after pruning; too many reduces effective exploration.
- **Bounds**: Exactly 200,000 retraining timesteps per task for PackNet ($= 0.2 \times 1\text{M}$).
- **Code ref**: [src/execution/componet.py]
- **Source**: Appendix E.2 (PackNet implementation details)
