"""
CRL Evaluation Metrics (Section 5.1).

Implements:
  - Average Performance P(T)
  - Forward Transfer FTr_i and mean FTr
  - Reference Forward Transfer RT
  - Forgetting F_i

All metrics defined per Wolczyk et al. (2021, 2022) and Section 5.1 of the paper.
"""

import numpy as np
from typing import List, Optional


def compute_auc(success_rates: np.ndarray) -> float:
    """
    Compute area under the success rate curve (AUC) for a single task.

    AUC_i = (1/Delta) * integral_{(i-1)*Delta}^{i*Delta} p_i(t) dt

    Approximated by the mean of recorded success rates during the task's training window.

    Args:
        success_rates: Array of success rate values recorded during task i's training [T_i]

    Returns:
        auc: Scalar AUC value in [0, 1]
    """
    return float(np.mean(success_rates))


def compute_forward_transfer(
    method_success_rates: List[np.ndarray],   # success rates during each task's training window
    baseline_success_rates: List[np.ndarray], # baseline success rates during same windows
) -> List[float]:
    """
    Compute per-task forward transfer FTr_i.

    FTr_i = (AUC_i - AUC_i^b) / (1 - AUC_i^b)

    Args:
        method_success_rates: List of length N; each element is array of success rates
                               recorded during task i's training window for the method
        baseline_success_rates: Same structure for the baseline (train-from-scratch) method

    Returns:
        ftr_list: List of N forward transfer values
    """
    ftr_list = []
    for sr_method, sr_baseline in zip(method_success_rates, baseline_success_rates):
        auc_i = compute_auc(sr_method)
        auc_b = compute_auc(sr_baseline)
        denom = 1.0 - auc_b
        if abs(denom) < 1e-8:
            # Baseline already solves the task: forward transfer is 0 by convention
            ftr = 0.0
        else:
            ftr = (auc_i - auc_b) / denom
        ftr_list.append(ftr)
    return ftr_list


def compute_average_performance(
    final_success_rates: np.ndarray,  # [N] final success rates at end of sequence
) -> float:
    """
    Compute average performance P(T).

    P(T) = (1/N) * sum_{i=1}^{N} p_i(T)

    Args:
        final_success_rates: Array of shape [N] containing p_i(T) for each task

    Returns:
        avg_perf: Scalar average performance in [0, 1]
    """
    return float(np.mean(final_success_rates))


def compute_reference_forward_transfer(
    ftr_matrix: np.ndarray,  # [N, N] FTr matrix; ftr_matrix[j, i] = FTr(j, i) for j < i
) -> float:
    """
    Compute Reference Forward Transfer (RT).

    RT = (1/N) * sum_{i=2}^{N} max_{j < i} FTr(j, i)

    Args:
        ftr_matrix: Square matrix of shape [N, N]; entry [j, i] is the forward transfer
                    obtained by training on task j from scratch and fine-tuning on task i.
                    Only entries with j < i are used (upper triangle).

    Returns:
        rt: Scalar reference forward transfer
    """
    N = ftr_matrix.shape[0]
    max_ftr_per_task = []
    for i in range(1, N):  # tasks 2..N (0-indexed: 1..N-1)
        max_ftr = max(ftr_matrix[j, i] for j in range(i))
        max_ftr_per_task.append(max_ftr)
    return float(np.mean(max_ftr_per_task))


def compute_forgetting(
    end_of_task_success: np.ndarray,  # [N] p_i(i * Delta) for each task i
    final_success: np.ndarray,         # [N] p_i(T) for each task i
) -> np.ndarray:
    """
    Compute per-task forgetting F_i.

    F_i = p_i(i * Delta) - p_i(T)

    Positive F_i: method forgot task i.
    Negative F_i: backward transfer (method improved on task i without revisiting it).

    Args:
        end_of_task_success: Array [N] of success rates immediately after completing task i
        final_success: Array [N] of success rates at the end of the full sequence

    Returns:
        forgetting: Array [N] of forgetting values
    """
    return end_of_task_success - final_success
