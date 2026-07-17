# Figure B.1: Memory Growth — CompoNet vs. ProgressiveNet (Meta-World Hyperparameters)

- **Source**: Figure B.1, Appendix B
- **Caption**: "Growth in memory of CompoNet and ProgressiveNet models as the number of tasks (assuming an NN module per task) increases. The count is given in the total number of parameters, depicted with solid lines, and in the number of trainable parameters (not frozen), in dashed lines. Note that we assume 32-bit floats are used to represent the parameters of the models. Hyperparameters correspond to the ones utilized in the Meta-World sequence: denc = 39, dmodel = 256, and |A| = 4."

## Hyperparameters
- d_enc = 39 (Meta-World state dimension)
- d_model = 256
- |A| = 4 (Meta-World action dimension)
- 32-bit float representation

## Parameter Count Growth (qualitative — exact values from Appendix B proof)

| Number of Tasks | CompoNet Total (approx.) | CompoNet Trainable (approx.) | ProgressiveNet Total (approx.) | ProgressiveNet Trainable (approx.) |
|-----------------|--------------------------|------------------------------|-------------------------------|-------------------------------------|
| 1 | ≈constant m | ≈m | ≈m | ≈m |
| 10 | ≈10m | ≈m | ≈quadratic | ≈m (only new frozen, one trainable) |
| 50 | ≈50m | ≈m | ≈50² × base | ≈m |
| 100 | ≈100m | ≈m | ≈100² × base | ≈m |

## Key Finding
- CompoNet total parameters scale linearly O(m·n) where m is constant
- CompoNet trainable parameters = m (constant — only current module is trainable)
- ProgressiveNet total parameters scale quadratically O(n²) — each new column adds lateral connections to all layers of all previous columns
- Figure B.2 (not shown) indicates CompoNet can scale to >10k modules on a single A5000 (24GB VRAM) GPU with d_model=256

Note: Exact parameter counts readable from the log-scale plot require the actual figure image; values above marked ≈ are qualitative trend readings.
