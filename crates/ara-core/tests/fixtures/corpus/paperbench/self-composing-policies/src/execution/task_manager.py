"""
Task Sequence Manager and Critic Networks for CompoNet.

Covers:
1. SoftQNetwork — SAC critic (twin Q-networks for continuous control)
2. PPO Critic — value function for discrete action (ALE) setting
3. Growth Mechanism — task boundary transition logic:
   - freeze current module, instantiate new CompoNet module
   - reset critic at each task boundary
   - reinitialize output heads for FT-1/FT-N

Reference: "Self-Composing Policies for Scalable Continual Reinforcement Learning"
           Malagón et al., ICML 2024 — Appendix E (Implementation Details)
Code: https://github.com/mikelma/componet
      experiments/meta-world/run_sac.py
      experiments/atari/run_ppo.py
"""

import torch
import torch.nn as nn
import torch.nn.functional as F
import numpy as np
from typing import List, Optional
from pathlib import Path


# ─────────────────────────────────────────────────────────────────────────────
# 4. SoftQNetwork / Critic
# ─────────────────────────────────────────────────────────────────────────────

def _shared_mlp(input_dim: int) -> nn.Sequential:
    """Two-layer shared MLP backbone used in SAC actor and critic.
    Maps input_dim → 256 → 256 (with ReLU activations).
    Source: experiments/meta-world/models/shared_arch.py
    """
    return nn.Sequential(
        nn.Linear(input_dim, 256),
        nn.ReLU(),
        nn.Linear(256, 256),
        nn.ReLU(),
    )


class SoftQNetwork(nn.Module):
    """
    SAC Twin Q-Network for continuous control (Meta-World).

    Architecture: shared_mlp(obs_dim + act_dim) → Linear(256 → 1)

    Used as critic in the SAC algorithm. Two instances (qf1, qf2) + two
    target networks (qf1_target, qf2_target) are maintained.
    Critic is reset at the beginning of each task (new instance created).

    Args:
        obs_dim: Observation/state dimension. Meta-World: 39.
        act_dim: Action dimension. Meta-World: 4.
    """

    def __init__(self, obs_dim: int, act_dim: int):
        super().__init__()
        self.fc = _shared_mlp(obs_dim + act_dim)
        self.fc_out = nn.Linear(256, 1)

    def forward(self, obs: torch.Tensor, action: torch.Tensor) -> torch.Tensor:
        """
        Args:
            obs:    (batch, obs_dim) — environment observation
            action: (batch, act_dim) — action taken

        Returns:
            q_value: (batch, 1) — estimated Q-value Q(s, a)
        """
        x = torch.cat([obs, action], dim=1)
        x = self.fc(x)
        return self.fc_out(x)


class PPOCritic(nn.Module):
    """
    PPO Value Network for discrete control (ALE: SpaceInvaders, Freeway).

    Architecture: Linear(d_enc → 1)
    Input is the CNN encoder output h_s shared with the actor.
    Initialized with orthogonal initialization (std=1.0).

    Args:
        hidden_dim: Encoder output dimension. ALE: 512.
    """

    def __init__(self, hidden_dim: int = 512):
        super().__init__()
        self.critic = nn.Linear(hidden_dim, 1)
        # Orthogonal initialization (std=1.0) as in CleanRL
        nn.init.orthogonal_(self.critic.weight, std=1.0)
        nn.init.constant_(self.critic.bias, 0.0)

    def forward(self, hidden: torch.Tensor) -> torch.Tensor:
        """
        Args:
            hidden: (batch, hidden_dim) — CNN encoder output h_s

        Returns:
            value: (batch, 1) — estimated state value V(s)
        """
        return self.critic(hidden)


# ─────────────────────────────────────────────────────────────────────────────
# Growth Mechanism — Task Boundary Transition Logic
# ─────────────────────────────────────────────────────────────────────────────

def grow_componet(
    current_module: nn.Module,
    save_dir: Path,
    obs_dim: int,
    act_dim: int,
    hidden_dim: int,
    ret_probs: bool,
    device: str = "cuda",
    encoder_class=None,
    prev_encoder_path: Optional[Path] = None,
) -> nn.Module:
    """
    Perform a task boundary transition for CompoNet (SAC/continuous actions).

    At each task k-1 → k:
    1. Save current module to disk (frozen for future tasks)
    2. Load all previous frozen modules from disk
    3. Instantiate a new CompoNet module referencing all previous modules
    4. (Caller is responsible for resetting critic)

    Args:
        current_module:   Current trainable CompoNet/FirstModuleWrapper module.
        save_dir:         Directory to save/load module checkpoints.
        obs_dim:          State encoding dimension d_enc.
        act_dim:          Action dimension |A|.
        hidden_dim:       Attention dimension d_model.
        ret_probs:        True for discrete (PPO), False for continuous (SAC).
        device:           Device string.
        encoder_class:    Optional CNN encoder class for visual tasks.
        prev_encoder_path: Path to previous module's encoder (for warm init).

    Returns:
        new_module: Freshly instantiated CompoNet module for the new task.
    """
    from componet import CompoNet, FirstModuleWrapper

    # Step 1: Save current module
    save_dir.mkdir(parents=True, exist_ok=True)
    torch.save(current_module, save_dir / "actor.pt")

    # Step 2: Load all previous frozen modules
    prev_paths = sorted(save_dir.parent.glob("task_*/actor.pt"))
    previous_units = [
        torch.load(p, map_location=device) for p in prev_paths
    ]

    # Step 3: Build internal policy MLP
    # Meta-World/SAC: input_dim + hidden_dim → hidden_dim → act_dim
    # ALE/PPO: hidden_dim + hidden_dim → hidden_dim → act_dim
    pol_in = obs_dim + hidden_dim
    internal_policy = nn.Sequential(
        nn.Linear(pol_in, hidden_dim),
        nn.ReLU(),
        nn.Linear(hidden_dim, act_dim),
    )

    # Step 4: Optionally load encoder for visual tasks
    encoder = None
    if encoder_class is not None:
        encoder = encoder_class(hidden_dim=hidden_dim)
        if prev_encoder_path is not None and prev_encoder_path.exists():
            encoder.load_state_dict(
                torch.load(prev_encoder_path, map_location=device).state_dict()
            )

    # Step 5: Instantiate new CompoNet module
    new_module = CompoNet(
        previous_units=previous_units,
        input_dim=obs_dim,
        hidden_dim=hidden_dim,
        out_dim=act_dim,
        internal_policy=internal_policy,
        ret_probs=ret_probs,
        encoder=encoder,
        device=device,
    ).to(device)

    return new_module


def reset_sac_critic(
    obs_dim: int, act_dim: int, device: str = "cuda"
):
    """
    Reset SAC critic at task boundary (standard CRL practice per Wolczyk et al. 2022).

    Creates fresh SoftQNetwork instances (qf1, qf2, qf1_target, qf2_target)
    with random initialization.

    Args:
        obs_dim: Observation dimension.
        act_dim: Action dimension.
        device:  Device string.

    Returns:
        qf1, qf2, qf1_target, qf2_target: Fresh SoftQNetwork instances.
    """
    qf1 = SoftQNetwork(obs_dim, act_dim).to(device)
    qf2 = SoftQNetwork(obs_dim, act_dim).to(device)
    qf1_target = SoftQNetwork(obs_dim, act_dim).to(device)
    qf2_target = SoftQNetwork(obs_dim, act_dim).to(device)
    qf1_target.load_state_dict(qf1.state_dict())
    qf2_target.load_state_dict(qf2.state_dict())
    return qf1, qf2, qf1_target, qf2_target


def reset_output_heads(model: nn.Module) -> None:
    """
    Reinitialize output heads (fc_mean, fc_logstd) at each task boundary.
    Used for FT-1, FT-N, and ProgressiveNet (per Wolczyk et al., 2021).

    Args:
        model: SimpleAgent or similar with fc_mean/fc_logstd Linear layers.
    """
    if hasattr(model, "reset_heads"):
        model.reset_heads()  # SimpleAgent.reset_heads()
    elif hasattr(model, "fc_mean"):
        nn.init.xavier_uniform_(model.fc_mean.weight)
        nn.init.zeros_(model.fc_mean.bias)
    elif hasattr(model, "fc_logstd"):
        nn.init.xavier_uniform_(model.fc_logstd.weight)
        nn.init.zeros_(model.fc_logstd.bias)
