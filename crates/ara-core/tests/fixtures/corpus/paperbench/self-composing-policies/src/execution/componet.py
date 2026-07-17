"""
CompoNet: Self-Composing Policy Module
Implements the core CompoNet architecture from Section 4.2 of the paper.

Key components:
  - OutputAttentionHead: proposes tentative action vector v by attending over Phi
  - InputAttentionHead: retrieves compositional context from [v; Phi]
  - InternalPolicy: feed-forward MLP producing residual correction delta
  - SelfComposingPolicyModule: combines the three blocks

Reference: "Self-Composing Policies for Scalable Continual Reinforcement Learning"
           Malagón et al., ICML 2024
"""

import math
import torch
import torch.nn as nn
import torch.nn.functional as F
from typing import List, Optional, Tuple


def cosine_positional_encoding(seq_len: int, d_model: int, device: torch.device) -> torch.Tensor:
    """
    Compute cosine positional encoding for a sequence.

    Args:
        seq_len: Number of positions (number of previous modules, k-1)
        d_model: Dimension of the model's hidden space (|A| in paper notation for this encoding)
        device: Target device

    Returns:
        E: Positional encoding matrix of shape (seq_len, d_model)
    """
    position = torch.arange(seq_len, dtype=torch.float32, device=device).unsqueeze(1)
    div_term = torch.exp(torch.arange(0, d_model, 2, device=device).float() * (-math.log(10000.0) / d_model))
    E = torch.zeros(seq_len, d_model, device=device)
    E[:, 0::2] = torch.sin(position * div_term)
    E[:, 1::2] = torch.cos(position * div_term[:d_model // 2])
    return E


def scaled_dot_product_attention(
    q: torch.Tensor,        # [batch, d_model]
    K: torch.Tensor,        # [batch, seq_len, d_model]
    V: torch.Tensor,        # [batch, seq_len, d_v]
    d_model: int,
) -> Tuple[torch.Tensor, torch.Tensor]:
    """
    Scaled dot-product attention: Attention(q, K, V) = softmax(q K^T / sqrt(d_model)) V

    Args:
        q: Query vector [batch, d_model]
        K: Key matrix [batch, seq_len, d_model]
        V: Value matrix [batch, seq_len, d_v]
        d_model: Scaling factor

    Returns:
        output: Attended output [batch, d_v]
        attn_weights: Attention weights [batch, seq_len]
    """
    # q: [batch, d_model] -> [batch, 1, d_model]
    q_expanded = q.unsqueeze(1)
    # scores: [batch, 1, seq_len]
    scores = torch.bmm(q_expanded, K.transpose(1, 2)) / math.sqrt(d_model)
    attn_weights = F.softmax(scores, dim=-1)  # [batch, 1, seq_len]
    # output: [batch, 1, d_v] -> [batch, d_v]
    output = torch.bmm(attn_weights, V).squeeze(1)
    return output, attn_weights.squeeze(1)


class OutputAttentionHead(nn.Module):
    """
    Output Attention Head (Section 4.2).

    Proposes a tentative action vector v as a weighted combination of Phi rows.
    Learnable: W_Q_out [d_enc x d_model], W_K_out [|A| x d_model]
    V = Phi (no transformation)
    """

    def __init__(self, d_enc: int, action_dim: int, d_model: int):
        """
        Args:
            d_enc: Dimension of encoded state h_s
            action_dim: |A|, action space dimension
            d_model: Hidden/key dimension
        """
        super().__init__()
        self.d_model = d_model
        self.action_dim = action_dim
        # W_Q_out in R^{d_enc x d_model}
        self.W_Q = nn.Linear(d_enc, d_model, bias=False)
        # W_K_out in R^{|A| x d_model}
        self.W_K = nn.Linear(action_dim, d_model, bias=False)

    def forward(
        self,
        h_s: torch.Tensor,       # [batch, d_enc]
        Phi: torch.Tensor,        # [batch, k-1, |A|]
    ) -> Tuple[torch.Tensor, torch.Tensor]:
        """
        Args:
            h_s: State encoding [batch, d_enc]
            Phi: Previous module outputs [batch, k-1, |A|]

        Returns:
            v: Tentative output [batch, |A|]
            attn_weights: Attention weights [batch, k-1]
        """
        batch, seq_len, _ = Phi.shape
        device = Phi.device

        # Positional encoding E_out: [seq_len, |A|]
        E_out = cosine_positional_encoding(seq_len, self.action_dim, device)

        # q = h_s @ W_Q: [batch, d_model]
        q = self.W_Q(h_s)

        # K = (Phi + E_out) @ W_K: [batch, seq_len, d_model]
        Phi_encoded = Phi + E_out.unsqueeze(0)  # broadcast over batch
        K = self.W_K(Phi_encoded)  # [batch, seq_len, d_model]

        # V = Phi (no transformation): [batch, seq_len, |A|]
        V = Phi

        v, attn_weights = scaled_dot_product_attention(q, K, V, self.d_model)
        return v, attn_weights


class InputAttentionHead(nn.Module):
    """
    Input Attention Head (Section 4.2).

    Retrieves context from P = [v; Phi] (v from OutputAttentionHead concatenated with Phi).
    Learnable: W_Q_in [d_enc x d_model], W_K_in [|A| x d_model], W_V_in [|A| x d_model]
    """

    def __init__(self, d_enc: int, action_dim: int, d_model: int):
        """
        Args:
            d_enc: Dimension of encoded state h_s
            action_dim: |A|
            d_model: Hidden/key/value dimension
        """
        super().__init__()
        self.d_model = d_model
        self.action_dim = action_dim
        # W_Q_in in R^{d_enc x d_model}
        self.W_Q = nn.Linear(d_enc, d_model, bias=False)
        # W_K_in in R^{|A| x d_model}
        self.W_K = nn.Linear(action_dim, d_model, bias=False)
        # W_V_in in R^{|A| x d_model}
        self.W_V = nn.Linear(action_dim, d_model, bias=False)

    def forward(
        self,
        h_s: torch.Tensor,   # [batch, d_enc]
        P: torch.Tensor,      # [batch, k, |A|]  (k = k-1 + 1 for v)
    ) -> Tuple[torch.Tensor, torch.Tensor]:
        """
        Args:
            h_s: State encoding [batch, d_enc]
            P: Row-wise concat of [v; Phi], shape [batch, k, |A|]

        Returns:
            context: Attended context [batch, d_model]
            attn_weights: Attention weights [batch, k]
        """
        batch, seq_len, _ = P.shape
        device = P.device

        # Positional encoding E_in: [seq_len, |A|]
        E_in = cosine_positional_encoding(seq_len, self.action_dim, device)

        # q = h_s @ W_Q: [batch, d_model]
        q = self.W_Q(h_s)

        # K = (P + E_in) @ W_K: [batch, seq_len, d_model]
        P_encoded = P + E_in.unsqueeze(0)
        K = self.W_K(P_encoded)

        # V = P @ W_V: [batch, seq_len, d_model]
        V = self.W_V(P)

        context, attn_weights = scaled_dot_product_attention(q, K, V, self.d_model)
        return context, attn_weights


class InternalPolicy(nn.Module):
    """
    Internal Policy feed-forward block (Section 4.2).

    Input: [h_s; context] of size d_enc + d_model
    Output: delta of size |A| (residual correction to add to v)
    """

    def __init__(self, d_enc: int, d_model: int, action_dim: int, num_layers: int = 2):
        """
        Args:
            d_enc: Encoded state dimension
            d_model: Context dimension (output of InputAttentionHead)
            action_dim: |A|
            num_layers: Number of hidden layers in the feed-forward block
        """
        super().__init__()
        input_dim = d_enc + d_model
        layers = []
        for i in range(num_layers):
            in_dim = input_dim if i == 0 else d_model
            layers.extend([nn.Linear(in_dim, d_model), nn.ReLU()])
        layers.append(nn.Linear(d_model, action_dim))
        self.net = nn.Sequential(*layers)

    def forward(self, h_s: torch.Tensor, context: torch.Tensor) -> torch.Tensor:
        """
        Args:
            h_s: State encoding [batch, d_enc]
            context: Output of InputAttentionHead [batch, d_model]

        Returns:
            delta: Residual correction [batch, |A|]
        """
        x = torch.cat([h_s, context], dim=-1)  # [batch, d_enc + d_model]
        return self.net(x)  # [batch, |A|]


class SelfComposingPolicyModule(nn.Module):
    """
    Full self-composing policy module (Section 4.2).

    Combines OutputAttentionHead + InputAttentionHead + InternalPolicy.
    Output = normalize(v + delta)

    For SAC (continuous): output is mean of Gaussian distribution; normalization via tanh.
    For PPO (discrete): output is logits; normalization via softmax at action selection.
    """

    def __init__(
        self,
        d_enc: int,
        action_dim: int,
        d_model: int,
        continuous: bool = False,
        num_internal_layers: int = 2,
    ):
        """
        Args:
            d_enc: Encoded state dimension
            action_dim: |A|
            d_model: Hidden dimension for attention heads and internal policy
            continuous: If True, output represents continuous action mean (tanh normalization)
            num_internal_layers: Layers in the internal policy MLP
        """
        super().__init__()
        self.d_enc = d_enc
        self.action_dim = action_dim
        self.d_model = d_model
        self.continuous = continuous

        self.out_head = OutputAttentionHead(d_enc, action_dim, d_model)
        self.in_head = InputAttentionHead(d_enc, action_dim, d_model)
        self.internal = InternalPolicy(d_enc, d_model, action_dim, num_internal_layers)

    def forward(
        self,
        h_s: torch.Tensor,                   # [batch, d_enc]
        Phi: Optional[torch.Tensor] = None,  # [batch, k-1, |A|] or None for first module
    ) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor, torch.Tensor]:
        """
        Full forward pass of one self-composing policy module.

        Args:
            h_s: Encoded state [batch, d_enc]
            Phi: Stacked outputs of all previous frozen modules [batch, k-1, |A|]
                 None if this is the first module (no predecessors)

        Returns:
            output: Final action distribution [batch, |A|]
            v: Tentative output from OutputAttentionHead [batch, |A|]
            out_attn: OutputAttentionHead attention weights [batch, k-1]
            in_attn: InputAttentionHead attention weights [batch, k]
        """
        if Phi is None or Phi.shape[1] == 0:
            # First module: no previous modules; internal policy acts alone
            dummy_context = torch.zeros(h_s.shape[0], self.d_model, device=h_s.device)
            delta = self.internal(h_s, dummy_context)
            v = torch.zeros_like(delta)
            out_attn = torch.zeros(h_s.shape[0], 0, device=h_s.device)
            in_attn = torch.zeros(h_s.shape[0], 0, device=h_s.device)
            raw = delta
        else:
            # Step 3: Output Attention Head
            v, out_attn = self.out_head(h_s, Phi)  # v: [batch, |A|]

            # Step 4: Construct P = [v; Phi]
            P = torch.cat([v.unsqueeze(1), Phi], dim=1)  # [batch, k, |A|]

            # Step 5: Input Attention Head
            context, in_attn = self.in_head(h_s, P)  # context: [batch, d_model]

            # Step 6: Internal Policy
            delta = self.internal(h_s, context)  # [batch, |A|]

            # Step 7: Combine
            raw = v + delta  # [batch, |A|]

        # Normalize based on action space type
        if self.continuous:
            output = torch.tanh(raw)
        else:
            # For discrete: return raw logits; caller applies softmax
            output = raw

        return output, v, out_attn, in_attn


class CriticNetwork(nn.Module):
    """
    Critic Network for SAC and PPO (Appendix E, Table E.1/E.2).

    - SAC (Meta-World): 2-layer MLP with twin Q-heads (Q1, Q2) for double Q-learning.
      Input: state [d_enc=39], output: scalar Q-value per head.
      Architecture: Linear(39→256) → ReLU → Linear(256→256) → ReLU → Linear(256→1) × 2 heads
      3 total layers per head (matching Table E.1 "CRITIC NET. LAYERS = 3").

    - PPO (ALE): Single fully connected layer mapping h_s [512] → scalar value.
      Input: h_s from CNN encoder [512], output: scalar V(s).

    The critic is RESET at each task boundary (common practice, Wolczyk et al. 2022).
    """

    def __init__(
        self,
        input_dim: int,
        d_model: int = 256,
        use_twin_q: bool = False,
        num_layers: int = 3,
    ):
        """
        Args:
            input_dim: Dimension of input state/encoding (39 for Meta-World, 512 for ALE)
            d_model: Hidden dimension (256 for SAC, irrelevant for PPO single-layer)
            use_twin_q: If True, instantiate twin Q-networks for SAC double-Q
            num_layers: Total layers (3 for SAC: 2 hidden + 1 output)
        """
        super().__init__()
        self.use_twin_q = use_twin_q

        def make_q_net(inp: int, hidden: int, n_layers: int) -> nn.Sequential:
            layers = []
            for i in range(n_layers - 1):
                in_d = inp if i == 0 else hidden
                layers.extend([nn.Linear(in_d, hidden), nn.ReLU()])
            layers.append(nn.Linear(hidden, 1))
            return nn.Sequential(*layers)

        self.q1 = make_q_net(input_dim, d_model, num_layers)
        if use_twin_q:
            self.q2 = make_q_net(input_dim, d_model, num_layers)

    def forward(self, x: torch.Tensor) -> Tuple[torch.Tensor, Optional[torch.Tensor]]:
        """
        Args:
            x: Input state or encoding [batch, input_dim]

        Returns:
            q1: Q-value from first head [batch, 1]
            q2: Q-value from second head [batch, 1] (only if use_twin_q=True, else None)
        """
        q1 = self.q1(x)
        q2 = self.q2(x) if self.use_twin_q else None
        return q1, q2

    def reset(self) -> None:
        """Reset all critic parameters (called at each task boundary)."""
        def weight_reset(m: nn.Module) -> None:
            if hasattr(m, 'reset_parameters'):
                m.reset_parameters()
        self.apply(weight_reset)


class CompoNet(nn.Module):
    """
    CompoNet: growing collection of self-composing policy modules.

    Manages the cascade of frozen + active modules. On task transition:
      - freeze current module
      - instantiate new trainable module

    Memory complexity: O(m * n) where m = constant params per module, n = number of tasks.
    """

    def __init__(
        self,
        d_enc: int,
        action_dim: int,
        d_model: int,
        continuous: bool = False,
        num_internal_layers: int = 2,
    ):
        super().__init__()
        self.d_enc = d_enc
        self.action_dim = action_dim
        self.d_model = d_model
        self.continuous = continuous
        self.num_internal_layers = num_internal_layers

        # List of all modules; last one is trainable, rest are frozen
        self.modules_list = nn.ModuleList()
        # Add the first (active) module
        self._add_module()

    def _add_module(self) -> None:
        """Add a new trainable self-composing policy module."""
        new_mod = SelfComposingPolicyModule(
            d_enc=self.d_enc,
            action_dim=self.action_dim,
            d_model=self.d_model,
            continuous=self.continuous,
            num_internal_layers=self.num_internal_layers,
        )
        self.modules_list.append(new_mod)

    def task_transition(self) -> None:
        """
        Called at task boundary.
        Freeze current (last) module and add a new trainable module.
        """
        # Freeze last module
        for param in self.modules_list[-1].parameters():
            param.requires_grad = False
        # Add new trainable module
        self._add_module()

    def forward(
        self, h_s: torch.Tensor
    ) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor, torch.Tensor]:
        """
        Forward pass through the entire CompoNet cascade.

        Only the LAST module is the active module for the current task.
        Previous modules are evaluated to build Phi.

        Args:
            h_s: Encoded state [batch, d_enc]

        Returns:
            output: Final action distribution [batch, |A|]
            v: Tentative output from active module's OutputAttentionHead [batch, |A|]
            out_attn: Active module's OutputAttentionHead attention weights
            in_attn: Active module's InputAttentionHead attention weights
        """
        # Evaluate all previous frozen modules to build Phi
        # Note: this is sequential (each module depends on previous outputs for k>1)
        prev_outputs = []
        for mod in self.modules_list[:-1]:
            with torch.no_grad():
                Phi_prev = (
                    torch.stack(prev_outputs, dim=1) if prev_outputs else
                    torch.zeros(h_s.shape[0], 0, self.action_dim, device=h_s.device)
                )
                mod_out, _, _, _ = mod(h_s, Phi_prev if Phi_prev.shape[1] > 0 else None)
                prev_outputs.append(mod_out)

        # Build Phi for active module
        if prev_outputs:
            Phi = torch.stack(prev_outputs, dim=1)  # [batch, k-1, |A|]
        else:
            Phi = None

        # Forward through active (last) module
        output, v, out_attn, in_attn = self.modules_list[-1](h_s, Phi)
        return output, v, out_attn, in_attn
