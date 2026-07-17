# Environment

## Python
- **Version**: Not specified in paper

## Framework
- **RL Library**: CleanRL (Huang et al., 2022) — high-quality single-file RL implementations; SAC and PPO implementations adapted with minimal changes
- **Deep Learning**: PyTorch (version not specified in paper)

## Environments

### Meta-World
- **Library**: Metaworld (Farama Foundation, github.com/Farama-Foundation/Metaworld)
- **Version**: v2 environments (hammer-v2, push-wall-v2, etc.) — v2 used due to v1 deprecation issues
- **Interface**: gymnasium (recommended replacement for gym)
- **Tasks**: 10 tasks × 2 repetitions = 20 total; Δ = 1M timesteps per task

### ALE (SpaceInvaders, Freeway)
- **Library**: Gymnasium (Farama Foundation, gymnasium.farama.org)
- **Environments**: ALE/SpaceInvaders-v5 (10 playing modes), ALE/Freeway-v5 (7 playing modes)
- **Observation**: 210×160 RGB images
- **Timesteps**: Δ = 1M per task

## Hardware

### Cluster Node 1
- **GPU**: 8× NVIDIA RTX3090
- **CPU**: Intel Xeon Silver 4210R
- **RAM**: 345GB

### Cluster Node 2
- **GPU**: 8× NVIDIA A5000
- **CPU**: AMD EPYC 7252
- **RAM**: 377GB

### Scalability Measurements (Figure 3, Appendix C.2)
- **GPU**: NVIDIA A5000
- **CPU**: AMD EPYC 7252

## Key Dependencies
- **Farama Metaworld**: github.com/Farama-Foundation/Metaworld
- **Gymnasium**: gymnasium.farama.org
- **CleanRL**: Reference implementation for SAC and PPO

## Execution Times
- **SpaceInvaders**: ~1.5 hours per task
- **Freeway**: ~1.5 hours per task
- **Meta-World**: ~3 hours per task

## Random Seeds
- **Number**: 10 random seeds per method per task sequence for main results (Table 1)
- **Number**: 5 random seeds for ablation studies (Appendix G) and scalability experiments (Appendix H)
- **Number**: 3 random seeds for forward transfer matrix computations (Appendix D.5)
- **Specific values**: Not specified in paper
