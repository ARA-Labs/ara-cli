# Experiments

## E01: Meta-World Benchmark — CRL Method Comparison
- **Verifies**: C02, C05
- **Setup**:
  - Model: CompoNet actor + SAC critic (2-layer MLP), plus 5 baselines: Baseline, FT-1, FT-N, ProgressiveNet, PackNet
  - Hardware: RTX3090 or Nvidia A5000 GPU, cluster nodes with 345GB or 377GB RAM
  - Dataset: Meta-World CW20 sequence — 10 tasks repeated twice (20 total): hammer-v2, push-wall-v2, faucet-close-v2, push-back-v2, stick-pull-v2, handle-press-side-v2, push-v2, shelf-place-v2, window-close-v2, peg-unplug-side-v2
  - System: SAC algorithm (Haarnoja et al., 2018), 39-dimensional state, 4-dimensional continuous action, 1M timesteps per task, critic reset each task boundary
- **Procedure**:
  1. Initialize all 6 methods with random parameters and 10 different random seeds each
  2. Train each method sequentially on the 20-task CW20 sequence with Δ=1M timesteps per task
  3. At each task transition: freeze current module parameters (for CompoNet/ProgressiveNet), reset critic, re-initialize output head
  4. Record success rate p_i(t) at regular intervals for all tasks and methods
  5. Compute AUC_i and AUC^b_i for each task
  6. Compute FTr_i = (AUC_i - AUC^b_i) / (1 - AUC^b_i) for each task
  7. Compute average performance P(T) = (1/N) Σ p_i(T)
  8. Compute forward transfer FTr = (1/N) Σ FTr_i
  9. Report mean ± std across 10 seeds
- **Metrics**: Average performance P(T) ∈ [0,1], Forward Transfer FTr (normalized, can be negative), measured over 10 seeds
- **Expected outcome**:
  - CompoNet achieves higher or equal average performance compared to all other methods
  - CompoNet achieves non-negative forward transfer (positive knowledge transfer) while other methods may show negative forward transfer due to interference
  - CompoNet's forward transfer should exceed the RT value for this sequence
  - Growing NN methods (CompoNet, ProgressiveNet) should outperform fixed-capacity methods (PackNet) in performance
- **Baselines**: Baseline (random init per task), FT-1 (single fine-tuned network), FT-N (per-task fine-tuned networks), ProgressiveNet, PackNet
- **Dependencies**: none

## E02: ALE Visual Control Benchmark — SpaceInvaders and Freeway
- **Verifies**: C02, C05
- **Setup**:
  - Model: CompoNet actor (with per-module CNN encoder, d_enc=512) + PPO critic, plus 5 baselines
  - Hardware: RTX3090 or Nvidia A5000 GPU cluster
  - Dataset: SpaceInvaders — 10 playing modes (Modes 0–9); Freeway — 7 playing modes (Modes 0–6); ALE/SpaceInvaders-v5 and ALE/Freeway-v5 via Gymnasium
  - System: PPO algorithm, discrete actions (6 for SpaceInvaders, 3 for Freeway), 210×160 RGB images encoded to d_enc=512 via CNN, Δ=1M timesteps per task
- **Procedure**:
  1. Define success scores for each task (90% of average final episodic return across all methods and seeds)
  2. Initialize all 6 methods with 10 random seeds each
  3. Train each method sequentially on 10 SpaceInvaders modes then 7 Freeway modes (treated as separate sequences)
  4. For CompoNet: initialize new module CNN encoder from previous module's encoder weights
  5. Record success rate p_i(t) at regular intervals
  6. Compute average performance P(T) and forward transfer FTr as in E01
  7. Report mean ± std across 10 seeds for each sequence independently
- **Metrics**: Average performance P(T), Forward Transfer FTr, measured over 10 seeds per sequence
- **Expected outcome**:
  - CompoNet achieves highest or tied-highest performance and forward transfer in both SpaceInvaders and Freeway sequences
  - CompoNet's forward transfer should exceed RT (positive for SpaceInvaders and Freeway)
  - FT-N and CompoNet should outperform FT-1 (which suffers forgetting) in performance
  - Baseline should perform worst, especially on later tasks in each sequence
- **Baselines**: Baseline, FT-1, FT-N, ProgressiveNet, PackNet
- **Dependencies**: none

## E03: Architectural Validation — Scenarios (i) and (iii)
- **Verifies**: C03
- **Setup**:
  - Model: CompoNet with 5 previous modules (1 informative + 4 non-informative for scenario i; 5 non-informative for scenario iii)
  - Hardware: RTX3090 or Nvidia A5000 GPU
  - Dataset: SpaceInvaders task 5 (scenario i) and task 6 (scenario iii)
  - System: PPO with the standard ALE hyperparameters (Table E.2), 1M timesteps
- **Procedure**:
  1. **Scenario (i)**: Pre-train one CompoNet module on SpaceInvaders task 5; freeze it. Create 4 non-informative modules that sample from a uniform Dirichlet distribution. Train a new CompoNet module on task 5 with these 5 predecessors.
  2. **Scenario (iii)**: Create 5 non-informative Dirichlet modules. Train a new CompoNet module on task 6 with these 5 predecessors.
  3. In parallel, train a Baseline (no prior modules) on the same task.
  4. Record at regular intervals (≤10k timesteps): episodic return, output attention head attention weights per predecessor, input attention head attention weights per predecessor, matching rates between (final output, output head), (final output, internal policy), (output head, internal policy)
  5. Aggregate over 10 random seeds
- **Metrics**: Episodic return curves, attention weight distributions, matching rates (frequency of action argmax agreement between components)
- **Expected outcome**:
  - Scenario (i): Output attention head rapidly assigns dominant attention to the informative module; internal policy matching rate with final output increases over time; CompoNet converges faster than Baseline
  - Scenario (iii): Both attention heads show uniform attention across all predecessors; internal policy determines the final output; CompoNet performance matches or slightly exceeds Baseline (no interference)
- **Baselines**: Baseline (single network trained from scratch on the same task)
- **Dependencies**: none

## E04: Scalability — Parameter Count and Inference Time vs Number of Tasks
- **Verifies**: C01, C04
- **Setup**:
  - Model: CompoNet and ProgressiveNet
  - Hardware: AMD EPYC 7252 CPU + NVIDIA A5000 GPU
  - Dataset: Synthetic (no actual environment needed — models are instantiated with increasing task counts)
  - System: Hyperparameters: d_enc=64, |A|=6, d_model=256, batch size=8 (Figure 3 caption); also d_enc=39, d_model=256, |A|=4 for Appendix B/C1
- **Procedure**:
  1. Instantiate CompoNet and ProgressiveNet models for k=1,2,...,300 tasks (one module per task)
  2. For each k: count total parameters and trainable parameters for both models
  3. Measure inference time by running the model for 1 minute and averaging over at least 40 inferences
  4. Plot total/trainable parameter count (log scale) vs number of tasks
  5. Plot inference time (seconds) vs number of tasks
- **Metrics**: Total parameter count, trainable parameter count, inference time in seconds — all as functions of number of tasks
- **Expected outcome**:
  - CompoNet total and trainable parameter counts follow a linear trend on a log-linear plot
  - ProgressiveNet total and trainable parameter counts follow a quadratic trend (steeper curve on log scale)
  - CompoNet's inference time grows substantially slower than ProgressiveNet's up to 300 tasks
  - ProgressiveNet's inference time shows clear quadratic growth while CompoNet's growth is much more gradual
- **Baselines**: ProgressiveNet
- **Dependencies**: none

## E05: Ablation — Output Attention Head and Input Attention Head Contributions
- **Verifies**: C03
- **Setup**:
  - Model: Original CompoNet vs CompoNet without Output Attention Head; Original CompoNet vs CompoNet without Input Attention Head
  - Hardware: RTX3090 or Nvidia A5000 GPU
  - Dataset: SpaceInvaders (10 modes) and Freeway (7 modes) full sequences; also SpaceInvaders task 5 with 1 informative + 4 non-informative predecessors (input head ablation)
  - System: PPO, standard ALE hyperparameters, Δ=1M timesteps per task, 5 seeds
- **Procedure**:
  1. **Output head ablation**: Remove the output attention head from CompoNet; train on full SpaceInvaders and Freeway sequences; compare episodic return curves task-by-task
  2. **Input head ablation**: Remove the input attention head; train on SpaceInvaders task 5 with 1 informative + 4 non-informative predecessors (output vectors shifted left by one element to prevent direct imitation); compare performance and matching rates to original CompoNet and Baseline
  3. Record episodic return, matching rates, attention weights over training
  4. Aggregate over 5 seeds
- **Metrics**: Episodic return curves per task, matching rate curves (for input head ablation)
- **Expected outcome**:
  - Removing Output Attention Head: minimal impact on SpaceInvaders; significant degradation on Freeway (sparse reward environment where policy reuse is critical)
  - Removing Input Attention Head: ablated CompoNet fails to extract shifted informative signal, performing similarly to Baseline trained from scratch; original CompoNet significantly outperforms both
- **Baselines**: Baseline (trained from scratch), original CompoNet
- **Dependencies**: E03
