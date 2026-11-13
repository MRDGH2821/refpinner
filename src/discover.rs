use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Returns the GitHub workflow files directly under `root/.github/workflows` and the
/// action metadata files at `root` or under `root/.github/actions`, sorted.
///
/// shortcut: only the project at `root` is scanned; project discovery and ignore rules
/// arrive with the discovery ticket (#3).
pub fn files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = root.join(".github").join("workflows");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries.into_iter().flatten() {
        let entry = entry?;
        let path = entry.path();
        let yaml = matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yml" | "yaml")
        );
        // `DirEntry::file_type` does not follow symlinks, so symlinks are skipped.
        if yaml && entry.file_type()?.is_file() {
            files.push(path);
        }
    }
    actions(root, &mut files)?;
    actions_under(&root.join(".github").join("actions"), &mut files)?;
    files.sort();
    Ok(files)
}

/// Adds the action metadata file (`action.yml` or `action.yaml`) directly in `dir`.
fn actions(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for name in ["action.yml", "action.yaml"] {
        let path = dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => files.push(path),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Adds action metadata files in `dir` and every real subdirectory; symlinks are skipped.
fn actions_under(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    actions(dir, files)?;
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            actions_under(&entry.path(), files)?;
        }
    }
    Ok(())
}
