# Phase 4 zero-cost record

Policy: [zero-cost-measurement.md](../../../collaborative-research/zero-cost-measurement.md) (sha256 `fdc31c732da18cd2…`), full repetitions.

| | Binary | SHA-256 |
|---|---|---|
| Parent | ara 0.1.24, code of `feat/collaborative-ara` at `fc3fce6` | `267f2c1a21bcfb0adeaa738c997e65e95608ce3cd4d6fd9df4d9fd6c2eec58d8` |
| Candidate | ara 0.1.25, the phase-4 branch including the review fixes | `80d41c1e90c4de2d3fdeb10c71fa90571d77c63124e6029178ad2bc7492d9e5b` |

Machine: macOS-27.0.1-arm64-arm-64bit, 18 CPUs, outside the Claude Code sandbox.

**Result: PASS, gate-valid.** 67 cases, 0 confirmed regressions, 0 failed output or access checks, 0 infeasible items. The worst candidate/parent median ratio is 1.035

- **Existing imports:** the first import into M and the 41st linear import into F3 (plan 04's "existing simple and long linear-history imports") stay within tolerance and give byte-identical normalized output. No import metadata from another key is present in either, so the merger takes the unchanged path.
- **Ordinary reads and writes:** outputs are identical under every store state, and `.ara/` changes only at `lock` and `transactions/`.
- **Build:** no new crates. Release binary 13,906,016 → 14,088,512 bytes (limit 14,249,672).
- **Run by hand:** the wasm32 build of `ara-core` and `ara-wasm` passes.

| Command | Fixture | State | Parent median ms | Candidate median ms | Ratio | Parent RSS MiB | Candidate RSS MiB | Verdict |
|---|---|---|---|---|---|---|---|---|
| status --json | F1 | S0 | 14.2 | 14.3 | 1.005 | 13.8 | 13.7 | pass |
| ls --json | F1 | S0 | 15.5 | 15.6 | 1.007 | 16.8 | 16.6 | pass |
| show C01 --json | F1 | S0 | 13.2 | 13.1 | 0.997 | 13.5 | 13.5 | pass |
| find "method" --json | F1 | S0 | 23.6 | 23.6 | 1.000 | 17.9 | 17.9 | pass |
| validate <dir> --json | F1 | S0 | 13.0 | 13.1 | 1.007 | 13.2 | 13.1 | pass |
| check <dir> | F1 | S0 | 13.2 | 13.3 | 1.006 | 13.3 | 13.2 | pass |
| status --json | F1 | S1 | 14.5 | 14.6 | 1.005 | 13.8 | 13.7 | pass |
| ls --json | F1 | S1 | 15.3 | 15.4 | 1.006 | 16.8 | 16.6 | pass |
| show C01 --json | F1 | S1 | 13.2 | 13.2 | 1.001 | 13.6 | 13.5 | pass |
| find "method" --json | F1 | S1 | 23.7 | 23.7 | 1.000 | 17.9 | 17.9 | pass |
| validate <dir> --json | F1 | S1 | 13.0 | 13.1 | 1.007 | 13.1 | 13.1 | pass |
| check <dir> | F1 | S1 | 13.4 | 13.4 | 1.000 | 13.2 | 13.1 | pass |
| status --json | F1 | S2 | 14.6 | 14.6 | 0.995 | 13.8 | 13.8 | pass |
| ls --json | F1 | S2 | 15.3 | 15.4 | 1.008 | 16.9 | 16.6 | pass |
| show C01 --json | F1 | S2 | 13.1 | 13.2 | 1.005 | 13.5 | 13.5 | pass |
| find "method" --json | F1 | S2 | 23.6 | 23.7 | 1.005 | 17.8 | 17.9 | pass |
| validate <dir> --json | F1 | S2 | 13.0 | 13.0 | 0.997 | 13.1 | 13.1 | pass |
| check <dir> | F1 | S2 | 13.4 | 13.4 | 1.005 | 13.3 | 13.2 | pass |
| status --json | F1 | S3 | 15.3 | 15.4 | 1.005 | 13.8 | 13.8 | pass |
| ls --json | F1 | S3 | 16.0 | 16.0 | 0.999 | 16.8 | 16.6 | pass |
| show C01 --json | F1 | S3 | 13.9 | 14.0 | 1.005 | 13.5 | 13.5 | pass |
| find "method" --json | F1 | S3 | 24.3 | 24.1 | 0.995 | 17.9 | 17.9 | pass |
| validate <dir> --json | F1 | S3 | 13.8 | 14.0 | 1.009 | 13.2 | 13.1 | pass |
| check <dir> | F1 | S3 | 14.1 | 14.2 | 1.006 | 13.2 | 13.2 | pass |
| status --json | F2 | S0 | 15.0 | 15.0 | 1.005 | 14.1 | 14.0 | pass |
| ls --json | F2 | S0 | 15.2 | 15.1 | 0.998 | 16.8 | 16.6 | pass |
| show C01 --json | F2 | S0 | 13.0 | 13.0 | 1.000 | 13.5 | 13.4 | pass |
| find "method" --json | F2 | S0 | 23.5 | 23.5 | 1.002 | 17.9 | 17.9 | pass |
| validate <dir> --json | F2 | S0 | 13.3 | 13.2 | 0.997 | 13.1 | 13.1 | pass |
| check <dir> | F2 | S0 | 13.2 | 13.3 | 1.001 | 13.3 | 13.2 | pass |
| status --json | F2 | S1 | 15.2 | 15.0 | 0.991 | 14.1 | 14.1 | pass |
| ls --json | F2 | S1 | 15.1 | 15.2 | 1.008 | 16.8 | 16.6 | pass |
| show C01 --json | F2 | S1 | 13.2 | 13.2 | 1.000 | 13.6 | 13.4 | pass |
| find "method" --json | F2 | S1 | 23.5 | 23.5 | 0.999 | 17.9 | 17.8 | pass |
| validate <dir> --json | F2 | S1 | 13.1 | 13.0 | 0.996 | 13.1 | 13.2 | pass |
| check <dir> | F2 | S1 | 13.3 | 13.3 | 1.001 | 13.3 | 13.2 | pass |
| status --json | F2 | S2 | 15.1 | 15.0 | 0.996 | 14.1 | 14.1 | pass |
| ls --json | F2 | S2 | 15.6 | 15.4 | 0.988 | 16.8 | 16.6 | pass |
| show C01 --json | F2 | S2 | 13.0 | 13.0 | 1.000 | 13.6 | 13.5 | pass |
| find "method" --json | F2 | S2 | 23.4 | 23.5 | 1.007 | 17.9 | 17.8 | pass |
| validate <dir> --json | F2 | S2 | 12.9 | 13.0 | 1.007 | 13.1 | 13.1 | pass |
| check <dir> | F2 | S2 | 13.1 | 13.1 | 1.001 | 13.2 | 13.2 | pass |
| status --json | F2 | S3 | 15.9 | 16.1 | 1.010 | 14.1 | 14.1 | pass |
| ls --json | F2 | S3 | 15.9 | 15.9 | 1.001 | 16.8 | 16.8 | pass |
| show C01 --json | F2 | S3 | 13.9 | 13.9 | 1.000 | 13.6 | 13.4 | pass |
| find "method" --json | F2 | S3 | 24.2 | 24.2 | 1.000 | 17.9 | 17.8 | pass |
| validate <dir> --json | F2 | S3 | 13.6 | 13.7 | 1.009 | 13.2 | 13.1 | pass |
| check <dir> | F2 | S3 | 14.4 | 14.4 | 0.999 | 13.2 | 13.2 | pass |
| status --json | F3 | S0 | 4.3 | 4.3 | 0.992 | 10.1 | 10.0 | pass |
| ls --json | F3 | S0 | 4.3 | 4.3 | 1.020 | 10.2 | 10.2 | pass |
| show C01 --json | F3 | S0 | 4.2 | 4.2 | 0.995 | 10.1 | 10.0 | pass |
| find "method" --json | F3 | S0 | 4.3 | 4.3 | 1.001 | 10.3 | 10.3 | pass |
| validate <dir> --json | F3 | S0 | 4.2 | 4.1 | 0.990 | 9.6 | 9.6 | pass |
| check <dir> | F3 | S0 | 4.2 | 4.2 | 1.012 | 9.8 | 9.8 | pass |
| add node | F1 | S0 | 58.7 | 59.4 | 1.012 | 16.7 | 16.9 | pass |
| apply | F1 | S0 | 58.5 | 60.0 | 1.026 | 16.8 | 16.9 | pass |
| add node | F1 | S1 | 58.1 | 60.1 | 1.035 | 16.7 | 16.8 | pass |
| apply | F1 | S1 | 60.4 | 60.1 | 0.995 | 16.9 | 16.9 | pass |
| add node | F1 | S2 | 59.6 | 59.7 | 1.002 | 16.8 | 16.9 | pass |
| apply | F1 | S2 | 59.7 | 60.2 | 1.009 | 16.9 | 16.9 | pass |
| add node | F1 | S3 | 79.5 | 79.3 | 0.997 | 16.8 | 16.8 | pass |
| apply | F1 | S3 | 78.3 | 79.5 | 1.015 | 16.9 | 16.9 | pass |
| merge first import | M | S0 | 53.6 | 53.7 | 1.003 | 16.2 | 16.1 | pass |
| merge first import | M | S1 | 51.1 | 52.3 | 1.024 | 16.2 | 16.1 | pass |
| merge first import | M | S2 | 51.8 | 52.8 | 1.018 | 16.2 | 16.1 | pass |
| merge first import | M | S3 | 71.1 | 69.4 | 0.976 | 16.2 | 16.0 | pass |
| merge 41st import | F3 | S0 | 152.6 | 152.7 | 1.000 | 124.5 | 124.3 | pass |
