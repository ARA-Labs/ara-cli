# Phase 2a zero-cost record

Policy: [zero-cost-measurement.md](../../../collaborative-research/zero-cost-measurement.md) (sha256 `fdc31c732da18cd2…`), full repetitions, not quick mode.

| | Binary | SHA-256 |
|---|---|---|
| Parent | ara 0.1.23 built from `0f51ebd` (code identical to `161de86`) | `9de82d4e4a406f954254f7df5a7e4bfde3eefd11225fc65d2b3667c72b64354d` |
| Candidate | ara 0.1.24 built from the phase-2a branch | `267f2c1a21bcfb0adeaa738c997e65e95608ce3cd4d6fd9df4d9fd6c2eec58d8` |

Machine: macOS-27.0.1-arm64-arm-64bit, 18 CPUs. Run outside the Claude Code sandbox because `/usr/bin/time` needs it.

**Result: PASS, gate-valid.** 67 cases, 0 confirmed regressions, 0 failed output or access checks, 0 infeasible items. The worst candidate/parent median ratio is 1.063.

- **Access:** under S2 (unreadable `.ara/vcs/`), S1, and S3, every ordinary command produced the same output as under S0. No command created `.ara/vcs/`, `snapshot.json`, or a `.ara-snapshot-` directory. `.ara/` changed only at `lock` and `transactions/` for writes and merges.
- **Build:** no new crates in the default tree (`libc` was already present transitively); only `ara-cli` and `ara-core` changed version. Release binary 13,826,528 → 13,906,016 bytes (limit 14,168,594).
- **Run by hand:** `cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked` passes. Clean build time was not measured; it is ungated.

| Command | Fixture | State | Parent median ms | Candidate median ms | Ratio | Parent RSS MiB | Candidate RSS MiB | Verdict |
|---|---|---|---|---|---|---|---|---|
| status --json | F1 | S0 | 15.4 | 15.4 | 1.001 | 13.6 | 13.8 | pass |
| ls --json | F1 | S0 | 15.4 | 15.5 | 1.006 | 16.6 | 16.8 | pass |
| show C01 --json | F1 | S0 | 13.3 | 13.4 | 1.007 | 13.2 | 13.5 | pass |
| find "method" --json | F1 | S0 | 23.9 | 23.8 | 0.998 | 17.7 | 17.9 | pass |
| validate <dir> --json | F1 | S0 | 13.7 | 13.6 | 0.998 | 12.9 | 13.2 | pass |
| check <dir> | F1 | S0 | 13.2 | 13.3 | 1.009 | 12.8 | 13.3 | pass |
| status --json | F1 | S1 | 14.4 | 14.5 | 1.004 | 13.6 | 13.8 | pass |
| ls --json | F1 | S1 | 15.5 | 15.6 | 1.008 | 16.6 | 16.8 | pass |
| show C01 --json | F1 | S1 | 13.3 | 13.2 | 0.992 | 13.2 | 13.5 | pass |
| find "method" --json | F1 | S1 | 24.0 | 23.9 | 0.996 | 17.8 | 17.8 | pass |
| validate <dir> --json | F1 | S1 | 13.1 | 13.2 | 1.007 | 12.8 | 13.1 | pass |
| check <dir> | F1 | S1 | 13.3 | 13.4 | 1.010 | 12.9 | 13.3 | pass |
| status --json | F1 | S2 | 14.5 | 14.6 | 1.004 | 13.6 | 13.7 | pass |
| ls --json | F1 | S2 | 15.4 | 15.4 | 1.001 | 16.5 | 16.8 | pass |
| show C01 --json | F1 | S2 | 13.1 | 13.3 | 1.016 | 13.2 | 13.5 | pass |
| find "method" --json | F1 | S2 | 23.8 | 23.7 | 0.996 | 17.7 | 17.9 | pass |
| validate <dir> --json | F1 | S2 | 13.0 | 13.0 | 1.000 | 12.8 | 13.1 | pass |
| check <dir> | F1 | S2 | 13.3 | 13.4 | 1.006 | 12.9 | 13.3 | pass |
| status --json | F1 | S3 | 15.6 | 15.6 | 1.001 | 13.5 | 13.8 | pass |
| ls --json | F1 | S3 | 16.3 | 16.4 | 1.009 | 16.5 | 16.8 | pass |
| show C01 --json | F1 | S3 | 14.5 | 14.6 | 1.009 | 13.2 | 13.6 | pass |
| find "method" --json | F1 | S3 | 25.3 | 25.1 | 0.994 | 17.8 | 17.9 | pass |
| validate <dir> --json | F1 | S3 | 14.4 | 14.5 | 1.010 | 12.9 | 13.2 | pass |
| check <dir> | F1 | S3 | 14.9 | 14.9 | 0.998 | 12.8 | 13.3 | pass |
| status --json | F2 | S0 | 17.6 | 17.9 | 1.022 | 13.8 | 14.1 | pass |
| ls --json | F2 | S0 | 17.2 | 17.5 | 1.020 | 16.6 | 16.8 | pass |
| show C01 --json | F2 | S0 | 14.3 | 14.3 | 0.995 | 13.1 | 13.5 | pass |
| find "method" --json | F2 | S0 | 26.7 | 26.6 | 0.997 | 17.8 | 18.0 | pass |
| validate <dir> --json | F2 | S0 | 15.8 | 15.9 | 1.005 | 12.8 | 13.2 | pass |
| check <dir> | F2 | S0 | 14.1 | 14.2 | 1.007 | 12.8 | 13.1 | pass |
| status --json | F2 | S1 | 17.4 | 17.4 | 0.996 | 13.8 | 14.1 | pass |
| ls --json | F2 | S1 | 17.6 | 17.7 | 1.002 | 16.6 | 16.9 | pass |
| show C01 --json | F2 | S1 | 14.1 | 14.2 | 1.010 | 13.2 | 13.5 | pass |
| find "method" --json | F2 | S1 | 25.3 | 25.4 | 1.006 | 17.7 | 17.9 | pass |
| validate <dir> --json | F2 | S1 | 13.6 | 13.7 | 1.012 | 12.7 | 13.2 | pass |
| check <dir> | F2 | S1 | 14.0 | 14.3 | 1.015 | 12.8 | 13.2 | pass |
| status --json | F2 | S2 | 16.1 | 16.1 | 0.998 | 13.8 | 14.1 | pass |
| ls --json | F2 | S2 | 16.3 | 16.4 | 1.002 | 16.6 | 16.8 | pass |
| show C01 --json | F2 | S2 | 13.7 | 13.8 | 1.006 | 13.2 | 13.5 | pass |
| find "method" --json | F2 | S2 | 25.5 | 25.4 | 0.998 | 17.8 | 17.9 | pass |
| validate <dir> --json | F2 | S2 | 13.9 | 14.0 | 1.006 | 12.9 | 13.2 | pass |
| check <dir> | F2 | S2 | 14.1 | 14.0 | 0.993 | 12.8 | 13.3 | pass |
| status --json | F2 | S3 | 17.1 | 17.2 | 1.004 | 13.9 | 14.1 | pass |
| ls --json | F2 | S3 | 16.8 | 16.9 | 1.007 | 16.5 | 16.8 | pass |
| show C01 --json | F2 | S3 | 14.5 | 14.7 | 1.017 | 13.2 | 13.5 | pass |
| find "method" --json | F2 | S3 | 26.4 | 26.1 | 0.991 | 17.7 | 17.8 | pass |
| validate <dir> --json | F2 | S3 | 14.5 | 14.6 | 1.002 | 12.9 | 13.2 | pass |
| check <dir> | F2 | S3 | 14.3 | 14.5 | 1.013 | 12.8 | 13.2 | pass |
| status --json | F3 | S0 | 4.6 | 4.6 | 0.990 | 10.1 | 10.1 | pass |
| ls --json | F3 | S0 | 4.5 | 4.7 | 1.029 | 10.3 | 10.2 | pass |
| show C01 --json | F3 | S0 | 4.3 | 4.3 | 0.989 | 10.0 | 10.1 | pass |
| find "method" --json | F3 | S0 | 4.5 | 4.5 | 1.003 | 10.4 | 10.3 | pass |
| validate <dir> --json | F3 | S0 | 4.3 | 4.3 | 0.992 | 9.6 | 9.7 | pass |
| check <dir> | F3 | S0 | 4.4 | 4.5 | 1.016 | 9.7 | 9.8 | pass |
| add node | F1 | S0 | 60.5 | 58.9 | 0.974 | 16.6 | 16.8 | pass |
| apply | F1 | S0 | 61.1 | 61.2 | 1.001 | 16.7 | 16.9 | pass |
| add node | F1 | S1 | 62.8 | 62.0 | 0.987 | 16.5 | 16.8 | pass |
| apply | F1 | S1 | 60.0 | 61.2 | 1.020 | 16.7 | 16.9 | pass |
| add node | F1 | S2 | 57.8 | 57.5 | 0.995 | 16.6 | 16.8 | pass |
| apply | F1 | S2 | 57.7 | 58.5 | 1.013 | 16.7 | 16.8 | pass |
| add node | F1 | S3 | 73.5 | 71.2 | 0.969 | 16.9 | 16.9 | pass |
| apply | F1 | S3 | 78.8 | 81.2 | 1.030 | 16.7 | 16.8 | pass |
| merge first import | M | S0 | 52.7 | 53.1 | 1.008 | 16.0 | 16.1 | pass |
| merge first import | M | S1 | 51.0 | 52.2 | 1.025 | 16.1 | 16.2 | pass |
| merge first import | M | S2 | 56.5 | 60.0 | 1.063 | 16.0 | 16.2 | pass |
| merge first import | M | S3 | 70.7 | 70.9 | 1.003 | 16.0 | 16.2 | pass |
| merge 41st import | F3 | S0 | 150.9 | 151.7 | 1.005 | 124.3 | 124.5 | pass |

Raw samples, normalized output digests, and access listings are in `zero-cost.json`.
