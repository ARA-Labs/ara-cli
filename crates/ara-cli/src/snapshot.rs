//! `ara snapshot`: an exact, offline capture of one artifact exported as a
//! verified package (`<output>/ara/` plus a canonical `snapshot.json`).
//! The contract is `docs/collaborative-research/snapshot-contract.md`.
use crate::agent::ReadOptions;
use crate::output::AgentError;
use crate::write::convert_error;
use ara_core::write::{ArtifactLock, ArtifactSnapshot};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const FORMAT: &str = "ara.snapshot/v1";
const CAPTURE_DOMAIN: &[u8] = b"ara.capture/v1\0";
const EXCLUDED: [&str; 3] = [".ara/", ".git/", ".ara-write-<pid>-<nonce>"];
static NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, clap::Args)]
pub struct SnapshotArgs {
    /// New package directory. It must not exist and must not overlap the artifact.
    #[arg(long)]
    pub output: PathBuf,
    #[command(flatten)]
    pub options: ReadOptions,
}

/// One captured, existing, nonprivate regular file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedFile {
    path: String,
    digest: String,
    size: u64,
    mode: u32,
}

/// The resolved package location: an existing canonical parent and a new name.
struct OutputPath {
    parent: PathBuf,
    name: String,
    path: PathBuf,
}

pub fn run(root: &Path, args: &SnapshotArgs) -> Result<Value, AgentError> {
    let output = resolve_output(root, &args.output)?;
    let _lock = ArtifactLock::acquire(root).map_err(convert_error)?;
    crate::context::ensure_no_pending_transaction(root)?;
    let capture = ArtifactSnapshot::load_complete(root).map_err(convert_error)?;
    let files = captured_files(&capture)?;
    let fingerprint = ara_core::merge::fingerprint(&capture);

    let staging = Staging::create(&output)?;
    let package_root = staging.path.join("ara");
    write_files(&capture, &files, &package_root)?;
    let diagnostics = diagnostics(&package_root)?;
    let (manifest, capture_id) = manifest(&fingerprint, &files, diagnostics);
    write_synced(&staging.path.join("snapshot.json"), &manifest)?;
    sync_directory(&staging.path)?;
    verify_package(&staging.path, &files, &fingerprint, &capture_id)?;

    test_hook::pause("recheck");
    let current = ArtifactSnapshot::load_complete(root).map_err(convert_error)?;
    if captured_files(&current)? != files {
        return Err(AgentError::semantic(
            "stale_snapshot_input",
            "The artifact changed during capture; stop direct writers and retry",
        ));
    }
    test_hook::pause("publish");
    staging.publish(&output)?;
    test_hook::fail_after_publish()?;
    sync_directory(&output.parent)?;
    let parsed: Value = serde_json::from_slice(&manifest).expect("canonical manifest is JSON");
    Ok(json!({
        "format": FORMAT,
        "capture_id": capture_id,
        "fingerprint": fingerprint,
        "file_count": files.len(),
        "diagnostics": {
            "errors": parsed["diagnostics"]["errors"],
            "warnings": parsed["diagnostics"]["warnings"],
        },
        "output": output.path,
    }))
}

fn resolve_output(root: &Path, requested: &Path) -> Result<OutputPath, AgentError> {
    let absolute = std::env::current_dir()
        .map_err(|error| AgentError::io(error.to_string()))?
        .join(requested);
    let name = match absolute.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.is_empty() && name != "." && name != ".." => name.to_owned(),
        _ => {
            return Err(AgentError::setup(
                "invalid_output",
                "--output must name a new directory with a UTF-8 final component",
            ));
        }
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| AgentError::setup("invalid_output", "--output has no parent directory"))?
        .canonicalize()
        .map_err(|error| AgentError::io(format!("--output parent: {error}")))?;
    let path = parent.join(&name);
    if fs::symlink_metadata(&path).is_ok() {
        return Err(output_exists(&path));
    }
    if crate::context::roots_overlap(root, &path) {
        return Err(AgentError::setup(
            "overlapping_roots",
            "The artifact and --output must not contain each other",
        ));
    }
    Ok(OutputPath { parent, name, path })
}

fn output_exists(path: &Path) -> AgentError {
    AgentError::setup(
        "output_exists",
        format!(
            "{} already exists; snapshot never replaces it",
            path.display()
        ),
    )
}

fn captured_files(snapshot: &ArtifactSnapshot) -> Result<Vec<CapturedFile>, AgentError> {
    snapshot
        .files
        .iter()
        .filter(|(path, file)| file.existed && !ara_core::write::source::private_path(path))
        .map(|(path, file)| {
            let permissions = file.permissions.as_ref().ok_or_else(|| {
                AgentError::io(format!("{path}: captured without file permissions"))
            })?;
            Ok(CapturedFile {
                path: path.clone(),
                digest: file.digest.clone(),
                size: file.bytes.len() as u64,
                mode: full_mode(permissions)?,
            })
        })
        .collect()
}

#[cfg(unix)]
fn full_mode(permissions: &fs::Permissions) -> Result<u32, AgentError> {
    use std::os::unix::fs::PermissionsExt;
    Ok(permissions.mode() & 0o7777)
}
#[cfg(not(unix))]
fn full_mode(_: &fs::Permissions) -> Result<u32, AgentError> {
    Err(unsupported_platform())
}
#[cfg(not(unix))]
fn unsupported_platform() -> AgentError {
    AgentError::setup(
        "snapshot_unsupported_platform",
        "This platform cannot represent captured octal file modes",
    )
}

fn write_files(
    capture: &ArtifactSnapshot,
    files: &[CapturedFile],
    package_root: &Path,
) -> Result<(), AgentError> {
    let mut directories = vec![package_root.to_path_buf()];
    fs::create_dir(package_root).map_err(|error| io_at(package_root, error))?;
    for file in files {
        let destination = package_root.join(&file.path);
        let parent = destination.parent().expect("package file has a parent");
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|error| io_at(parent, error))?;
            let mut ancestor = Some(parent);
            while let Some(directory) = ancestor.filter(|dir| *dir != package_root) {
                directories.push(directory.to_path_buf());
                ancestor = directory.parent();
            }
        }
        write_synced(&destination, &capture.files[&file.path].bytes)?;
        set_mode(&destination, file.mode)?;
    }
    directories.sort();
    directories.dedup();
    for directory in directories.iter().rev() {
        sync_directory(directory)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), AgentError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        AgentError::io(format!(
            "{}: cannot restore captured mode {mode:04o}: {error}",
            path.display()
        ))
    })
}
#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> Result<(), AgentError> {
    Err(unsupported_platform())
}

/// The `ara status` diagnostics, evaluated on the captured bytes in staging.
fn diagnostics(package_root: &Path) -> Result<Value, AgentError> {
    let loaded = ara_core::parse_dir_detailed(package_root);
    if !loaded.io_issues.is_empty() {
        return Err(AgentError::io(format!(
            "Cannot read captured artifact: {:?}",
            loaded.io_issues
        )));
    }
    let items = loaded
        .report
        .errors()
        .iter()
        .chain(loaded.report.warnings())
        .map(|diagnostic| {
            json!({
                "severity": diagnostic.severity,
                "code": diagnostic.code.to_string(),
                "path": diagnostic.path,
                "message": diagnostic.message,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "errors": loaded.report.errors().len(),
        "warnings": loaded.report.warnings().len(),
        "items": items,
    }))
}

/// Returns the canonical manifest bytes and the capture ID they bind.
fn manifest(fingerprint: &str, files: &[CapturedFile], diagnostics: Value) -> (Vec<u8>, String) {
    let files = files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "digest": file.digest,
                "size": file.size,
                "mode": format!("{:04o}", file.mode),
            })
        })
        .collect::<Vec<_>>();
    let mut manifest = json!({
        "format": FORMAT,
        "fingerprint_scheme": "ara.artifact/v1",
        "fingerprint": fingerprint,
        "root": "ara",
        "files": files,
        "excluded": EXCLUDED,
        "diagnostics": diagnostics,
    });
    let capture_id = capture_id(&manifest);
    manifest["capture_id"] = Value::String(capture_id.clone());
    (canonical_json(&manifest).into_bytes(), capture_id)
}

/// `sha256:` over the hash domain and the JCS manifest without `capture_id`.
pub fn capture_id(manifest: &Value) -> String {
    let mut unbound = manifest.clone();
    if let Some(object) = unbound.as_object_mut() {
        object.remove("capture_id");
    }
    let mut hasher = Sha256::new();
    hasher.update(CAPTURE_DOMAIN);
    hasher.update(canonical_json(&unbound).as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// RFC 8785 encoding for the manifest's value domain: objects, arrays, strings,
/// booleans, null and integers. Keys sort by UTF-16 code units.
pub fn canonical_json(value: &Value) -> String {
    let mut output = String::new();
    write_canonical(value, &mut output);
    output
}
fn write_canonical(value: &Value, output: &mut String) {
    match value {
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
            output.push('{');
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&Value::String(key.clone()).to_string());
                output.push(':');
                write_canonical(value, output);
            }
            output.push('}');
        }
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(item, output);
            }
            output.push(']');
        }
        Value::Number(number) => {
            assert!(
                number.is_i64() || number.is_u64(),
                "canonical manifests hold integers only"
            );
            output.push_str(&number.to_string());
        }
        scalar => output.push_str(&scalar.to_string()),
    }
}

/// Reload the staging copy and its manifest; any difference is internal.
fn verify_package(
    staging: &Path,
    files: &[CapturedFile],
    fingerprint: &str,
    capture_id: &str,
) -> Result<(), AgentError> {
    let internal = |what: &str| {
        AgentError::io(format!(
            "internal error: exported package {what} differs from the capture"
        ))
    };
    let package = ArtifactSnapshot::load_complete(&staging.join("ara")).map_err(convert_error)?;
    if captured_files(&package)? != files {
        return Err(internal("inventory, bytes or modes"));
    }
    if ara_core::merge::fingerprint(&package) != fingerprint {
        return Err(internal("fingerprint"));
    }
    let written = fs::read(staging.join("snapshot.json"))
        .map_err(|error| AgentError::io(format!("snapshot.json: {error}")))?;
    let parsed: Value = serde_json::from_slice(&written).map_err(|_| internal("manifest"))?;
    let recorded = parsed.get("capture_id").and_then(Value::as_str);
    if recorded != Some(capture_id) || self::capture_id(&parsed) != capture_id {
        return Err(internal("capture ID"));
    }
    Ok(())
}

/// A private sibling of the output. Dropped without publication, it is removed.
struct Staging {
    path: PathBuf,
    published: bool,
}
impl Staging {
    fn create(output: &OutputPath) -> Result<Self, AgentError> {
        let path = output.parent.join(format!(
            ".{}.ara-snapshot-{}-{}",
            output.name,
            std::process::id(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|error| io_at(&path, error))?;
        Ok(Self {
            path,
            published: false,
        })
    }
    fn publish(mut self, output: &OutputPath) -> Result<(), AgentError> {
        rename_no_replace(&self.path, &output.path).map_err(|error| {
            if matches!(
                error.kind(),
                std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::DirectoryNotEmpty
            ) {
                output_exists(&output.path)
            } else {
                io_at(&output.path, error)
            }
        })?;
        self.published = true;
        Ok(())
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(target_os = "macos")]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both arguments are valid NUL-terminated paths for the call.
    let status = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(target_os = "linux")]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // The raw syscall works with both glibc and musl, whose `libc` bindings
    // differ in exposing the renameat2 wrapper.
    // SAFETY: both arguments are valid NUL-terminated paths for the call.
    let status = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rename_no_replace(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace rename is unavailable on this platform",
    ))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), AgentError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_at(path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_at(path, error))
}

fn sync_directory(path: &Path) -> Result<(), AgentError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_at(path, error))
}

fn io_at(path: &Path, error: std::io::Error) -> AgentError {
    AgentError::io(format!("{}: {error}", path.display()))
}

/// Deterministic race points for the integration tests. Release builds compile
/// these to nothing, so they add no cost or behavior to shipped binaries.
mod test_hook {
    #[cfg(debug_assertions)]
    pub fn pause(point: &str) {
        let (Some(directory), Ok(at)) = (
            std::env::var_os("ARA_SNAPSHOT_TEST_PAUSE"),
            std::env::var("ARA_SNAPSHOT_TEST_PAUSE_AT"),
        ) else {
            return;
        };
        if at != point {
            return;
        }
        let directory = std::path::PathBuf::from(directory);
        let _ = std::fs::write(directory.join("ready"), point);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !directory.join("go").exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[cfg(debug_assertions)]
    pub fn fail_after_publish() -> Result<(), super::AgentError> {
        if std::env::var_os("ARA_SNAPSHOT_TEST_FAIL_AFTER_PUBLISH").is_some() {
            return Err(super::AgentError::io(
                "injected failure after publication, before durability acknowledgment",
            ));
        }
        Ok(())
    }
    #[cfg(not(debug_assertions))]
    pub fn pause(_: &str) {}
    #[cfg(not(debug_assertions))]
    pub fn fail_after_publish() -> Result<(), super::AgentError> {
        Ok(())
    }
}
