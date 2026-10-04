# Phase 4 zero-cost record

Policy: [zero-cost-measurement.md](../../../collaborative-research/zero-cost-measurement.md) (sha256 `fdc31c732da18cd2…`), full repetitions.

| | Binary | SHA-256 |
|---|---|---|
| Parent | ara 0.1.24, code of `feat/collaborative-ara` at `fc3fce6` | `267f2c1a21bcfb0adeaa738c997e65e95608ce3cd4d6fd9df4d9fd6c2eec58d8` |
| Candidate | ara 0.1.25, the phase-4 branch | `b948708397eeca772e9eb2bba5d2f3b4bd906bec6c8e04d71d08ba12f0fa9e0e` |

Machine: macOS-27.0.1-arm64-arm-64bit, 18 CPUs, outside the Claude Code sandbox.

**Result: PASS, gate-valid.** 67 cases, 0 confirmed regressions, 0 failed output or access checks, 0 infeasible items. The worst candidate/parent median ratio is 1.045.

- **Existing imports:** the first import into M and the 41st linear import into F3 (plan 04's "existing simple and long linear-history imports") stay within tolerance and give byte-identical normalized output. No import metadata from another key is present in either, so the merger takes the unchanged path.
- **Ordinary reads and writes:** outputs are identical under every store state, and `.ara/` changes only at `lock` and `transactions/`.
- **Build:** no new crates. Release binary 13,906,016 → 14,044,784 bytes (limit 14,249,672).
- **Run by hand:** the wasm32 build of `ara-core` and `ara-wasm` passes.

| Command | Fixture | State | Parent median ms | Candidate median ms | Ratio | Parent RSS MiB | Candidate RSS MiB | Verdict |
|---|---|---|---|---|---|---|---|---|
| status --json | F1 | S0 | 14.7 | 14.6 | 0.992 | 13.8 | 13.8 | pass |
| ls --json | F1 | S0 | 15.6 | 15.5 | 0.996 | 16.8 | 16.8 | pass |
| show C01 --json | F1 | S0 | 13.4 | 13.4 | 1.000 | 13.5 | 13.6 | pass |
| find "method" --json | F1 | S0 | 24.4 | 24.4 | 1.000 | 17.9 | 18.1 | pass |
| validate <dir> --json | F1 | S0 | 13.1 | 13.1 | 0.997 | 13.1 | 13.2 | pass |
| check <dir> | F1 | S0 | 13.6 | 13.5 | 0.995 | 13.3 | 13.3 | pass |
| status --json | F1 | S1 | 14.9 | 14.9 | 0.998 | 13.8 | 13.8 | pass |
| ls --json | F1 | S1 | 15.7 | 15.6 | 0.992 | 16.8 | 16.7 | pass |
| show C01 --json | F1 | S1 | 13.3 | 13.3 | 0.999 | 13.6 | 13.6 | pass |
| find "method" --json | F1 | S1 | 24.3 | 24.2 | 0.998 | 17.9 | 18.0 | pass |
| validate <dir> --json | F1 | S1 | 13.4 | 13.4 | 0.998 | 13.1 | 13.2 | pass |
| check <dir> | F1 | S1 | 13.9 | 13.8 | 0.995 | 13.1 | 13.3 | pass |
| status --json | F1 | S2 | 14.7 | 14.7 | 1.004 | 13.9 | 13.9 | pass |
| ls --json | F1 | S2 | 16.1 | 16.1 | 1.003 | 17.0 | 16.8 | pass |
| show C01 --json | F1 | S2 | 13.3 | 13.3 | 0.999 | 13.5 | 13.5 | pass |
| find "method" --json | F1 | S2 | 24.2 | 24.2 | 1.000 | 17.9 | 18.0 | pass |
| validate <dir> --json | F1 | S2 | 13.1 | 13.1 | 0.999 | 13.2 | 13.2 | pass |
| check <dir> | F1 | S2 | 13.4 | 13.4 | 0.998 | 13.2 | 13.3 | pass |
| status --json | F1 | S3 | 15.5 | 15.4 | 0.999 | 13.8 | 13.8 | pass |
| ls --json | F1 | S3 | 16.4 | 16.3 | 0.994 | 16.8 | 16.8 | pass |
| show C01 --json | F1 | S3 | 14.2 | 14.1 | 0.995 | 13.5 | 13.5 | pass |
| find "method" --json | F1 | S3 | 24.9 | 24.8 | 0.998 | 17.8 | 18.0 | pass |
| validate <dir> --json | F1 | S3 | 13.8 | 13.9 | 1.005 | 13.3 | 13.3 | pass |
| check <dir> | F1 | S3 | 14.4 | 14.2 | 0.992 | 13.3 | 13.3 | pass |
| status --json | F2 | S0 | 15.4 | 15.4 | 1.002 | 14.1 | 14.2 | pass |
| ls --json | F2 | S0 | 15.7 | 15.6 | 0.997 | 16.8 | 16.8 | pass |
| show C01 --json | F2 | S0 | 13.3 | 13.3 | 1.000 | 13.5 | 13.5 | pass |
| find "method" --json | F2 | S0 | 23.9 | 24.0 | 1.002 | 17.9 | 18.0 | pass |
| validate <dir> --json | F2 | S0 | 13.2 | 13.2 | 1.004 | 13.2 | 13.3 | pass |
| check <dir> | F2 | S0 | 13.4 | 13.4 | 1.000 | 13.2 | 13.2 | pass |
| status --json | F2 | S1 | 15.3 | 15.3 | 0.997 | 14.1 | 14.1 | pass |
| ls --json | F2 | S1 | 15.4 | 15.5 | 1.003 | 16.9 | 16.6 | pass |
| show C01 --json | F2 | S1 | 13.3 | 13.3 | 0.997 | 13.5 | 13.5 | pass |
| find "method" --json | F2 | S1 | 23.9 | 23.9 | 0.999 | 17.8 | 18.0 | pass |
| validate <dir> --json | F2 | S1 | 13.2 | 13.3 | 1.009 | 13.2 | 13.2 | pass |
| check <dir> | F2 | S1 | 13.6 | 13.5 | 0.997 | 13.3 | 13.3 | pass |
| status --json | F2 | S2 | 15.3 | 15.2 | 0.995 | 14.1 | 14.1 | pass |
| ls --json | F2 | S2 | 15.5 | 15.5 | 1.000 | 16.8 | 16.8 | pass |
| show C01 --json | F2 | S2 | 13.3 | 13.2 | 0.997 | 13.5 | 13.5 | pass |
| find "method" --json | F2 | S2 | 23.9 | 23.8 | 0.994 | 17.9 | 18.0 | pass |
| validate <dir> --json | F2 | S2 | 13.2 | 13.3 | 1.004 | 13.2 | 13.2 | pass |
| check <dir> | F2 | S2 | 13.4 | 13.4 | 0.999 | 13.2 | 13.2 | pass |
| status --json | F2 | S3 | 16.1 | 15.9 | 0.984 | 14.1 | 14.2 | pass |
| ls --json | F2 | S3 | 15.9 | 15.8 | 0.993 | 16.8 | 16.8 | pass |
| show C01 --json | F2 | S3 | 13.9 | 14.0 | 1.003 | 13.6 | 13.5 | pass |
| find "method" --json | F2 | S3 | 24.4 | 24.4 | 0.999 | 17.9 | 18.0 | pass |
| validate <dir> --json | F2 | S3 | 13.8 | 13.7 | 0.997 | 13.2 | 13.3 | pass |
| check <dir> | F2 | S3 | 14.1 | 14.1 | 1.001 | 13.1 | 13.2 | pass |
| status --json | F3 | S0 | 4.5 | 4.5 | 1.013 | 10.1 | 10.1 | pass |
| ls --json | F3 | S0 | 4.4 | 4.5 | 1.019 | 10.2 | 10.4 | pass |
| show C01 --json | F3 | S0 | 4.3 | 4.3 | 1.000 | 10.0 | 10.2 | pass |
| find "method" --json | F3 | S0 | 4.5 | 4.5 | 1.005 | 10.3 | 10.4 | pass |
| validate <dir> --json | F3 | S0 | 4.2 | 4.2 | 1.000 | 9.6 | 9.8 | pass |
| check <dir> | F3 | S0 | 4.3 | 4.3 | 1.003 | 9.8 | 9.9 | pass |
| add node | F1 | S0 | 61.8 | 62.2 | 1.007 | 16.9 | 16.8 | pass |
| apply | F1 | S0 | 61.3 | 60.8 | 0.992 | 16.8 | 16.8 | pass |
| add node | F1 | S1 | 63.6 | 63.6 | 1.001 | 16.8 | 16.8 | pass |
| apply | F1 | S1 | 61.5 | 64.3 | 1.045 | 16.9 | 16.8 | pass |
| add node | F1 | S2 | 59.9 | 60.4 | 1.008 | 16.8 | 16.8 | pass |
| apply | F1 | S2 | 60.3 | 60.3 | 1.000 | 16.9 | 16.8 | pass |
| add node | F1 | S3 | 80.5 | 79.2 | 0.984 | 16.8 | 16.7 | pass |
| apply | F1 | S3 | 80.4 | 80.2 | 0.997 | 16.9 | 16.9 | pass |
| merge first import | M | S0 | 53.5 | 54.0 | 1.009 | 16.2 | 16.1 | pass |
| merge first import | M | S1 | 51.3 | 51.0 | 0.994 | 16.1 | 16.1 | pass |
| merge first import | M | S2 | 52.1 | 52.1 | 0.999 | 16.2 | 16.1 | pass |
| merge first import | M | S3 | 62.4 | 63.0 | 1.008 | 16.2 | 16.1 | pass |
| merge 41st import | F3 | S0 | 149.0 | 149.4 | 1.003 | 124.4 | 124.3 | pass |
