mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Builds a fresh folder under the Cargo test tmpdir with files at the given relative paths.
fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("config")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create tree");
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        std::fs::write(path, body).expect("write file");
    }
    root
}

/// Runs refpinner in `dir` with `REFPINNER_*` variables cleared, then `env` applied.
fn refpinner(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(args)
        .current_dir(dir)
        .env_remove("REFPINNER_CONFIG")
        .env_remove("REFPINNER_NO_SYMLINK_WARNINGS")
        // A closed local port keeps tests away from real forges.
        .env("REFPINNER_GITHUB_API_URL", "http://127.0.0.1:9");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run refpinner")
}

fn lint(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["lint"];
    all.extend(args);
    refpinner(dir, &[], &all)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A workflow with one unpinned reference at line 4, column 15.
const UNPINNED: &str = "jobs:\n  a:\n    steps:\n      - uses: owner/repo@v1\n";

/// The lint line for [`UNPINNED`] at `path`.
fn unpinned(path: &str) -> String {
    format!("{path}:4:15: unpinned owner/repo@v1\n")
}

/// Workflows `a.yml` to `e.yml`, one per configuration name in search order.
const WORKFLOWS: [&str; 5] = ["a", "b", "c", "d", "e"];

#[test]
fn configured_custom_forgejo_file_uses_instance_source() {
    let forge = common::Forge::new(&[(
        "/api/v1/repos/team/repo/git/refs/heads/main",
        200,
        r#"[{"ref":"refs/heads/main","object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}]"#,
    )]);
    let config = format!(
        "files = [{{ path = 'custom.yml', platform = 'forgejo' }}]\nforgejo_action_source = '{}'\n",
        forge.url
    );
    let root = tree(
        "custom-forgejo",
        &[
            ("refpinner.toml", &config),
            (
                "custom.yml",
                "jobs:\n  build:\n    steps:\n      - uses: team/repo@main\n",
            ),
        ],
    );
    let output = refpinner(&root, &[], &["pin"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("team/repo@1111111111111111111111111111111111111111 # main"));
    assert_eq!(
        forge.count("/api/v1/repos/team/repo/git/refs/heads/main"),
        1
    );
}

#[test]
fn configured_custom_gitlab_file_uses_gitlab_parser() {
    let root = tree(
        "custom-gitlab",
        &[
            (
                "refpinner.toml",
                "files = [{ path = 'custom.yml', platform = 'gitlab' }]\n",
            ),
            (
                "custom.yml",
                "include:\n  - project: group/project\n    ref: main\n    file: /ci.yml\nimage: node:18\n",
            ),
        ],
    );
    let output = lint(&root, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("unpinned group/project@main"), "{text}");
    assert!(text.contains("unpinned node:18"), "{text}");
}

#[test]
fn search_selects_the_first_existing_configuration_file() {
    let names = [
        "refpinner.toml",
        ".refpinner.toml",
        ".config/refpinner.toml",
        ".config/.refpinner.toml",
        ".config/refpinner/config.toml",
    ];
    for first in 0..names.len() {
        let mut files: Vec<(String, String)> = WORKFLOWS
            .iter()
            .map(|w| (format!(".github/workflows/{w}.yml"), UNPINNED.to_owned()))
            .collect();
        // Each file excludes a different workflow, so the missing line names the selected file.
        for (name, workflow) in names.iter().zip(WORKFLOWS).skip(first) {
            files.push((
                (*name).to_owned(),
                format!("exclude = [\"{workflow}.yml\"]\n"),
            ));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, b)| (p.as_str(), b.as_str()))
            .collect();
        let root = tree(&format!("search-{first}"), &borrowed);

        let output = lint(&root, &[]);

        let expected: String = WORKFLOWS
            .iter()
            .filter(|w| **w != WORKFLOWS[first])
            .map(|w| unpinned(&format!(".github/workflows/{w}.yml")))
            .collect();
        assert_eq!(stdout(&output), expected, "selected {}", names[first]);
    }
}

#[test]
fn config_flag_overrides_environment_which_overrides_search() {
    let root = tree(
        "explicit",
        &[
            (".github/workflows/a.yml", UNPINNED),
            (".github/workflows/b.yml", UNPINNED),
            (".github/workflows/c.yml", UNPINNED),
            ("refpinner.toml", "exclude = [\"a.yml\"]\n"),
            ("env.toml", "exclude = [\"b.yml\"]\n"),
            ("flag.toml", "exclude = [\"c.yml\"]\n"),
        ],
    );
    let env = [("REFPINNER_CONFIG", "env.toml")];
    let without = |skipped: &str| -> String {
        ["a", "b", "c"]
            .iter()
            .filter(|w| **w != skipped)
            .map(|w| unpinned(&format!(".github/workflows/{w}.yml")))
            .collect()
    };

    let searched = refpinner(&root, &[], &["lint"]);
    let from_env = refpinner(&root, &env, &["lint"]);
    let from_flag = refpinner(&root, &env, &["lint", "--config", "flag.toml"]);

    assert_eq!(stdout(&searched), without("a"));
    assert_eq!(stdout(&from_env), without("b"));
    assert_eq!(stdout(&from_flag), without("c"));
}

#[test]
fn exclude_flag_replaces_configured_exclusions() {
    let root = tree(
        "exclude-flag",
        &[
            (".github/workflows/a.yml", UNPINNED),
            (".github/workflows/b.yml", UNPINNED),
            ("refpinner.toml", "exclude = [\"a.yml\"]\n"),
        ],
    );

    let output = lint(&root, &["--exclude", "b.yml"]);

    assert_eq!(stdout(&output), unpinned(".github/workflows/a.yml"));
}

#[test]
fn extends_merges_parents_in_order_then_the_child_and_lists_replace() {
    let root = tree(
        "extends",
        &[
            (".github/workflows/a.yml", UNPINNED),
            (".github/workflows/b.yml", UNPINNED),
            (".github/workflows/c.yml", UNPINNED),
            ("ci/c.yml", UNPINNED),
            // Paths in `extends` are relative to the file that declares them.
            (
                ".config/refpinner.toml",
                "extends = [\"../shared/first.toml\", \"../shared/second.toml\"]\n",
            ),
            (
                "shared/first.toml",
                concat!(
                    "extends = [\"base/files.toml\"]\n",
                    "exclude = [\"a.yml\", \"b.yml\"]\n",
                ),
            ),
            (
                "shared/base/files.toml",
                "files = [{ path = \"../../ci/c.yml\", platform = \"github\" }]\n",
            ),
            // The later parent wins, and its list replaces the earlier list.
            ("shared/second.toml", "exclude = [\"c.yml\"]\n"),
        ],
    );

    let output = lint(&root, &[]);

    assert_eq!(
        stdout(&output),
        format!(
            "{}{}{}",
            unpinned(".github/workflows/a.yml"),
            unpinned(".github/workflows/b.yml"),
            unpinned("ci/c.yml"),
        )
    );

    std::fs::write(
        root.join(".config/refpinner.toml"),
        concat!(
            "extends = [\"../shared/first.toml\", \"../shared/second.toml\"]\n",
            "exclude = []\n",
        ),
    )
    .expect("write child");
    let child = lint(&root, &[]);
    assert_eq!(
        stdout(&child),
        format!(
            "{}{}{}{}",
            unpinned(".github/workflows/a.yml"),
            unpinned(".github/workflows/b.yml"),
            unpinned(".github/workflows/c.yml"),
            unpinned("ci/c.yml"),
        )
    );
}

#[test]
fn validation_is_offline_and_rejects_invalid_inheritance_before_writes() {
    for (name, config, message) in [
        ("cycle", "extends = [\"refpinner.toml\"]\n", "cycle"),
        (
            "missing",
            "extends = [\"missing.toml\"]\n",
            "cannot read config",
        ),
        ("glob", "exclude = [\"[\"]\n", "pattern"),
        (
            "regex",
            "version_rules = [{ pattern = \"*\", regex = \"[\" }]\n",
            "pattern",
        ),
    ] {
        let root = tree(
            name,
            &[
                ("refpinner.toml", config),
                (".github/workflows/ci.yml", UNPINNED),
            ],
        );
        let validate = refpinner(&root, &[], &["config", "validate"]);
        assert_eq!(
            validate.status.code(),
            Some(2),
            "{name}: {}",
            stderr(&validate)
        );
        assert!(
            stderr(&validate).contains(message),
            "{name}: {}",
            stderr(&validate)
        );
        let write = refpinner(&root, &[], &["pin", "--write"]);
        assert_eq!(write.status.code(), Some(2));
        assert!(stderr(&write).contains(message), "{}", stderr(&write));
        assert_eq!(
            std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap(),
            UNPINNED
        );
        assert!(!root.join(".github/workflows/ci.yml.bkp").exists());
    }
    let root = tree(
        "valid-offline",
        &[("refpinner.toml", "exclude = [\"*.yml\"]\n")],
    );
    let output = refpinner(&root, &[], &["config", "validate"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn reference_rules_filter_lint_and_suppress_warnings_last_match_wins() {
    let source = concat!(
        "jobs:\n  a:\n    steps:\n",
        "      - uses: owner/repo@v1\n",
        "      - uses: owner/other@v1\n",
        "      - uses: ${{ matrix.action }}\n"
    );
    let root = tree(
        "reference-rules",
        &[
            (".github/workflows/ci.yml", source),
            (
                "refpinner.toml",
                concat!(
                    "suppress = [\"unsupported dynamic reference *\"]\n",
                    "references = [{ pattern = \"owner/*\", exclude = true }, { pattern = \"owner/repo\", exclude = false }]\n",
                ),
            ),
        ],
    );
    let output = lint(&root, &[]);
    assert_eq!(stdout(&output), unpinned(".github/workflows/ci.yml"));
    let audit = lint(&root, &["--include-excluded"]);
    assert!(stdout(&audit).contains("owner/other@v1"));
}

#[test]
fn platform_and_warning_options_obey_cli_environment_config_precedence() {
    let root = tree(
        "platform-precedence",
        &[
            (".github/workflows/ci.yml", UNPINNED),
            ("custom.yml", UNPINNED),
            (
                "refpinner.toml",
                "platforms = []\nfiles = [{ path = \"custom.yml\", platform = \"github\" }]\n",
            ),
        ],
    );
    let disabled = lint(&root, &[]);
    assert_eq!(disabled.status.code(), Some(2));
    let env = [("REFPINNER_PLATFORM", "github")];
    let enabled = refpinner(&root, &env, &["lint"]);
    assert_eq!(
        stdout(&enabled),
        format!(
            "{}{}",
            unpinned(".github/workflows/ci.yml"),
            unpinned("custom.yml")
        )
    );
    let explicit = refpinner(&root, &env, &["lint", "--platform", "gitlab"]);
    assert_eq!(explicit.status.code(), Some(2));
}

#[test]
fn explicit_cli_limits_override_environment_then_reference_then_global_limits() {
    let commit = "1111111111111111111111111111111111111111";
    let body = format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{commit} # v1.2.0\n");
    let root = tree(
        "update-limits",
        &[
            (".github/workflows/ci.yml", &body),
            (
                "refpinner.toml",
                "update_limit = \"major\"\nreferences = [{ pattern = \"owner/*\", update_limit = \"minor\" }, { pattern = \"owner/tool\", update_limit = \"patch\" }]\n",
            ),
        ],
    );
    let response =
        r#"{"object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}}"#;
    let forge = common::Forge::new(&[
        ("/repos/owner/tool/git/ref/tags/v1.2.0", 200, response),
        ("/repos/owner/tool/git/ref/tags/v1.2.1", 200, response),
        ("/repos/owner/tool/git/ref/tags/v1.3.0", 200, response),
        ("/repos/owner/tool/git/ref/tags/v2.0.0", 200, response),
        (
            "/repos/owner/tool/git/matching-refs/tags/v",
            200,
            r#"[{"ref":"refs/tags/v1.2.1"},{"ref":"refs/tags/v1.3.0"},{"ref":"refs/tags/v2.0.0"}]"#,
        ),
    ]);
    let configured = refpinner(
        &root,
        &[("REFPINNER_GITHUB_API_URL", &forge.url)],
        &["update"],
    );
    assert_eq!(forge.count("/repos/owner/tool/git/ref/tags/v1.2.0"), 1);
    assert!(forge.total() >= 2);
    assert_eq!(configured.status.code(), Some(0), "{}", stderr(&configured));
    assert!(
        stdout(&configured).contains("v1.2.0 -> v1.2.1"),
        "{}",
        stdout(&configured)
    );
    let env = [
        ("REFPINNER_GITHUB_API_URL", forge.url.as_str()),
        ("REFPINNER_UPDATE_LIMIT", "minor"),
    ];
    let environment = refpinner(&root, &env, &["update"]);
    assert!(
        stdout(&environment).contains("v1.2.0 -> v1.3.0"),
        "{}",
        stderr(&environment)
    );
    let explicit = refpinner(&root, &env, &["update", "--major"]);
    assert!(
        stdout(&explicit).contains("v1.2.0 -> v2.0.0"),
        "{}",
        stderr(&explicit)
    );
    let explicit_minor = refpinner(
        &root,
        &[],
        &["update", "--minor", "--config", "missing.toml"],
    );
    assert_eq!(explicit_minor.status.code(), Some(2));
    assert!(stderr(&explicit_minor).contains("cannot read config"));
}

#[test]
fn nested_projects_use_their_own_configuration_and_do_not_inherit() {
    let root = tree(
        "nested",
        &[
            ("refpinner.toml", "exclude = [\"a.yml\"]\n"),
            (".github/workflows/a.yml", UNPINNED),
            ("plain/.github/workflows/a.yml", UNPINNED),
            ("own/.github/workflows/a.yml", UNPINNED),
            ("own/.github/workflows/b.yml", UNPINNED),
            ("own/.config/refpinner.toml", "exclude = [\"b.yml\"]\n"),
        ],
    );

    let output = lint(&root, &[]);

    assert_eq!(
        stdout(&output),
        format!(
            "{}{}",
            unpinned("own/.github/workflows/a.yml"),
            unpinned("plain/.github/workflows/a.yml"),
        )
    );
}
