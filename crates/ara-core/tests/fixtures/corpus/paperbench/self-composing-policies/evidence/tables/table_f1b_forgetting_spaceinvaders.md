# Table F.1b: Forgetting on SpaceInvaders Sequence
- **Source**: Table F.1b, Appendix F.2
- **Caption**: "Forgetting of the baseline and FT-1 methods in all of the considered task sequences."
- **Conditions**: F_i = p_i(i·Δ) - p_i(T); 10-mode SpaceInvaders sequence (ALE/SpaceInvaders-v5); PPO; Δ=1M timesteps per task; 10 seeds

| METHOD | TASK 0 | TASK 1 | TASK 2 | TASK 3 | TASK 4 | TASK 5 | TASK 6 | TASK 7 | TASK 8 | TASK 9 | AVG. |
|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|------|
| BASELINE | 0.66±0.47 | 0.85±0.34 | 0.81±0.39 | 0.85±0.36 | 0.99±0.10 | 0.99±0.10 | 1.00±0.00 | 1.00±0.00 | 0.91±0.26 | 0.74±0.44 | 0.88±0.32 |
| FT-1 | 0.58±0.49 | 0.57±0.49 | 0.19±0.39 | 0.32±0.47 | 0.56±0.50 | 0.61±0.49 | 0.60±0.49 | 0.67±0.47 | 0.67±0.47 | 0.70±0.46 | 0.36±0.48 |
