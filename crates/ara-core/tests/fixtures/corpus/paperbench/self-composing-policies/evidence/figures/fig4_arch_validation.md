# Figure 4: Architectural Validation — Scenarios (i) and (iii)

- **Source**: Figure 4, Section 5.4
- **Caption**: "Empirical results on the fulfillment of objectives (i) and (iii) from Section 4 that motivated the design of CompoNet. In the leftmost figures, CompoNet is trained on the fifth task of SpaceInvaders with four non-informative previous policies that sample their output from a uniform Dirichlet distribution, and one policy trained to solve the current task (Inf. Mod.). In the rightmost figures, CompoNet is trained on the sixth task of SpaceInvaders with five non-informative previous policies. Results aggregate 10 random seeds."
- **Axis**: X = Timestep [0, 1×10^6]; Y varies by subplot

## Scenario (i) — 4 non-informative + 1 informative (Inf. Mod.) previous policies, task 5

### 4a: Episodic Return (CompoNet vs. Baseline)
| Observation | Value |
|-------------|-------|
| CompoNet mean episodic return at 1M steps | ≈600 (plateau) |
| Baseline mean episodic return at 1M steps | ≈400 (still rising) |
| CompoNet rises faster, significantly outperforms Baseline | confirmed |

### 4b: Matching Rates (first 3×10^5 steps)
| Metric | Pattern |
|--------|---------|
| Out = Out head (matching rate) | Rises sharply to >0.8 within ~10k steps, plateaus |
| Out = Int. pol. (matching rate) | Low initially (~0.125), rises after ~200k steps |
| Out head = Int. pol. | Rises over training as internal policy mimics out head |

### 4c: Input Attention Head Values
| Module | Pattern |
|--------|---------|
| Prev. 0–3 (non-informative) | Drop to ≈0.0 within 10k steps |
| Inf. Mod. | Rises and plateaus at ≈0.16 |
| Out head | Rises sharply and plateaus at ≈0.80 |

### 4d: Output Attention Head Values
| Module | Pattern |
|--------|---------|
| Prev. 0–3 (non-informative) | Drop to ≈0.0 within 10k steps |
| Inf. Mod. | Rises sharply to ≈1.0 within 10k steps, stays at 1.0 |

## Scenario (iii) — 5 non-informative previous policies, task 6

### 4e: Episodic Return (CompoNet vs. Baseline)
| Observation | Value |
|-------------|-------|
| CompoNet performance | Matches Baseline (slightly higher due to extra stochasticity) |
| Performance at 1M steps | Both ≈400 |

### 4f: Matching Rates (first 3×10^5 steps)
| Metric | Pattern |
|--------|---------|
| Out = Int. pol. (matching rate) | Rises quickly to ≈1.0 within ~50k steps |
| Out = Out head | Low, drops near 0 quickly |
| Out head = Int. pol. | Near 0 |

### 4g: Input Attention Head Values
| Module | Pattern |
|--------|---------|
| All 5 modules + Out head | Uniform ≈1/6 ≈ 0.167 throughout (no module preferred) |

### 4h: Output Attention Head Values
| Module | Pattern |
|--------|---------|
| All 5 modules | Uniform ≈0.2 throughout (no module preferred) |

Note: Exact numerical values are approximate (≈) readings from figures.
