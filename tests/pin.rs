use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

/// A local stand-in for the GitHub REST API that records each request path.
struct Forge {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Forge {
    /// Serves `(path, status, body)` routes; unknown paths get 404.
    fn new(routes: &[(&str, u16, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock forge");
        let url = format!("http://{}", listener.local_addr().expect("mock address"));
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(path, status, body)| ((*path).to_owned(), *status, (*body).to_owned()))
            .collect();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let path = request_line.split(' ').nth(1).unwrap_or("").to_owned();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                }
                log.lock().expect("request log").push(path.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(route, ..)| *route == path)
                    .map_or((404, r#"{"message":"Not Found"}"#), |(_, status, body)| {
                        (*status, body.as_str())
                    });
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { url, requests }
    }

    fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .expect("request log")
            .iter()
            .filter(|request| *request == path)
            .count()
    }

    fn total(&self) -> usize {
        self.requests.lock().expect("request log").len()
    }
}

/// Routes for `actions/setup-node`: tag `v4` and annotated tag `v4.2.1` share commit `1111…`.
fn setup_node() -> Vec<(&'static str, u16, &'static str)> {
    vec![
        (
            "/repos/actions/setup-node/git/ref/tags/v4",
            200,
            r#"{"ref":"refs/tags/v4","object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}}"#,
        ),
        (
            "/repos/actions/setup-node/git/matching-refs/tags/v4.",
            200,
            concat!(
                r#"[{"ref":"refs/tags/v4.0.0","object":{"sha":"2222222222222222222222222222222222222222","type":"commit"}},"#,
                r#"{"ref":"refs/tags/v4.2.1","object":{"sha":"3333333333333333333333333333333333333333","type":"tag"}}]"#,
            ),
        ),
        (
            "/repos/actions/setup-node/git/tags/3333333333333333333333333333333333333333",
            200,
            r#"{"sha":"3333333333333333333333333333333333333333","object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}}"#,
        ),
    ]
}

/// Builds a fresh project under the Cargo test tmpdir; paths are relative to the project root.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("pin")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create project");
        std::fs::write(path, body).expect("write file");
    }
    root
}

fn pin(dir: &Path, forge: &Forge) -> Output {
    refpinner(dir, forge, &["pin"])
}

fn refpinner(dir: &Path, forge: &Forge, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(args)
        .current_dir(dir)
        .env("REFPINNER_GITHUB_API_URL", &forge.url);
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

#[test]
fn preview_pins_floating_tag_with_exact_version_and_leaves_file_unchanged() {
    let workflow = concat!(
        "jobs:\n",
        "  build:\n",
        "    steps:\n",
        "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n",
        "      - uses: actions/setup-node@v4 # install node\n",
        "      - uses: ./.github/actions/local\n",
    );
    let root = project("tag", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&setup_node());

    let output = pin(&root, &forge);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:5:15: actions/setup-node@v4\n",
            "-      - uses: actions/setup-node@v4 # install node\n",
            "+      - uses: actions/setup-node@1111111111111111111111111111111111111111 # v4.2.1 (track: v4) # install node\n",
        )
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read workflow"),
        workflow
    );
    assert!(
        forge
            .requests
            .lock()
            .expect("request log")
            .iter()
            .all(|path| path.starts_with("/repos/actions/setup-node/")),
        "existing pins must not be resolved"
    );
}

#[test]
fn repeated_reference_resolves_once_and_reports_each_location() {
    let workflow = concat!(
        "jobs:\n",
        "  a:\n",
        "    steps:\n",
        "      - uses: actions/setup-node@v4\n",
        "  b:\n",
        "    steps:\n",
        "      - uses: actions/setup-node@v4\n",
    );
    let root = project(
        "repeated",
        &[
            (".github/workflows/a.yml", workflow),
            (".github/workflows/b.yml", workflow),
        ],
    );
    let forge = Forge::new(&setup_node());

    let output = pin(&root, &forge);

    assert_eq!(output.status.code(), Some(0));
    let out = stdout(&output);
    let locations: Vec<&str> = out
        .lines()
        .filter(|line| !line.starts_with(['-', '+']))
        .collect();
    assert_eq!(
        locations,
        [
            ".github/workflows/a.yml:4:15: actions/setup-node@v4",
            ".github/workflows/a.yml:7:15: actions/setup-node@v4",
            ".github/workflows/b.yml:4:15: actions/setup-node@v4",
            ".github/workflows/b.yml:7:15: actions/setup-node@v4",
        ]
    );
    assert_eq!(forge.count("/repos/actions/setup-node/git/ref/tags/v4"), 1);
    assert_eq!(forge.total(), 3);
}

#[test]
fn branch_keeps_its_track_and_exact_tag_keeps_its_version() {
    let workflow = concat!(
        "jobs:\n",
        "  a:\n",
        "    steps:\n",
        "      - uses: owner/tool/sub@main\n",
        "      - uses: owner/tool@v1.2.3\n",
    );
    let root = project("branch", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&[
        (
            "/repos/owner/tool/git/ref/heads/main",
            200,
            r#"{"ref":"refs/heads/main","object":{"sha":"4444444444444444444444444444444444444444","type":"commit"}}"#,
        ),
        (
            "/repos/owner/tool/git/ref/tags/v1.2.3",
            200,
            r#"{"ref":"refs/tags/v1.2.3","object":{"sha":"5555555555555555555555555555555555555555","type":"commit"}}"#,
        ),
    ]);

    let output = pin(&root, &forge);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:4:15: owner/tool/sub@main\n",
            "-      - uses: owner/tool/sub@main\n",
            "+      - uses: owner/tool/sub@4444444444444444444444444444444444444444 # main\n",
            ".github/workflows/ci.yml:5:15: owner/tool@v1.2.3\n",
            "-      - uses: owner/tool@v1.2.3\n",
            "+      - uses: owner/tool@5555555555555555555555555555555555555555 # v1.2.3\n",
        )
    );
}

#[test]
fn failed_resolution_is_reported_without_suppressing_other_references() {
    let workflow = concat!(
        "jobs:\n",
        "  a:\n",
        "    steps:\n",
        "      - uses: missing/action@v1\n",
        "      - uses: broken/action@v1\n",
        "      - uses: actions/setup-node@v4\n",
    );
    let root = project("failure", &[(".github/workflows/ci.yml", workflow)]);
    let mut routes = setup_node();
    routes.push((
        "/repos/broken/action/git/ref/tags/v1",
        403,
        r#"{"message":"API rate limit exceeded"}"#,
    ));
    let forge = Forge::new(&routes);

    let output = pin(&root, &forge);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        concat!(
            "refpinner: .github/workflows/ci.yml:4:15: missing/action@v1: no tag or branch named v1 in missing/action\n",
            "refpinner: .github/workflows/ci.yml:5:15: broken/action@v1: GitHub API returned HTTP 403 for /repos/broken/action/git/ref/tags/v1\n",
        )
    );
    assert!(
        stdout(&output).starts_with(".github/workflows/ci.yml:6:15: actions/setup-node@v4\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn preview_preserves_quotes_line_endings_and_non_ascii_text() {
    let workflow = concat!(
        "# 日本語のコメント\r\n",
        "jobs:\r\n",
        "  a:\r\n",
        "    steps:\r\n",
        "      - name: \"ビルド 🚀\"\r\n",
        "        uses: \"actions/setup-node@v4\"  # ノード\r\n",
        "      - uses: 'actions/setup-node@v4'\r\n",
    );
    let root = project("encoding", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&setup_node());

    let output = pin(&root, &forge);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:6:15: actions/setup-node@v4\n",
            "-        uses: \"actions/setup-node@v4\"  # ノード\n",
            "+        uses: \"actions/setup-node@1111111111111111111111111111111111111111\" # v4.2.1 (track: v4)  # ノード\n",
            ".github/workflows/ci.yml:7:15: actions/setup-node@v4\n",
            "-      - uses: 'actions/setup-node@v4'\n",
            "+      - uses: 'actions/setup-node@1111111111111111111111111111111111111111' # v4.2.1 (track: v4)\n",
        )
    );
}

#[test]
fn floating_tag_without_a_matching_exact_version_keeps_only_its_track() {
    let workflow = "jobs:\n  a:\n    steps:\n      - uses: owner/tool@v2\n";
    let root = project("no-exact", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&[
        (
            "/repos/owner/tool/git/ref/tags/v2",
            200,
            r#"{"ref":"refs/tags/v2","object":{"sha":"6666666666666666666666666666666666666666","type":"commit"}}"#,
        ),
        (
            "/repos/owner/tool/git/matching-refs/tags/v2.",
            200,
            r#"[{"ref":"refs/tags/v2.0.0","object":{"sha":"7777777777777777777777777777777777777777","type":"commit"}}]"#,
        ),
    ]);

    let output = pin(&root, &forge);

    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        concat!(
            ".github/workflows/ci.yml:4:15: owner/tool@v2\n",
            "-      - uses: owner/tool@v2\n",
            "+      - uses: owner/tool@6666666666666666666666666666666666666666 # v2\n",
        )
    );
}

#[test]
fn reference_spelled_with_escapes_is_reported_and_not_resolved() {
    let workflow = "jobs:\n  a:\n    steps:\n      - uses: \"actions/setup-node@\\x764\"\n";
    let root = project("escape", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&setup_node());

    let output = pin(&root, &forge);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "refpinner: .github/workflows/ci.yml:4:15: actions/setup-node@v4: cannot edit a reference with escapes or line breaks\n"
    );
    assert_eq!(forge.total(), 0);
}

#[test]
fn reference_that_would_change_the_api_path_is_rejected_without_a_request() {
    let workflow = concat!(
        "jobs:\n",
        "  a:\n",
        "    steps:\n",
        "      - uses: ../evil@v1\n",
        "      - uses: owner/tool@../../../user\n",
    );
    let root = project("traversal", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&[]);

    let output = pin(&root, &forge);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        concat!(
            "refpinner: .github/workflows/ci.yml:4:15: ../evil@v1: ../evil is not an owner/repo action\n",
            "refpinner: .github/workflows/ci.yml:5:15: owner/tool@../../../user: ../../../user is not a valid Git ref name\n",
        )
    );
    assert_eq!(forge.total(), 0);
}

#[test]
fn reusable_workflows_and_composite_actions_are_previewed_and_warnings_are_not_resolved() {
    let workflow = concat!(
        "jobs:\n",
        "  call:\n",
        "    uses: actions/setup-node/.github/workflows/build.yml@v4\n",
        "  a:\n",
        "    steps:\n",
        "      - uses: actions/setup-node@${{ matrix.ref }}\n",
    );
    let action = "runs:\n  using: composite\n  steps:\n    - uses: actions/setup-node@v4\n";
    let anchored = concat!(
        "jobs:\n",
        "  a:\n",
        "    steps:\n",
        "      - &step\n",
        "        uses: actions/setup-node@v4\n",
        "  b:\n",
        "    steps:\n",
        "      - *step\n",
    );
    let root = project(
        "kinds",
        &[
            (".github/workflows/ci.yml", workflow),
            (".github/workflows/anchored.yml", anchored),
            ("action.yml", action),
        ],
    );
    let forge = Forge::new(&setup_node());

    let output = pin(&root, &forge);

    assert_eq!(
        stderr(&output),
        "refpinner: .github/workflows/anchored.yml:5:15: actions/setup-node@v4: not edited because the file uses YAML anchors or aliases\n"
    );
    assert_eq!(output.status.code(), Some(2));
    let out = stdout(&output);
    let headers: Vec<&str> = out
        .lines()
        .filter(|line| !line.starts_with(['-', '+']))
        .collect();
    assert_eq!(
        headers,
        [
            ".github/workflows/anchored.yml:4:9: warning unsupported YAML anchor",
            ".github/workflows/anchored.yml:8:9: warning unsupported YAML alias",
            ".github/workflows/ci.yml:3:11: actions/setup-node/.github/workflows/build.yml@v4",
            ".github/workflows/ci.yml:6:15: warning unsupported dynamic reference actions/setup-node@${{ matrix.ref }}",
            "action.yml:4:13: actions/setup-node@v4",
        ]
    );
    assert!(out.contains(
        "+    uses: actions/setup-node/.github/workflows/build.yml@1111111111111111111111111111111111111111 # v4.2.1 (track: v4)\n"
    ));
    assert_eq!(forge.count("/repos/actions/setup-node/git/ref/tags/v4"), 1);
}

#[test]
fn unattended_write_pins_file_keeps_exact_backup_and_reports_it() {
    let workflow = concat!(
        "# 日本語\r\n",
        "jobs:\r\n",
        "  a:\r\n",
        "    steps:\r\n",
        "      - uses: \"actions/setup-node@v4\"  # node\r\n",
        "      - uses: owner/missing@v1\r\n",
    );
    let root = project("write", &[(".github/workflows/ci.yml", workflow)]);
    let forge = Forge::new(&setup_node());

    let output = refpinner(&root, &forge, &["pin", "--write"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        "refpinner: .github/workflows/ci.yml:6:15: owner/missing@v1: no tag or branch named v1 in owner/missing\n"
    );
    assert!(
        stdout(&output)
            .ends_with("wrote .github/workflows/ci.yml (backup .github/workflows/ci.yml.bkp)\n"),
        "{}",
        stdout(&output)
    );
    let path = root.join(".github/workflows/ci.yml");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read workflow"),
        concat!(
            "# 日本語\r\n",
            "jobs:\r\n",
            "  a:\r\n",
            "    steps:\r\n",
            "      - uses: \"actions/setup-node@1111111111111111111111111111111111111111\" # v4.2.1 (track: v4)  # node\r\n",
            "      - uses: owner/missing@v1\r\n",
        )
    );
    assert_eq!(
        std::fs::read(root.join(".github/workflows/ci.yml.bkp")).expect("read backup"),
        workflow.as_bytes()
    );
}

#[test]
fn backup_takes_next_free_number_skips_symlinks_keeps_permissions_and_repeat_run_writes_nothing() {
    let workflow = "jobs:\n  a:\n    steps:\n      - uses: actions/setup-node@v4\n";
    let root = project(
        "numbered",
        &[
            (".github/workflows/ci.yml", workflow),
            (".github/workflows/ci.yml.bkp", "older backup\n"),
        ],
    );
    let dir = root.join(".github/workflows");
    let target = root.join("outside.txt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join("ci.yml"), std::fs::Permissions::from_mode(0o640))
            .expect("set permissions");
        // A dangling symlink is still an occupied name and must not be followed.
        std::os::unix::fs::symlink(&target, dir.join("ci.yml.bkp2")).expect("symlink");
    }
    let expected = if cfg!(unix) {
        "ci.yml.bkp3"
    } else {
        "ci.yml.bkp2"
    };
    let forge = Forge::new(&setup_node());

    let output = refpinner(&root, &forge, &["pin", "--write"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout(&output).ends_with(&format!(
            "wrote .github/workflows/ci.yml (backup .github/workflows/{expected})\n"
        )),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(expected)).expect("read backup"),
        workflow
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("ci.yml.bkp")).expect("read older backup"),
        "older backup\n"
    );
    assert!(!target.exists(), "backup must not follow the symlink");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["ci.yml", expected] {
            let mode = std::fs::metadata(dir.join(name))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o640, "{name}");
        }
    }

    let again = refpinner(&root, &forge, &["pin", "--write"]);

    assert_eq!(again.status.code(), Some(0));
    assert_eq!(stdout(&again), "");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("list workflows")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    let mut want = vec!["ci.yml", "ci.yml.bkp", expected];
    if cfg!(unix) {
        want.insert(2, "ci.yml.bkp2");
    }
    assert_eq!(names, want);
}

/// A read-only directory makes the backup fail; root ignores directory permissions.
#[cfg(unix)]
#[test]
fn failed_backup_leaves_original_unchanged_and_other_files_are_written() {
    use std::os::unix::fs::PermissionsExt;
    let workflow = "jobs:\n  a:\n    steps:\n      - uses: actions/setup-node@v4\n";
    let root = project(
        "readonly",
        &[
            (".github/workflows/ci.yml", workflow),
            (".github/actions/build/action.yml", workflow),
        ],
    );
    let locked = root.join(".github/actions/build");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("lock");
    if std::fs::write(locked.join("probe"), "").is_ok() {
        return;
    }
    let forge = Forge::new(&setup_node());

    let output = refpinner(&root, &forge, &["pin", "--write"]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlock");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).starts_with(
            "refpinner: .github/actions/build/action.yml: not written: cannot create backup: "
        ),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        std::fs::read_to_string(locked.join("action.yml")).expect("read action"),
        workflow
    );
    assert!(
        stdout(&output)
            .ends_with("wrote .github/workflows/ci.yml (backup .github/workflows/ci.yml.bkp)\n"),
        "{}",
        stdout(&output)
    );
}

/// util-linux `script` gives refpinner a terminal; other platforms lack a portable pty.
#[cfg(target_os = "linux")]
#[test]
fn interactive_write_applies_only_the_selected_pins() {
    use std::process::Stdio;
    let workflow = concat!(
        "jobs:\n",
        "  a:\n",
        "    steps:\n",
        "      - uses: actions/setup-node@v4\n",
        "      - uses: owner/tool@v1.2.3\n",
    );
    let root = project("interactive", &[(".github/workflows/ci.yml", workflow)]);
    let mut routes = setup_node();
    routes.push((
        "/repos/owner/tool/git/ref/tags/v1.2.3",
        200,
        r#"{"ref":"refs/tags/v1.2.3","object":{"sha":"5555555555555555555555555555555555555555","type":"commit"}}"#,
    ));
    let forge = Forge::new(&routes);
    let command = format!("{} pin --write", env!("CARGO_BIN_EXE_refpinner"));
    let Ok(mut child) = Command::new("script")
        .args(["-qec", &command, "/dev/null"])
        .current_dir(&root)
        .env("REFPINNER_GITHUB_API_URL", &forge.url)
        .env_remove("HTTP_PROXY")
        .env_remove("http_proxy")
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return;
    };
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"n\ny\n")
        .expect("answer prompts");
    let output = child.wait_with_output().expect("run script");

    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(stdout(&output).matches("apply this pin? [y/N]").count(), 2);
    assert_eq!(
        std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read workflow"),
        concat!(
            "jobs:\n",
            "  a:\n",
            "    steps:\n",
            "      - uses: actions/setup-node@v4\n",
            "      - uses: owner/tool@5555555555555555555555555555555555555555 # v1.2.3\n",
        )
    );
}

#[test]
fn pin_discovers_nested_projects_and_skips_backups() {
    let workflow = "jobs:\n  a:\n    steps:\n      - uses: actions/setup-node@v4\n";
    let root = project(
        "discover",
        &[
            ("team/app/.github/workflows/ci.yml", workflow),
            ("team/app/.github/workflows/ci.yml.bkp", workflow),
        ],
    );
    let forge = Forge::new(&setup_node());

    let output = refpinner(&root, &forge, &["pin", "--path", "team"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        concat!(
            "app/.github/workflows/ci.yml:4:15: actions/setup-node@v4\n",
            "-      - uses: actions/setup-node@v4\n",
            "+      - uses: actions/setup-node@1111111111111111111111111111111111111111 # v4.2.1 (track: v4)\n",
        )
    );
}

#[test]
fn empty_preview_succeeds() {
    let root = project("empty-preview", &[("README.md", "no CI here\n")]);
    let forge = Forge::new(&[]);

    let output = pin(&root, &forge);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "");
    assert_eq!(forge.total(), 0);
}
