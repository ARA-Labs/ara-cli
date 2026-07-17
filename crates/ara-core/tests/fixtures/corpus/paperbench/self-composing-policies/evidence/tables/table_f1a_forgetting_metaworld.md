# Table F.1a: Forgetting on Meta-World Sequence
- **Source**: Table F.1a, Appendix F.2
- **Caption**: "Forgetting of the baseline and FT-1 methods in all of the considered task sequences. Note that the rest of the methods are omitted as their forgetting is zero under the assumptions from Section 3. Results from 10 random seeds are aggregated."
- **Conditions**: F_i = p_i(i·Δ) - p_i(T); 20-task Meta-World CW20 sequence; SAC; Δ=1M timesteps per task; 10 seeds

| METHOD | TASK 0 | TASK 1 | TASK 2 | TASK 3 | TASK 4 | TASK 5 | TASK 6 | TASK 7 | TASK 8 | TASK 9 | AVG. |
|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|------|
| BASELINE | 0.02±0.30 | 0.15±0.00 | 0.89±0.27 | 0.29±0.00 | 0.00±0.00 | 0.97±0.17 | 0.10±0.00 | 0.00±0.00 | 0.98±0.00 | 0.01±0.49 | 0.34±0.46 |
| FT-1 | 0.12±0.00 | 0.19±0.00 | 0.99±0.00 | 0.45±0.00 | 0.00±0.00 | 0.95±0.22 | 0.02±0.00 | 0.00±0.00 | 0.88±0.10 | 0.17±0.45 | 0.38±0.42 |
