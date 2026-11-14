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
fn composite_actions_in_recognized_locations_are_reported() {
    let output = lint(&fixture("composite"));

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/actions/setup/action.yaml:6:13: unpinned actions/setup-node@v4\n",
            ".github/actions/setup/nested/action.yml:6:13: unpinned owner/repo/path@main\n",
            "action.yml:6:13: pinned actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
        )
    );
}

#[test]
fn reusable_workflow_references_are_reported() {
    let root = project(
        "reusable",
        &[(
            "call.yml",
            concat!(
                "jobs:\n",
                "  pinned:\n",
                "    uses: owner/repo/.github/workflows/build.yml@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
                "  floating:\n",
                "    uses: owner/repo/.github/workflows/build.yml@v1\n",
                "  local:\n",
                "    uses: ./.github/workflows/build.yml\n",
                "  self:\n",
                "    uses: $/.github/workflows/build.yml\n",
            ),
        )],
    );

    let output = lint(&root);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/call.yml:3:11: pinned owner/repo/.github/workflows/build.yml@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
            ".github/workflows/call.yml:5:11: unpinned owner/repo/.github/workflows/build.yml@v1\n",
        )
    );
}

#[test]
fn dynamic_references_anchors_and_aliases_warn_without_failing() {
    let root = project(
        "unsupported",
        &[(
            "ci.yml",
            concat!(
                "jobs:\n",
                "  a:\n",
                "    steps: &steps\n",
                "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
                "  b:\n",
                "    steps: *steps\n",
                "  c:\n",
                "    uses: ${{ matrix.workflow }}\n",
            ),
        )],
    );

    let output = lint(&root);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:3:12: warning unsupported YAML anchor\n",
            ".github/workflows/ci.yml:4:15: pinned actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
            ".github/workflows/ci.yml:6:12: warning unsupported YAML alias\n",
            ".github/workflows/ci.yml:8:11: warning unsupported dynamic reference ${{ matrix.workflow }}\n",
        )
    );
}

#[test]
fn aliased_reference_is_not_resolved() {
    let root = project(
        "aliased",
        &[(
            "ci.yml",
            concat!(
                "x: &action actions/checkout@v4\n",
                "jobs:\n",
                "  a:\n",
                "    steps:\n",
                "      - uses: *action\n",
            ),
        )],
    );

    let output = lint(&root);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:1:4: warning unsupported YAML anchor\n",
            ".github/workflows/ci.yml:5:15: warning unsupported YAML alias\n",
        )
    );
}

#[test]
fn warnings_do_not_hide_unpinned_failures() {
    let root = project(
        "warn-and-fail",
        &[(
            "ci.yml",
            concat!(
                "jobs:\n",
                "  a:\n",
                "    steps:\n",
                "      - uses: ${{ matrix.action }}\n",
                "      - uses: actions/checkout@v4\n",
            ),
        )],
    );

    let output = lint(&root);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:4:15: warning unsupported dynamic reference ${{ matrix.action }}\n",
            ".github/workflows/ci.yml:5:15: unpinned actions/checkout@v4\n",
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

#[test]
fn composite_actions_are_found_in_discovered_projects() {
    let action = "runs:\n  steps:\n    - uses: owner/repo@v1\n";
    let root = tree(
        "nested-actions",
        &[
            ("app/action.yml", action),
            ("app/.git/HEAD", "ref: refs/heads/main\n"),
            ("app/.github/actions/build/deep/action.yaml", action),
            ("docs/action.yml", action),
        ],
    );

    let output = lint(&root);

    assert_eq!(
        stdout(&output),
        concat!(
            "app/.github/actions/build/deep/action.yaml:3:13: unpinned owner/repo@v1\n",
            "app/action.yml:3:13: unpinned owner/repo@v1\n",
        )
    );
}

#[test]
fn gitlab_project_includes_are_reported_and_unsupported_includes_warn() {
    let ci = concat!(
        "include:\n",
        "  - local: /ci/build.yml\n",
        "  - ci/test.yml\n",
        "  - https://example.com/ci.yml\n",
        "  - remote: https://example.com/remote.yml\n",
        "  - template: Auto-DevOps.gitlab-ci.yml\n",
        "  - component: $CI_SERVER_FQDN/group/proj/name@1.0\n",
        "  - project: group/pinned\n",
        "    ref: 0123456789abcdef0123456789abcdef01234567\n",
        "    file: /a.yml\n",
        "  - project: group/tagged\n",
        "    ref: v1.2.3\n",
        "    file: /a.yml\n",
        "  - project: group/default\n",
        "    file: /a.yml\n",
        "  - project: group/$NAME\n",
        "    ref: $CI_COMMIT_REF_NAME\n",
        "    file: /a.yml\n",
        "  - {project: group/flow, ref: main, file: /a.yml}\n",
    );
    let single = "include:\n  project: group/sub\n  file: /a.yml\n";
    let root = tree(
        "gitlab-includes",
        &[
            (".gitlab-ci.yml", ci),
            ("team/app/.gitlab-ci.yml", single),
            ("team/app/sub/.gitlab-ci.yml", single),
        ],
    );

    let output = lint(&root);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        concat!(
            ".gitlab-ci.yml:4:5: warning unsupported remote include https://example.com/ci.yml\n",
            ".gitlab-ci.yml:5:13: warning unsupported remote include https://example.com/remote.yml\n",
            ".gitlab-ci.yml:6:15: warning unsupported template include Auto-DevOps.gitlab-ci.yml\n",
            ".gitlab-ci.yml:7:16: warning unsupported component include $CI_SERVER_FQDN/group/proj/name@1.0\n",
            ".gitlab-ci.yml:9:10: pinned group/pinned@0123456789abcdef0123456789abcdef01234567\n",
            ".gitlab-ci.yml:12:10: unpinned group/tagged@v1.2.3\n",
            ".gitlab-ci.yml:14:14: unpinned group/default\n",
            ".gitlab-ci.yml:16:14: warning unsupported dynamic reference group/$NAME@$CI_COMMIT_REF_NAME\n",
            ".gitlab-ci.yml:19:32: unpinned group/flow@main\n",
            "team/app/.gitlab-ci.yml:2:12: unpinned group/sub\n",
        )
    );
}
