---
title: "Self-Composing Policies for Scalable Continual Reinforcement Learning"
authors: ["Mikel Malagón", "Josu Ceberio", "Jose A. Lozano"]
year: 2024
venue: "ICML 2024 (Proceedings of the 41st International Conference on Machine Learning, PMLR 235)"
doi: "arXiv:2506.14811v1"
ara_version: "1.0"
domain: "Continual Reinforcement Learning, Modular Neural Networks"
keywords:
  - continual reinforcement learning
  - catastrophic forgetting
  - growing neural networks
  - policy composition
  - attention mechanism
  - knowledge transfer
  - modular architecture
  - forward transfer
  - Meta-World
  - Arcade Learning Environment
claims_summary:
  - "CompoNet grows linearly O(n) in total parameters with respect to number of tasks, vs quadratic O(n²) for ProgressiveNet"
  - "CompoNet achieves superior average performance and forward transfer compared to all baselines (Baseline, FT-1, FT-N, ProgressiveNet, PackNet) across all three task sequences"
  - "CompoNet correctly handles all three CRL scenarios: reusing a previous policy, composing previous policies, and learning from scratch without interference"
  - "CompoNet's empirical inference time grows substantially slower than ProgressiveNet in practice, despite theoretical O(n²) complexity"
abstract: "This work introduces a growable and modular neural network architecture that naturally avoids catastrophic forgetting and interference in continual reinforcement learning. The structure of each module allows the selective combination of previous policies along with its internal policy, accelerating the learning process on the current task. Unlike previous growing neural network approaches, we show that the number of parameters of the proposed approach grows linearly with respect to the number of tasks, and does not sacrifice plasticity to scale. Experiments conducted in benchmark continuous control and visual problems reveal that the proposed approach achieves greater knowledge transfer and performance than alternative methods."
---

# Self-Composing Policies for Scalable Continual Reinforcement Learning

## Overview
CompoNet is a growable, modular neural network architecture for Continual Reinforcement Learning (CRL) that avoids catastrophic forgetting by freezing previously trained modules and adding a new self-composing module per task. Each module contains three blocks — an Output Attention Head, an Input Attention Head, and an Internal Policy — that together allow selective combination of all previous policies' outputs alongside the current state, enabling knowledge reuse, composition, or independent learning depending on task relatedness.

Unlike prior growing approaches (e.g., ProgressiveNet, which connects hidden layers laterally and grows quadratically), CompoNet composes only the output vectors of previous modules. This design yields linear parameter growth O(n) per task, superior empirical scalability in inference time, and competitive or superior performance across three benchmark task sequences: 20 Meta-World robotic manipulation tasks (SAC), 10 SpaceInvaders game modes (PPO), and 7 Freeway game modes (PPO).

## Layer Index

### Cognitive Layer (`/logic`)
| File | Description |
|------|-------------|
| [problem.md](logic/problem.md) | Observations → gaps → key insight motivating CompoNet |
| [claims.md](logic/claims.md) | 5 falsifiable claims (C01–C05) |
| [concepts.md](logic/concepts.md) | 12 key terms with formal definitions |
| [experiments.md](logic/experiments.md) | 5 verification plans (E01–E05) |
| [solution/architecture.md](logic/solution/architecture.md) | System design: CompoNet cascading module graph |
| [solution/algorithm.md](logic/solution/algorithm.md) | Self-composing attention mechanism, O(n²) inference complexity |
| [solution/constraints.md](logic/solution/constraints.md) | Boundary conditions: shared action space, known task boundaries |
| [solution/heuristics.md](logic/solution/heuristics.md) | 5 convergence tricks |
| [related_work.md](logic/related_work.md) | 9 typed dependencies |

### Physical Layer (`/src`)
| File | Description | Claims |
|------|-------------|--------|
| [execution/componet.py](src/execution/componet.py) | CompoNet module: output head, input head, internal policy | C01, C02, C03 |
| [execution/encoder.py](src/execution/encoder.py) | CNN encoder for ALE visual observations | C02, C03 |
| [execution/metrics.py](src/execution/metrics.py) | CRL metrics: average performance, forward transfer, RT | C02 |
| [configs/training.md](src/configs/training.md) | SAC and PPO hyperparameters with rationale | — |
| [configs/model.md](src/configs/model.md) | Model architecture configurations | — |
| [environment.md](src/environment.md) | Hardware, deps, seeds | — |

### Exploration Graph (`/trace`)
| File | Description |
|------|-------------|
| [exploration_tree.yaml](trace/exploration_tree.yaml) | 12-node research DAG (nested YAML) |

### Evidence (`/evidence`)
| File | Description |
|------|-------------|
| [README.md](evidence/README.md) | Full index of 6 tables + 3 figures |
| [tables/table1_main_results.md](evidence/tables/table1_main_results.md) | Main benchmark results across all methods and sequences |
| [tables/table_d1a_spaceinvaders_success.md](evidence/tables/table_d1a_spaceinvaders_success.md) | Success score thresholds for SpaceInvaders tasks |
| [tables/table_d1b_freeway_success.md](evidence/tables/table_d1b_freeway_success.md) | Success score thresholds for Freeway tasks |
| [tables/table_f1a_forgetting_metaworld.md](evidence/tables/table_f1a_forgetting_metaworld.md) | Forgetting values for Meta-World (baseline and FT-1) |
| [tables/table_f1b_forgetting_spaceinvaders.md](evidence/tables/table_f1b_forgetting_spaceinvaders.md) | Forgetting values for SpaceInvaders |
| [tables/table_f1c_forgetting_freeway.md](evidence/tables/table_f1c_forgetting_freeway.md) | Forgetting values for Freeway |
| [figures/figure3_scalability.md](evidence/figures/figure3_scalability.md) | Inference time and parameter count vs number of tasks |
| [figures/figure_d2_ftr_matrices.md](evidence/figures/figure_d2_ftr_matrices.md) | Forward transfer matrices for all three sequences |
| [figures/figure4_architectural_validation.md](evidence/figures/figure4_architectural_validation.md) | Attention scores and matching rates in scenarios i and iii |
