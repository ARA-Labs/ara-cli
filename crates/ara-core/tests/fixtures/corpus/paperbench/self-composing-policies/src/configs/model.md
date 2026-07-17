# Model Configuration

## CompoNet Module

### d_model (Hidden Dimension)
- **Value**: 256 (Meta-World/SAC); 512 (ALE/PPO)
- **Rationale**: Matches the internal policy and attention head hidden size; chosen consistent with baseline MLP sizes.
- **Source**: Table E.1, Table E.2, Appendix B

### d_enc (State Encoding Dimension)
- **Value**: 39 (Meta-World, no encoder); 512 (ALE, CNN encoder output)
- **Rationale**: Meta-World state is a 39-dim vector. ALE CNN encoder produces a 512-dim feature vector.
- **Source**: Appendix D.1 (39-dim state), Appendix E.1 (512-dim encoder output)

### |A| (Action Space Size)
- **Value**: 4 (Meta-World, continuous); 6 (SpaceInvaders, discrete); 3 (Freeway, discrete)
- **Rationale**: Determined by environment action spaces.
- **Source**: Appendix D.1 (4-dim continuous), Appendix D.2 (6 actions for SpaceInvaders), Appendix D.3 (3 actions for Freeway)

### W_Q_out Shape
- **Value**: R^{d_enc × d_model}
- **Source**: Section 4.2

### W_K_out Shape
- **Value**: R^{|A| × d_model}
- **Source**: Section 4.2

### W_Q_in Shape
- **Value**: R^{d_enc × d_model}
- **Source**: Section 4.2

### W_K_in Shape
- **Value**: R^{|A| × d_model}
- **Source**: Section 4.2

### W_V_in Shape
- **Value**: R^{|A| × d_model}
- **Source**: Section 4.2

### Internal Policy Architecture
- **Value**: Multi-layer feed-forward MLP; input size = d_enc + d_model; output size = |A|; Meta-World: 2 hidden layers (width 256); ALE: 2 hidden layers (width 512)
- **Rationale**: For Meta-World (SAC): follows the actor design (2-layer MLP + two output heads for mean and log-std); for ALE (PPO): matches the 2-layer design with width 512.
- **Source**: Table E.1, Table E.2, Appendix E.2

### Positional Encoding Type
- **Value**: Cosine positional encoding (Vaswani et al., 2017)
- **Source**: Section 4.2

## CNN Encoder (ALE Tasks)

### Convolutional Layers
- **Value**: 3 layers; channels: [32, 64, 64]; filter sizes: [8, 4, 3]
- **Rationale**: Standard Atari CNN architecture from CleanRL (Huang et al., 2022).
- **Source**: Appendix E.1

### Dense Output Layer
- **Value**: Output dimension = 512
- **Rationale**: Provides a rich fixed-size representation for the attention heads.
- **Source**: Appendix E.1

### Encoder Initialization for New Modules
- **Value**: Initialize new module's encoder from previous module's frozen encoder weights
- **Source**: Appendix E.2

## Baseline / FT-1 / FT-N Network

### Meta-World Actor
- **Value**: 2-layer MLP (width 256) + two output heads (mean and log-std of Gaussian), total 3 layers; input 39-dim, output 4-dim
- **Source**: Appendix E, Table E.1

### ALE Actor
- **Value**: CNN encoder (same as above) + 2 single-layer output heads: one for action logits (categorical), one for value (critic)
- **Source**: Appendix E, Table E.2

## SAC Critic

### Architecture
- **Value**: 2-layer MLP + 2 output heads (Q1 and Q2 for double Q-learning); width d_model=256; 3 total layers
- **Rationale**: Standard twin-critic architecture for SAC.
- **Source**: Table E.1

## PPO Value Function (ALE)

### Architecture
- **Value**: Single fully connected layer taking h_s (512-dim CNN output) to scalar value
- **Source**: Appendix E.2 (CompoNet implementation: "the value function, defined as a single fully connected layer")
