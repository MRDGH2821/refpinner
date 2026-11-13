use std::path::Path;
use std::process::{Command, Output};

fn lint(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_refpinner"))
        .arg("lint")
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
            ".github/actions/setup/action.yaml:5:13: unpinned actions/setup-node@v4\n",
            ".github/actions/setup/nested/action.yml:5:13: unpinned owner/repo/path@main\n",
            "action.yml:5:13: pinned actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
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
