# Problem Specification

## Observations

### O1: Neural Networks Suffer Catastrophic Forgetting in Sequential Task Learning
- **Statement**: When a single NN is fine-tuned sequentially on new tasks, updating weights for a new task overwrites parameters relevant to previously solved tasks, causing performance on earlier tasks to drop to near zero.
- **Evidence**: Established in McCloskey & Cohen (1989); confirmed in the paper's Table F.1 where Baseline average forgetting on Meta-World is 0.34±0.46 and on SpaceInvaders is 0.88±0.32.
- **Implication**: Naive sequential fine-tuning cannot retain knowledge from past tasks.

### O2: Growing Neural Networks (e.g., ProgressiveNet) Grow Quadratically in Parameters
- **Statement**: ProgressiveNet (Rusu et al., 2016) adds lateral connections between hidden layers of all previous modules and a new module, resulting in O(n²) parameter growth with respect to the number of tasks n.
- **Evidence**: Figure B.1 and Figure 3 in the paper show ProgressiveNet's parameter count growing quadratically on a log scale, while CompoNet's grows linearly. Appendix B states ProgressiveNet parameter complexity explicitly.
- **Implication**: Quadratic growth severely limits scalability to long task sequences.

### O3: Compositional NN Approaches Require a Dedicated Composer Network
- **Statement**: Prior neural composition methods (Rosenbaum et al., 2019; Khetarpal et al., 2022) require a separate NN to learn the composing strategy, creating a non-stationary training problem since the composer depends on optimizing the composed modules and vice versa.
- **Evidence**: Section 2; Rosenbaum et al. (2019) explicitly identified this instability.
- **Implication**: Joint training of composer + modules is unstable and makes the learning process difficult.

### O4: Methods Avoiding Forgetting via Plasticity Reduction Create a Dilemma
- **Statement**: Approaches like EWC (Kirkpatrick et al., 2017) and PackNet (Mallya & Lazebnik, 2018) overcome forgetting by restricting which parameters can be updated, introducing a stability-plasticity dilemma where preventing forgetting limits the ability to learn new tasks.
- **Evidence**: Section 2; PackNet results in Table 1 show Performance=0.24±0.40 on Meta-World despite having a fixed network capacity.
- **Implication**: Reducing plasticity trades the forgetting problem for a learning capacity problem.

### O5: Knowledge Transfer Between Related Tasks Can Accelerate Learning
- **Statement**: Fine-tuning a policy trained on task j then applied to a related task i can achieve significantly higher area-under-the-curve performance compared to training from scratch, as measured by the RT metric.
- **Evidence**: Forward transfer matrices in Figure D.2 show up to 0.97 FTr(j,i) values for SpaceInvaders; RT=0.70 for SpaceInvaders and RT=0.67 for Freeway.
- **Implication**: Directly composing learned policies (output-level) is a promising alternative to hidden-layer sharing.

## Gaps

### G1: No Growing NN Achieves Both Linear Scaling and Full Plasticity
- **Statement**: Existing growing NN methods either grow quadratically (ProgressiveNet) or sacrifice plasticity to reduce growth (PackNet, Hung et al. 2019 with pruning).
- **Caused by**: O2, O4
- **Existing attempts**: ProgressiveNet uses lateral hidden connections (O(n²)); DEN/CPG (Yoon et al., 2018; Hung et al., 2019) prune and selectively retrain but introduce plasticity–memory tradeoffs.
- **Why they fail**: Sharing hidden layer representations inherently couples module size to all previous modules' hidden sizes.

### G2: Policy-Level Composition Without a Dedicated Composer Is Unsolved
- **Statement**: No prior CRL method composes learned policies at the output level without requiring a separately trained router/composer network.
- **Caused by**: O3
- **Existing attempts**: Rosenbaum et al. (2018, 2019) routing networks; Mendez & Eaton (2022) modular lifelong RL — all require jointly learned composition strategies.
- **Why they fail**: Joint optimization of composer and composed modules is non-stationary and unstable.

## Key Insight

- **Insight**: If a new module has direct access to the output probability vectors of all previous modules (rather than their hidden activations), it can use attention mechanisms conditioned on the current state to (a) selectively imitate the best previous policy, (b) learn a function over previous policies, or (c) ignore all previous outputs and learn from scratch — all without a dedicated composer and with only a constant-sized module per task.
- **Derived from**: O1, O2, O3, O5
- **Enables**: Self-composing modules where each module autonomously decides how to combine previous policy outputs with its own internal policy, yielding linear parameter growth and eliminating the need for a separate composer.

## Assumptions

- A1: The action space A remains constant across all tasks (soft assumption — tasks with disjoint action sets can be merged by zeroing irrelevant action probabilities).
- A2: Task transition boundaries and task identifiers are known to the agent at runtime.
- A3: Tasks share a similar state space S^(i) ≈ S^(j); large domain shifts require separate encoders or foundational vision models.
- A4: For image-based tasks, low-resolution images allow per-module CNN encoders; the encoder for a new module is initialized from the previous module's encoder weights.
