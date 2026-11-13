use std::ffi::OsStr;
use std::io;
use std::path::{Component, Path, PathBuf};

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;

use crate::lint::display;

/// Project folders deeper than this many levels below the scan path are not discovered (ADR 0001).
const PROJECT_DEPTH: usize = 2;

/// Selects the CI files a command reads.
#[derive(clap::Args)]
pub struct Scan {
    /// Folder in which to discover projects up to two levels below it
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    /// Skip discovered paths matching this gitignore-style glob, relative to --path
    #[arg(long, value_name = "GLOB")]
    exclude: Vec<String>,
    /// Do not warn about symlinks found during discovery
    #[arg(long)]
    no_symlink_warnings: bool,
    /// Files to read instead of discovering them; ignore rules and exclusions do not apply
    #[arg(conflicts_with = "path")]
    files: Vec<PathBuf>,
}

impl Scan {
    /// Returns the selected files, sorted. Prints a warning on stdout for each skipped symlink,
    /// and for each explicitly supplied backup.
    pub fn files(&self) -> io::Result<Vec<PathBuf>> {
        if self.files.is_empty() {
            discover(&self.path, &self.exclude, !self.no_symlink_warnings)
        } else {
            Ok(explicit(&self.files))
        }
    }
}

/// Returns the GitHub workflow files in `<project>/.github/workflows`, where `<project>` is
/// `root` or a folder up to two levels below it.
///
/// Git ignore rules apply inside Git repositories, whether `.git` is a folder or a worktree
/// file. Symlinks are never followed.
///
/// shortcut: a tracked file that matches a Git ignore rule is skipped, because discovery does
/// not read the Git index; read the index if users report missing tracked workflows.
fn discover(root: &Path, excludes: &[String], warn: bool) -> io::Result<Vec<PathBuf>> {
    let mut overrides = OverrideBuilder::new(root);
    for glob in excludes {
        overrides
            .add(&format!("!{glob}"))
            .map_err(io::Error::other)?;
    }
    let base = root.to_path_buf();
    let walk = WalkBuilder::new(root)
        .hidden(false)
        .ignore(false)
        .sort_by_file_name(OsStr::cmp)
        .overrides(overrides.build().map_err(io::Error::other)?)
        .filter_entry(move |entry| {
            entry.file_name() != ".git" && location(relative(&base, entry.path())).is_some()
        })
        .build();
    let mut files = Vec::new();
    for entry in walk {
        let entry = entry.map_err(io::Error::other)?;
        let (Some(kind), Some(found)) = (entry.file_type(), location(relative(root, entry.path())))
        else {
            continue;
        };
        if kind.is_symlink() {
            if warn && !matches!(found, Location::Project) {
                let shown = display(relative(root, entry.path()));
                println!("{shown}: warning: skipped symlink");
            }
        } else if kind.is_file() && matches!(found, Location::Workflow) {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

/// Returns the explicitly supplied `files` that are neither symlinks nor backups, and prints a
/// warning for each skipped file.
fn explicit(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut selected = Vec::new();
    for file in files {
        if file.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) {
            println!("{}: warning: skipped symlink", display(file));
        } else if is_backup(file) {
            println!("{}: warning: skipped backup", display(file));
        } else {
            selected.push(file.clone());
        }
    }
    selected.sort();
    selected
}

fn relative<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

enum Location {
    /// A path at most two levels below the scan root, which may be a project folder.
    Project,
    /// A recognized CI folder: `.github` or `.github/workflows`.
    Ci,
    /// A GitHub workflow file.
    Workflow,
}

/// Classifies a path relative to the scan root, or returns `None` when discovery skips it.
fn location(relative: &Path) -> Option<Location> {
    let parts: Vec<&OsStr> = relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let depth = parts.len();
    let Some(ci) = parts.iter().position(|part| *part == ".github") else {
        return (depth <= PROJECT_DEPTH).then_some(Location::Project);
    };
    if ci > PROJECT_DEPTH {
        return None;
    }
    match depth - ci {
        1 => Some(Location::Ci),
        2 if parts[ci + 1] == "workflows" => Some(Location::Ci),
        3 if parts[ci + 1] == "workflows" && is_workflow(relative) => Some(Location::Workflow),
        _ => None,
    }
}

fn is_workflow(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("yml" | "yaml")
    )
}

/// Matches the numbered backup names `.bkp`, `.bkp2`, `.bkp3`, and so on.
fn is_backup(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .and_then(|ext| ext.strip_prefix("bkp"))
        .is_some_and(|number| number.chars().all(|c| c.is_ascii_digit()))
}
