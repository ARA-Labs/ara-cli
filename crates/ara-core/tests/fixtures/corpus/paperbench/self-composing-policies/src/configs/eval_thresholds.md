# Evaluation Success Thresholds

## Overview
Success score = 90% of average final episodic return across all 8 methods (10 random seeds per method).
If the episodic return in a task is greater or equal to this score, the task is considered solved (success = 1), otherwise 0.

**Source**: Table D.1a and Table D.1b, Appendix D.4

---

## SpaceInvaders (ALE/SpaceInvaders-v5, 10 playing modes)
- **Random policy baseline episodic return**: 148.0
- **All success scores exceed the random baseline**

| Task 0 | Task 1 | Task 2 | Task 3 | Task 4 | Task 5 | Task 6 | Task 7 | Task 8 | Task 9 |
|--------|--------|--------|--------|--------|--------|--------|--------|--------|--------|
| 340.94 | 366.762 | 391.16 | 386.99 | 379.41 | 383.73 | 393.83 | 367.98 | 484.23 | 456.19 |

---

## Freeway (ALE/Freeway-v5, 7 playing modes)
- **Random policy baseline episodic return**: 0.0 (sparse reward environment)

| Task 0 | Task 1 | Task 2 | Task 3 | Task 4 | Task 5 | Task 6 | Task 7 |
|--------|--------|--------|--------|--------|--------|--------|--------|
| 16.65 | 15.1 | 8.27 | 17.09 | 18.54 | 9.43 | 9.14 | 13.96 |
