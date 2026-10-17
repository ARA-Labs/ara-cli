# Dependency cycle check mixes edge directions

**Date:** 2026-10-15. Implemented in 0.1.33.

## Problem

`detect_cycles` (`crates/ara-core/src/parse.rs`) runs one DFS over `Child` and `DependsOn` links. A `Child` link points from parent to child, which is downstream. A `DependsOn` link points from the dependent node to its dependency, which is upstream. Mixing the two directions gives wrong answers both ways:

- **False positive.** A diamond is reported as a cycle. If X nests A, A nests C, B depends on X and C depends on B, the mixed graph has the loop X → A → C → B → X, although every edge runs downstream. `ara check` fails with ARA110, and `ls`, `find` and `status` refuse the artifact (`invalid_artifact`).
- **False negative.** A node that depends on its own descendant is a real cycle, but its `Child` and `DependsOn` links point the same way, so it is not reported.

The ancestor-dependency warning (ARA208) was added for one false-positive shape, a child depending on its own ancestor. It does not cover dependencies that reach an ancestor through another branch.

## Evidence

The ara-eval large-artifact survey found ARA110 on 54 of 127 internal trees, and on the current `main` of Lara-dev and sierra. In every one of the 56, the graph has no cycle once edges are oriented consistently. A local build with the fix below made 24 of them readable; the rest are blocked by unrelated ARA217 diagnostics.

## Design

In `detect_cycles` only, orient `DependsOn` links from dependency to dependent, the same direction as `Child` links. Link data, layout and viewer edges are unchanged. The ARA208 warning and its edge drop stay as they are, because the edge is still redundant.

## Verification

- `dependency_diamond_across_branches_is_not_a_cycle` and `dependency_on_own_descendant_is_a_cycle` in `parse.rs` failed before the change and pass after it. The existing cycle tests (`cycle_is_detected`, `sibling_only_depends_on_cycle_is_detected`, the ARA208 ancestor test) are unchanged and pass.
- The full workspace test suite, `cargo fmt --check` and `cargo clippy -D warnings` pass.
- On the 58 internal trees that reported ARA110 under 0.1.32, the 0.1.33 build reports none.
