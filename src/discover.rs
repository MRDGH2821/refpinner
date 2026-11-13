use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Returns the GitHub workflow files directly under `root/.github/workflows`, sorted.
///
/// shortcut: only the project at `root` is scanned; project discovery and ignore rules
/// arrive with the discovery ticket (#3).
pub fn files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = root.join(".github").join("workflows");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
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
    files.sort();
    Ok(files)
}
