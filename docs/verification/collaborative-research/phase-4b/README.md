# Phase 4b zero-cost record

Policy: [zero-cost-measurement.md](../../../collaborative-research/zero-cost-measurement.md) (sha256 `fdc31c732da18cd2…`), full repetitions.

| | Binary | SHA-256 |
|---|---|---|
| Parent | ara 0.1.25, code of `feat/collaborative-ara` after #104 | `80d41c1e90c4de2d3fdeb10c71fa90571d77c63124e6029178ad2bc7492d9e5b` |
| Candidate | ara 0.1.26, the phase-4b fix branch | `f0ce6371c528ff966d02e2310ced11a2ceba4194dff664993303fb8c30b03a8b` |

Machine: macOS-27.0.1-arm64-arm-64bit, 18 CPUs, outside the Claude Code sandbox.

**Result: PASS, gate-valid.** 67 cases, 0 failures, 0 infeasible items. The worst candidate/parent median ratio is 1.095. There are no new crates in the default tree. Release binary: 14,088,512 → 14,054,528 bytes (limit 14,435,818).

| Command | Fixture | State | Parent median ms | Candidate median ms | Ratio | Verdict |
|---|---|---|---|---|---|---|
| status --json | F1 | S0 | 16.3 | 16.2 | 0.992 | pass |
| ls --json | F1 | S0 | 17.2 | 17.0 | 0.992 | pass |
| show C01 --json | F1 | S0 | 14.6 | 14.6 | 0.994 | pass |
| find "method" --json | F1 | S0 | 25.8 | 25.7 | 0.996 | pass |
| validate <dir> --json | F1 | S0 | 14.5 | 14.3 | 0.985 | pass |
| check <dir> | F1 | S0 | 14.9 | 14.7 | 0.986 | pass |
| status --json | F1 | S1 | 16.2 | 16.1 | 0.994 | pass |
| ls --json | F1 | S1 | 17.2 | 17.4 | 1.012 | pass |
| show C01 --json | F1 | S1 | 14.7 | 14.6 | 0.992 | pass |
| find "method" --json | F1 | S1 | 26.0 | 25.8 | 0.990 | pass |
| validate <dir> --json | F1 | S1 | 14.8 | 14.7 | 0.994 | pass |
| check <dir> | F1 | S1 | 15.1 | 15.2 | 1.005 | pass |
| status --json | F1 | S2 | 16.1 | 16.4 | 1.016 | pass |
| ls --json | F1 | S2 | 17.6 | 17.9 | 1.015 | pass |
| show C01 --json | F1 | S2 | 14.6 | 14.5 | 0.991 | pass |
| find "method" --json | F1 | S2 | 25.9 | 25.8 | 0.996 | pass |
| validate <dir> --json | F1 | S2 | 14.5 | 14.5 | 1.005 | pass |
| check <dir> | F1 | S2 | 15.0 | 14.7 | 0.980 | pass |
| status --json | F1 | S3 | 16.6 | 16.7 | 1.006 | pass |
| ls --json | F1 | S3 | 17.3 | 17.2 | 0.995 | pass |
| show C01 --json | F1 | S3 | 14.9 | 14.9 | 0.996 | pass |
| find "method" --json | F1 | S3 | 26.1 | 26.1 | 1.001 | pass |
| validate <dir> --json | F1 | S3 | 14.8 | 14.8 | 1.004 | pass |
| check <dir> | F1 | S3 | 15.1 | 15.2 | 1.003 | pass |
| status --json | F2 | S0 | 17.0 | 17.1 | 1.007 | pass |
| ls --json | F2 | S0 | 17.0 | 17.0 | 1.003 | pass |
| show C01 --json | F2 | S0 | 14.6 | 14.5 | 0.991 | pass |
| find "method" --json | F2 | S0 | 25.9 | 25.9 | 0.999 | pass |
| validate <dir> --json | F2 | S0 | 14.4 | 14.5 | 1.002 | pass |
| check <dir> | F2 | S0 | 14.5 | 14.7 | 1.008 | pass |
| status --json | F2 | S1 | 17.2 | 17.2 | 0.998 | pass |
| ls --json | F2 | S1 | 17.2 | 17.3 | 1.007 | pass |
| show C01 --json | F2 | S1 | 14.7 | 14.9 | 1.012 | pass |
| find "method" --json | F2 | S1 | 25.8 | 25.7 | 0.997 | pass |
| validate <dir> --json | F2 | S1 | 14.4 | 14.5 | 1.010 | pass |
| check <dir> | F2 | S1 | 15.0 | 14.9 | 0.995 | pass |
| status --json | F2 | S2 | 17.0 | 17.0 | 1.002 | pass |
| ls --json | F2 | S2 | 17.1 | 17.2 | 1.006 | pass |
| show C01 --json | F2 | S2 | 14.5 | 14.5 | 1.002 | pass |
| find "method" --json | F2 | S2 | 25.7 | 25.6 | 0.998 | pass |
| validate <dir> --json | F2 | S2 | 14.3 | 14.4 | 1.008 | pass |
| check <dir> | F2 | S2 | 14.8 | 15.0 | 1.013 | pass |
| status --json | F2 | S3 | 17.2 | 17.6 | 1.021 | pass |
| ls --json | F2 | S3 | 18.2 | 18.2 | 1.003 | pass |
| show C01 --json | F2 | S3 | 15.6 | 15.9 | 1.019 | pass |
| find "method" --json | F2 | S3 | 26.7 | 26.9 | 1.005 | pass |
| validate <dir> --json | F2 | S3 | 16.3 | 16.3 | 1.001 | pass |
| check <dir> | F2 | S3 | 16.8 | 16.9 | 1.006 | pass |
| status --json | F3 | S0 | 5.4 | 5.4 | 1.003 | pass |
| ls --json | F3 | S0 | 5.8 | 5.7 | 0.971 | pass |
| show C01 --json | F3 | S0 | 5.2 | 5.5 | 1.051 | pass |
| find "method" --json | F3 | S0 | 5.3 | 5.4 | 1.012 | pass |
| validate <dir> --json | F3 | S0 | 5.3 | 5.1 | 0.955 | pass |
| check <dir> | F3 | S0 | 5.4 | 5.6 | 1.034 | pass |
| add node | F1 | S0 | 65.4 | 65.6 | 1.003 | pass |
| apply | F1 | S0 | 76.4 | 73.8 | 0.966 | pass |
| add node | F1 | S1 | 63.5 | 64.2 | 1.011 | pass |
| apply | F1 | S1 | 62.8 | 64.0 | 1.019 | pass |
| add node | F1 | S2 | 61.4 | 61.8 | 1.007 | pass |
| apply | F1 | S2 | 61.2 | 62.5 | 1.021 | pass |
| add node | F1 | S3 | 77.0 | 75.9 | 0.985 | pass |
| apply | F1 | S3 | 73.9 | 74.3 | 1.005 | pass |
| merge first import | M | S0 | 51.3 | 51.4 | 1.002 | pass |
| merge first import | M | S1 | 52.7 | 53.3 | 1.012 | pass |
| merge first import | M | S2 | 58.4 | 58.6 | 1.003 | pass |
| merge first import | M | S3 | 72.1 | 78.9 | 1.095 | pass |
| merge 41st import | F3 | S0 | 206.9 | 207.9 | 1.004 | pass |
