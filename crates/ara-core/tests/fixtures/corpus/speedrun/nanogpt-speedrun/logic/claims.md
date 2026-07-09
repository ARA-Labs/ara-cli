---
type: claims
paper: nanogpt-speedrun
---

# Claims

## C01 — 16× Training Speedup Through Incremental Optimization
- **Statement**: Human-authored optimizations compress GPT-2 124M training (val_loss ≤ 3.28) from 49.5 min to 3.1 min across 21 records, achieving a 16.1× wall-clock speedup on 8×H100.
- **Status**: supported
- **Falsification**: If any record fails to achieve val_loss ≤ 3.28, or if the cumulative speedup is substantially less than 16×.
- **Proof**: evidence/tables/table1_speedrun_progression.md — all 21 records with exact timing and val_loss.
- **Tags**: [speedup, benchmark, quantitative]

## C02 — Frontier LLM Agents Cannot Reproduce Single-Record Optimizations
- **Statement**: No frontier LLM (DeepSeek R1, o3-mini, Gemini 2.5, Claude 3.7 Sonnet) can reproduce any single record-to-record optimization, even when given pseudocode-level hints.
- **Status**: supported
- **Falsification**: If any model achieves the target train_time for any record with val_loss ≤ 3.28.
- **Proof**: Agent evaluation results across 4 models × 20 records × 4 hint levels.
- **Dependencies**: [C01]
- **Tags**: [agent-failure, evaluation, negative-result]

## C03 — Muon Optimizer Is the Single Largest Speedup
- **Statement**: The Muon optimizer (OrthogonalNesterov + AdamW hybrid) achieves the single largest record-to-record speedup: 36.8 min → 23.1 min (37% reduction) in Record 3.
- **Status**: supported
- **Falsification**: If another single-record optimization achieves a larger absolute or relative wall-clock reduction.
- **Proof**: evidence/tables/table1_speedrun_progression.md, Record 3 entry. src/execution/muon_optimizer.py.
- **Dependencies**: [C01]
- **Tags**: [optimizer, muon, quantitative]

## C04 — FlexAttention Enables Document-Aware 64K Context
- **Statement**: FlexAttention with document-aware causal masking and sliding window (1024 tokens) enables training on 64K token sequences with negligible overhead compared to standard attention at shorter context.
- **Status**: supported
- **Falsification**: If FlexAttention adds >10% wall-clock overhead vs. standard attention at equivalent context, or if masking causes val_loss regression.
- **Proof**: Records 11-12 timing data. src/execution/flex_attention.py.
- **Dependencies**: [C05]
- **Tags**: [attention, architecture, systems]

## C05 — Architectural Innovations Compound Multiplicatively
- **Statement**: Architectural changes (ReLU², padded vocab, zero-init projections, QK norm, GQA, U-Net skip connections) compound multiplicatively with optimizer gains, achieving combined speedups beyond what either axis alone provides.
- **Status**: supported
- **Falsification**: If ablating architectural changes while keeping Muon shows <20% additional speedup vs. ablating Muon alone.
- **Proof**: Records 5, 7, 8, 10, 15 timing progression.
- **Dependencies**: [C01, C03]
- **Tags**: [architecture, compounding, quantitative]

## C06 — Agent Failure Mode Is Implementation, Not Ideation
- **Statement**: LLM agents correctly identify the optimization direction from hints but fail at implementation: distributed training bugs, CUDA kernel errors, torch.compile incompatibilities, and numerical precision issues.
- **Status**: supported
- **Falsification**: If agents fail to identify correct optimization direction >50% of the time, or if >50% of failures are due to wrong optimization strategy.
- **Proof**: Bug analysis from agent workspace logs across 4 models.
- **Dependencies**: [C02]
- **Tags**: [agent-failure, implementation, qualitative]

## C07 — Tree-Structured Search Outperforms Linear Search
- **Statement**: BoN (Best-of-N) with tree-structured branching recovers more of the human-to-human speedup gap than linear (AIDE-style) search, across all models tested.
- **Status**: supported
- **Falsification**: If linear search achieves equal or higher gap-recovered on >50% of records.
- **Proof**: Search strategy comparison across Forest, Tree, Flat, AIDE, Multi-AIDE.
- **Dependencies**: [C02]
- **Tags**: [search-strategy, agent-design, quantitative]

## C08 — Later Records Are Exponentially Harder
- **Statement**: Agent success rate (even partial) decreases monotonically with record number, as each optimization compounds implementation constraints from all prior records.
- **Status**: supported
- **Falsification**: If agent success rate is non-monotonic or shows improvement on later records.
- **Proof**: Per-record agent success rates across all models and hint levels.
- **Dependencies**: [C02, C06]
- **Tags**: [difficulty-scaling, quantitative]

## C09 — Hint Abstraction Level Minimally Affects Agent Success
- **Statement**: Providing more detailed hints (pseudocode vs. paper-level description) does not significantly improve agent success rate, reinforcing that the bottleneck is implementation, not understanding.
- **Status**: supported
- **Falsification**: If pseudocode hints yield >2× higher success rate than paper-level hints.
- **Proof**: Hint-level ablation across all models and records.
- **Dependencies**: [C02, C06]
- **Tags**: [hints, abstraction, evaluation]

## C10 — FP8 and Quantized Operations Enable Final 15% Speedup
- **Statement**: FP8 linear heads with custom CUDA ops and precision tuning contribute the final ~15% speedup (Records 18-20), requiring hardware-specific systems engineering beyond standard PyTorch.
- **Status**: supported
- **Falsification**: If removing FP8 ops increases training time by <10%.
- **Proof**: Records 18-20 timing data. src/execution/fp8_linear.py.
- **Tags**: [fp8, quantization, systems, hardware]
