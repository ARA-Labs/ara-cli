# Constraints and Limitations

## Boundary Conditions

### BC1: Constant Action Space Required
- **Condition**: The action space $A$ must remain constant across all tasks in the sequence.
- **Rationale**: The $\Phi^{k;s}$ matrix has fixed column dimension $|A|$; all attention weight matrices are of size $|A| \times d_{model}$.
- **Relaxation**: Soft — tasks with different action subsets can be unified by setting probabilities of irrelevant actions to zero (masking).

### BC2: Known Task Boundaries and Identifiers
- **Condition**: The agent must know when one task ends and the next begins, and must be able to identify which task it is currently in.
- **Rationale**: Module freezing and new module instantiation are triggered at task boundaries. Critic reset also requires this knowledge.
- **Impact**: This is a standard assumption in the CRL literature (Wolczyk et al., 2021, 2022; Khetarpal et al., 2022). Removing it would require automatic task detection — noted as future work.

### BC3: Similar State Spaces Across Tasks
- **Condition**: $S^{(i)} \approx S^{(j)}$ for all task pairs; large domain shifts violate the assumption.
- **Rationale**: Encoders are frozen per module; if state distributions shift dramatically, a frozen encoder produces non-stationary inputs to later modules.
- **Relaxation**: Foundational models (e.g., DINOv2) may handle wider distributions; preliminary results in Appendix A show feasibility.

### BC4: Fixed Hyperparameters $d_{enc}$, $d_{model}$, $|A|$ Throughout Training
- **Condition**: Architecture parameters $d_{enc}$, $d_{model}$, $|A|$ must be set before training begins and cannot change.
- **Rationale**: All weight matrices depend on these dimensions; changing them would invalidate frozen module compatibility.

### BC5: Output-Level Composition Only
- **Condition**: CompoNet composes at the policy output level (action distributions), not at intermediate hidden representations.
- **Consequence**: Cannot reuse partial feature abstractions learned in previous tasks; entire compositional signal comes from final action probabilities.

## Known Limitations

### L1: Theoretical $O(n^2)$ Inference Complexity
- Sequential module evaluation is required because module $k$ depends on module $k-1$'s output. Even with parallelism within each module, total complexity is $O(n^2)$.
- **Mitigation**: Empirical performance shows much slower growth than theoretical bound up to 300 tasks; quantization and policy distillation are suggested future directions.

### L2: Memory Still Grows Linearly
- Although linear growth is a major improvement over quadratic, indefinitely long task sequences will eventually exhaust memory.
- **Practical limit**: With $d_{model}=256$ and $d_{enc}=39$, CompoNet can grow to more than 10,000 modules within 24GB VRAM (Figure B.2).

### L3: No Backward Transfer
- Frozen parameters prevent any module from benefiting from knowledge acquired in later tasks. Methods like FT-1 can in principle achieve negative forgetting (backward transfer), which CompoNet cannot.

### L4: Requires Actor-Critic Separation
- CRL is applied only to the actor (CompoNet); the critic is reset at each task boundary. This may limit critic stability in practice.

### L5: Non-Deterministic Module Composition Count
- The $\Phi^{k;s}$ matrix grows with $k$, making the input to attention heads dynamically sized. Implementation must handle variable-length sequences in the attention computation.
