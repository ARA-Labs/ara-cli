# Figure 3: Empirical Computational Cost of Inference and Parameter Growth

- **Source**: Figure 3, Section 4.3
- **Caption**: "Empirical computational cost of inference (left) and growth in the number of parameters (right) with respect to the number of tasks for CompoNet and ProgressiveNet methods. Hyperparameters are: denc = 64, |A| = 6, dmodel = 256, and a batch size of 8. Measurements have been taken in a machine with an AMD EPYC 7252 CPU and an NVIDIA A5000 GPU."
- **Axis (Inference time)**: X = Number of tasks {10, 50, 100}; Y = Inference time in seconds (range 0.00–1.25)
- **Axis (Parameters)**: X = Number of tasks {10, 50, 100}; Y = Number of parameters (log scale, around 10^10 range shown)

## Qualitative Data Points (from figure, approximate readings)

### Inference Time (seconds) — approximate readings

| Number of Tasks | CompoNet (approx.) | ProgressiveNet (approx.) |
|-----------------|-------------------|--------------------------|
| 10 | ≈0.02 | ≈0.05 |
| 50 | ≈0.10 | ≈0.50 |
| 100 | ≈0.20 | ≈1.20 |
| 300 | Not shown in main figure; Appendix C.1 shows sub-quadratic growth |

### Parameter Count — qualitative trends
- CompoNet total parameters: grows linearly O(n) — confirmed by Appendix B mathematical proof
- CompoNet trainable parameters: grows linearly (only current module trainable)
- ProgressiveNet total parameters: grows quadratically O(n²)
- ProgressiveNet trainable parameters: grows quadratically

**Key finding**: CompoNet inference time grows substantially slower than ProgressiveNet; ProgressiveNet shows quadratic growth while CompoNet does not exhibit quadratic growth empirically up to 300 tasks (Appendix C.2, Figure C.1).

### Appendix C.1 Data (Figure C.1 — up to 300 tasks, d_enc=39, d_model=256, |A|=4)

| Number of Tasks | CompoNet inference (s, approx.) | ProgressiveNet inference (s, approx.) |
|-----------------|--------------------------------|--------------------------------------|
| 1 | ≈0.00 | ≈0.00 |
| 50 | ≈0.05 | ≈0.20 |
| 100 | ≈0.10 | ≈0.60 |
| 200 | ≈0.25 | ≈1.10 |
| 300 | ≈0.45 | ≈1.40 |

Note: Exact numerical values not printed in figures; readings are best-effort approximations marked ≈.
