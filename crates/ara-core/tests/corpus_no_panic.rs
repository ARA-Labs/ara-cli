//! Real-ARA no-panic regression net.
//!
//! Real ARA artifacts exercise a wider schema than `ara-core` models today, so
//! they legitimately produce warnings and errors. This test does **not** assert
//! a clean parse; it asserts the weaker, permanent robustness contract:
//!
//! 1. `parse_dir` does **not unwind-panic**, and
//! 2. it produces a `ParseReport` — `Ok((manifest, report))` or `Err(report)`.
//!    Both outcomes pass; only a panic fails.
//!
//! Two tests share the same helpers:
//! - an **always-on** test over the vendored subset under
//!   `tests/fixtures/corpus/` (hermetic, offline, runs in default `cargo test`);
//! - an **opt-in** full sweep over the `corpus-external/` git submodules, gated
//!   by both `#[ignore]` and `RUN_CORPUS_SWEEP=1` so a fresh clone still passes.
//!
//! **Scope of the guarantee.** `catch_unwind` catches *unwinding* panics only.
//! It does not catch a stack-overflow `SIGABRT`, and there is no per-test
//! timeout, so a hang is not detected either. Hardening the parser against
//! pathologically deep recursion is tracked separately as `T-PARSE-DEPTH`.
//!
//! **Dependency on unwinding panics.** This net relies on panics unwinding. If a
//! build profile ever sets `panic = "abort"`, `catch_unwind` becomes a no-op and
//! this net stops catching anything. None do today; keep it that way.

#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

/// Repo root, derived from this crate's manifest dir (`crates/ara-core`).
#[cfg(feature = "native")]
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// Recursively collect every directory containing `trace/exploration_tree.yaml`.
///
/// Uses only `std::fs` (no `walkdir` dependency). The result is **sorted** so a
/// failure message lists artifacts deterministically. Missing/unreadable dirs
/// yield an empty result rather than panicking — callers guard on the count.
#[cfg(feature = "native")]
fn discover_artifacts(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(root, &mut found);
    found.sort();
    found
}

#[cfg(feature = "native")]
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join("trace/exploration_tree.yaml").is_file() {
        out.push(dir.to_path_buf());
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // Skip symlinks: the opt-in sweep walks less-controlled submodule
        // content, and a symlinked directory cycle would recurse until the
        // stack overflows — an abort `catch_unwind` cannot catch, which is
        // exactly the failure mode this net exists to avoid. `file_type` does
        // not follow the link, unlike `path.is_dir()`.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() && !file_type.is_symlink() {
            collect(&entry.path(), out);
        }
    }
}

/// Assert `parse_dir(dir)` runs to completion without an unwinding panic.
///
/// Both `Ok((_, report))` and `Err(report)` pass — the `ParseReport` is present
/// by type in either arm. Only an unwound panic fails, naming the artifact.
#[cfg(feature = "native")]
fn assert_parses_without_panic(dir: &Path) {
    let result = std::panic::catch_unwind(|| ara_core::parse_dir(dir));
    // Outer `Err` means the closure unwound a panic — the one failure mode.
    // Inner `Ok`/`Err` both carry a `ParseReport`; both are a pass.
    assert!(
        result.is_ok(),
        "PANIC parsing {} — parser unwound instead of returning a report",
        dir.display()
    );
}

/// Pure gate for the opt-in sweep: run only when explicitly enabled **and** the
/// submodule directory is actually present. Factored out so it is unit-testable
/// always-on (see `sweep_gate_logic`), guarding the fresh-clone-passes invariant
/// inside CI without checking out the real corpora.
fn should_run_sweep(env_set: bool, dir_exists: bool) -> bool {
    env_set && dir_exists
}

/// Always-on hermetic check: every vendored artifact parses without panic.
#[test]
#[cfg(feature = "native")]
fn vendored_corpus_never_panics() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
    let dirs = discover_artifacts(&corpus);

    // Count guard: closes the vacuous-pass hole. If the fixtures are ever moved
    // or emptied, the walk finds zero dirs and this test would otherwise pass
    // while guarding nothing.
    assert!(
        dirs.len() >= 6,
        "expected >= 6 vendored corpus artifacts under {}, found {} — fixtures moved or emptied?",
        corpus.display(),
        dirs.len()
    );

    for dir in &dirs {
        assert_parses_without_panic(dir);
    }
}

/// Opt-in full sweep over the `corpus-external/` submodules. `#[ignore]` (never
/// runs in default `cargo test`) **and** env-gated (`RUN_CORPUS_SWEEP=1`). Skips
/// cleanly — logging why — when the env var is unset or the submodules are
/// absent, so a fresh clone without submodules still passes under `--ignored`.
#[test]
#[ignore = "opt-in: set RUN_CORPUS_SWEEP=1 and init submodules"]
#[cfg(feature = "native")]
fn full_corpus_sweep_never_panics() {
    let external = repo_root().join("corpus-external");
    let env_set = std::env::var_os("RUN_CORPUS_SWEEP").is_some();
    let dir_exists = external.is_dir();

    if !should_run_sweep(env_set, dir_exists) {
        eprintln!(
            "skipping full corpus sweep: RUN_CORPUS_SWEEP set = {env_set}, \
             {} exists = {dir_exists}",
            external.display()
        );
        return;
    }

    let dirs = discover_artifacts(&external);
    assert!(
        !dirs.is_empty(),
        "RUN_CORPUS_SWEEP=1 and {} exists, but no artifacts were found — \
         did you run `git submodule update --init`?",
        external.display()
    );

    eprintln!(
        "sweeping {} corpus artifacts under {}",
        dirs.len(),
        external.display()
    );
    for dir in &dirs {
        assert_parses_without_panic(dir);
    }
}

/// Always-on coverage of the one new codepath the sweep introduces: the gate
/// decision. Guards the fresh-clone-passes invariant (env unset **or** dir
/// absent ⇒ skip) inside CI, without ever checking out the 34 real artifacts.
#[test]
fn sweep_gate_logic() {
    assert!(should_run_sweep(true, true), "enabled + present ⇒ run");
    assert!(!should_run_sweep(true, false), "enabled + absent ⇒ skip");
    assert!(!should_run_sweep(false, true), "disabled + present ⇒ skip");
    assert!(!should_run_sweep(false, false), "disabled + absent ⇒ skip");
}
