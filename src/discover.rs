use std::ffi::OsStr;
use std::io;
use std::path::{Component, Path, PathBuf};

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;

use crate::lint::display;

/// GitLab's default CI file name; custom CI file paths are not discovered.
pub const GITLAB_CI: &str = ".gitlab-ci.yml";

/// Project folders deeper than this many levels below the scan path are not discovered (ADR 0001).
const PROJECT_DEPTH: usize = 2;

/// Selects the CI files a command reads.
#[derive(clap::Args)]
pub struct Scan {
    #[command(flatten)]
    pub config: crate::config::Options,
    /// Folder in which to discover projects up to two levels below it
    #[arg(long, default_value = ".", env = "REFPINNER_PATH")]
    pub path: PathBuf,
    /// Skip discovered paths matching this gitignore-style glob, relative to --path
    #[arg(
        long,
        value_name = "GLOB",
        env = "REFPINNER_EXCLUDE",
        value_delimiter = ','
    )]
    exclude: Vec<String>,
    /// Only scan this platform, overriding the configured platform list
    #[arg(long, env = "REFPINNER_PLATFORM")]
    platform: Option<crate::config::Platform>,
    /// Include references excluded by configuration, for an exclusion audit
    #[arg(long, env = "REFPINNER_INCLUDE_EXCLUDED")]
    pub include_excluded: bool,
    /// Do not warn about symlinks found during discovery (accepts an explicit false)
    #[arg(long, env = "REFPINNER_NO_SYMLINK_WARNINGS", num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    no_symlink_warnings: Option<bool>,
    /// Files to read instead of discovering them; ignore rules and exclusions do not apply
    #[arg(conflicts_with = "path")]
    files: Vec<PathBuf>,
}

pub struct Input {
    pub path: PathBuf,
    pub settings: crate::config::Settings,
    pub platform: crate::config::Platform,
}

impl Scan {
    /// Validates every project's configuration before returning any files to a writer.
    /// Prints a warning on stdout for skipped symlinks and explicitly supplied backups.
    pub fn files(&self) -> io::Result<Vec<Input>> {
        let root = self.path.canonicalize()?;
        let mut projects = vec![root.clone()];
        if self.files.is_empty() {
            let walk = WalkBuilder::new(&root)
                .hidden(false)
                .ignore(false)
                .max_depth(Some(PROJECT_DEPTH))
                .filter_entry(|entry| {
                    !matches!(
                        entry.file_name().to_str(),
                        Some(".git" | ".github" | ".config")
                    )
                })
                .build();
            for entry in walk {
                let entry = entry.map_err(io::Error::other)?;
                if entry.depth() > 0 && entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    projects.push(entry.into_path());
                }
            }
        }
        let mut configurations = Vec::new();
        for project in projects {
            let settings = crate::config::load(&project, self.config.config.as_deref())
                .map_err(io::Error::other)?;
            configurations.push((project, settings));
        }
        let mut candidates = if self.files.is_empty() {
            discover(&root, &[], |file| {
                let project = project_for(&root, file);
                let configured = configurations
                    .iter()
                    .find(|(path, _)| *path == project)
                    .and_then(|(_, settings)| settings.no_symlink_warnings);
                !self.no_symlink_warnings.or(configured).unwrap_or(false)
            })?
        } else {
            explicit(&self.files)
                .into_iter()
                .map(|file| root.join(file))
                .collect()
        };
        if self.files.is_empty() {
            for (project, _) in &configurations {
                if project.join(".git").symlink_metadata().is_ok() {
                    tracked_files(project, &root, &mut candidates)?;
                }
            }
            candidates.sort();
            candidates.dedup();
        }
        let mut selected = Vec::new();
        for (project, settings) in configurations {
            let excludes = if self.exclude.is_empty() {
                settings.exclude.as_deref().unwrap_or_default()
            } else {
                &self.exclude
            };
            let exclude_root = if self.exclude.is_empty() {
                &project
            } else {
                &root
            };
            let mut builder = OverrideBuilder::new(exclude_root);
            for pattern in excludes {
                builder
                    .add(&format!("!{pattern}"))
                    .map_err(io::Error::other)?;
            }
            let matcher = builder.build().map_err(io::Error::other)?;
            for file in &candidates {
                if self.files.is_empty() && project_for(&root, file) != project {
                    continue;
                }
                if self.files.is_empty()
                    && !self.include_excluded
                    && excluded(&matcher, exclude_root, file)
                {
                    continue;
                }
                selected.push(Input {
                    path: file.clone(),
                    settings: settings.clone(),
                    platform: platform(file),
                });
            }
            if self.files.is_empty() {
                for file in settings.files.as_deref().unwrap_or_default() {
                    for path in explicit(std::slice::from_ref(&file.path)) {
                        let path = path.canonicalize().unwrap_or(path);
                        selected.retain(|input| input.path != path);
                        selected.push(Input {
                            path,
                            settings: settings.clone(),
                            platform: file.platform,
                        });
                    }
                }
            }
        }
        selected.retain(|input| match self.platform {
            Some(platform) => input.platform == platform,
            None => input
                .settings
                .platforms
                .as_ref()
                .is_none_or(|platforms| platforms.contains(&input.platform)),
        });
        selected.sort_by(|a, b| a.path.cmp(&b.path));
        selected.dedup_by(|a, b| a.path == b.path);
        Ok(selected)
    }
}

fn platform(file: &Path) -> crate::config::Platform {
    use crate::config::Platform;
    if file.file_name() == Some(OsStr::new(GITLAB_CI)) {
        return Platform::Gitlab;
    }
    match crate::settings::platform(file) {
        ".forgejo" => Platform::Forgejo,
        ".gitea" => Platform::Gitea,
        _ => Platform::Github,
    }
}

fn project_for(root: &Path, file: &Path) -> PathBuf {
    let mut project = root.to_path_buf();
    for component in relative(root, file).components() {
        if matches!(
            component.as_os_str().to_str(),
            Some(".github" | ".forgejo" | ".gitea")
        ) {
            return project;
        }
        project.push(component);
    }
    file.parent().unwrap_or(root).to_path_buf()
}

fn tracked_files(project: &Path, root: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut command = std::process::Command::new("git");
    command.args([
        "-C",
        project
            .to_str()
            .ok_or_else(|| io::Error::other("non-UTF-8 project path"))?,
        "ls-files",
        "-z",
        "--cached",
    ]);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(key);
    }
    let output = command.output()?;
    // Fake `.git` markers in fixtures are not usable repos; the walker already found files.
    if !output.status.success() {
        return Ok(());
    }
    for name in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name =
            std::str::from_utf8(name).map_err(|_| io::Error::other("non-UTF-8 Git index path"))?;
        let file = project.join(name);
        let Some(found) = location(relative(root, &file)) else {
            continue;
        };
        if !matches!(found, Location::CiFile | Location::ProjectAction) || is_backup(&file) {
            continue;
        }
        if file
            .components()
            .scan(PathBuf::new(), |path, component| {
                path.push(component);
                Some(
                    path.symlink_metadata()
                        .is_ok_and(|metadata| metadata.file_type().is_symlink()),
                )
            })
            .any(|symlink| symlink)
        {
            continue;
        }
        if file.is_file() {
            files.push(file);
        }
    }
    Ok(())
}

fn excluded(matcher: &ignore::overrides::Override, root: &Path, file: &Path) -> bool {
    file.ancestors()
        .take_while(|path| *path != root)
        .any(|path| matcher.matched(path, path.is_dir()).is_ignore())
}

/// Returns the recognized CI files of each project, sorted. A project is `root` or a folder up
/// to two levels below it. Its CI files are `.github/workflows/*.{yml,yaml}`, the same under
/// `.forgejo` and `.gitea`, `.gitlab-ci.yml`, and `action.{yml,yaml}` at the project root or
/// anywhere under `.github/actions`.
///
/// Git ignore rules apply inside Git repositories, whether `.git` is a folder or a worktree
/// file. Symlinks are never followed.
///
fn discover(
    root: &Path,
    excludes: &[String],
    warn: impl Fn(&Path) -> bool,
) -> io::Result<Vec<PathBuf>> {
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
        let recognized = match found {
            Location::Project => false,
            Location::ProjectAction => is_project(entry.path()),
            Location::Ci | Location::CiFile => true,
        };
        if kind.is_symlink() {
            if recognized && warn(entry.path()) {
                let shown = display(relative(root, entry.path()));
                println!("{shown}: warning: skipped symlink");
            }
        } else if kind.is_file() && recognized && !matches!(found, Location::Ci) {
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

/// Reports whether the folder holding `file` is a project: it has Git metadata (a `.git`
/// folder or worktree file) or a `.github` folder.
fn is_project(file: &Path) -> bool {
    let folder = file.parent().unwrap_or(file);
    [".git", ".github", ".forgejo", ".gitea"]
        .iter()
        .any(|marker| folder.join(marker).symlink_metadata().is_ok())
}

fn relative<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

enum Location {
    /// A path at most two levels below the scan root, which may be a project folder.
    Project,
    /// A recognized CI folder: `.github`, `.github/workflows` or `.github/actions/**`, or the
    /// `.forgejo` and `.gitea` equivalents of the first two.
    Ci,
    /// A workflow file, action metadata under `.github/actions` or at the scan root, or a
    /// GitLab CI file.
    CiFile,
    /// Action metadata in a folder below the scan root; read only when that folder is a project.
    ProjectAction,
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
    let action = matches!(
        relative.file_name().and_then(OsStr::to_str),
        Some("action.yml" | "action.yaml")
    );
    let gitlab = relative.file_name() == Some(OsStr::new(GITLAB_CI));
    let Some(ci) = parts
        .iter()
        .position(|part| matches!(part.to_str(), Some(".github" | ".forgejo" | ".gitea")))
    else {
        return match depth {
            1 if action => Some(Location::CiFile),
            _ if gitlab && depth <= PROJECT_DEPTH + 1 => Some(Location::CiFile),
            _ if action && depth <= PROJECT_DEPTH + 1 => Some(Location::ProjectAction),
            _ if depth <= PROJECT_DEPTH => Some(Location::Project),
            _ => None,
        };
    };
    if ci > PROJECT_DEPTH {
        return None;
    }
    let folder = parts.get(ci + 1).and_then(|part| part.to_str());
    match (depth - ci, folder) {
        (1, _) | (2, Some("workflows")) => Some(Location::Ci),
        (_, Some("workflows"))
            if is_yaml(relative) && (depth - ci == 3 || parts[ci] != ".github") =>
        {
            Some(Location::CiFile)
        }
        (_, Some("workflows")) if parts[ci] != ".github" => Some(Location::Ci),
        (_, Some("actions")) if action => Some(Location::CiFile),
        (_, Some("actions")) => Some(Location::Ci),
        _ => None,
    }
}

fn is_yaml(path: &Path) -> bool {
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
