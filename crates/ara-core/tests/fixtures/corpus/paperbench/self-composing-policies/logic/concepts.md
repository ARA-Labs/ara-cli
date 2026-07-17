# Concepts

## CompoNet
- **Notation**: CompoNet with modules $\{\pi^{(1)}, \ldots, \pi^{(n)}\}$
- **Definition**: A growable modular neural network architecture for CRL that adds one self-composing policy module per task, freezes all previous modules, and allows each new module to access and compose the output vectors of all previous modules via attention mechanisms.
- **Boundary conditions**: Requires known task boundaries; assumes constant action space across tasks; state spaces should be approximately similar across tasks.
- **Related concepts**: Self-Composing Policy Module, ProgressiveNet, Continual Reinforcement Learning

## Self-Composing Policy Module
- **Notation**: $\pi^{(k)}(a \mid s, \Phi^{k;s})$
- **Definition**: The basic unit of CompoNet, comprising three blocks — Output Attention Head, Input Attention Head, Internal Policy — that together produce an action distribution by composing the current state encoding $h_s$ with the matrix of previous module outputs $\Phi^{k;s}$.
- **Boundary conditions**: The first module has no previous modules to compose, so $\Phi^{1;s}$ is empty and the module reduces to a standard policy network. After training, the module's parameters are frozen permanently.
- **Related concepts**: Output Attention Head, Input Attention Head, Internal Policy, Phi Matrix

## Phi Matrix
- **Notation**: $\Phi^{k;s} \in \mathbb{R}^{(k-1) \times |A|}$
- **Definition**: The matrix whose j-th row is the output vector of the j-th frozen module when given state $s$ as input, for the current task k. Rows represent probability values (discrete) or mean vectors (continuous) of the action distribution of each previous module.
- **Boundary conditions**: Only defined when $k \geq 2$; dimension grows with number of tasks but each module receives it as a dynamic input, not a fixed-size parameter.
- **Related concepts**: Self-Composing Policy Module, Output Attention Head, Input Attention Head

## Output Attention Head
- **Notation**: $v = \text{Attention}(q, K, V)$ where $q = h_s W^Q_{out}$, $K = (\Phi^{k;s} + E_{out}) W^K_{out}$, $V = \Phi^{k;s}$
- **Definition**: A single-head scaled dot-product attention block that produces a tentative output vector $v \in \mathbb{R}^{|A|}$ as a weighted linear combination of the rows of $\Phi^{k;s}$, conditioned on the current state encoding $h_s$. Learnable parameters: $W^Q_{out} \in \mathbb{R}^{d_{enc} \times d_{model}}$, $W^K_{out} \in \mathbb{R}^{|A| \times d_{model}}$.
- **Boundary conditions**: $V = \Phi^{k;s}$ requires no transformation; positional encoding $E_{out}$ is cosine-based (Vaswani et al., 2017).
- **Related concepts**: Scaled Dot-Product Attention, Phi Matrix, State Encoding

## Input Attention Head
- **Notation**: Output $= \text{Attention}(q, K, V)$ where $q = h_s W^Q_{in}$, $K = (P + E_{in}) W^K_{in}$, $V = P W^V_{in}$, $P = [v; \Phi^{k;s}]$ (row-wise concat)
- **Definition**: A single-head scaled dot-product attention block that retrieves relevant information from the concatenation of the tentative vector $v$ (from Output Attention Head) and $\Phi^{k;s}$, producing a context vector of size $d_{model}$ used as input to the Internal Policy. Learnable parameters: $W^Q_{in} \in \mathbb{R}^{d_{enc} \times d_{model}}$, $W^K_{in} \in \mathbb{R}^{|A| \times d_{model}}$, $W^V_{in} \in \mathbb{R}^{|A| \times d_{model}}$.
- **Boundary conditions**: Unlike the Output Attention Head, values are linearly transformed ($V = PW^V_{in}$), enabling more expressive retrieval.
- **Related concepts**: Output Attention Head, Internal Policy, Phi Matrix

## Internal Policy
- **Notation**: $\delta = \text{FF}([h_s; \text{InHead}(h_s, P)]) \in \mathbb{R}^{|A|}$; final output $= v + \delta$
- **Definition**: A multi-layer feed-forward network that takes the concatenation of $h_s$ (size $d_{enc}$) and the output of the Input Attention Head (size $d_{model}$) as input, producing a residual correction vector $\delta$ of size $|A|$ that is added to the tentative vector $v$ from the Output Attention Head to form the module's final output.
- **Boundary conditions**: When previous policies are irrelevant, $\delta$ dominates $v$; when a previous policy already solves the task, $\delta \approx 0$ (analogous to a residual connection). Output may require normalization depending on action space type.
- **Related concepts**: Output Attention Head, Input Attention Head, Self-Composing Policy Module

## Scaled Dot-Product Attention
- **Notation**: $\text{Attention}(q, K, V) = \text{softmax}\!\left(\frac{qK^T}{\sqrt{d_{model}}}\right) V$
- **Definition**: The attention mechanism (Vaswani et al., 2017) that computes a weighted sum of value vectors $V$ where weights are determined by the similarity of a query vector $q$ to key vectors $K$, scaled by $1/\sqrt{d_{model}}$ for numerical stability.
- **Boundary conditions**: Applied without multi-head extension in CompoNet (single head per attention block).
- **Related concepts**: Output Attention Head, Input Attention Head

## State Encoding
- **Notation**: $h_s \in \mathbb{R}^{d_{enc}}$
- **Definition**: A fixed-dimensional representation of the current environment state $s$. For low-dimensional vector states (Meta-World): $h_s = s$, $d_{enc} = 39$. For image-based states (ALE): $h_s = \text{CNN}(s)$, $d_{enc} = 512$ (output of a 3-layer CNN with dense final layer). Each module has its own CNN encoder for ALE tasks, initialized from the previous module's encoder.
- **Boundary conditions**: The encoder's parameters are frozen when the corresponding module is frozen. For extremely long sequences or high-resolution images, a shared foundational model (e.g., DINOv2) may be used instead.
- **Related concepts**: Self-Composing Policy Module, CompoNet

## Continual Reinforcement Learning (CRL)
- **Notation**: Non-stationary MDP sequence $\{M^{(k)}\}_{k=1,\ldots,N}$ where $M^{(k)} = \langle S^{(k)}, A^{(k)}, p^{(k)}, r^{(k)}, \gamma^{(k)} \rangle$
- **Definition**: A setting where an agent learns a sequence of tasks presented one at a time, with a limited budget $\Delta$ timesteps per task, aiming to accelerate performance on new tasks by leveraging knowledge from previous ones.
- **Boundary conditions**: The paper assumes: (1) constant action space $A$, (2) known task boundaries and identifiers, (3) approximately similar state spaces.
- **Related concepts**: Forward Transfer, Average Performance, Catastrophic Forgetting

## Forward Transfer
- **Notation**: $\text{FTr}_i = \frac{\text{AUC}_i - \text{AUC}^b_i}{1 - \text{AUC}^b_i}$
- **Definition**: A metric measuring the normalized area between a method's success rate training curve on task $i$ and the baseline's training curve on the same task, where $\text{AUC}_i = \frac{1}{\Delta}\int_{(i-1)\Delta}^{i\Delta} p_i(t)\,dt$ and $\text{AUC}^b_i = \frac{1}{\Delta}\int_0^\Delta p^b_i(t)\,dt$. Positive FTr indicates the method learns task $i$ faster than a randomly initialized baseline.
- **Boundary conditions**: Denominator $1 - \text{AUC}^b_i$ prevents division by zero when the baseline solves the task immediately; metric is bounded to $(-\infty, 1]$.
- **Related concepts**: Reference Forward Transfer, Average Performance, Continual Reinforcement Learning

## Reference Forward Transfer (RT)
- **Notation**: $\text{RT} = \frac{1}{N}\sum_{i=2}^{N} \max_{j<i} \text{FTr}(j, i)$
- **Definition**: The average over tasks of the maximum forward transfer achievable by training from scratch on any previous task $j$ and fine-tuning on the current task $i$. It represents the performance of an ideal single-source fine-tuning oracle and serves as a strong baseline for CRL methods.
- **Boundary conditions**: Computed from $N(N-1)/2$ fine-tuning experiments (the forward transfer matrix); a CRL method can exceed RT by composing knowledge from multiple previous tasks simultaneously.
- **Related concepts**: Forward Transfer, CompoNet

## Catastrophic Forgetting
- **Notation**: $F_i = p_i(i \cdot \Delta) - p_i(T)$
- **Definition**: The loss in performance on task $i$ that occurs after the agent has been trained on subsequent tasks. Formally, the difference between the success rate at the end of training on task $i$ and the success rate at the end of the entire sequence $T = N \cdot \Delta$. Growing NN architectures (CompoNet, ProgressiveNet) achieve zero forgetting by design through parameter freezing.
- **Boundary conditions**: Only methods that share a single network (Baseline, FT-1) suffer non-zero forgetting; methods that freeze parameters per task have $F_i = 0$ by construction.
- **Related concepts**: Continual Reinforcement Learning, Forward Transfer
