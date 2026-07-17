"""
CompoNet core module implementation.
Implements the self-composing policy module with dual attention heads.
Based on: componet/impl.py in the official repository.

This stub covers the NOVEL contribution: the forward pass of CompoNet
with output attention head, input attention head, and internal policy.
"""

import torch
import torch.nn as nn
import torch.nn.functional as F
import numpy as np
from typing import List, Optional, Tuple


def get_position_encoding(seq_len: int, d: int, n: int = 10_000) -> np.ndarray:
    """
    Compute cosine positional encoding matrix.
    
    Args:
        seq_len: Number of positions (number of previous modules)
        d:       Dimension of each encoding (= |A|, action space size)
        n:       Base for sinusoidal encoding (default: 10000)
    
    Returns:
        P: np.ndarray of shape (seq_len, d)
    """
    P = np.zeros((seq_len, d))
    for k in range(seq_len):
        for i in np.arange(int(d / 2)):
            denominator = np.power(n, 2 * i / d)
            P[k, 2 * i] = np.sin(k / denominator)
            P[k, 2 * i + 1] = np.cos(k / denominator)
    return P


class CompoNet(nn.Module):
    """
    Self-Composing Policy Module (CompoNet).

    Each instance represents the trainable module for one task k.
    Previous modules are frozen and stored in `self.previous_units`.

    Architecture:
        1. Output Attention Head: proposes tentative output v by attending over Φ
        2. Input Attention Head:  retrieves context c from P=[v;Φ] for internal policy
        3. Internal Policy:       MLP that produces residual δ; final output = v + δ

    Parameter count per module (constant, independent of number of tasks n):
        - W_Q_out: (d_enc, d_model)
        - W_K_out: (|A|, d_model)
        - W_Q_in:  (d_enc, d_model)
        - W_K_in:  (|A|, d_model)
        - W_V_in:  (|A|, d_model)
        - internal_policy: MLP(d_enc + d_model → |A|)
    """

    def __init__(
        self,
        previous_units: List[nn.Module],
        input_dim: int,           # d_enc: encoder output size (39 for MetaWorld, 512 for ALE)
        hidden_dim: int,          # d_model: attention hidden dim (256 for MetaWorld, 512 for ALE)
        out_dim: int,             # |A|: action space size
        internal_policy: nn.Module,  # MLP: (d_enc + d_model) → |A|
        ret_probs: bool,          # True for discrete (softmax output), False for continuous
        encoder: Optional[nn.Module] = None,  # CNN encoder for visual tasks; None = identity
        device: str = "cuda" if torch.cuda.is_available() else "cpu",
        proj_bias: bool = True,
    ):
        super().__init__()
        self.hidden_dim = hidden_dim
        self.out_dim = out_dim
        self.ret_probs = ret_probs
        self.internal_policy = internal_policy
        self.encoder = encoder if encoder is not None else nn.Identity()
        self.att_temp = np.sqrt(hidden_dim)  # attention temperature √d_model

        # --- Output Attention Head parameters ---
        # W_Q_out: (d_enc, d_model)
        self.headout_wq = nn.Linear(input_dim, hidden_dim, bias=proj_bias)
        # W_K_out: (|A|, d_model)
        self.headout_wk = nn.Linear(out_dim, hidden_dim, bias=proj_bias)

        # --- Input Attention Head parameters ---
        # W_Q_in: (d_enc, d_model)
        self.headin_wq = nn.Linear(input_dim, hidden_dim, bias=proj_bias)
        # W_K_in: (|A|, d_model)
        self.headin_wk = nn.Linear(out_dim, hidden_dim, bias=proj_bias)
        # W_V_in: (|A|, d_model)
        self.headin_wv = nn.Linear(out_dim, hidden_dim, bias=proj_bias)

        # --- Pre-compute positional encodings as non-trainable buffers ---
        n_prev = len(previous_units)
        # pe1: (1, n_prev+1, out_dim) — for input attention head (includes v from output head)
        pe1 = torch.tensor(
            get_position_encoding(seq_len=n_prev + 1, d=out_dim),
            dtype=torch.float32, device=device,
        )
        self.pe1 = pe1[None, :, :]  # (1, n_prev+1, out_dim)

        # pe0: (1, n_prev, out_dim) — for output attention head
        self.pe0 = self.pe1[:, :-1, :] if n_prev >= 2 else None

        # --- Freeze and store previous units ---
        for unit in previous_units:
            if hasattr(unit, "previous_units"):
                del unit.previous_units
            unit.eval()
            for param in unit.parameters():
                param.requires_grad = False
        self.previous_units = nn.Sequential(*previous_units)

    def _forward_output_attention_head(
        self,
        hs: torch.Tensor,    # (batch, d_enc) — encoded state
        phi: torch.Tensor,   # (batch, k-1, |A|) — previous policy outputs
    ) -> Tuple[torch.Tensor, torch.Tensor]:
        """
        Output Attention Head forward pass.
        
        Computes: v = softmax(q K^T / √d_model) · V
        where:
          q = hs @ W_Q_out                        shape: (batch, d_model)
          K = (phi + E_out) @ W_K_out             shape: (batch, k-1, d_model)
          V = phi                                  shape: (batch, k-1, |A|)
        
        Returns:
            v:   (batch, 1, |A|) — tentative output
            att: (batch, 1, k-1) — attention weights
        """
        query = self.headout_wq(hs)                                     # (batch, d_model)
        keys = self.headout_wk(phi + self.pe0 if self.pe0 is not None else phi)  # (batch, k-1, d_model)
        values = phi                                                     # (batch, k-1, |A|)

        # Scaled dot-product attention
        w = torch.matmul(query[:, None, :], keys.permute(0, 2, 1))      # (batch, 1, k-1)
        att = F.softmax(w / self.att_temp, dim=-1)                      # (batch, 1, k-1)
        v = torch.matmul(att, values)                                   # (batch, 1, |A|)
        return v, att

    def _forward_input_attention_head_and_internal_policy(
        self,
        hs: torch.Tensor,    # (batch, d_enc)
        phi_extended: torch.Tensor,  # (batch, k, |A|) = [v; Phi] row-wise
    ) -> Tuple[torch.Tensor, torch.Tensor]:
        """
        Input Attention Head + Internal Policy forward pass.
        
        Input Attention Head:
          q  = hs @ W_Q_in                          (batch, d_model)
          K  = (P + E_in) @ W_K_in                  (batch, k, d_model)
          V  = P @ W_V_in                           (batch, k, d_model)
          c  = softmax(q K^T / √d_model) · V        (batch, d_model)
        
        Internal Policy:
          δ = MLP([c; hs])                           (batch, |A|)
        
        Returns:
            delta: (batch, |A|) — residual correction
            att:   (batch, 1, k) — attention weights
        """
        query = self.headin_wq(hs)                                    # (batch, d_model)
        values = self.headin_wv(phi_extended)                          # (batch, k, d_model)
        keys = self.headin_wk(phi_extended + self.pe1)                 # (batch, k, d_model)

        w = torch.matmul(query[:, None, :], keys.permute(0, 2, 1))   # (batch, 1, k)
        att = F.softmax(w / self.att_temp, dim=-1)                    # (batch, 1, k)
        c = torch.matmul(att, values)[:, 0, :]                        # (batch, d_model)

        # Concatenate encoded state and attention context
        policy_in = torch.cat([c, hs], dim=1)                         # (batch, d_enc + d_model)
        delta = self.internal_policy(policy_in)                       # (batch, |A|)
        return delta, att

    def forward(
        self,
        s: torch.Tensor,
        return_atts: bool = False,
        ret_int_pol: bool = False,
        ret_head_out: bool = False,
        ret_encoder_out: bool = False,
        prevs_to_noise: int = 0,
    ) -> list:
        """
        Full CompoNet forward pass for the current (trainable) module.

        Args:
            s:              Input state tensor (batch, state_dim) or (batch, C, H, W) for visual
            return_atts:    If True, return attention weights of both heads
            ret_int_pol:    If True, return internal policy output δ
            ret_head_out:   If True, return tentative vector v
            ret_encoder_out: If True, return encoded state h_s
            prevs_to_noise: Replace first N previous module outputs with Dirichlet noise

        Returns:
            [out, phi, (optional: hs, att_in, att_out, head_out, int_pol)]
        """
        # Step 1: Collect outputs of frozen previous modules
        with torch.no_grad():
            phi, _s = self.previous_units(s)  # phi: (batch, k-1, |A|)
            if prevs_to_noise > 0:
                if self.ret_probs:
                    m = torch.distributions.Dirichlet(
                        torch.tensor([1.0 / self.out_dim] * self.out_dim)
                    )
                    r = m.sample(sample_shape=[phi.size(0), prevs_to_noise])
                else:
                    r = torch.randn((phi.size(0), prevs_to_noise, phi.size(-1)))
                phi[:, :prevs_to_noise, :] = r

        # Step 2: Encode state
        hs = self.encoder(s)  # (batch, d_enc)

        # Step 3: Output Attention Head → tentative output v
        v, att_head_out = self._forward_output_attention_head(hs, phi)
        # v: (batch, 1, |A|)

        # Step 4: Build P = [v; Φ] and run Input Attention Head + Internal Policy
        phi_extended = torch.cat([phi, v], dim=1)    # (batch, k, |A|)
        delta, att_head_in = self._forward_input_attention_head_and_internal_policy(hs, phi_extended)

        # Step 5: Compose final output
        v_squeezed = v[:, 0, :]    # (batch, |A|)
        out = v_squeezed + delta   # residual composition

        # Step 6: Normalize if needed
        if self.ret_probs:
            out = F.softmax(out, dim=-1)

        # Update phi matrix with current output
        out_unsq = out[:, None, :]           # (batch, 1, |A|)
        phi_full = torch.cat([phi, out_unsq], dim=1)

        ret_vals = [out, phi_full]
        if ret_encoder_out:
            ret_vals.append(hs)
        if return_atts:
            ret_vals += [att_head_in, att_head_out]
        if ret_int_pol:
            ret_vals.append(delta)
        if ret_head_out:
            ret_vals.append(v_squeezed)
        return ret_vals


def make_meta_world_internal_policy(obs_dim: int = 39, act_dim: int = 4, hidden_dim: int = 256) -> nn.Module:
    """
    Internal policy MLP for Meta-World (SAC, continuous actions).
    Input: (d_enc + d_model) = (39 + 256) = 295
    Output: act_dim = 4
    """
    return nn.Sequential(
        nn.Linear(obs_dim + hidden_dim, hidden_dim),
        nn.ReLU(),
        nn.Linear(hidden_dim, hidden_dim),
        nn.ReLU(),
        nn.Linear(hidden_dim, act_dim),
    )


def make_atari_internal_policy(enc_dim: int = 512, hidden_dim: int = 512, act_dim: int = 6) -> nn.Module:
    """
    Internal policy MLP for ALE tasks (PPO, discrete actions).
    Input: (d_enc + d_model) = (512 + 512) = 1024
    Output: act_dim (6 for SpaceInvaders, 3 for Freeway)
    With orthogonal initialization.
    """
    def layer_init(layer, std=np.sqrt(2), bias_const=0.0):
        torch.nn.init.orthogonal_(layer.weight, std)
        torch.nn.init.constant_(layer.bias, bias_const)
        return layer

    return nn.Sequential(
        layer_init(nn.Linear(enc_dim + hidden_dim, hidden_dim)),
        nn.ReLU(),
        layer_init(nn.Linear(hidden_dim, act_dim), std=0.01),
    )
