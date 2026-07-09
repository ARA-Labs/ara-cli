# Claims

## C01: The proposed QoE metric fully captures user experience in text streaming
- **Statement**: The QoE metric defined as `QoE = 1 - S_delay/S_whole` (Equation 3) captures all four foundational user experience cases: perfect delivery, initial delay, slow streaming, and mid-stream pauses — including cases that TTFT and average TPOT cannot detect (e.g., Figure 5d pause scenario).
- **Status**: supported
- **Falsification criteria**: A scenario exists where QoE = 1 yet the user perceives degraded experience, OR QoE < 1 yet the user perceives perfect experience. Alternatively, TTFT and average TPOT capture all cases that QoE captures.
- **Proof**: [E01]
- **Dependencies**: none
- **Tags**: QoE, metric, user-experience, text-streaming

## C02: Andes improves average QoE by up to 4.7× compared to vLLM under bursty load
- **Statement**: On synthetic cyclic burst traces across four model architectures and three datasets, Andes achieves up to 4.7× higher average QoE than vLLM (FCFS) when burst intensity is varied.
- **Status**: supported
- **Falsification criteria**: On the same hardware/models/traces, Andes achieves ≤1× average QoE improvement relative to vLLM.
- **Proof**: [E02]
- **Dependencies**: C01
- **Tags**: QoE-improvement, burst, synthetic-trace, vLLM-comparison

## C03: Andes saves up to 61% GPU resources while maintaining average QoE ≥ 0.95
- **Statement**: To maintain average QoE ≥ 0.95, Andes requires up to 61% fewer GPUs compared to vLLM, or equivalently handles up to 2.6× more burst intensity with the same GPU count.
- **Status**: supported
- **Falsification criteria**: Andes requires ≥61% of vLLM's GPU resources (no savings) to maintain QoE ≥ 0.95 under the same load patterns.
- **Proof**: [E02]
- **Dependencies**: C01, C02
- **Tags**: resource-efficiency, GPU-savings, burst-intensity

## C04: Andes achieves QoE ≥ 0.95 for 97% of requests on real-world BurstGPT traces
- **Statement**: On a one-hour BurstGPT trace replay, 97% of requests served by Andes achieve QoE ≥ 0.95, compared to only 75% under vLLM. Andes raises average QoE from 0.88 to 0.99 and reduces average TTFT from 10.5s to 1.8s.
- **Status**: supported
- **Falsification criteria**: Fewer than 97% of Andes-served requests achieve QoE ≥ 0.95 on the BurstGPT one-hour trace, or average TTFT is not reduced to 1.8s.
- **Proof**: [E01]
- **Dependencies**: C01, C02
- **Tags**: real-world, BurstGPT, TTFT, QoE-distribution

## C05: The overhead-aware refiner is necessary for high QoE; removing it causes QoE collapse
- **Statement**: Andes without the overhead-aware refiner incurs excessive preemptions as burst duration increases, significantly degrading average QoE compared to Andes with the refiner.
- **Status**: supported
- **Falsification criteria**: Andes without overhead awareness achieves equivalent or higher average QoE compared to Andes with overhead awareness across all burst durations tested.
- **Proof**: [E04]
- **Dependencies**: C02
- **Tags**: ablation, overhead, preemption, refiner

## C06: The greedy knapsack solver achieves near-optimal QoE while being ~20× faster than 3D DP
- **Statement**: The O(N log N) greedy solver achieves slightly better average QoE than the optimal O(MN²) 3D DP solver in real-time settings because its speed enables more frequent scheduling decisions. It runs approximately 20× faster than the DP solver.
- **Status**: supported
- **Falsification criteria**: The greedy solver achieves substantially lower QoE than the 3D DP solver, or the speed advantage is less than 2×.
- **Proof**: [E05]
- **Dependencies**: C02
- **Tags**: algorithm, greedy, dynamic-programming, solver-comparison

## C07: Andes reduces peak queue length by 85% compared to vLLM on BurstGPT
- **Statement**: Under the one-hour BurstGPT trace, Andes reduces peak waiting queue length during load surges by 85% through token-level preemptive scheduling.
- **Status**: supported
- **Falsification criteria**: Peak queue length reduction is less than 85% under the same BurstGPT trace and hardware configuration.
- **Proof**: [E01]
- **Dependencies**: C04
- **Tags**: queue-length, head-of-line-blocking, BurstGPT

## C08: Andes's QoE improvement is robust across model architectures, datasets, and arrival distributions
- **Statement**: Andes consistently outperforms vLLM, Sarathi-Serve, and LQSF in average QoE across Dense/MoE architectures, MHA/GQA attention, three input/output datasets, varying burst intensities, burst durations, and Poisson arrival patterns.
- **Status**: supported
- **Falsification criteria**: There exists a tested model/dataset/arrival-pattern combination where Andes does not outperform all baselines in average QoE.
- **Proof**: [E02, E03, E06]
- **Dependencies**: C02, C03
- **Tags**: robustness, generalization, model-diversity, dataset-diversity
