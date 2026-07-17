# Algorithm: Self-Composing Policy Forward Pass

## Mathematical Formulation

Given: current state $s \in S^{(k)}$, set of frozen modules $\{\pi^{(j)}\}_{j=1}^{k-1}$, trainable module $\pi^{(k)}$

**Step 1 — State Encoding:**
$$h_s = \begin{cases} s & \text{(low-dim vector states, Meta-World)} \\ \text{CNN}_k(s) & \text{(image states, ALE)} \end{cases} \in \mathbb{R}^{d_{enc}}$$

**Step 2 — Collect Previous Outputs:**
$$\Phi^{k;s} = \begin{bmatrix} \pi^{(1)}(s) \\ \pi^{(2)}(s, \Phi^{2;s}) \\ \vdots \\ \pi^{(k-1)}(s, \Phi^{(k-1);s}) \end{bmatrix} \in \mathbb{R}^{(k-1) \times |A|}$$

**Step 3 — Output Attention Head:**
$$q = h_s W^Q_{out}, \quad W^Q_{out} \in \mathbb{R}^{d_{enc} \times d_{model}}$$
$$K = (\Phi^{k;s} + E_{out}) W^K_{out}, \quad W^K_{out} \in \mathbb{R}^{|A| \times d_{model}}, \quad E_{out} \in \mathbb{R}^{(k-1) \times |A|}$$
$$V = \Phi^{k;s}$$
$$v = \text{softmax}\!\left(\frac{qK^T}{\sqrt{d_{model}}}\right) V \in \mathbb{R}^{|A|}$$

**Step 4 — Construct Input Matrix:**
$$P = \begin{bmatrix} v \\ \Phi^{k;s} \end{bmatrix} \in \mathbb{R}^{k \times |A|}$$

**Step 5 — Input Attention Head:**
$$q' = h_s W^Q_{in}, \quad W^Q_{in} \in \mathbb{R}^{d_{enc} \times d_{model}}$$
$$K' = (P + E_{in}) W^K_{in}, \quad W^K_{in} \in \mathbb{R}^{|A| \times d_{model}}, \quad E_{in} \in \mathbb{R}^{k \times |A|}$$
$$V' = P W^V_{in}, \quad W^V_{in} \in \mathbb{R}^{|A| \times d_{model}}$$
$$\text{ctx} = \text{softmax}\!\left(\frac{q'(K')^T}{\sqrt{d_{model}}}\right) V' \in \mathbb{R}^{d_{model}}$$

**Step 6 — Internal Policy:**
$$\delta = \text{FF}([h_s \,\|\, \text{ctx}]) \in \mathbb{R}^{|A|}, \quad \text{input dim} = d_{enc} + d_{model}$$

**Step 7 — Final Output:**
$$\text{out} = v + \delta \in \mathbb{R}^{|A|}$$
$$\pi^{(k)}(a \mid s, \Phi^{k;s}) = \text{normalize}(\text{out}) \quad \text{[softmax for discrete; tanh + scale for continuous]}$$

## Pseudocode

```python
def componet_forward(s, frozen_modules, current_module):
    # Step 1: Encode state
    h_s = current_module.encoder(s)  # [d_enc]

    # Step 2: Collect previous outputs (sequential, each depends on prior)
    Phi = []
    for mod in frozen_modules:
        out = mod.forward(s)  # [|A|]
        Phi.append(out)
    Phi = stack(Phi)  # [(k-1) x |A|]

    # Step 3: Output Attention Head
    q = h_s @ W_Q_out  # [d_model]
    K = (Phi + E_out) @ W_K_out  # [(k-1) x d_model]
    V = Phi  # [(k-1) x |A|]
    attn_weights = softmax(q @ K.T / sqrt(d_model))  # [(k-1)]
    v = attn_weights @ V  # [|A|]

    # Step 4: Construct P
    P = concat([v.unsqueeze(0), Phi], dim=0)  # [k x |A|]

    # Step 5: Input Attention Head
    q_in = h_s @ W_Q_in  # [d_model]
    K_in = (P + E_in) @ W_K_in  # [k x d_model]
    V_in = P @ W_V_in  # [k x d_model]
    attn_in = softmax(q_in @ K_in.T / sqrt(d_model))  # [k]
    ctx = attn_in @ V_in  # [d_model]

    # Step 6: Internal Policy
    delta = feedforward(concat([h_s, ctx]))  # [|A|]

    # Step 7: Final output
    out = v + delta  # [|A|]
    return normalize(out)  # action distribution
```

## Complexity Analysis

**Per module inference:** $T_{module}(n) = O(n)$ where $n$ = number of previous modules
- Output attention head: $O(n)$ — dot product of query with $(n-1)$ keys
- Input attention head: $O(n)$ — dot product with $n$ keys
- Internal policy: $O(1)$ — fixed-size input/output, independent of $n$

**Total CompoNet inference** (all $n$ modules sequentially): $n \cdot O(n) = O(n^2)$

**Empirical note**: Despite $O(n^2)$ theoretical complexity, parallel GPU computation of attention keys/values makes CompoNet's empirical inference time grow substantially slower than ProgressiveNet up to 300 tasks tested (Figure 3 and Figure C.1).

**Memory complexity**: $O(m \cdot n)$ where $m$ = constant parameters per module = $O(1)$ with respect to $n$.
