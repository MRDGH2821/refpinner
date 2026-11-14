mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Forge;

const OLD: &str = "1111111111111111111111111111111111111111";
const NEW: &str = "2222222222222222222222222222222222222222";
const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MOVED: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("online_lint")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create");
        std::fs::write(path, body).expect("write");
    }
    root
}

fn lint(dir: &Path, url: Option<&str>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command.arg("lint").args(args).current_dir(dir);
    if let Some(url) = url {
        command.env("REFPINNER_GITHUB_API_URL", url);
    }
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

fn exact_tag_routes(commit: &str) -> Vec<(&'static str, u16, String)> {
    vec![
        (
            "/repos/owner/tool/git/ref/tags/v4.2.1",
            200,
            format!(
                r#"{{"ref":"refs/tags/v4.2.1","object":{{"sha":"{commit}","type":"commit"}}}}"#
            ),
        ),
        (
            "/repos/owner/tool/git/matching-refs/tags/v",
            200,
            format!(
                concat!(
                    r#"[{{"ref":"refs/tags/v4.2.1","object":{{"sha":"{commit}","type":"commit"}}}},"#,
                    r#"{{"ref":"refs/tags/v4.2.3","object":{{"sha":"2222222222222222222222222222222222222222","type":"commit"}}}},"#,
                    r#"{{"ref":"refs/tags/v5.1.0","object":{{"sha":"5555555555555555555555555555555555555555","type":"commit"}}}}]"#
                ),
                commit = commit
            ),
        ),
        (
            "/repos/owner/tool/git/ref/tags/v4.2.3",
            200,
            r#"{"ref":"refs/tags/v4.2.3","object":{"sha":"2222222222222222222222222222222222222222","type":"commit"}}"#
                .into(),
        ),
        (
            "/repos/owner/tool/git/ref/tags/v5.1.0",
            200,
            r#"{"ref":"refs/tags/v5.1.0","object":{"sha":"5555555555555555555555555555555555555555","type":"commit"}}"#
                .into(),
        ),
    ]
}

/// Serves owned `String` bodies; unknown paths get 404.
fn forge_owned(routes: &[(&str, u16, String)]) -> Forge {
    let borrowed: Vec<(&str, u16, &str)> = routes
        .iter()
        .map(|(path, status, body)| (*path, *status, body.as_str()))
        .collect();
    Forge::new(&borrowed)
}

#[test]
fn json_offline_reports_findings_and_status() {
    let root = project(
        "json-offline",
        &[(
            ".github/workflows/ci.yml",
            "jobs:\n  a:\n    steps:\n      - uses: owner/repo@v1\n",
        )],
    );

    let output = lint(&root, None, &["--json"]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json");
    assert_eq!(json["status"], 1);
    assert_eq!(json["results"][0]["state"], "unpinned");
    assert_eq!(json["results"][0]["text"], "owner/repo@v1");
    assert!(json["warnings"].as_array().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn json_includes_symlink_warnings() {
    let root = project(
        "json-symlink",
        &[
            (
                ".github/workflows/ci.yml",
                concat!(
                    "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@",
                    "3d3c42e5aac5ba805825da76410c181273ba90b1\n"
                ),
            ),
            ("shared/linked.yml", "jobs: {}\n"),
        ],
    );
    std::os::unix::fs::symlink(
        root.join("shared/linked.yml"),
        root.join(".github/workflows/linked.yml"),
    )
    .expect("symlink");

    let output = lint(&root, None, &["--json"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json");
    assert_eq!(json["warnings"][0]["file"], ".github/workflows/linked.yml");
    assert_eq!(json["warnings"][0]["message"], "skipped symlink");
    assert_eq!(json["results"][0]["state"], "pinned");
}

#[test]
fn offline_lint_makes_no_network_requests() {
    let forge = Forge::new(&[(
        "/repos/owner/tool/git/ref/tags/v4.2.1",
        200,
        r#"{"ref":"refs/tags/v4.2.1","object":{"sha":"1111111111111111111111111111111111111111","type":"commit"}}"#,
    )]);
    let root = project(
        "offline-no-net",
        &[(
            ".github/workflows/ci.yml",
            &format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{OLD} # v4.2.1\n"),
        )],
    );

    let output = lint(&root, Some(&forge.url), &[]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("pinned"));
    assert_eq!(forge.total(), 0);
}

#[test]
fn online_reports_exact_tag_mismatch() {
    let forge = forge_owned(&exact_tag_routes(NEW));
    let root = project(
        "mismatch",
        &[(
            ".github/workflows/ci.yml",
            &format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{OLD} # v4.2.1\n"),
        )],
    );

    let output = lint(&root, Some(&forge.url), &["--online"]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("mismatch") && stdout(&output).contains("moved to"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn online_reports_update_without_failing_including_major() {
    let forge = forge_owned(&exact_tag_routes(OLD));
    let root = project(
        "update-ok",
        &[(
            ".github/workflows/ci.yml",
            &format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{OLD} # v4.2.1\n"),
        )],
    );

    let output = lint(&root, Some(&forge.url), &["--online"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("update"), "{out}");
    assert!(out.contains("v5.1.0") || out.contains("v4.2.3"), "{out}");
}

#[test]
fn fail_on_update_treats_updates_as_findings() {
    let forge = forge_owned(&exact_tag_routes(OLD));
    let root = project(
        "fail-update",
        &[(
            ".github/workflows/ci.yml",
            &format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{OLD} # v4.2.1\n"),
        )],
    );

    let output = lint(&root, Some(&forge.url), &["--online", "--fail-on-update"]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).contains("update"), "{}", stdout(&output));
}

#[test]
fn online_incomplete_check_exits_distinctly_and_continues() {
    let forge = Forge::new(&[
        (
            "/repos/broken/action/git/ref/tags/v1",
            403,
            r#"{"message":"API rate limit exceeded"}"#,
        ),
        (
            "/repos/owner/tool/git/ref/tags/v4.2.1",
            200,
            &format!(r#"{{"ref":"refs/tags/v4.2.1","object":{{"sha":"{OLD}","type":"commit"}}}}"#),
        ),
        (
            "/repos/owner/tool/git/matching-refs/tags/v",
            200,
            &format!(
                r#"[{{"ref":"refs/tags/v4.2.1","object":{{"sha":"{OLD}","type":"commit"}}}}]"#
            ),
        ),
    ]);
    let root = project(
        "incomplete",
        &[(
            ".github/workflows/ci.yml",
            &format!(
                "jobs:\n  a:\n    steps:\n      - uses: broken/action@{OLD} # v1\n      - uses: owner/tool@{OLD} # v4.2.1\n"
            ),
        )],
    );

    let output = lint(&root, Some(&forge.url), &["--online", "--json"]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json");
    assert_eq!(json["status"], 3);
    let states: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["state"].as_str().unwrap())
        .collect();
    assert!(states.contains(&"incomplete"), "{states:?}");
    assert!(states.contains(&"pinned"), "{states:?}");
}

#[test]
fn online_branch_movement_is_update_not_mismatch() {
    let tip = "3333333333333333333333333333333333333333";
    let forge = Forge::new(&[(
        "/repos/owner/tool/git/ref/heads/main",
        200,
        &format!(r#"{{"ref":"refs/heads/main","object":{{"sha":"{tip}","type":"commit"}}}}"#),
    )]);
    let root = project(
        "branch-move",
        &[(
            ".github/workflows/ci.yml",
            &format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{OLD} # main\n"),
        )],
    );

    let output = lint(&root, Some(&forge.url), &["--online"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(
        out.contains("update") && out.contains("main moved"),
        "{out}"
    );
    assert!(!out.contains("mismatch"), "{out}");
}

#[test]
fn include_excluded_audits_excluded_references_online() {
    let forge = forge_owned(&exact_tag_routes(OLD));
    let root = project(
        "include-excluded",
        &[
            (
                ".config/refpinner.toml",
                "references = [{ pattern = \"owner/tool\", exclude = true }]\n",
            ),
            (
                ".github/workflows/ci.yml",
                &format!("jobs:\n  a:\n    steps:\n      - uses: owner/tool@{OLD} # v4.2.1\n"),
            ),
        ],
    );

    let hidden = lint(&root, Some(&forge.url), &["--online"]);
    let audit = lint(&root, Some(&forge.url), &["--online", "--include-excluded"]);

    assert_eq!(hidden.status.code(), Some(0), "{}", stderr(&hidden));
    assert!(
        !stdout(&hidden).contains("owner/tool@"),
        "{}",
        stdout(&hidden)
    );
    assert_eq!(audit.status.code(), Some(0), "{}", stderr(&audit));
    assert!(stdout(&audit).contains("owner/tool@"), "{}", stdout(&audit));
}

/// Local OCI registry stand-in that returns a fixed digest for one tag.
fn registry(digest: &str) -> (String, std::sync::Arc<std::sync::Mutex<usize>>) {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let hits = Arc::new(Mutex::new(0usize));
    let log = Arc::clone(&hits);
    let body = digest.to_owned();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(&stream);
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
            }
            *log.lock().expect("hits") += 1;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nDocker-Content-Digest: {body}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]"
            );
        }
    });
    (url, hits)
}

#[test]
fn online_image_digest_mismatch_fails() {
    let (url, hits) = registry(MOVED);
    let root = project(
        "image-mismatch",
        &[(
            ".github/workflows/ci.yml",
            &format!(
                "jobs:\n  a:\n    container: alpine:3.19@{DIGEST}\n    steps:\n      - run: true\n"
            ),
        )],
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(["lint", "--online"])
        .current_dir(&root)
        .env("REFPINNER_REGISTRY_URL", &url);
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
    let output = command.output().expect("run");

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("mismatch") && stdout(&output).contains("moved to"),
        "{}",
        stdout(&output)
    );
    assert!(*hits.lock().expect("hits") > 0);
}
