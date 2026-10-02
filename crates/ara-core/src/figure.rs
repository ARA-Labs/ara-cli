//! Local raster figure references shared by native ingestion and the viewer.

/// Accept a relative filesystem path, not a URL or an encoded URL path.
/// Literal `%`, `#`, and `?` are filename characters at this boundary.
pub fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Supported raster MIME, selected case-insensitively from the filename.
pub fn image_mime(path: &str) -> Option<&'static str> {
    match path.rsplit_once('.')?.1 {
        ext if ext.eq_ignore_ascii_case("png") => Some("image/png"),
        ext if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") => {
            Some("image/jpeg")
        }
        _ => None,
    }
}

/// Validate a manifest's artifact-relative image reference independently of ingestion.
pub fn valid_image_reference(path: &str) -> bool {
    safe_relative_path(path) && path.starts_with("evidence/") && image_mime(path).is_some()
}

/// Resolve a regular image inside the canonical evidence root and check its signature.
/// The selected artifact root and contained assets may be symlinks. This preflight is
/// not a race-proof sandbox against concurrent hostile filesystem mutation.
#[cfg(feature = "native")]
pub fn resolve_image(
    evidence_root: &std::path::Path,
    relative: &str,
) -> std::io::Result<(std::path::PathBuf, &'static str)> {
    use std::io::{Error, ErrorKind, Read};
    if !safe_relative_path(relative) {
        return Err(Error::new(ErrorKind::InvalidInput, "unsafe image path"));
    }
    let mime = image_mime(relative)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "unsupported image format"))?;
    let root = evidence_root.canonicalize()?;
    let path = root.join(relative).canonicalize()?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "image escapes evidence root or is not a regular file",
        ));
    }
    let mut signature = [0u8; 8];
    std::fs::File::open(&path)?.read_exact(&mut signature)?;
    let valid = match mime {
        "image/png" => signature == *b"\x89PNG\r\n\x1a\n",
        "image/jpeg" => signature[..3] == [0xff, 0xd8, 0xff],
        _ => false,
    };
    if !valid {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "image signature does not match its format",
        ));
    }
    Ok((path, mime))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_reject_url_and_traversal_but_keep_filename_punctuation() {
        for path in [
            "/evidence/x.png",
            "//host/x.png",
            "https://host/x.png",
            "data:image/png",
            "evidence/../x.png",
            "evidence/./x.png",
            "evidence\\x.png",
            "C:/x.png",
            "evidence/x\n.png",
            "evidence/x.svg",
        ] {
            assert!(!valid_image_reference(path), "{path:?}");
        }
        assert!(valid_image_reference("evidence/figures/雪 % # ?.PNG"));
    }
}
