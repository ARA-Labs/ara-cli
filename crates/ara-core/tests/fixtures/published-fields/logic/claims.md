# Claims

## C01: Every ARA is a ready-to-use RL training environment
- **Statement**: Each artifact contains exactly what reinforcement learning requires — a task (logic/experiments.md) and a reward (verifiable outcomes from evidence/ plus preference signals logged in the trajectory).
- **Status**: hypothesis
- **Dependencies**: []

## C02: Existing benchmark infrastructure beats bespoke ground truth
- **Statement**: Reusing PaperBench's expert-authored rubrics as evaluation ground truth removes weeks of biased rubric authorship and anchors ARA's eval to a recognized benchmark.
- **Status**: supported
- **Dependencies**: [C01]
