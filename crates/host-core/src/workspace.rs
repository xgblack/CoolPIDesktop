use crate::runtime::{HostError, Result};
use std::path::{Path, PathBuf};

pub fn canonical_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(HostError::new(
            "directory_invalid",
            "Directory must be absolute",
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| HostError::new("directory_missing", e))?;
    if !canonical.is_dir() {
        return Err(HostError::new(
            "directory_invalid",
            "Path is not a directory",
        ));
    }
    std::fs::read_dir(&canonical).map_err(|e| HostError::new("directory_unreadable", e))?;
    Ok(canonical)
}

pub fn validate_roots(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        return Err(HostError::new(
            "directory_invalid",
            "A primary directory is required",
        ));
    }
    let mut canonical = Vec::new();
    for path in paths {
        let resolved = canonical_directory(path)?;
        if canonical.contains(&resolved) {
            return Err(HostError::new(
                "directory_duplicate",
                "Duplicate real directory",
            ));
        }
        canonical.push(resolved);
    }
    Ok(canonical)
}

pub fn verify_snapshot(original: &[PathBuf], expected: &[PathBuf], trusted: bool) -> Result<()> {
    if validate_roots(original)? != expected {
        return Err(HostError::new(
            "directory_changed",
            "Directory target changed; relocate and confirm trust",
        ));
    }
    if !trusted {
        return Err(HostError::new(
            "project_untrusted",
            "Confirm trust before starting OMP",
        ));
    }
    Ok(())
}
