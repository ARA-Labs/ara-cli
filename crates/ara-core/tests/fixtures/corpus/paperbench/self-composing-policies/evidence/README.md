# Evidence Index

## Tables
| File | Source | Claims | Description |
|------|--------|--------|-------------|
| [tables/table1_main_results.md](tables/table1_main_results.md) | Table 1, §5.3 | C02, C05 | Main benchmark results — average performance and forward transfer for all 6 methods across all 3 task sequences, showing CompoNet achieves best results in every column. |
| [tables/table_f1_forgetting.md](tables/table_f1_forgetting.md) | Table F.1, Appendix F.2 | C03 | Forgetting of Baseline and FT-1 methods across all three task sequences (Meta-World, SpaceInvaders, Freeway). |
| [tables/table_f1a_forgetting_metaworld.md](tables/table_f1a_forgetting_metaworld.md) | Table F.1a, Appendix F.2 | C03 | Forgetting values (mean ± std, 10 seeds) for Baseline and FT-1 on the Meta-World sequence, showing both methods suffer significant catastrophic forgetting. |
| [tables/table_f1b_forgetting_spaceinvaders.md](tables/table_f1b_forgetting_spaceinvaders.md) | Table F.1b, Appendix F.2 | C03 | Forgetting values for Baseline and FT-1 on the SpaceInvaders sequence — Baseline shows near-complete forgetting on later tasks. |
| [tables/table_f1c_forgetting_freeway.md](tables/table_f1c_forgetting_freeway.md) | Table F.1c, Appendix F.2 | C03 | Forgetting values for Baseline and FT-1 on the Freeway sequence, confirming that only non-growing methods experience forgetting. |

## Figures
| File | Source | Claims | Description |
|------|--------|--------|-------------|
| [figures/fig3_scalability.md](figures/fig3_scalability.md) | Figure 3, §4.3 + Appendix C.2 | C01, C04 | Empirical inference time (left) and parameter count (right, log scale) vs number of tasks for CompoNet and ProgressiveNet, demonstrating linear vs quadratic growth. |
| [figures/fig4_arch_validation.md](figures/fig4_arch_validation.md) | Figure 4, §5.4 | C03 | Episodic return, matching rates, input attention weights, and output attention weights during scenario (i) and (iii) validation experiments on SpaceInvaders tasks 5 and 6. |
| [figures/figure_d2_ftr_matrices.md](figures/figure_d2_ftr_matrices.md) | Figure D.2, Appendix D.5 | C05 | Forward transfer matrices for all three sequences showing per-task pairwise FTr(j,i) values, used to compute RT values reported in Table 1. |
| [figures/figb1_memory_growth.md](figures/figb1_memory_growth.md) | Figure B.1, Appendix B | C01, C04 | Memory growth (parameter count) for CompoNet vs ProgressiveNet as number of tasks increases, confirming linear vs quadratic scaling. |
