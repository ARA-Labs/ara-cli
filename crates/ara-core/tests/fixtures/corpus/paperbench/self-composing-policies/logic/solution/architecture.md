# System Architecture: CompoNet

## Overview
CompoNet is a cascading graph of self-composing policy modules that grows in depth with the number of tasks. At task k, states s are fed to all k modules; the k-th (active) module receives both s and the outputs of all k-1 frozen modules.

## Component Graph

```
State s ──────────────────────────────────────────────────────────────┐
         │                                                             │
         ├──► Module 1 (frozen) ──────────────────────────────────────┤
         │         │ π^(1)(a|s)                                        │
         ├──► Module 2 (frozen) ──────────────────────────────────────┤
         │         │ π^(2)(a|s, Φ^{2;s})                              │
         │         ...                                                 │
         ├──► Module k-1 (frozen) ─────────────────────────────────── ► Φ^{k;s}
         │                                                             │
         └──► Module k (trainable) ◄──────────────────────────────────┘
                   │
                   ▼
              π^(k)(a|s, Φ^{k;s})  [final action distribution]
```

## Module k Internal Architecture

```
h_s (from encoder or raw s)
      │                         Φ^{k;s} [(k-1) × |A|]
      │                              │
      ├────────────────────────────► Output Attention Head
      │                              │
      │                         v ∈ R^|A| (tentative output)
      │                              │
      ├──────────────────────────────┼──► [v; Φ^{k;s}] = P [(k) × |A|]
      │                              │
      ├────────────────────────────► Input Attention Head
      │                              │
      │                    context ∈ R^{d_model}
      │                              │
      └──► [h_s ; context] ──────────► Internal Policy (FF MLP)
                                       │
                                  δ ∈ R^|A|
                                       │
                              v + δ = final output
                                       │
                               [normalize if needed]
                                       │
                              π^(k)(a|s, Φ^{k;s})
```

## Components

### State Encoder
- **Purpose**: Produce fixed-dimensional state representation h_s
- **Inputs**: Raw state s (39-dim vector for Meta-World; 210×160 RGB for ALE)
- **Outputs**: h_s ∈ R^{d_enc}
- **Design**: For Meta-World: identity (h_s = s, d_enc = 39). For ALE: 3-layer CNN (channels: 32, 64, 64; filters: 8, 4, 3) + dense layer (output: 512), giving d_enc = 512. Each CompoNet module has its own encoder for ALE tasks; new encoder initialized from previous module's encoder weights.
- **Interactions**: Feeds h_s into all three blocks of the module; also fed directly to value function (critic).

### Output Attention Head
- **Purpose**: Propose a tentative output action vector v by attending over previous policy outputs
- **Inputs**: h_s ∈ R^{d_enc}, Φ^{k;s} ∈ R^{(k-1) × |A|}
- **Outputs**: v ∈ R^{|A|}
- **Parameters**: W^Q_out ∈ R^{d_enc × d_model}, W^K_out ∈ R^{|A| × d_model}; V = Φ^{k;s} (no transformation)
- **Key design**: No learned value transformation — allows direct soft-copying of previous policy outputs
- **Interactions**: v fed to Input Attention Head (as row 0 of P) and added to Internal Policy output

### Input Attention Head
- **Purpose**: Retrieve relevant compositional context from previous policies and the tentative output
- **Inputs**: h_s ∈ R^{d_enc}, P = [v; Φ^{k;s}] ∈ R^{k × |A|}
- **Outputs**: context ∈ R^{d_model}
- **Parameters**: W^Q_in ∈ R^{d_enc × d_model}, W^K_in ∈ R^{|A| × d_model}, W^V_in ∈ R^{|A| × d_model}
- **Key design**: Values are transformed (V = P W^V_in), enabling expressive information retrieval beyond simple output copying
- **Interactions**: context concatenated with h_s and fed to Internal Policy

### Internal Policy (Feed-Forward Block)
- **Purpose**: Adjust, overwrite, or retain the tentative output from the Output Attention Head
- **Inputs**: [h_s; context] ∈ R^{d_enc + d_model}
- **Outputs**: δ ∈ R^{|A|}
- **Architecture**: Multi-layer MLP; for Meta-World: 2 hidden layers, width d_model=256; for ALE: 2 hidden layers, width d_model=512
- **Key design**: Residual addition (v + δ) — when δ=0, the output attention head's proposal is passed unchanged (like a residual connection)
- **Interactions**: δ added to v; result normalized if needed (softmax for discrete, tanh-clamp for continuous)

### Critic Network
- **Purpose**: Estimate state value V(s) for actor-critic training
- **Design**: Reset at each task boundary; separate from actor (CompoNet not applied to critic)
- **Meta-World (SAC)**: 2-layer MLP with two output heads (mean and log-std of normal distribution), 3 total layers
- **ALE (PPO)**: Single fully connected layer taking h_s (output of shared encoder) to scalar value

## Growth Policy
- Task k begins: freeze parameters of module k-1; instantiate new module k with random parameters (except ALE encoder: initialized from module k-1's encoder)
- Task k ends: freeze module k
- Total modules after N tasks: N (one per task, all frozen except the current one)
- Memory: O(m · N) where m = constant parameters per module

## Positional Encoding
- **Type**: Cosine positional encoding (Vaswani et al., 2017)
- **Applied to**: Keys matrix in both attention heads (E_out for Output Head, E_in for Input Head)
- **Purpose**: Differentiate module positions in Φ^{k;s} to allow the model to reason about which previous module is which
