mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Forge;

/// The pinned commit of `v4.2.1` in [`tool`].
const OLD: &str = "1111111111111111111111111111111111111111";

/// Routes for `owner/tool`: `v4.2.1` is at `1111…`; newer tags exist in v4 and v5.
fn tool() -> Vec<(&'static str, u16, &'static str)> {
    vec![
        (
            "/repos/owner/tool/git/ref/tags/v4.2.1",
            200,
            r#"{"ref":"refs/tags/v4.2.1","object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}}"#,
        ),
        (
            "/repos/owner/tool/git/matching-refs/tags/v",
            200,
            concat!(
                r#"[{"ref":"refs/tags/v3.9.9","object":{"sha":"9999999999999999999999999999999999999999","type":"commit"}},"#,
                r#"{"ref":"refs/tags/v4","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}},"#,
                r#"{"ref":"refs/tags/v4.2.1","object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}},"#,
                r#"{"ref":"refs/tags/v4.2.3","object":{"sha":"2222222222222222222222222222222222222222","type":"commit"}},"#,
                r#"{"ref":"refs/tags/v4.10.0","object":{"sha":"3333333333333333333333333333333333333333","type":"tag"}},"#,
                r#"{"ref":"refs/tags/v4.11","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}},"#,
                r#"{"ref":"refs/tags/v5.1.0","object":{"sha":"5555555555555555555555555555555555555555","type":"commit"}},"#,
                r#"{"ref":"refs/tags/release-9","object":{"sha":"8888888888888888888888888888888888888888","type":"commit"}}]"#,
            ),
        ),
        (
            "/repos/owner/tool/git/ref/tags/v4.2.3",
            200,
            r#"{"ref":"refs/tags/v4.2.3","object":{"sha":"2222222222222222222222222222222222222222","type":"commit"}}"#,
        ),
        (
            "/repos/owner/tool/git/ref/tags/v4.10.0",
            200,
            r#"{"ref":"refs/tags/v4.10.0","object":{"sha":"3333333333333333333333333333333333333333","type":"tag"}}"#,
        ),
        (
            "/repos/owner/tool/git/tags/3333333333333333333333333333333333333333",
            200,
            r#"{"sha":"3333333333333333333333333333333333333333","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}}"#,
        ),
        (
            "/repos/owner/tool/git/ref/tags/v5.1.0",
            200,
            r#"{"ref":"refs/tags/v5.1.0","object":{"sha":"5555555555555555555555555555555555555555","type":"commit"}}"#,
        ),
    ]
}

/// Builds a fresh project under the Cargo test tmpdir; paths are relative to the project root.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("update")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create project");
        std::fs::write(path, body).expect("write file");
    }
    root
}

/// A project whose workflow has one step per entry of `uses`.
fn steps(name: &str, uses: &[&str]) -> PathBuf {
    let mut workflow = String::from("jobs:\n  a:\n    steps:\n");
    for value in uses {
        workflow.push_str(&format!("      - uses: {value}\n"));
    }
    project(name, &[(".github/workflows/ci.yml", &workflow)])
}

fn workflow(root: &Path) -> String {
    std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read workflow")
}

fn update(dir: &Path, url: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .arg("update")
        .args(args)
        .current_dir(dir)
        .env("REFPINNER_GITHUB_API_URL", url);
    for proxy in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env_remove(proxy);
    }
    command.output().expect("run refpinner")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Tag routes with the current tag at OLD and each candidate at a different commit.
fn versions(current: &str, candidates: &[&str]) -> Forge {
    let mut routes = Vec::new();
    let mut entries = Vec::new();
    for (index, tag) in std::iter::once(current)
        .chain(candidates.iter().copied())
        .enumerate()
    {
        let commit = if index == 0 {
            OLD.to_owned()
        } else {
            format!("{index:040x}")
        };
        let body = serde_json::json!({
            "ref": format!("refs/tags/{tag}"),
            "object": { "sha": commit, "type": "commit" }
        });
        entries.push(body.clone());
        routes.push((
            format!("/repos/owner/tool/git/ref/tags/{}", tag.replace('+', "%2B")),
            200,
            body.to_string(),
        ));
    }
    routes.push((
        "/repos/owner/tool/git/matching-refs/tags/v".to_owned(),
        200,
        serde_json::to_string(&entries).unwrap(),
    ));
    Forge::new(
        &routes
            .iter()
            .map(|(path, status, body)| (path.as_str(), *status, body.as_str()))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn stable_updates_filter_prereleases_before_ordering() {
    let root = steps(
        "prerelease-policy",
        &[&format!("owner/tool@{OLD} # v4.2.1")],
    );
    let forge = versions(
        "v4.2.1",
        &["v4.3.0", "v4.4.0-rc.2", "v4.4.0-rc.10", "v4.5.0-beta"],
    );
    for (args, chosen) in [
        (&[][..], "v4.3.0"),
        (&["--rc"][..], "v4.4.0-rc.10"),
        (&["--pre"][..], "v4.5.0-beta"),
    ] {
        let output = update(&root, &forge.url, args);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).contains(&format!("v4.2.1 -> {chosen}\n")),
            "{}",
            stdout(&output)
        );
    }
}

#[test]
fn existing_prereleases_advance_within_their_release_and_graduate() {
    for (index, (current, candidates, chosen)) in [
        (
            "v4.3.0-beta.1",
            vec!["v4.3.0-beta.2", "v4.4.0-rc.1"],
            "v4.3.0-beta.2",
        ),
        (
            "v4.3.0-rc.1",
            vec!["v4.3.0-rc.2", "v4.3.0", "v4.4.0-beta"],
            "v4.3.0",
        ),
        (
            "v4.3.0-rc.1",
            vec!["v4.3.0", "v4.4.0", "v4.5.0-rc"],
            "v4.4.0",
        ),
        ("v4.3-rc.1", vec!["v4.3-rc.2", "v4.4-beta"], "v4.3-rc.2"),
    ]
    .into_iter()
    .enumerate()
    {
        let root = steps(
            &format!("prerelease-sequence-{index}"),
            &[&format!("owner/tool@{OLD} # {current}")],
        );
        let forge = versions(current, &candidates);
        let output = update(&root, &forge.url, &[]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).contains(&format!("{current} -> {chosen}\n")),
            "{}",
            stdout(&output)
        );
    }
}

#[test]
fn rc_recognition_is_case_insensitive_but_not_substring_matching() {
    for (index, rc) in ["rc", "rc.1", "rc1", "RC", "Rc.2", "rC3"]
        .into_iter()
        .enumerate()
    {
        let chosen = format!("v4.3.0-{rc}");
        let forge = versions(
            "v4.2.1",
            &[
                &chosen,
                "v4.9.0-preview",
                "v4.9.0-source",
                "v4.9.0-rc-preview",
                "v4.9.0-rc.1.beta",
            ],
        );
        let root = steps(
            &format!("rc-form-{index}"),
            &[&format!("owner/tool@{OLD} # v4.2.1")],
        );
        let output = update(&root, &forge.url, &["--rc"]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).contains(&format!("v4.2.1 -> {chosen}\n")),
            "{}",
            stdout(&output)
        );
    }
}

#[test]
fn release_options_obey_version_limits_and_keep_stable_eligible() {
    let forge = versions("v4.2.1", &["v4.2.2-rc.1", "v4.3.0-rc.1", "v5.0.0-rc.1"]);
    for (args, chosen) in [
        (&["--rc", "--patch"][..], "v4.2.2-rc.1"),
        (&["--pre", "--patch"][..], "v4.2.2-rc.1"),
        (&["--rc", "--minor"][..], "v4.3.0-rc.1"),
        (&["--pre", "--minor"][..], "v4.3.0-rc.1"),
        (&["--rc", "--major"][..], "v5.0.0-rc.1"),
        (&["--pre", "--major"][..], "v5.0.0-rc.1"),
    ] {
        let root = steps(
            "release-limits",
            &[&format!("owner/tool@{OLD} # v4.2.1 (track: v4)")],
        );
        let output = update(&root, &forge.url, args);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).contains(&format!("v4.2.1 -> {chosen}\n")),
            "{}",
            stdout(&output)
        );
    }
    let forge = versions("v4.2.1", &["v4.3.0-rc.10", "v4.3.0"]);
    for args in [&["--rc"][..], &["--pre"][..]] {
        let root = steps("stable-eligible", &[&format!("owner/tool@{OLD} # v4.2.1")]);
        let output = update(&root, &forge.url, args);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).contains("v4.2.1 -> v4.3.0\n"),
            "{}",
            stdout(&output)
        );
    }
}

#[test]
fn prerelease_write_keeps_track_comments_and_backup_and_repeats_cleanly() {
    let root = steps(
        "prerelease-write",
        &[&format!(
            "'owner/tool@{OLD}' # v4.2.1 (track: v4.2) # build"
        )],
    );
    let before = workflow(&root);
    let forge = versions("v4.2.1", &["v4.3.0-rc.1"]);
    let output = update(&root, &forge.url, &["--rc", "--write"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        workflow(&root),
        "jobs:\n  a:\n    steps:\n      - uses: 'owner/tool@0000000000000000000000000000000000000001' # v4.3.0-rc.1 (track: v4.3) # build\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".github/workflows/ci.yml.bkp")).unwrap(),
        before
    );
    let repeated = update(&root, &forge.url, &["--rc", "--write"]);
    assert_eq!(repeated.status.code(), Some(0), "{}", stderr(&repeated));
    assert_eq!(stdout(&repeated), "");
    assert!(!root.join(".github/workflows/ci.yml.bkp2").exists());
}

#[test]
fn metadata_only_changes_do_not_update_or_downgrade() {
    for (index, current) in ["v4.3.0+build.1", "v4.3.0-rc.10+build.1"]
        .into_iter()
        .enumerate()
    {
        let root = steps(
            &format!("metadata-{index}"),
            &[&format!("owner/tool@{OLD} # {current}")],
        );
        let candidates = if index == 0 {
            vec!["v4.3.0+build.2", "v4.3.0-rc.10", "v4.2.9"]
        } else {
            vec!["v4.3.0-rc.10+build.2", "v4.3.0-rc.2", "v4.2.9"]
        };
        let forge = versions(current, &candidates);
        let output = update(&root, &forge.url, &["--pre"]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(stdout(&output), "");
    }
}

#[test]
fn default_and_minor_select_highest_exact_version_within_major_and_keep_track() {
    for args in [&[][..], &["--minor"]] {
        let root = steps(
            "default",
            &[
                &format!("owner/tool@{OLD} # v4.2.1 (track: v4) # build"),
                "owner/tool@v4",
            ],
        );
        let before = workflow(&root);
        let forge = Forge::new(&tool());

        let output = update(&root, &forge.url, args);

        assert_eq!(stderr(&output), "");
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            stdout(&output),
            format!(
                concat!(
                    ".github/workflows/ci.yml:4:15: owner/tool@{old}: v4.2.1 -> v4.10.0\n",
                    "-      - uses: owner/tool@{old} # v4.2.1 (track: v4) # build\n",
                    "+      - uses: owner/tool@4444444444444444444444444444444444444444 # v4.10.0 (track: v4) # build\n",
                ),
                old = OLD
            ),
            "unpinned references are left to pin"
        );
        assert_eq!(workflow(&root), before, "preview must not write");
    }
}

#[test]
fn patch_stays_within_minor() {
    let root = steps(
        "patch",
        &[&format!("'owner/tool@{OLD}' # v4.2.1, 2026-11-10")],
    );
    let forge = Forge::new(&tool());

    let output = update(&root, &forge.url, &["--patch"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:4:15: owner/tool@{old}: v4.2.1 -> v4.2.3\n",
                "-      - uses: 'owner/tool@{old}' # v4.2.1, 2026-11-10\n",
                "+      - uses: 'owner/tool@2222222222222222222222222222222222222222' # v4.2.3, 2026-11-10\n",
            ),
            old = OLD
        )
    );
}

#[test]
fn major_upgrade_moves_the_track_to_the_new_major() {
    let root = steps(
        "major",
        &[&format!("owner/tool@{OLD} # v4.2.1 (track: v4)")],
    );
    let forge = Forge::new(&tool());

    let output = update(&root, &forge.url, &["--major"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:4:15: owner/tool@{old}: v4.2.1 -> v5.1.0\n",
                "-      - uses: owner/tool@{old} # v4.2.1 (track: v4)\n",
                "+      - uses: owner/tool@5555555555555555555555555555555555555555 # v5.1.0 (track: v5)\n",
            ),
            old = OLD
        )
    );
}

#[test]
fn highest_version_is_up_to_date_and_never_downgraded() {
    let root = steps(
        "latest",
        &["owner/tool@5555555555555555555555555555555555555555 # v5.1.0"],
    );
    let forge = Forge::new(&tool());

    let output = update(&root, &forge.url, &["--major"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
}

#[test]
fn conflicting_limits_are_rejected_before_any_request() {
    let root = steps("conflict", &[&format!("owner/tool@{OLD} # v4.2.1")]);
    let forge = Forge::new(&tool());

    for args in [
        ["--major", "--patch"],
        ["--minor", "--patch"],
        ["--major", "--minor"],
        ["--pre", "--rc"],
    ] {
        let output = update(&root, &forge.url, &args);

        assert_eq!(output.status.code(), Some(2));
        assert!(
            stderr(&output).contains("cannot be used with"),
            "{}",
            stderr(&output)
        );
    }
    assert_eq!(forge.total(), 0);
}

#[test]
fn moved_exact_tag_needs_review_and_does_not_block_other_references() {
    let root = steps(
        "moved",
        &[
            &format!("owner/tool@{OLD} # v4.2.3"),
            &format!("owner/tool@{OLD} # v4.2.1"),
        ],
    );
    let forge = Forge::new(&tool());

    let output = update(&root, &forge.url, &["--patch"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        format!(
            "refpinner: .github/workflows/ci.yml:4:15: owner/tool@{OLD}: tag v4.2.3 moved to 2222222222222222222222222222222222222222; review it before updating\n"
        )
    );
    assert!(
        stdout(&output).starts_with(&format!(
            ".github/workflows/ci.yml:5:15: owner/tool@{OLD}: v4.2.1 -> v4.2.3\n"
        )),
        "{}",
        stdout(&output)
    );
}

#[test]
fn branch_movement_is_an_update_that_keeps_the_branch() {
    let root = steps(
        "branch",
        &[
            &format!("owner/tool@{OLD} # main"),
            &format!("owner/tool@{OLD} # main"),
        ],
    );
    let forge = Forge::new(&[(
        "/repos/owner/tool/git/ref/heads/main",
        200,
        r#"{"ref":"refs/heads/main","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}}"#,
    )]);

    let output = update(&root, &forge.url, &["--patch"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:4:15: owner/tool@{old}: main moved\n",
                "-      - uses: owner/tool@{old} # main\n",
                "+      - uses: owner/tool@4444444444444444444444444444444444444444 # main\n",
                ".github/workflows/ci.yml:5:15: owner/tool@{old}: main moved\n",
                "-      - uses: owner/tool@{old} # main\n",
                "+      - uses: owner/tool@4444444444444444444444444444444444444444 # main\n",
            ),
            old = OLD
        )
    );
    assert_eq!(forge.count("/repos/owner/tool/git/ref/heads/main"), 1);
}

#[test]
fn unannotated_pin_follows_the_default_branch_only_with_explicit_option() {
    let root = steps("unannotated", &[&format!("owner/tool@{OLD} # build step")]);
    let forge = Forge::new(&[
        ("/repos/owner/tool", 200, r#"{"default_branch":"trunk"}"#),
        (
            "/repos/owner/tool/git/ref/heads/trunk",
            200,
            r#"{"ref":"refs/heads/trunk","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}}"#,
        ),
    ]);

    let unattended = update(&root, &forge.url, &[]);

    assert_eq!(unattended.status.code(), Some(2));
    assert_eq!(stdout(&unattended), "");
    assert_eq!(
        stderr(&unattended),
        format!(
            "refpinner: .github/workflows/ci.yml:4:15: owner/tool@{OLD}: pin has no annotation; pass --follow-default-branch to follow the default branch\n"
        )
    );
    assert_eq!(forge.total(), 0);

    let approved = update(&root, &forge.url, &["--follow-default-branch"]);

    assert_eq!(stderr(&approved), "");
    assert_eq!(approved.status.code(), Some(0));
    assert_eq!(
        stdout(&approved),
        format!(
            concat!(
                ".github/workflows/ci.yml:4:15: owner/tool@{old}: default branch trunk\n",
                "-      - uses: owner/tool@{old} # build step\n",
                "+      - uses: owner/tool@4444444444444444444444444444444444444444 # trunk # build step\n",
            ),
            old = OLD
        )
    );
}

#[test]
fn floating_track_without_exact_version_resolves_it_or_reports_it() {
    let mut routes = tool();
    routes.push((
        "/repos/owner/tool/git/matching-refs/tags/v4.",
        200,
        r#"[{"ref":"refs/tags/v4.2.1","object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}}]"#,
    ));
    let forge = Forge::new(&routes);
    let resolved = steps("precision", &[&format!("owner/tool@{OLD} # v4")]);

    let output = update(&resolved, &forge.url, &["--patch"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:4:15: owner/tool@{old}: v4.2.1 -> v4.2.3\n",
                "-      - uses: owner/tool@{old} # v4\n",
                "+      - uses: owner/tool@2222222222222222222222222222222222222222 # v4.2.3 (track: v4)\n",
            ),
            old = OLD
        )
    );

    let unknown = "0000000000000000000000000000000000000000";
    let ambiguous = steps("ambiguous", &[&format!("owner/tool@{unknown} # v4")]);

    let output = update(&ambiguous, &forge.url, &["--patch"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "refpinner: .github/workflows/ci.yml:4:15: owner/tool@{unknown}: cannot find the exact version of {unknown} on track v4\n"
        )
    );
}

#[test]
fn floating_track_movement_is_an_update_when_no_exact_version_matches() {
    let unknown = "0000000000000000000000000000000000000000";
    let root = steps("track-moved", &[&format!("owner/tool@{unknown} # v4")]);
    let mut routes = tool();
    routes.push((
        "/repos/owner/tool/git/ref/tags/v4",
        200,
        r#"{"ref":"refs/tags/v4","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}}"#,
    ));
    let forge = Forge::new(&routes);

    let output = update(&root, &forge.url, &[]);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:4:15: owner/tool@{old}: track v4 moved\n",
                "-      - uses: owner/tool@{old} # v4\n",
                "+      - uses: owner/tool@4444444444444444444444444444444444444444 # v4\n",
            ),
            old = unknown
        )
    );
}

#[test]
fn offline_update_fails_without_network_access() {
    let root = steps("offline", &[&format!("owner/tool@{OLD} # v4.2.1")]);
    let forge = Forge::new(&tool());

    let output = update(&root, &forge.url, &["--offline"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        "refpinner: update needs network access and cannot run offline\n"
    );
    assert_eq!(forge.total(), 0);

    // Port 9 (discard) on loopback is closed on test machines, so the connection is refused.
    let unreachable = update(&root, "http://127.0.0.1:9", &[]);

    assert_eq!(unreachable.status.code(), Some(2));
    assert!(
        stderr(&unreachable).contains("cannot reach GitHub API"),
        "{}",
        stderr(&unreachable)
    );
}

#[test]
fn unattended_write_applies_updates_with_backup_and_skips_unapproved_default_branch() {
    let root = steps(
        "write",
        &[
            &format!("owner/tool@{OLD} # v4.2.1 (track: v4)"),
            &format!("owner/tool@{OLD} # build step"),
        ],
    );
    let before = workflow(&root);
    let forge = Forge::new(&tool());

    let output = update(&root, &forge.url, &["--write"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        format!(
            "refpinner: .github/workflows/ci.yml:5:15: owner/tool@{OLD}: pin has no annotation; pass --follow-default-branch to follow the default branch\n"
        )
    );
    assert!(
        stdout(&output)
            .ends_with("wrote .github/workflows/ci.yml (backup .github/workflows/ci.yml.bkp)\n"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        workflow(&root),
        format!(
            concat!(
                "jobs:\n",
                "  a:\n",
                "    steps:\n",
                "      - uses: owner/tool@4444444444444444444444444444444444444444 # v4.10.0 (track: v4)\n",
                "      - uses: owner/tool@{old} # build step\n",
            ),
            old = OLD
        )
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".github/workflows/ci.yml.bkp")).expect("read backup"),
        before
    );
}
