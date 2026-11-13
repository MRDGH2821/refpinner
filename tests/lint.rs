use std::path::Path;
use std::process::{Command, Output};

fn lint(dir: &Path) -> Output {
    lint_with(dir, &[])
}

fn lint_with(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_refpinner"))
        .arg("lint")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run refpinner")
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Builds a fresh project under the Cargo test tmpdir with the given workflow files.
fn project(name: &str, workflows: &[(&str, &str)]) -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join(".github/workflows");
    std::fs::create_dir_all(&dir).expect("create project");
    for (file, body) in workflows {
        std::fs::write(dir.join(file), body).expect("write workflow");
    }
    root
}

/// Builds a fresh folder under the Cargo test tmpdir with files at the given relative paths.
fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create tree");
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        std::fs::write(path, body).expect("write file");
    }
    root
}

/// A workflow with one unpinned reference at line 4, column 15.
const UNPINNED_WORKFLOW: &str = "jobs:\n  a:\n    steps:\n      - uses: owner/repo@v1\n";

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn unpinned_action_fails_and_reports_each_location() {
    let output = lint(&fixture("mixed"));

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:6:15: pinned actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
            ".github/workflows/ci.yml:7:15: unpinned actions/setup-node@v4\n",
        )
    );
}

#[test]
fn empty_scan_fails() {
    let output = lint(&project("empty", &[]));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("no supported CI files found"));
}

#[test]
fn all_pinned_succeeds() {
    let output = lint(&fixture("pinned"));

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        ".github/workflows/release.yaml:6:15: pinned actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n"
    );
}

#[test]
fn invalid_file_is_reported_and_other_files_still_lint() {
    let pinned = std::fs::read_to_string(fixture("pinned/.github/workflows/release.yaml"))
        .expect("read fixture");
    let root = project(
        "invalid",
        &[("a-broken.yml", "jobs: [unclosed\n"), ("b-ok.yml", &pinned)],
    );

    let output = lint(&root);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains(".github/workflows/a-broken.yml"));
    assert_eq!(
        stdout(&output),
        ".github/workflows/b-ok.yml:6:15: pinned actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n"
    );
}

#[test]
fn short_sha_and_branch_are_unpinned() {
    let root = project(
        "short",
        &[(
            "ci.yml",
            "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@3d3c42e\n      - uses: owner/repo/path@main\n",
        )],
    );

    let output = lint(&root);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:4:15: unpinned actions/checkout@3d3c42e\n",
            ".github/workflows/ci.yml:5:15: unpinned owner/repo/path@main\n",
        )
    );
}

#[test]
fn path_discovers_projects_up_to_two_levels_below() {
    let root = tree(
        "depth",
        &[
            (".github/workflows/top.yml", UNPINNED_WORKFLOW),
            ("team/.github/workflows/one.yaml", UNPINNED_WORKFLOW),
            ("team/app/.github/workflows/two.yml", UNPINNED_WORKFLOW),
            (
                "team/app/sub/.github/workflows/three.yml",
                UNPINNED_WORKFLOW,
            ),
            ("team/app/.github/workflows/notes.txt", "not a workflow\n"),
        ],
    );

    let output = lint_with(root.parent().expect("parent"), &["--path", "depth"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/top.yml:4:15: unpinned owner/repo@v1\n",
            "team/.github/workflows/one.yaml:4:15: unpinned owner/repo@v1\n",
            "team/app/.github/workflows/two.yml:4:15: unpinned owner/repo@v1\n",
        )
    );
}

#[test]
fn git_ignore_rules_apply_in_git_folders_and_worktrees() {
    let root = tree(
        "ignored",
        &[
            ("repo/.git/HEAD", "ref: refs/heads/main\n"),
            ("repo/.gitignore", ".github/workflows/generated.yml\n"),
            ("repo/.github/workflows/generated.yml", UNPINNED_WORKFLOW),
            ("repo/.github/workflows/untracked.yml", UNPINNED_WORKFLOW),
            ("worktree/.git", "gitdir: /nonexistent/worktrees/worktree\n"),
            ("worktree/.gitignore", "*.yml\n"),
            (
                "worktree/.github/workflows/generated.yml",
                UNPINNED_WORKFLOW,
            ),
        ],
    );

    let output = lint(&root);

    assert_eq!(
        stdout(&output),
        "repo/.github/workflows/untracked.yml:4:15: unpinned owner/repo@v1\n"
    );
}

#[test]
fn exclusions_skip_discovered_files() {
    let root = tree(
        "excluded",
        &[
            (".github/workflows/kept.yml", UNPINNED_WORKFLOW),
            (".github/workflows/skipped.yml", UNPINNED_WORKFLOW),
            ("vendor/.github/workflows/ci.yml", UNPINNED_WORKFLOW),
        ],
    );

    let output = lint_with(&root, &["--exclude", "skipped.yml", "--exclude", "vendor/"]);

    assert_eq!(
        stdout(&output),
        ".github/workflows/kept.yml:4:15: unpinned owner/repo@v1\n"
    );
}

#[test]
fn explicit_files_override_ignore_rules_and_exclusions() {
    let root = tree(
        "explicit",
        &[
            (".git/HEAD", "ref: refs/heads/main\n"),
            (".gitignore", "*.yml\n"),
            (".github/workflows/ci.yml", UNPINNED_WORKFLOW),
            ("elsewhere/build.yaml", UNPINNED_WORKFLOW),
        ],
    );

    let output = lint_with(
        &root,
        &[
            "--exclude",
            "*.yaml",
            "elsewhere/build.yaml",
            ".github/workflows/ci.yml",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:4:15: unpinned owner/repo@v1\n",
            "elsewhere/build.yaml:4:15: unpinned owner/repo@v1\n",
        )
    );
}

#[test]
fn backups_are_skipped() {
    let root = tree(
        "backups",
        &[
            (".github/workflows/ci.yml", UNPINNED_WORKFLOW),
            (".github/workflows/ci.yml.bkp", UNPINNED_WORKFLOW),
            (".github/workflows/ci.yml.bkp2", UNPINNED_WORKFLOW),
        ],
    );

    let discovered = lint(&root);
    let explicit = lint_with(
        &root,
        &[".github/workflows/ci.yml", ".github/workflows/ci.yml.bkp2"],
    );

    let only_ci = ".github/workflows/ci.yml:4:15: unpinned owner/repo@v1\n";
    assert_eq!(stdout(&discovered), only_ci);
    assert_eq!(
        stdout(&explicit),
        format!(".github/workflows/ci.yml.bkp2: warning: skipped backup\n{only_ci}")
    );
}

#[test]
fn empty_explicit_selection_fails_lint() {
    let root = tree("only-backup", &[("ci.yml.bkp", UNPINNED_WORKFLOW)]);

    let output = lint_with(&root, &["ci.yml.bkp"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "ci.yml.bkp: warning: skipped backup\n");
    assert!(stderr(&output).contains("no supported CI files found"));
}

#[cfg(unix)]
#[test]
fn symlinks_are_skipped_with_warnings() {
    let root = tree(
        "symlinks",
        &[
            (".github/workflows/ci.yml", UNPINNED_WORKFLOW),
            ("shared/linked.yml", UNPINNED_WORKFLOW),
        ],
    );
    std::os::unix::fs::symlink(
        root.join("shared/linked.yml"),
        root.join(".github/workflows/linked.yml"),
    )
    .expect("symlink file");
    std::os::unix::fs::symlink(root.join(".github"), root.join("shared/.github"))
        .expect("symlink folder");
    let only_ci = ".github/workflows/ci.yml:4:15: unpinned owner/repo@v1\n";

    let discovered = lint(&root);
    let quiet = lint_with(&root, &["--no-symlink-warnings"]);
    let explicit = lint_with(
        &root,
        &[
            "--no-symlink-warnings",
            ".github/workflows/ci.yml",
            ".github/workflows/linked.yml",
        ],
    );

    assert_eq!(
        stdout(&discovered),
        concat!(
            ".github/workflows/linked.yml: warning: skipped symlink\n",
            "shared/.github: warning: skipped symlink\n",
            ".github/workflows/ci.yml:4:15: unpinned owner/repo@v1\n",
        )
    );
    assert_eq!(stdout(&quiet), only_ci);
    assert_eq!(
        stdout(&explicit),
        format!(".github/workflows/linked.yml: warning: skipped symlink\n{only_ci}")
    );
}
