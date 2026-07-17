"""
SAC + CompoNet Training Script for Meta-World CW20 Continual RL.

Reproduces the Meta-World experiment from Section 5.2 / Table 1:
  - CW20 sequence: 10 unique Meta-World tasks x 2 repetitions = 20 tasks
  - SAC algorithm with hyperparameters from Table E.1
  - CompoNet grows one module per task; critic reset at each boundary
  - 10 seeds for main results; 1M timesteps per task

Usage:
    python train_sac_metaworld.py --seed 0 --num-seeds 10 --device cuda

Reference: "Self-Composing Policies for Scalable Continual Reinforcement Learning"
           Malagon et al., ICML 2024
Hyperparameters: Table E.1 (Appendix E)
"""

import os
import sys
import time
import argparse
import copy
import json
from pathlib import Path
from collections import deque
from typing import Dict, List, Tuple, Optional

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
import torch.optim as optim

# Local imports — all from the existing execution/ directory
from componet import CompoNet, CriticNetwork, SelfComposingPolicyModule
from task_manager import SoftQNetwork, reset_sac_critic, grow_componet, reset_output_heads
from metrics import compute_auc, compute_forward_transfer, compute_average_performance, compute_forgetting


# ═══════════════════════════════════════════════════════════════════════════════
# Meta-World CW20 Task Sequence Definition
# ═══════════════════════════════════════════════════════════════════════════════

CW20_TASKS = [
    "hammer-v2",
    "push-wall-v2",
    "faucet-close-v2",
    "push-back-v2",
    "stick-pull-v2",
    "handle-press-side-v2",
    "push-v2",
    "shelf-place-v2",
    "window-close-v2",
    "peg-unplug-side-v2",
] * 2  # 10 unique tasks x 2 repetitions = 20 tasks in sequence

OBS_DIM = 39   # Meta-World observation dimension (d_enc)
ACT_DIM = 4    # Meta-World action dimension (|A|)
D_MODEL = 256  # Hidden dimension for CompoNet and critic (Table E.1)


def make_metaworld_env(task_name: str, seed: int):
    """
    Create a Meta-World v2 environment for a given task.

    Uses Farama Metaworld library (github.com/Farama-Foundation/Metaworld).
    Returns a gymnasium-compatible env with 39-dim observations and 4-dim actions.
    """
    import metaworld
    import metaworld.envs

    mt1 = metaworld.MT1(task_name, seed=seed)
    env = mt1.train_classes[task_name](seed=seed)
    task = mt1.train_tasks[0]
    env.set_task(task)
    env.seed(seed)
    return env


# ═══════════════════════════════════════════════════════════════════════════════
# SAC Actor (Gaussian Policy with CompoNet backbone)
# ═══════════════════════════════════════════════════════════════════════════════

LOG_STD_MAX = 2.0       # exp(2)  — Table E.1: MAX STD
LOG_STD_MIN = -20.0     # exp(-20) — Table E.1: MIN STD


class SACActorCompoNet(nn.Module):
    """
    SAC actor using CompoNet as the policy backbone.

    Architecture:
      - CompoNet produces a 4-dim raw output (continuous action mean candidate)
      - Two output heads: fc_mean and fc_logstd map CompoNet output to Gaussian params
      - Action sampled via reparameterization trick with tanh squashing

    For the first task, CompoNet has a single module (no predecessors).
    For subsequent tasks, previous modules are frozen and a new module is added.
    """

    def __init__(self, componet: CompoNet):
        super().__init__()
        self.componet = componet
        # Output heads: CompoNet output (|A|=4) -> mean and log_std
        # These are re-initialized at each task boundary (H04 for baselines)
        self.fc_mean = nn.Linear(ACT_DIM, ACT_DIM)
        self.fc_logstd = nn.Linear(ACT_DIM, ACT_DIM)

    def forward(self, obs: torch.Tensor) -> Tuple[torch.Tensor, torch.Tensor]:
        """Return action mean and log_std from CompoNet output."""
        componet_out, _, _, _ = self.componet(obs)  # [batch, |A|]
        mean = self.fc_mean(componet_out)
        log_std = self.fc_logstd(componet_out)
        log_std = torch.clamp(log_std, LOG_STD_MIN, LOG_STD_MAX)
        return mean, log_std

    def get_action(self, obs: torch.Tensor) -> Tuple[torch.Tensor, torch.Tensor]:
        """
        Sample action via reparameterization trick with tanh squashing.

        Returns:
            action: Squashed action [batch, |A|], values in [-1, 1]
            log_prob: Log probability of the action [batch, 1]
        """
        mean, log_std = self.forward(obs)
        std = log_std.exp()
        normal = torch.distributions.Normal(mean, std)
        x_t = normal.rsample()  # reparameterization trick
        action = torch.tanh(x_t)

        # Log probability with tanh correction (Haarnoja et al., 2018, Eq. 21)
        log_prob = normal.log_prob(x_t)
        log_prob -= torch.log(1 - action.pow(2) + 1e-6)
        log_prob = log_prob.sum(dim=-1, keepdim=True)
        return action, log_prob

    def reset_heads(self):
        """Re-initialize output heads at task boundary (H04)."""
        nn.init.xavier_uniform_(self.fc_mean.weight)
        nn.init.zeros_(self.fc_mean.bias)
        nn.init.xavier_uniform_(self.fc_logstd.weight)
        nn.init.zeros_(self.fc_logstd.bias)


# ═══════════════════════════════════════════════════════════════════════════════
# Replay Buffer
# ═══════════════════════════════════════════════════════════════════════════════

class ReplayBuffer:
    """
    Standard replay buffer for off-policy SAC.

    Capacity: 10^6 (Table E.1: BUFFER SIZE).
    Stores (obs, action, reward, next_obs, done) tuples.
    """

    def __init__(self, capacity: int = 1_000_000):
        self.buffer = deque(maxlen=capacity)

    def push(self, obs, action, reward, next_obs, done):
        self.buffer.append((obs, action, reward, next_obs, done))

    def sample(self, batch_size: int) -> Tuple:
        indices = np.random.randint(0, len(self.buffer), size=batch_size)
        batch = [self.buffer[i] for i in indices]
        obs, actions, rewards, next_obs, dones = zip(*batch)
        return (
            torch.FloatTensor(np.array(obs)),
            torch.FloatTensor(np.array(actions)),
            torch.FloatTensor(np.array(rewards)).unsqueeze(1),
            torch.FloatTensor(np.array(next_obs)),
            torch.FloatTensor(np.array(dones)).unsqueeze(1),
        )

    def __len__(self):
        return len(self.buffer)

    def clear(self):
        self.buffer.clear()


# ═══════════════════════════════════════════════════════════════════════════════
# SAC Training Loop (per task)
# ═══════════════════════════════════════════════════════════════════════════════

def train_sac_one_task(
    actor: SACActorCompoNet,
    qf1: SoftQNetwork,
    qf2: SoftQNetwork,
    qf1_target: SoftQNetwork,
    qf2_target: SoftQNetwork,
    env,
    task_idx: int,
    device: torch.device,
    total_timesteps: int = 1_000_000,
    eval_interval: int = 10_000,
) -> Dict:
    """
    Train SAC for one task in the CW20 sequence.

    Hyperparameters from Table E.1:
      - lr=1e-3 (actor and Q), gamma=0.99, tau=0.005
      - batch_size=128, alpha=0.2 (auto-tuned)
      - policy_update_freq=2, target_update_freq=1
      - num_random_actions=10000, learning_starts=5000

    Args:
        actor: SACActorCompoNet with CompoNet backbone
        qf1, qf2: Twin Q-networks
        qf1_target, qf2_target: Target Q-networks
        env: Meta-World gymnasium environment
        task_idx: Index of current task in CW20 sequence
        device: torch device
        total_timesteps: Training budget per task (1M)
        eval_interval: Steps between evaluation checkpoints

    Returns:
        metrics: Dict with success_rates, eval_returns, etc.
    """
    # ── Hyperparameters (Table E.1) ──
    lr_actor = 1e-3
    lr_q = 1e-3
    gamma = 0.99
    tau = 0.005
    batch_size = 128
    alpha_init = 0.2
    policy_update_freq = 2
    target_update_freq = 1
    num_random_actions = 10_000
    learning_starts = 5_000

    # ── Optimizers ──
    actor_optimizer = optim.Adam(actor.parameters(), lr=lr_actor, betas=(0.9, 0.999))
    q_optimizer = optim.Adam(
        list(qf1.parameters()) + list(qf2.parameters()),
        lr=lr_q, betas=(0.9, 0.999),
    )

    # ── Auto-tuning entropy coefficient (Table E.1: AUTO. TUNING OF alpha = YES) ──
    target_entropy = -float(ACT_DIM)  # -|A| heuristic (Haarnoja et al., 2018)
    log_alpha = torch.tensor(np.log(alpha_init), requires_grad=True, device=device)
    alpha_optimizer = optim.Adam([log_alpha], lr=lr_q)
    alpha = log_alpha.exp().item()

    # ── Replay buffer (Table E.1: BUFFER SIZE = 10^6) ──
    replay_buffer = ReplayBuffer(capacity=1_000_000)

    # ── Metrics tracking ──
    success_rates = []
    eval_returns = []
    episode_rewards = []
    episode_successes = []

    # ── Training loop ──
    obs, _ = env.reset()
    episode_reward = 0.0
    episode_success = False

    for global_step in range(total_timesteps):
        # ── Action selection ──
        if global_step < num_random_actions:
            # Pure random exploration (Table E.1: NUMBER OF RANDOM ACTIONS = 10^4)
            action = env.action_space.sample()
        else:
            with torch.no_grad():
                obs_tensor = torch.FloatTensor(obs).unsqueeze(0).to(device)
                action_tensor, _ = actor.get_action(obs_tensor)
                action = action_tensor.cpu().numpy()[0]

        # ── Environment step ──
        next_obs, reward, terminated, truncated, info = env.step(action)
        done = terminated or truncated

        # Track success (Meta-World provides 'success' in info)
        if info.get("success", 0.0) > 0.5:
            episode_success = True

        replay_buffer.push(obs, action, reward, next_obs, float(done))
        episode_reward += reward
        obs = next_obs

        if done:
            episode_rewards.append(episode_reward)
            episode_successes.append(float(episode_success))
            obs, _ = env.reset()
            episode_reward = 0.0
            episode_success = False

        # ── Learning (Table E.1: TIMESTEP TO START LEARNING = 5x10^3) ──
        if global_step < learning_starts:
            continue

        # Sample batch
        s_obs, s_actions, s_rewards, s_next_obs, s_dones = replay_buffer.sample(batch_size)
        s_obs = s_obs.to(device)
        s_actions = s_actions.to(device)
        s_rewards = s_rewards.to(device)
        s_next_obs = s_next_obs.to(device)
        s_dones = s_dones.to(device)

        # ── Critic update ──
        with torch.no_grad():
            next_actions, next_log_probs = actor.get_action(s_next_obs)
            q1_next = qf1_target(s_next_obs, next_actions)
            q2_next = qf2_target(s_next_obs, next_actions)
            q_next = torch.min(q1_next, q2_next) - alpha * next_log_probs
            q_target = s_rewards + gamma * (1.0 - s_dones) * q_next

        q1_pred = qf1(s_obs, s_actions)
        q2_pred = qf2(s_obs, s_actions)
        qf1_loss = F.mse_loss(q1_pred, q_target)
        qf2_loss = F.mse_loss(q2_pred, q_target)
        q_loss = qf1_loss + qf2_loss

        q_optimizer.zero_grad()
        q_loss.backward()
        q_optimizer.step()

        # ── Actor update (Table E.1: POLICY UPDATE FREQ = 2) ──
        if global_step % policy_update_freq == 0:
            new_actions, log_probs = actor.get_action(s_obs)
            q1_new = qf1(s_obs, new_actions)
            q2_new = qf2(s_obs, new_actions)
            q_new = torch.min(q1_new, q2_new)
            actor_loss = (alpha * log_probs - q_new).mean()

            actor_optimizer.zero_grad()
            actor_loss.backward()
            actor_optimizer.step()

            # ── Alpha auto-tuning ──
            alpha_loss = -(log_alpha * (log_probs.detach() + target_entropy)).mean()
            alpha_optimizer.zero_grad()
            alpha_loss.backward()
            alpha_optimizer.step()
            alpha = log_alpha.exp().item()

        # ── Target network update (Table E.1: TARGET NET. UPDATE FREQ = 1, tau=0.005) ──
        if global_step % target_update_freq == 0:
            for p_target, p in zip(qf1_target.parameters(), qf1.parameters()):
                p_target.data.copy_(tau * p.data + (1.0 - tau) * p_target.data)
            for p_target, p in zip(qf2_target.parameters(), qf2.parameters()):
                p_target.data.copy_(tau * p.data + (1.0 - tau) * p_target.data)

        # ── Evaluation checkpoint ──
        if (global_step + 1) % eval_interval == 0:
            recent_window = min(100, len(episode_successes))
            if recent_window > 0:
                sr = np.mean(episode_successes[-recent_window:])
            else:
                sr = 0.0
            success_rates.append(sr)

            recent_returns = episode_rewards[-recent_window:] if recent_window > 0 else [0.0]
            eval_returns.append(np.mean(recent_returns))

            print(
                f"  Task {task_idx:2d} | Step {global_step+1:>7d}/{total_timesteps} | "
                f"Success Rate: {sr:.3f} | Avg Return: {np.mean(recent_returns):.1f} | "
                f"Alpha: {alpha:.4f}"
            )

    return {
        "success_rates": np.array(success_rates),
        "eval_returns": np.array(eval_returns),
        "episode_successes": episode_successes,
        "final_success_rate": success_rates[-1] if success_rates else 0.0,
    }


# ═══════════════════════════════════════════════════════════════════════════════
# Evaluation: Compute CRL Metrics
# ═══════════════════════════════════════════════════════════════════════════════

def evaluate_all_tasks(
    actor: SACActorCompoNet,
    task_names: List[str],
    device: torch.device,
    seed: int,
    num_eval_episodes: int = 50,
) -> np.ndarray:
    """
    Evaluate current actor on ALL tasks seen so far.

    Returns per-task success rates needed for computing P(T), forgetting, etc.
    """
    actor.eval()
    success_rates = np.zeros(len(task_names))

    for i, task_name in enumerate(task_names):
        env = make_metaworld_env(task_name, seed=seed + 1000 + i)
        successes = 0
        for ep in range(num_eval_episodes):
            obs, _ = env.reset()
            done = False
            while not done:
                with torch.no_grad():
                    obs_t = torch.FloatTensor(obs).unsqueeze(0).to(device)
                    action, _ = actor.get_action(obs_t)
                    action = action.cpu().numpy()[0]
                obs, _, terminated, truncated, info = env.step(action)
                done = terminated or truncated
                if info.get("success", 0.0) > 0.5:
                    successes += 1
                    break
        success_rates[i] = successes / num_eval_episodes
        env.close()

    actor.train()
    return success_rates


# ═══════════════════════════════════════════════════════════════════════════════
# Full CW20 Training Pipeline
# ═══════════════════════════════════════════════════════════════════════════════

def run_cw20_seed(seed: int, device: torch.device, output_dir: Path) -> Dict:
    """
    Run the full CW20 sequence for one seed.

    Pipeline per task:
      1. Create Meta-World env for the current task
      2. Train SAC for 1M timesteps
      3. Record end-of-task success rate on current task
      4. At task boundary:
         a. Freeze current CompoNet module (grow_componet or componet.task_transition)
         b. Reset SAC critic (fresh qf1, qf2, targets) — H02
         c. Reset replay buffer
      5. After all 20 tasks: evaluate all tasks to get P(T) and forgetting

    Returns:
        results: Dict with all CRL metrics (AUC, FTr, P(T), forgetting)
    """
    print(f"\n{'='*70}")
    print(f"  SEED {seed} — CW20 Meta-World SAC + CompoNet")
    print(f"{'='*70}")

    torch.manual_seed(seed)
    np.random.seed(seed)

    seed_dir = output_dir / f"seed_{seed}"
    seed_dir.mkdir(parents=True, exist_ok=True)

    # ── Initialize CompoNet (d_model=256, d_enc=39, continuous=True) ──
    componet = CompoNet(
        d_enc=OBS_DIM,
        action_dim=ACT_DIM,
        d_model=D_MODEL,
        continuous=True,
        num_internal_layers=2,
    ).to(device)

    actor = SACActorCompoNet(componet).to(device)

    # ── Initialize SAC critic (H02: fresh critic for first task) ──
    qf1, qf2, qf1_target, qf2_target = reset_sac_critic(OBS_DIM, ACT_DIM, str(device))

    # ── Tracking across all tasks ──
    all_task_metrics = []        # per-task training metrics
    end_of_task_success = []     # p_i(i*Delta) for forgetting computation
    per_task_success_rates = []  # success_rates arrays for AUC/FTr

    for task_idx, task_name in enumerate(CW20_TASKS):
        print(f"\n--- Task {task_idx}/{len(CW20_TASKS)-1}: {task_name} ---")

        # Create environment
        env = make_metaworld_env(task_name, seed=seed)

        # Train SAC for 1M timesteps on this task
        task_metrics = train_sac_one_task(
            actor=actor,
            qf1=qf1, qf2=qf2,
            qf1_target=qf1_target, qf2_target=qf2_target,
            env=env,
            task_idx=task_idx,
            device=device,
            total_timesteps=1_000_000,
            eval_interval=10_000,
        )

        all_task_metrics.append(task_metrics)
        end_of_task_success.append(task_metrics["final_success_rate"])
        per_task_success_rates.append(task_metrics["success_rates"])

        # Save task checkpoint
        torch.save({
            "actor_state_dict": actor.state_dict(),
            "task_idx": task_idx,
            "task_name": task_name,
            "metrics": task_metrics,
        }, seed_dir / f"task_{task_idx:02d}_{task_name}.pt")

        env.close()

        # ── Task boundary transition (skip after last task) ──
        if task_idx < len(CW20_TASKS) - 1:
            print(f"  [Transition] Freezing module {task_idx}, adding new module...")

            # Freeze current CompoNet module and add new one
            componet.task_transition()

            # H02: Reset SAC critic (fresh Q-networks with random init)
            qf1, qf2, qf1_target, qf2_target = reset_sac_critic(
                OBS_DIM, ACT_DIM, str(device)
            )

            # Rebuild actor optimizer (new module parameters)
            print(f"  [Transition] Critic reset. CompoNet now has "
                  f"{len(componet.modules_list)} modules "
                  f"({len(componet.modules_list)-1} frozen + 1 active)")

    # ── Final evaluation on all tasks: compute P(T) ──
    print(f"\n--- Final evaluation on all {len(CW20_TASKS)} tasks ---")
    final_success = evaluate_all_tasks(
        actor, CW20_TASKS, device, seed, num_eval_episodes=50
    )

    # ── Compute CRL metrics (Section 5.1) ──
    # Average Performance P(T)
    avg_perf = compute_average_performance(final_success)

    # Forward Transfer (requires baseline AUC — computed separately or loaded)
    # For standalone runs, we compute AUC per task from training success rates
    per_task_auc = [compute_auc(sr) for sr in per_task_success_rates]

    # Forgetting: F_i = p_i(i*Delta) - p_i(T)
    forgetting = compute_forgetting(
        np.array(end_of_task_success),
        final_success[:len(end_of_task_success)],
    )

    results = {
        "seed": seed,
        "avg_performance": avg_perf,
        "per_task_auc": per_task_auc,
        "end_of_task_success": end_of_task_success,
        "final_success_rates": final_success.tolist(),
        "forgetting": forgetting.tolist(),
        "avg_forgetting": float(np.mean(forgetting)),
    }

    # Save results
    with open(seed_dir / "results.json", "w") as f:
        json.dump(results, f, indent=2)

    print(f"\n  Seed {seed} Results:")
    print(f"    P(T) = {avg_perf:.3f}")
    print(f"    Avg Forgetting = {float(np.mean(forgetting)):.3f}")
    print(f"    Mean AUC = {np.mean(per_task_auc):.3f}")

    return results


# ═══════════════════════════════════════════════════════════════════════════════
# Multi-Seed Aggregation
# ═══════════════════════════════════════════════════════════════════════════════

def aggregate_results(all_results: List[Dict]) -> Dict:
    """
    Aggregate results across seeds (mean +/- std).

    Reports metrics in the format of Table 1:
      - Performance: mean +/- std of P(T) across seeds
      - Forward Transfer: mean +/- std of FTr across seeds
    """
    perfs = [r["avg_performance"] for r in all_results]
    forgetting = [r["avg_forgetting"] for r in all_results]

    summary = {
        "num_seeds": len(all_results),
        "performance_mean": float(np.mean(perfs)),
        "performance_std": float(np.std(perfs)),
        "forgetting_mean": float(np.mean(forgetting)),
        "forgetting_std": float(np.std(forgetting)),
    }

    print(f"\n{'='*70}")
    print(f"  AGGREGATED RESULTS ({len(all_results)} seeds)")
    print(f"{'='*70}")
    print(f"  Performance P(T): {summary['performance_mean']:.2f} +/- {summary['performance_std']:.2f}")
    print(f"  Forgetting:       {summary['forgetting_mean']:.2f} +/- {summary['forgetting_std']:.2f}")
    print(f"  (Table 1 reference — CompoNet: 0.42 +/- 0.49 performance, 0.01 +/- 0.14 fwd transfer)")

    return summary


# ═══════════════════════════════════════════════════════════════════════════════
# Main Entry Point
# ═══════════════════════════════════════════════════════════════════════════════

def parse_args():
    parser = argparse.ArgumentParser(
        description="SAC + CompoNet training on Meta-World CW20 (Table 1 reproduction)"
    )
    parser.add_argument("--seed", type=int, default=0, help="Starting seed")
    parser.add_argument("--num-seeds", type=int, default=10,
                        help="Number of seeds to run (10 for main results)")
    parser.add_argument("--device", type=str, default="cuda",
                        choices=["cuda", "cpu"], help="Device")
    parser.add_argument("--output-dir", type=str, default="results/metaworld_sac",
                        help="Output directory for checkpoints and results")
    parser.add_argument("--timesteps-per-task", type=int, default=1_000_000,
                        help="Training timesteps per task (default: 1M)")
    return parser.parse_args()


def main():
    args = parse_args()
    device = torch.device(args.device if torch.cuda.is_available() else "cpu")
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    print("=" * 70)
    print("  SAC + CompoNet — Meta-World CW20 Continual RL")
    print("  Reference: Malagon et al., ICML 2024, Table 1")
    print("=" * 70)
    print(f"  Device:        {device}")
    print(f"  Seeds:         {args.seed} to {args.seed + args.num_seeds - 1}")
    print(f"  Tasks:         {len(CW20_TASKS)} (CW20: 10 unique x 2 reps)")
    print(f"  Steps/task:    {args.timesteps_per_task:,}")
    print(f"  Output:        {output_dir}")
    print(f"  CompoNet:      d_enc={OBS_DIM}, d_model={D_MODEL}, |A|={ACT_DIM}")
    print(f"  SAC:           lr=1e-3, gamma=0.99, tau=0.005, alpha=0.2 (auto)")
    print("=" * 70)

    all_results = []
    for seed in range(args.seed, args.seed + args.num_seeds):
        results = run_cw20_seed(seed, device, output_dir)
        all_results.append(results)

    # Aggregate and save
    summary = aggregate_results(all_results)
    with open(output_dir / "summary.json", "w") as f:
        json.dump(summary, f, indent=2)
    with open(output_dir / "all_results.json", "w") as f:
        json.dump(all_results, f, indent=2)

    print(f"\nResults saved to {output_dir}/")


if __name__ == "__main__":
    main()
