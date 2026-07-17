# Table 1: Main Results Across All Task Sequences and Methods
- **Source**: Table 1, Section 5.3
- **Caption**: "Summary of results across all task sequences and methods. Metrics from Section 5.1 are presented as averages and standard deviations from 10 random seeds, with the best results highlighted in bold. The last row indicates the reference forward transfer (RT) for each sequence. CompoNet, the method proposed in this paper, achieves superior performance and forward transfer in all three sequences."
- **Conditions**: 10 random seeds per method per sequence; Δ=1M timesteps per task; SAC for Meta-World (20 tasks), PPO for SpaceInvaders (10 tasks) and Freeway (7 tasks)

| METHOD | META-WORLD PERF. | META-WORLD FWD. TRANSF. | SPACEINVADERS PERF. | SPACEINVADERS FWD. TRANSF. | FREEWAY PERF. | FREEWAY FWD. TRANSF. |
|--------|-----------------|------------------------|--------------------|-----------------------------|--------------|---------------------|
| BASELINE | 0.06±0.12 | 0.00±0.00 | 0.56±0.37 | 0.00±0.00 | 0.19±0.26 | 0.00±0.00 |
| FT-1 | 0.03±0.09 | -0.21±0.38 | 0.44±0.50 | 0.73±0.25 | 0.15±0.36 | 0.64±0.09 |
| FT-N | 0.37±0.48 | -0.21±0.38 | 0.99±0.01 | 0.73±0.25 | 0.81±0.01 | 0.64±0.09 |
| PROGNET | 0.41±0.49 | -0.04±0.04 | 0.71±0.25 | 0.10±0.07 | 0.47±0.28 | 0.30±0.18 |
| PACKNET | 0.24±0.40 | -0.67±1.38 | 0.63±0.33 | 0.36±0.31 | 0.51±0.20 | 0.31±0.25 |
| COMPONET | 0.42±0.49 | 0.01±0.14 | 0.99±0.01 | 0.74±0.22 | 0.94±0.06 | 0.80±0.07 |
| RT (reference) | — | -0.06 | — | 0.70 | — | 0.67 |
