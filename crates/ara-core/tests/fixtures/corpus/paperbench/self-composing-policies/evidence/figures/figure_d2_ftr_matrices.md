# Figure D.2: Forward Transfer Matrices for All Three Sequences
- **Source**: Figure D.2, Appendix D.5
- **Caption**: "Forward transfer matrices for all sequences. Each element in the matrices is computed as the average forward transfer of training a model from scratch in the first task (Y-axis) and fine-tuning it in the second (X-axis). Results aggregate values from 3 different random seeds. Note that Figure D.2a is a 10×10 matrix and not 20×20, corresponding with the 10 different tasks that comprise the sequence, as the remaining 10 are repetitions of these."
- **Conditions**: FTr(j,i) computed by training from scratch on task j, fine-tuning on task i; 3 random seeds; values used to compute RT in Table 1.

## (a) Meta-World Forward Transfer Matrix (10×10)
RT = -0.06 (negative, indicating high interference in this sequence)

| First\Second | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|-------------|---|---|---|---|---|---|---|---|---|---|
| 0 | -0.13 | 0.09 | -4.2 | 0.06 | 0.03 | -9.8 | -0.06 | 0.08 | -4.7 | -0.61 |
| 1 | -0.21 | -0.08 | -4.9 | -0.03 | 0.03 | -9.7 | -0.08 | 0.08 | -5.1 | -0.71 |
| 2 | 0.8 | 0.8 | -0.06 | 0.79 | 0.82 | -1.1 | 0.77 | 0.84 | -0.06 | 0.71 |
| 3 | -0.22 | -0.03 | -4.6 | -0.03 | 0.05 | -10 | 0.01 | (missing) | -5.1 | -0.55 |
| 4 | -0.22 | -0.09 | -4.9 | -0.03 | (diag) | -10 | -0.08 | (missing) | -5.1 | -0.72 |
| 5 | 0.86 | 0.88 | 0.28 | 0.89 | 0.88 | -0.23 | 0.89 | 0.86 | 0.47 | 0.85 |
| 6 | -0.21 | -0.08 | -4.8 | -0.02 | 0.01 | -9.9 | -0.06 | 0.02 | -4.9 | -0.7 |
| 7 | -0.22 | -0.09 | -4.9 | -0.03 | (missing) | -10 | -0.08 | (diag) | -5.1 | -0.72 |
| 8 | 0.73 | 0.76 | -0.02 | 0.63 | 0.75 | (missing) | 0.82 | 0.81 | -0.33 | 0.51 |
| 9 | 0.26 | 0.26 | -4.9 | 0.17 | 0.62 | (missing) | 0.16 | 0.43 | -2.8 | -0.35 |

## (b) SpaceInvaders Forward Transfer Matrix (10×10)
RT = 0.70

| First\Second | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|-------------|---|---|---|---|---|---|---|---|---|---|
| 0 | 0.93 | 0.63 | 0.71 | 0.75 | 0.96 | 0.9 | 0.86 | 0.81 | 0.25 | 0.78 |
| 1 | 0.8 | 0.4 | 0.49 | 0.95 | 0.87 | 0.79 | 0.97 | -0.04 | 0.22 | (missing) |
| 2 | 0.36 | 0.32 | 0.94 | 0.67 | 0.87 | 0.71 | 0.9 | 0.74 | 0.52 | 0.57 |
| 3 | -0.43 | 0.3 | 0.26 | 0.68 | 0.53 | 0.52 | 0.59 | 0.76 | -0.09 | 0.38 |
| 4 | -1.3 | -0.16 | 0.05 | 0.83 | 0.39 | 0.42 | 0.16 | -0.15 | -0.03 | (missing) |
| 5 | -2.3 | -0.94 | -0.81 | -0.3 | 0.42 | 0.63 | 0.12 | 0.38 | -0.3 | -0.1 |
| 6 | -3.2 | -1.1 | -0.26 | -0.15 | 0.03 | 0.05 | 0.42 | 0.04 | -0.27 | -0.1 |
| 7 | -4.3 | -1.4 | -0.92 | -0.37 | -0.44 | -0.09 | 0.04 | 0.32 | -0.3 | -0.12 |
| 8 | -0.04 | 0.45 | 0.69 | 0.73 | 0.86 | 0.89 | 0.81 | 0.68 | 0.69 | 0.71 |
| 9 | -0.73 | -0.01 | 0.22 | 0.48 | 0.59 | 0.74 | 0.47 | 0.53 | 0.14 | 0.63 |

## (c) Freeway Forward Transfer Matrix (7×7 shown as 8×8 in paper, 7 tasks)
RT = 0.67

| First\Second | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|-------------|---|---|---|---|---|---|---|---|
| 0 | 0.06 | 0.87 | 0.92 | 0.76 | 0.19 | 0.92 | 0.92 | 0.81 |
| 1 | -3.3 | 0.87 | 0.61 | 0.32 | -5.2 | 0.82 | 0.83 | 0.51 |
| 2 | -15 | -0.53 | 0.27 | -1.9 | -12 | 0.53 | 0.63 | -0.21 |
| 3 | -0.17 | 0.87 | 0.92 | 0.76 | -0.13 | 0.92 | 0.92 | 0.79 |
| 4 | 0.06 | 0.87 | 0.92 | 0.76 | 0.19 | 0.92 | 0.92 | 0.81 |
| 5 | -18 | -0.55 | 0.63 | -1.9 | -12 | 0.46 | 0.37 | -1.4 |
| 6 | -18 | -0.55 | 0.35 | -1.9 | -12 | 0.37 | 0.49 | -1.4 |
| 7 | -14 | 0.27 | 0.91 | -1.2 | -11 | 0.92 | 0.92 | 0.22 |
