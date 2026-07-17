# Training Hyperparameters

## SAC (Meta-World Sequence)

### Optimizer
- **Value**: Adam
- **Rationale**: Standard optimizer for SAC; Adam's adaptive learning rates suit the non-stationary CRL training distribution.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Adam β1 and β2
- **Value**: β1=0.9, β2=0.999
- **Rationale**: Standard Adam defaults
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Discount Rate (γ)
- **Value**: 0.99
- **Rationale**: Standard for dense-reward robotic tasks; promotes long-horizon planning.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1

### Maximum Standard Deviation (Policy)
- **Value**: exp(2)
- **Rationale**: Upper bound for log-std of Gaussian policy; prevents overly diffuse action distributions.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Minimum Standard Deviation (Policy)
- **Value**: exp(-20)
- **Rationale**: Lower bound for log-std; prevents degenerate near-deterministic policies early in training.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Activation Function
- **Value**: ReLU
- **Rationale**: Standard non-linearity for MLP policies in SAC.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Hidden Dimension (d_model, SAC)
- **Value**: 256
- **Rationale**: Provides sufficient model capacity for 39-dimensional Meta-World state space.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1; Appendix B (d_model=256 for Meta-World)

### Batch Size (SAC)
- **Value**: 128
- **Rationale**: Standard SAC batch size balancing gradient noise and memory usage.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1 (from rubric cross-reference)

### Buffer Size
- **Value**: 10^6 (1,000,000)
- **Rationale**: Large replay buffer reduces correlation in sampled transitions; reset at each task boundary in standard CRL practice.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1

### Target Smoothing Coefficient (τ)
- **Value**: 0.005
- **Rationale**: Slow target network update stabilizes Q-value bootstrapping.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1

### Entropy Regularization Coefficient (α)
- **Value**: 0.2 (initial value; auto-tuned)
- **Rationale**: Controls exploration-exploitation tradeoff; auto-tuning adjusts α to match a target entropy.
- **Search range**: Not specified in paper
- **Sensitivity**: high
- **Source**: Table E.1

### Auto-Tuning of α
- **Value**: YES
- **Rationale**: Automatic entropy tuning removes the need to hand-tune α per task.
- **Search range**: N/A
- **Sensitivity**: medium
- **Source**: Table E.1

### Policy Update Frequency
- **Value**: 2 (every 2 environment steps)
- **Rationale**: Delays actor update relative to critic to stabilize training.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1

### Target Network Update Frequency
- **Value**: 1 (every environment step)
- **Rationale**: Frequent soft target updates with small τ provide stable value estimates.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Noise Clip
- **Value**: 0.5
- **Rationale**: Clips target policy noise to prevent excessively large perturbations.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Number of Random Actions (Warm-Up)
- **Value**: 10^4 (10,000)
- **Rationale**: Pure random exploration before learning begins; fills replay buffer with diverse transitions.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1

### Timestep to Start Learning
- **Value**: 5×10^3 (5,000)
- **Rationale**: Begin gradient updates after minimal replay buffer fill.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.1

### Target Network Layers
- **Value**: 3
- **Rationale**: 2-layer MLP + 2 output heads architecture for the target Q-network.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Critic Network Layers
- **Value**: 3
- **Rationale**: 2-layer MLP + 2 output heads (mean and log-std).
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.1

### Actor Learning Rate (SAC)
- **Value**: 10^-3
- **Rationale**: Standard learning rate for SAC actor.
- **Search range**: Not specified in paper
- **Sensitivity**: high
- **Source**: Table E.1

### Q-Network Learning Rate
- **Value**: 10^-3
- **Rationale**: Standard learning rate for SAC critic.
- **Search range**: Not specified in paper
- **Sensitivity**: high
- **Source**: Table E.1

---

## PPO (SpaceInvaders and Freeway Sequences)

### Optimizer (PPO)
- **Value**: AdamW
- **Rationale**: Weight decay regularization helps prevent overfitting in on-policy discrete control.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.2

### AdamW β1 and β2
- **Value**: β1=0.9, β2=0.999
- **Rationale**: Standard Adam defaults
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.2

### Maximum Gradient Norm
- **Value**: 0.5
- **Rationale**: Gradient clipping prevents exploding gradients in PPO's on-policy updates.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### Discount Rate (γ, PPO)
- **Value**: 0.99
- **Rationale**: Standard for Atari-style tasks.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### Activation Function (PPO)
- **Value**: ReLU
- **Rationale**: Standard activation for PPO networks.
- **Search range**: Not specified in paper
- **Sensitivity**: low
- **Source**: Table E.2

### Hidden Dimension (d_model, PPO)
- **Value**: 512
- **Rationale**: Larger hidden dimension accommodates the richer 512-dimensional CNN-encoded state.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2; Appendix E.1

### Learning Rate (PPO)
- **Value**: 2.5×10^-4
- **Rationale**: Standard PPO learning rate for Atari-scale tasks (from CleanRL reference implementation).
- **Search range**: Not specified in paper
- **Sensitivity**: high
- **Source**: Table E.2

### PPO Value Function Coefficient
- **Value**: 0.5
- **Rationale**: Balances value loss relative to policy loss in the combined PPO objective.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### GAE λ
- **Value**: 0.95
- **Rationale**: Standard GAE parameter providing a good bias-variance tradeoff in advantage estimation.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### Number of Parallel Environments
- **Value**: 8
- **Rationale**: Parallelism increases sample diversity and training throughput.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2 (from rubric)

### Batch Size (PPO)
- **Value**: 1024
- **Rationale**: Total number of transitions per update step (num_envs × num_steps = 8 × 128 = 1024).
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### Mini-Batch Size
- **Value**: Not specified in paper
- **Rationale**: N/A
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2 (value not provided in available paper text)

### Number of Mini-Batches
- **Value**: Not specified in paper
- **Rationale**: N/A
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2 (value not provided in available paper text)

### Update Epochs
- **Value**: 4
- **Rationale**: Multiple passes over collected rollout data improve sample efficiency.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### PPO Clipping Coefficient
- **Value**: 0.2
- **Rationale**: Standard PPO clip range; prevents large policy updates per iteration.
- **Search range**: Not specified in paper
- **Sensitivity**: high
- **Source**: Table E.2

### PPO Entropy Coefficient
- **Value**: 0.01
- **Rationale**: Small entropy bonus encourages exploration without dominating the policy objective.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2

### Learning Rate Annealing
- **Value**: YES
- **Rationale**: Linearly decay learning rate over training to reduce variance in later stages.
- **Search range**: N/A
- **Sensitivity**: medium
- **Source**: Table E.2

### Clip Value Loss
- **Value**: YES
- **Rationale**: Stabilizes value function training by bounding value updates.
- **Search range**: N/A
- **Sensitivity**: low
- **Source**: Table E.2

### Normalize Advantage
- **Value**: YES
- **Rationale**: Normalizing advantages across mini-batch reduces gradient variance.
- **Search range**: N/A
- **Sensitivity**: low
- **Source**: Table E.2

### Number of Steps per Rollout
- **Value**: 128
- **Rationale**: Balances rollout length vs update frequency; total batch = 8 envs × 128 steps = 1024.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Table E.2 (from rubric)

### PackNet Retrain Steps
- **Value**: 200,000 (20% of Δ=1M)
- **Rationale**: Sufficient retraining steps after pruning to recover task performance before freezing parameters.
- **Search range**: Not specified in paper
- **Sensitivity**: medium
- **Source**: Appendix E.2
