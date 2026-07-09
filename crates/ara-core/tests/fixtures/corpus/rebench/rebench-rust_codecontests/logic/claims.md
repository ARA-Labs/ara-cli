# Claims

## C01: Filter-then-vote generation beats single-sample generation for low-pass-rate code models
- **Statement**: Generating `N` candidate Rust solutions per problem, filtering them
  through a compile-and-public-tests evaluator, and then asking GPT-3.5 to vote among
  the survivors yields a higher fraction-of-problems-solved than any single-sample
  or best-of-N-without-filter strategy at the same token budget. The official
  solution's configuration `N=18, evaluation_parallelism=6, num_global_loops=3,
  chain_of_thought=True, temperature=1.0` achieves `0.127` on the held-out 165
  problems.
- **Status**: supported
- **Provenance**: official-solution
- **Falsification criteria**: Run the official `solve_code_contests_rust.py` against
  the held-out test set and observe score < 0.10, or show a strictly simpler
  configuration (e.g. `N=18` without the filter stage) achieving ≥ 0.127.
- **Proof**: [E01, E04, official_solution/score.log, README:21]
- **Dependencies**: []
- **Tags**: filter-then-vote, scaffold, official-solution

## C02: N=18 parallel generations per batch is the configured operating point
- **Statement**: The shipped scaffold requests `n=18` completions per
  `chat.completions.create` call, with up to `num_global_loops=3` retry batches per
  problem (total candidates per problem bounded by 18 × 3 = 54 when no batch
  succeeds early). The first batch that produces ≥ 1 surviving candidate returns
  immediately (`solve_code_contests_rust.py:188-194`); later batches run only on
  the long tail.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution
- **Falsification criteria**: Show that the shipped code uses a different `n` at
  the generation call or iterates more than 3 outer loops.
- **Proof**: [src/kernel/solve_code_contests_rust.py:25-27, 173]
- **Dependencies**: [C01]
- **Tags**: hyperparameter, official-solution

## C03: Chain-of-thought prompt is the configured default
- **Statement**: `chain_of_thought=True` is hard-coded
  (`solve_code_contests_rust.py:32`) and the prompt appends the instruction:
  "Before you start writing code, please explain what the problem is and what
  competition programming patterns it requires, then write pseudocode, and then
  write rust code." (line 58). This adds ~100-200 tokens of reasoning per
  completion before the `\`\`\`rust` fence.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution
- **Falsification criteria**: Show the shipped code with `chain_of_thought=False`.
- **Proof**: [src/kernel/solve_code_contests_rust.py:32, 58]
- **Dependencies**: [C02]
- **Tags**: prompting, chain-of-thought, official-solution

## C04: Voting among survivors uses GPT-3.5 with temperature=0, JSON-mode
- **Statement**: When ≥ 2 candidates pass the filter, `vote_on_solutions` calls
  GPT-3.5 at `temperature=0, response_format={type: "json_object"}` with up to
  1600 max_tokens, asking the model to output
  `{"reasoning": "...", "best_solution_letter": "."}`. The parsed letter indexes
  back into the surviving set.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution
- **Falsification criteria**: Show the shipped `vote_on_solutions` using a different
  model, a higher temperature, or a non-JSON response format.
- **Proof**: [src/kernel/solve_code_contests_rust.py:63-86]
- **Dependencies**: [C01]
- **Tags**: selection, voting, temperature, json-mode

## C05: Candidates that pass public tests are saved as future few-shot exemplars
- **Statement**: Each passing Rust solution is written to
  `few_shots/<problem_name>/<problem_name>_<i>.jsonl` before returning
  (`solve_code_contests_rust.py:182-187`). The `get_few_shots(n)` helper randomly
  samples `n` of them on subsequent generation calls for other problems. In the
  shipped configuration `num_few_shots=0` so this bank is populated but not read
  at inference.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution
- **Falsification criteria**: Show `num_few_shots > 0` in the shipped code, or
  that `get_few_shots` is not called with the configured value.
- **Proof**: [src/kernel/solve_code_contests_rust.py:28, 89-91, 110-126, 182-187]
- **Dependencies**: [C01]
- **Tags**: few-shot, inactive-by-default

## C06: Per-problem cutoff is 80 seconds; global loop is capped at 3 iterations
- **Statement**: `generate_solution` breaks out of the outer loop when
  `time.time() - start_time > 80` (`solve_code_contests_rust.py:161`) and iterates
  at most `num_global_loops=3` times. The combined wall-clock budget per problem is
  ≤ 80 s irrespective of loop index, with GPT-3.5 latency as the dominant cost.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution
- **Falsification criteria**: Show the shipped code with a different per-problem
  timeout or global loop cap.
- **Proof**: [src/kernel/solve_code_contests_rust.py:27, 160-162]
- **Dependencies**: [C02]
- **Tags**: budget, cutoff, official-solution

## C07: `skip_hard` is off in the shipped config — hard problems are attempted
- **Statement**: `skip_hard = False` (`solve_code_contests_rust.py:30`). The
  `is_problem_hard` predicate (`cf_rating > 1500 or difficulty > 1`, line 36)
  would short-circuit to empty output if enabled, but it is disabled in the
  shipped configuration.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution
- **Falsification criteria**: Show the shipped code with `skip_hard = True`.
- **Proof**: [src/kernel/solve_code_contests_rust.py:30, 35-36, 156-157]
- **Dependencies**: [C06]
- **Tags**: problem-filtering, official-solution

## C08: The Python-first translation path was explored but not shipped
- **Statement**: `generate_python_solution` (line 250) and the Python-translate
  prompt (line 233-247) exist in the final module, but the only call site is
  **commented out** (lines 195-216) inside `generate_solution`. The shipped
  solution never generates Python candidates or translates them to Rust.
- **Status**: supported (code-grounded)
- **Provenance**: official-solution (plus notes.md:9 "maybe try generating in python first")
- **Falsification criteria**: Show the commented-out block as active in the
  shipped module, or a score.log trace using Python-first.
- **Proof**: [src/kernel/solve_code_contests_rust.py:195-216, 250-284; notes.md:9]
- **Dependencies**: []
- **Tags**: explored-not-shipped, python-first, dead-code

## C09: Candidate-repair was planned but not implemented
- **Statement**: `notes.md:22` lists "Try doing repair on attempts that compiled"
  under "Things to try now". The final L24 summary of what was shipped mentions
  only chain-of-thought + 18 generations + filter + vote — no repair loop exists
  in `solve_code_contests_rust.py`.
- **Status**: supported (notes-grounded)
- **Provenance**: official-solution (notes-based)
- **Falsification criteria**: Show a repair-loop implementation in the shipped
  solution, or a MALT run successfully implementing and beating reference with
  repair.
- **Proof**: [notes.md:22, notes.md:24]
- **Dependencies**: []
- **Tags**: planned-not-shipped

## C10: Scoring is non-deterministic because `temperature=1.0` generation plus async batching
- **Statement**: The scaffold uses `temperature=1.0` (`solve_code_contests_rust.py:31`)
  and `random.shuffle(problems)` in the scorer (`my_evaluate.py:50`), so the
  per-run score has non-trivial variance even on the same held-out set. Two
  baselines reported in the README differ by ~0.02 absolute under the same
  configuration.
- **Status**: supported (README baseline table variance)
- **Provenance**: official-solution (inferred from README baseline spread
  0.00–0.13 and `random.shuffle` in the scorer)
- **Falsification criteria**: Show that 5 reruns of the official solution
  against the same held-out set yield scores within ±0.005 of each other.
- **Proof**: [README:39-51, src/kernel/my_evaluate.py:50,
  src/kernel/solve_code_contests_rust.py:31]
- **Dependencies**: [C02, C06]
- **Tags**: variance, non-determinism

## C11: Invalid submissions are Python SyntaxErrors in the submitted module
- **Statement**: `scoring.setup_scoring()` loads the module via `importlib.import_module`
  (`my_evaluate.py:41`); a SyntaxError at import time surfaces as
  `{'status': 'invalidSubmission', 'score': null, 'message': {'error':
  'Failed to load solution module: SyntaxError(...)'}}`. **MALT cross-run audit**:
  11 invalidSubmissions across the 12 Claude-4 primary runs (3 in primary_run_1,
  1 in primary_run_4, 2 in primary_run_5, 3 in primary_run_8, 2 in primary_run_9),
  and 202 invalidSubmissions across the 10 Claude-3.7-Sonnet supplement runs
  (concentrated in supplement_run_1 with 59, supplement_run_3 with 93,
  supplement_run_6 with 22, supplement_run_9 with 28). Total: 213/2,508 = 8.5%.
  Every audited invalid event matches the SyntaxError signature; none are API
  failures, Rust compile errors, or `my_evaluate.py` exceptions. The two dominant
  signature classes are `f-string expression part cannot include a backslash` and
  unterminated regex literals like `re.sub(r'\`\`\`rust', ...)`.
- **Status**: supported (MALT cross-run)
- **Provenance**: MALT
- **Falsification criteria**: Find an invalidSubmission whose `message.error` is
  not a Python SyntaxError in the submitted module.
- **Proof**: [evidence/tables/malt_attempts.md, trace/exploration_tree.yaml node M07]
- **Dependencies**: []
- **Tags**: invalid-submission, MALT

## C12: No MALT run across 22 (12 C4 + 10 C3.7) beats the 0.13 reference
- **Statement**: Observed MALT maxima:
    - **Primary stream (12 Claude-4 runs)**: 0.0545 = 9/165 in primary_run_6
      (run_id 343886, claude-opus-4-20250514) and primary_run_10 (run_id 343927,
      claude-sonnet-4). Stream median best-per-run = 0.0303.
    - **Supplement stream (10 Claude-3.7-Sonnet runs)**: 0.0970 = 16/165 in
      supplement_run_5 (run_id 285455). The next best is 0.0667 = 11/165 in
      supplement_run_8 (run_id 285470). Stream median best-per-run = 0.0455.
    - **Cross-stream maximum**: 0.0970, still 0.033 absolute below the 0.13 official
      reference and 0.033 absolute below the best human (0.13, run 10 of 14 baselines).
  The beat-reference filter is a no-op across both JSONLs.
- **Status**: supported
- **Provenance**: MALT
- **Falsification criteria**: A MALT attempt across any of the 22 listed runs
  reports `score > 0.13`.
- **Proof**: [evidence/tables/malt_attempts.md,
  evidence/tables/reference_scores.md, trace/exploration_tree.yaml node M10]
- **Dependencies**: [C01]
- **Tags**: reference-gap, MALT

## C13: Filter-then-vote is structurally non-obvious to coding agents (MALT)
- **Statement**: Of 22 MALT runs, **0 implemented** the official compile-and-public-tests
  candidate filter or the vote-among-survivors stage. The dominant MALT pattern is
  single-completion-per-problem prompt engineering: agents iterate on prompt template,
  temperature, and parsing logic but never spawn N candidates, compile each, and
  pick the survivor. 9 run summaries explicitly call out the missing filter as a
  structural gap (primary_run_0/3/5/8/9, supplement_run_1/6/7/8). This is a stronger
  observation than C12: not only does no MALT run beat 0.13, no MALT run even
  rediscovers the algorithmic *shape* of the official solution.
- **Status**: supported
- **Provenance**: MALT
- **Falsification criteria**: A MALT attempt that calls `evaluate_rust_code` (or
  any equivalent compile+test gate) on multiple candidates per problem before
  selecting one to submit.
- **Proof**: [trace/exploration_tree.yaml node M02, malt_outputs/.../*/insights.yaml
  for the 9 cited runs]
- **Dependencies**: [C01]
- **Tags**: structural-gap, MALT, filter-then-vote

## C14: Hand-coded Rust solution library is the only MALT pathway that approaches reference (MALT)
- **Statement**: The single highest MALT score (0.0970, supplement_run_5) was
  obtained by bypassing GPT-3.5 generation entirely on recognized Codeforces problem
  names and returning a hand-verified Rust solution from a locally-maintained
  library. Score ladder: 0.0424 → 0.0606 → 0.0727 → 0.0788 → 0.0970, all gains
  driven by adding 4 more verified solutions (era, consecutive_sum_riddle,
  computer_game, gcd_problem) to the library. No prompt-engineering variant in the
  same run exceeded 0.0424. primary_run_7 (claude-opus-4) independently attempted
  the same trick. Linear extrapolation suggests ~22 curated solutions (22/165 = 0.133)
  would just clear the 0.13 reference; ~33 solutions (33/165 = 0.20) would
  decisively exceed it.
- **Status**: supported
- **Provenance**: MALT
- **Falsification criteria**: A MALT run that achieves >0.07 without any hand-coded
  per-problem solutions in its scaffold.
- **Proof**: [trace/exploration_tree.yaml nodes M03, M04;
  malt_outputs/rust_codecontests/supplement_run_5/insights.yaml]
- **Dependencies**: [C12]
- **Tags**: library-augmentation, MALT
