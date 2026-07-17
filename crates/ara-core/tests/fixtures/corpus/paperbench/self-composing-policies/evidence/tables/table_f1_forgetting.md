# Table F.1: Forgetting of Baseline and FT-1 Methods

- **Source**: Table F.1, Appendix F.2
- **Caption**: "Forgetting of the baseline and FT-1 methods in all of the considered task sequences. Note that the rest of the methods are omitted as their forgetting is zero under the assumptions from Section 3. Results from 10 random seeds are aggregated."
- **Conditions**: F_i = p_i(i·Δ) - p_i(T); positive values indicate forgetting; zero or negative indicate no forgetting or backward transfer.

## (a) Meta-World

| METHOD | TASK 0 | TASK 1 | TASK 2 | TASK 3 | TASK 4 | TASK 5 | TASK 6 | TASK 7 | TASK 8 | TASK 9 | AVG. |
|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|------|
| BASELINE | 0.02±0.30 | 0.15±0.00 | 0.89±0.27 | 0.29±0.00 | 0.00±0.00 | 0.97±0.17 | 0.10±0.00 | 0.00±0.00 | 0.98±0.00 | 0.01±0.49 | 0.34±0.46 |
| FT-1 | 0.12±0.00 | 0.19±0.00 | 0.99±0.00 | 0.45±0.00 | 0.00±0.00 | 0.95±0.22 | 0.02±0.00 | 0.00±0.00 | 0.88±0.10 | 0.17±0.45 | 0.38±0.42 |

## (b) SpaceInvaders

| METHOD | TASK 0 | TASK 1 | TASK 2 | TASK 3 | TASK 4 | TASK 5 | TASK 6 | TASK 7 | TASK 8 | TASK 9 | AVG. |
|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|------|
| BASELINE | 0.66±0.47 | 0.85±0.34 | 0.81±0.39 | 0.85±0.36 | 0.99±0.10 | 0.99±0.10 | 1.00±0.00 | 1.00±0.00 | 0.91±0.26 | 0.74±0.44 | 0.88±0.32 |
| FT-1 | 0.58±0.49 | 0.57±0.49 | 0.19±0.39 | 0.32±0.47 | 0.56±0.50 | 0.61±0.49 | 0.60±0.49 | 0.67±0.47 | 0.67±0.47 | 0.70±0.46 | 0.36±0.48 |

## (c) Freeway

| METHOD | TASK 0 | TASK 1 | TASK 2 | TASK 3 | TASK 4 | TASK 5 | TASK 6 | TASK 7 | AVG. |
|--------|--------|--------|--------|--------|--------|--------|--------|--------|------|
| BASELINE | 0.02±0.44 | 0.87±0.00 | 0.49±0.35 | 1.00±0.00 | 0.51±0.44 | 0.80±0.24 | 0.70±0.24 | 0.60±0.43 | 0.62±0.42 |
| FT-1 | 0.49±0.45 | 0.75±0.33 | 0.62±0.11 | 0.88±0.33 | 0.54±0.42 | 0.69±0.37 | 0.62±0.34 | 0.72±0.33 | 0.58±0.43 |
