//! Real-binary checks for `ara snapshot` (docs/collaborative-research/snapshot-contract.md).
#![cfg(unix)]
use assert_cmd::Command;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path, process::Stdio, time::Duration};
use tempfile::TempDir;

fn put(root: &Path, path: &str, bytes: &[u8]) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}
fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}
fn artifact(root: &Path) {
    put(
        root,
        "trace/exploration_tree.yaml",
        b"tree:\n  - id: N01\n    type: question\n    title: Why does M help?\n",
    );
    put(
        root,
        "logic/claims.md",
        b"# Claims\n\n## C01: M helps\n- **Statement**: M improves T.\n- **Status**: hypothesis\n",
    );
    put(root, "src/train.py", b"print('train')\n");
    chmod(&root.join("src/train.py"), 0o755);
    put(root, "evidence/raw.bin", &[0, 159, 146, 150, 255]);
}
fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .env_remove("ARA_DIR")
        .env("ARA_NO_DUPLICATE_CHECK", "1")
        .arg("-C")
        .arg(root);
    command
}
fn snapshot(root: &Path, output: &Path) -> Value {
    let out = ara(root)
        .args(["snapshot", "--json", "--output"])
        .arg(output)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap()
}
fn snapshot_error(root: &Path, output: &Path, exit: i32) -> Value {
    let out = ara(root)
        .args(["snapshot", "--json", "--output"])
        .arg(output)
        .assert()
        .code(exit)
        .get_output()
        .stderr
        .clone();
    serde_json::from_slice::<Value>(&out).unwrap()["error"].clone()
}
fn manifest(package: &Path) -> Value {
    serde_json::from_slice(&fs::read(package.join("snapshot.json")).unwrap()).unwrap()
}
fn manifest_paths(package: &Path) -> Vec<String> {
    manifest(package)["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_owned())
        .collect()
}
fn leftovers(parent: &Path) -> Vec<String> {
    fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.contains(".ara-snapshot-"))
        .collect()
}

#[test]
fn package_matches_capture_and_manifest_contract() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    chmod(&source.join("logic/claims.md"), 0o640);
    let result = snapshot(&source, &dir.path().join("pkg"));
    let package = dir.path().join("pkg");
    assert_eq!(result["format"], "ara.snapshot/v1");
    assert_eq!(result["file_count"], 4);
    assert!(result.get("recorded").is_none());
    let manifest = manifest(&package);
    assert_eq!(manifest["capture_id"], result["capture_id"]);
    assert_eq!(manifest["fingerprint"], result["fingerprint"]);
    assert_eq!(manifest["root"], "ara");
    assert_eq!(
        manifest["excluded"],
        serde_json::json!([".ara/", ".git/", ".ara-write-<pid>-<nonce>"])
    );
    // The file bytes are canonical: recomputing the capture ID reproduces it.
    assert_eq!(
        ara_cli::snapshot::capture_id(&manifest),
        result["capture_id"].as_str().unwrap()
    );
    let written = fs::read_to_string(package.join("snapshot.json")).unwrap();
    assert_eq!(written, ara_cli::snapshot::canonical_json(&manifest));
    for file in manifest["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        let bytes = fs::read(package.join("ara").join(path)).unwrap();
        assert_eq!(bytes, fs::read(source.join(path)).unwrap(), "{path}");
        assert_eq!(file["size"], bytes.len());
        assert_eq!(
            file["mode"].as_str().unwrap(),
            format!("{:04o}", mode(&source.join(path)))
        );
        assert_eq!(
            mode(&package.join("ara").join(path)),
            mode(&source.join(path))
        );
    }
    assert_eq!(mode(&package.join("ara/src/train.py")), 0o755);
    assert_eq!(mode(&package.join("ara/logic/claims.md")), 0o640);
    // Absent canonical sources are never materialized.
    assert!(!package.join("ara/PAPER.md").exists());
    assert!(
        !package.join("ara/.ara").exists(),
        "the lock directory is private"
    );
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn private_namespaces_are_excluded_at_every_depth_without_reading_them() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    for private in [
        ".git/config",
        "logic/.git/objects/x",
        "logic/.ara/vcs/blob",
        "src/.ara/state",
        "trace/.ara-write-12-34",
        "evidence/nested/.git/HEAD",
    ] {
        put(&source, private, b"private");
    }
    // An unreadable private directory proves the loaders never descend into it.
    chmod(&source.join("logic/.git"), 0o000);
    let result = snapshot(&source, &dir.path().join("pkg"));
    chmod(&source.join("logic/.git"), 0o755);
    let paths = manifest_paths(&dir.path().join("pkg"));
    assert_eq!(
        paths,
        [
            "evidence/raw.bin",
            "logic/claims.md",
            "src/train.py",
            "trace/exploration_tree.yaml"
        ]
    );
    assert_eq!(result["file_count"], 4);
}

#[test]
fn fingerprint_matches_merge_and_exported_package_merges_like_the_live_fork() {
    let dir = TempDir::new().unwrap();
    let seed = dir.path().join("seed");
    artifact(&seed);
    let fork = dir.path().join("fork");
    artifact(&fork);
    put(
        &fork,
        "logic/claims.md",
        b"# Claims\n\n## C01: M helps\n- **Statement**: M improves T.\n- **Status**: hypothesis\n\n## C02: Ablation\n- **Statement**: Removing K hurts.\n- **Status**: hypothesis\n",
    );
    let result = snapshot(&fork, &dir.path().join("pkg"));
    let mut logs = Vec::new();
    for (name, theirs) in [
        ("via-live", fork.clone()),
        ("via-package", dir.path().join("pkg/ara")),
    ] {
        let destination = dir.path().join(name);
        artifact(&destination);
        ara(&destination)
            .args(["merge", "--source-key", "fork-a", "--json", "--base"])
            .arg(&seed)
            .arg("--theirs")
            .arg(&theirs)
            .args(["--as", "fork-a"])
            .assert()
            .success();
        let log = fs::read_to_string(destination.join("trace/merge_log.yaml")).unwrap();
        assert!(
            log.contains(result["fingerprint"].as_str().unwrap()),
            "merge must record the snapshot fingerprint"
        );
        let claims = fs::read(destination.join("logic/claims.md")).unwrap();
        logs.push((strip_times(&log), claims));
    }
    assert_eq!(logs[0], logs[1]);
}
fn strip_times(log: &str) -> String {
    log.split('"')
        .map(|part| {
            if part.len() == 20 && part.ends_with('Z') && part.as_bytes()[10] == b'T' {
                "<time>"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("\"")
}

#[test]
fn mode_only_change_keeps_fingerprint_and_changes_capture_id() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    chmod(&source.join("logic/claims.md"), 0o640);
    let first = snapshot(&source, &dir.path().join("a"));
    chmod(&source.join("logic/claims.md"), 0o644);
    let second = snapshot(&source, &dir.path().join("b"));
    assert_eq!(first["fingerprint"], second["fingerprint"]);
    assert_ne!(first["capture_id"], second["capture_id"]);
    assert_eq!(mode(&dir.path().join("a/ara/logic/claims.md")), 0o640);
    assert_eq!(mode(&dir.path().join("b/ara/logic/claims.md")), 0o644);
    // Identical inputs give byte-identical manifests and results.
    let third = snapshot(&source, &dir.path().join("c"));
    assert_eq!(third["capture_id"], second["capture_id"]);
    assert_eq!(
        fs::read(dir.path().join("b/snapshot.json")).unwrap(),
        fs::read(dir.path().join("c/snapshot.json")).unwrap()
    );
}

#[test]
fn diagnostics_errors_are_reported_not_rejected_and_reads_match() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    put(
        &source,
        "trace/exploration_tree.yaml",
        b"tree:\n  - id: N01\n    type: question\n    title: Q\n    also_depends_on: [N99]\n",
    );
    let status = |root: &Path| -> Value {
        let out = ara(root).args(["status", "--json"]).output().unwrap();
        let mut value: Value = serde_json::from_slice(&out.stdout).unwrap();
        value.as_object_mut().unwrap().remove("artifact_location");
        value
    };
    let before = status(&source);
    let errors = before["diagnostics"]["errors"].as_u64().unwrap();
    assert!(errors > 0, "fixture needs a diagnostic error: {before}");
    let result = snapshot(&source, &dir.path().join("pkg"));
    assert_eq!(result["diagnostics"]["errors"], errors);
    let manifest = manifest(&dir.path().join("pkg"));
    let items = manifest["diagnostics"]["items"].as_array().unwrap();
    assert_eq!(
        items.len() as u64,
        errors + before["diagnostics"]["warnings"].as_u64().unwrap()
    );
    assert!(
        items
            .iter()
            .all(|item| item["code"].as_str().unwrap().starts_with("ARA"))
    );
    let package = dir.path().join("pkg/ara");
    assert_eq!(status(&package), before);
    for args in [["ls", "--json"], ["show", "C01"]] {
        let left = ara(&source).args(args).arg("--json").output().unwrap();
        let right = ara(&package).args(args).arg("--json").output().unwrap();
        assert_eq!(left.stdout, right.stdout, "{args:?}");
    }
}

#[test]
fn unsafe_locations_and_sources_reject_without_output() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    fs::create_dir(dir.path().join("taken")).unwrap();
    assert_eq!(
        snapshot_error(&source, &dir.path().join("taken"), 2)["code"],
        "output_exists"
    );
    assert_eq!(
        snapshot_error(&source, &source.join("inner"), 2)["code"],
        "overlapping_roots"
    );
    // A symlinked parent that aliases into the source is resolved first.
    std::os::unix::fs::symlink(source.join("src"), dir.path().join("alias")).unwrap();
    assert_eq!(
        snapshot_error(&source, &dir.path().join("alias/pkg"), 2)["code"],
        "overlapping_roots"
    );
    assert!(!source.join("src/pkg").exists());
    std::os::unix::fs::symlink("train.py", source.join("src/link.py")).unwrap();
    assert_eq!(
        snapshot_error(&source, &dir.path().join("pkg"), 1)["code"],
        "write.path"
    );
    fs::remove_file(source.join("src/link.py")).unwrap();
    fs::create_dir_all(source.join(".ara/transactions/active.preimages")).unwrap();
    fs::write(
        source.join(".ara/transactions/active.json.prepared"),
        r#"{"format":"ara.transaction/v1","entries":[],"created_dirs":[]}"#,
    )
    .unwrap();
    for relative in [".ara/transactions", ".ara/transactions/active.preimages"] {
        chmod(&source.join(relative), 0o700);
    }
    assert_eq!(
        snapshot_error(&source, &dir.path().join("pkg"), 2)["code"],
        "pending_transaction"
    );
    assert!(!dir.path().join("pkg").exists());
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn failure_before_publication_leaves_no_output() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    let parent = dir.path().join("readonly");
    fs::create_dir(&parent).unwrap();
    chmod(&parent, 0o555);
    let error = snapshot_error(&source, &parent.join("pkg"), 2);
    chmod(&parent, 0o755);
    assert_eq!(error["code"], "io_error");
    assert!(!parent.join("pkg").exists());
    assert!(leftovers(&parent).is_empty());
}

#[test]
fn failure_after_publication_reports_io_with_a_complete_package() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    let out = ara(&source)
        .env("ARA_SNAPSHOT_TEST_FAIL_AFTER_PUBLISH", "1")
        .args(["snapshot", "--json", "--output"])
        .arg(dir.path().join("pkg"))
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(out.stdout.is_empty(), "no false success");
    let package = dir.path().join("pkg");
    let manifest = manifest(&package);
    assert_eq!(
        ara_cli::snapshot::capture_id(&manifest),
        manifest["capture_id"].as_str().unwrap()
    );
    assert_eq!(manifest_paths(&package).len(), 4);
}

/// Starts a paused snapshot, runs `between` once it reaches `point`, then
/// resumes it and returns its exit code and stderr.
fn paused(source: &Path, output: &Path, point: &str, between: impl FnOnce()) -> (i32, Value) {
    let gate = TempDir::new().unwrap();
    let child = std::process::Command::new(assert_cmd::cargo::cargo_bin("ara"))
        .env_remove("ARA_DIR")
        .env("ARA_SNAPSHOT_TEST_PAUSE", gate.path())
        .env("ARA_SNAPSHOT_TEST_PAUSE_AT", point)
        .arg("-C")
        .arg(source)
        .args(["snapshot", "--json", "--output"])
        .arg(output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..3000 {
        if gate.path().join("ready").exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(gate.path().join("ready").exists(), "snapshot never paused");
    between();
    fs::write(gate.path().join("go"), "").unwrap();
    let out = child.wait_with_output().unwrap();
    let stderr = if out.stderr.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["error"].clone()
    };
    (out.status.code().unwrap(), stderr)
}

#[test]
fn byte_or_mode_change_during_capture_is_stale_and_exposes_nothing() {
    for change in ["bytes", "mode", "new file"] {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("fork");
        artifact(&source);
        let output = dir.path().join("pkg");
        let (code, error) = paused(&source, &output, "recheck", || match change {
            "bytes" => put(&source, "src/train.py", b"print('changed')\n"),
            "mode" => chmod(&source.join("src/train.py"), 0o644),
            _ => put(&source, "evidence/late.csv", b"a,b\n"),
        });
        assert_eq!(
            (code, error["code"].as_str()),
            (1, Some("stale_snapshot_input")),
            "{change}"
        );
        assert!(!output.exists(), "{change}");
        assert!(leftovers(dir.path()).is_empty(), "{change}");
    }
}

#[test]
fn concurrent_creator_at_publication_keeps_its_directory() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    let output = dir.path().join("pkg");
    let (code, error) = paused(&source, &output, "publish", || {
        put(&output, "theirs.txt", b"other creator");
    });
    assert_eq!((code, error["code"].as_str()), (2, Some("output_exists")));
    assert_eq!(
        fs::read(output.join("theirs.txt")).unwrap(),
        b"other creator"
    );
    assert!(!output.join("snapshot.json").exists());
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn cooperating_writer_serializes_with_snapshot() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("fork");
    artifact(&source);
    let output = dir.path().join("pkg");
    let mut writer = None;
    let (code, _) = paused(&source, &output, "recheck", || {
        // A guarded writer blocks on the artifact lock until the snapshot ends.
        let child = std::process::Command::new(assert_cmd::cargo::cargo_bin("ara"))
            .env("ARA_NO_DUPLICATE_CHECK", "1")
            .arg("-C")
            .arg(&source)
            .args([
                "add",
                "node",
                "--type",
                "question",
                "--parent",
                "N01",
                "--title",
                "Later",
                "--set",
                "description=Asked later",
                "--provenance",
                "user",
                "--json",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !fs::read_to_string(source.join("trace/exploration_tree.yaml"))
                .unwrap()
                .contains("Later"),
            "the writer must wait for the snapshot lock"
        );
        writer = Some(child);
    });
    assert_eq!(code, 0);
    assert!(output.join("snapshot.json").exists());
    let written = writer.unwrap().wait_with_output().unwrap();
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    assert!(
        fs::read_to_string(source.join("trace/exploration_tree.yaml"))
            .unwrap()
            .contains("Later")
    );
    assert!(
        !fs::read_to_string(output.join("ara/trace/exploration_tree.yaml"))
            .unwrap()
            .contains("Later"),
        "the package holds the pre-write capture"
    );
}
