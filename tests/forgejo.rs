mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Forge;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("forgejo")
        .join(format!("{name}-{}-{unique}", std::process::id()));
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create project");
        std::fs::write(path, body).expect("write file");
    }
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command.args(args).current_dir(root);
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "REFPINNER_FORGEJO_INSTANCE",
        "REFPINNER_FORGEJO_ACTION_SOURCE",
    ] {
        command.env_remove(name);
    }
    command.output().expect("run refpinner")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn branches() -> Vec<(&'static str, u16, &'static str)> {
    vec![(
        "/api/v1/repos/team/repo/git/refs/heads/main",
        200,
        concat!(
            r#"[{"ref":"refs/heads/main-other","object":{"type":"commit","sha":"2222222222222222222222222222222222222222"}},"#,
            r#"{"ref":"refs/heads/main","object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}]"#,
        ),
    )]
}

#[test]
fn pins_distinct_action_instance_and_absolute_hosts_without_rewriting_the_source() {
    let actions = Forge::new(&branches());
    let instance = Forge::new(&branches());
    let absolute = Forge::new(&branches());
    let source = format!(
        "jobs:\n  reuse:\n    uses: team/repo/.forgejo/workflows/reuse.yml@main\n  build:\n    steps:\n      - uses: team/repo/subpath@main\n      - uses: '{}/team/repo@main' # retain\n",
        absolute.url,
    );
    let composite = "runs:\n  using: composite\n  steps:\n    - uses: team/repo@main\n";
    let root = project(
        "hosts",
        &[
            (".forgejo/workflows/ci.yml", &source),
            (".forgejo/actions/tool/action.yml", composite),
        ],
    );
    let output = run(
        &root,
        &[
            "--forgejo-instance",
            &instance.url,
            "--forgejo-action-source",
            &actions.url,
            "pin",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("team/repo/subpath@1111111111111111111111111111111111111111 # main"),
        "{text}"
    );
    assert!(
        text.contains(
            "team/repo/.forgejo/workflows/reuse.yml@1111111111111111111111111111111111111111 # main"
        ),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "'{}/team/repo@1111111111111111111111111111111111111111' # main # retain",
            absolute.url
        )),
        "{text}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".forgejo/workflows/ci.yml")).expect("workflow"),
        source
    );
    for forge in [&actions, &instance, &absolute] {
        assert_eq!(
            forge.count("/api/v1/repos/team/repo/git/refs/heads/main"),
            1
        );
        assert_eq!(forge.total(), 2);
    }
}

fn versions(new_sha: &str) -> Vec<(String, u16, String)> {
    vec![
        ("/api/v1/repos/team/repo/git/refs/tags/v1".into(), 200,
            r#"[{"ref":"refs/tags/v1.0.0","object":{"type":"commit","sha":"2222222222222222222222222222222222222222"}},{"ref":"refs/tags/v1","object":{"type":"tag","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}]"#.into()),
        ("/api/v1/repos/team/repo/git/tags/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(), 200,
            r#"{"object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}"#.into()),
        ("/api/v1/repos/team/repo/git/refs/tags/v1.".into(), 200,
            r#"[{"ref":"refs/tags/v1.0.0","object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}]"#.into()),
        ("/api/v1/repos/team/repo/git/refs/tags/v1.0.0".into(), 200,
            r#"[{"ref":"refs/tags/v1.0.0","object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}]"#.into()),
        ("/api/v1/repos/team/repo/git/refs/tags/v".into(), 200,
            r#"[{"ref":"refs/tags/v1.0.0"},{"ref":"refs/tags/v1.1.0"},{"ref":"refs/tags/v2.0.0"}]"#.into()),
        ("/api/v1/repos/team/repo/git/refs/tags/v1.1.0".into(), 200,
            format!(r#"[{{"ref":"refs/tags/v1.1.0","object":{{"type":"commit","sha":"{new_sha}"}}}}]"#)),
    ]
}

#[test]
fn writes_pins_and_updates_using_shared_annotations_backups_and_version_limits() {
    let routes = versions("2222222222222222222222222222222222222222");
    let routes: Vec<_> = routes
        .iter()
        .map(|(path, status, body)| (path.as_str(), *status, body.as_str()))
        .collect();
    let forge = Forge::new(&routes);
    let source = "jobs:\r\n  build:\r\n    steps:\r\n      - uses: 'team/repo/sub@v1' # retain\r\n";
    let root = project("write", &[(".forgejo/workflows/ci.yml", source)]);
    let file = root.join(".forgejo/workflows/ci.yml");
    let output = run(
        &root,
        &["pin", "--write", "--forgejo-action-source", &forge.url],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let pinned = source.replace(
        "@v1'",
        "@1111111111111111111111111111111111111111' # v1.0.0 (track: v1)",
    );
    assert_eq!(std::fs::read_to_string(&file).expect("pin"), pinned);
    assert_eq!(
        std::fs::read_to_string(file.with_extension("yml.bkp")).expect("backup"),
        source
    );

    let output = run(&root, &["update", "--forgejo-action-source", &forge.url]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("v1.0.0 -> v1.1.0"),
        "{}",
        stdout(&output)
    );
    assert_eq!(std::fs::read_to_string(&file).expect("preview"), pinned);
    let output = run(
        &root,
        &["update", "--write", "--forgejo-action-source", &forge.url],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(&file).expect("update"),
        pinned
            .replace(
                "1111111111111111111111111111111111111111",
                "2222222222222222222222222222222222222222"
            )
            .replace("v1.0.0", "v1.1.0")
    );
    assert_eq!(
        std::fs::read_to_string(file.with_extension("yml.bkp2")).expect("backup"),
        pinned
    );
}

#[test]
fn gitea_defaults_to_github_and_forgejo_can_explicitly_use_github() {
    let forge = Forge::new(&[(
        "/repos/team/repo/git/ref/heads/main",
        200,
        r#"{"object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}"#,
    )]);
    let workflow = "jobs:\n  build:\n    steps:\n      - uses: team/repo@main\n";
    for (name, folder, extra) in [
        ("gitea", ".gitea", Vec::new()),
        (
            "github-source",
            ".forgejo",
            vec!["--forgejo-action-source", "github"],
        ),
    ] {
        let file = format!("{folder}/workflows/ci.yml");
        let root = project(name, &[(&file, workflow)]);
        let output = Command::new(env!("CARGO_BIN_EXE_refpinner"))
            .arg("pin")
            .args(extra)
            .current_dir(&root)
            .env("REFPINNER_GITHUB_API_URL", &forge.url)
            .env_remove("REFPINNER_FORGEJO_ACTION_SOURCE")
            .env("NO_PROXY", "127.0.0.1")
            .output()
            .expect("run");
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(
            stdout(&output).contains("team/repo@1111111111111111111111111111111111111111 # main")
        );
    }
    assert_eq!(forge.count("/repos/team/repo/git/ref/heads/main"), 2);
}

#[test]
fn environment_self_source_and_root_composite_use_configured_instance() {
    let forge = Forge::new(&branches());
    let root = project(
        "self",
        &[
            (".forgejo/workflows/ci.yml", "jobs: {}\n"),
            (
                "action.yml",
                "runs:\n  using: composite\n  steps:\n    - uses: team/repo@main\n",
            ),
        ],
    );
    let output = Command::new(env!("CARGO_BIN_EXE_refpinner"))
        .args(["pin", "--forgejo-action-source", "self"])
        .current_dir(&root)
        .env("REFPINNER_FORGEJO_INSTANCE", &forge.url)
        .env("NO_PROXY", "127.0.0.1")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("action.yml:4:13:"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        forge.count("/api/v1/repos/team/repo/git/refs/heads/main"),
        1
    );
}

#[test]
fn unsupported_and_invalid_references_never_make_requests_or_block_safe_writes() {
    let forge = Forge::new(&branches());
    let source = concat!(
        "jobs:\n  build:\n    steps:\n",
        "      - uses: team/repo@main\n",
        "      - uses: https://invalid.example/team/repo@main\n",
        "      - uses: ${{ inputs.action }}\n",
        "      - uses: ./local\n",
    );
    let root = project("partial", &[(".forgejo/workflows/ci.yml", source)]);
    // A local response failure checks partial writes without depending on DNS or real hosts.
    let source = source.replace(
        "https://invalid.example/team/repo",
        &format!("{}/missing/repo", forge.url),
    );
    std::fs::write(root.join(".forgejo/workflows/ci.yml"), &source).expect("workflow");
    let output = run(
        &root,
        &["pin", "--write", "--forgejo-action-source", &forge.url],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("no tag or branch named main in missing/repo"));
    let written =
        std::fs::read_to_string(root.join(".forgejo/workflows/ci.yml")).expect("workflow");
    assert!(written.contains("team/repo@1111111111111111111111111111111111111111 # main"));
    assert!(written.contains("${{ inputs.action }}") && written.contains("./local"));
    let requests = forge.total();
    let root = project(
        "invalid",
        &[(
            ".forgejo/workflows/ci.yml",
            concat!(
                "jobs:\n  build:\n    steps:\n",
                "      - uses: team/repo@../bad\n",
                "      - uses: https://bad.example/team/repo@main\n",
                "      - uses: ftp://127.0.0.1/team/repo@main\n",
                "      - uses: team/repo@main\n",
            ),
        )],
    );
    let output = run(&root, &["pin", "--forgejo-action-source", "self"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Forgejo instance is unknown"));
    assert_eq!(forge.total(), requests);
    let source = "jobs:\n  build:\n    steps:\n      - uses: team/repo@../bad\n";
    let root = project("invalid-revision", &[(".forgejo/workflows/ci.yml", source)]);
    let output = run(&root, &["pin", "--forgejo-action-source", &forge.url]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("not a valid Git ref name"));
    assert_eq!(forge.total(), requests);
}

#[test]
fn discovers_forge_workflows_and_composites_preserving_local_and_warning_semantics() {
    let root = project(
        "lint",
        &[
            (
                ".forgejo/workflows/nested/ci.yml",
                concat!(
                    "jobs:\n",
                    "  reuse:\n",
                    "    uses: team/repo/.forgejo/workflows/reuse.yml@main\n",
                    "  build:\n",
                    "    steps:\n",
                    "      - uses: ./local\n",
                    "      - uses: $/local\n",
                    "      - uses: ${{ inputs.action }}\n",
                    "      - uses: https://codeberg.org/team/repo/path@main\n",
                ),
            ),
            (
                ".forgejo/actions/tool/action.yml",
                "runs:\n  using: composite\n  steps:\n    - uses: team/repo@main\n",
            ),
            (
                ".gitea/workflows/ci.yaml",
                "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@main\n",
            ),
        ],
    );
    let output = run(&root, &["lint"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains(".forgejo/workflows/nested/ci.yml:3:11: unpinned team/repo/"),
        "{text}"
    );
    assert!(
        text.contains(".forgejo/actions/tool/action.yml:4:13: unpinned team/repo@main"),
        "{text}"
    );
    assert!(
        text.contains(".gitea/workflows/ci.yaml:4:15: unpinned actions/checkout@main"),
        "{text}"
    );
    assert!(
        text.contains("warning unsupported dynamic reference"),
        "{text}"
    );
    assert!(
        text.contains("unpinned https://codeberg.org/team/repo/path@main"),
        "{text}"
    );
    assert!(
        !text.contains("./local") && !text.contains("$/local"),
        "{text}"
    );
}

#[test]
fn mixed_forgejo_action_and_image_are_written_together() {
    let source =
        "jobs:\n  build:\n    container: node:18\n    steps:\n      - uses: team/repo@main\n";
    let root = project("mixed-image", &[(".forgejo/workflows/ci.yml", source)]);
    let action = Forge::new(&branches());
    let registry = Forge::new(&[("/v2/library/node/manifests/18", 200, "{}")]);
    let output = Command::new(env!("CARGO_BIN_EXE_refpinner"))
        .current_dir(&root)
        .args(["pin", "--write", "--forgejo-action-source", &action.url])
        .env("REFPINNER_REGISTRY_URL", &registry.url)
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .output()
        .expect("run refpinner");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let result =
        std::fs::read_to_string(root.join(".forgejo/workflows/ci.yml")).expect("read workflow");
    assert!(
        result.contains("team/repo@1111111111111111111111111111111111111111 # main"),
        "{result}"
    );
    assert!(result.contains("node:18@sha256:"), "{result}");
    assert_eq!(
        std::fs::read_to_string(root.join(".forgejo/workflows/ci.yml.bkp")).expect("backup"),
        source
    );
    assert_eq!(
        action.count("/api/v1/repos/team/repo/git/refs/heads/main"),
        1
    );
    assert_eq!(registry.count("/v2/library/node/manifests/18"), 2);
}
