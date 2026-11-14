// cspell:ignore dXNlcjpzZWNyZXQ
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Mock forge that keys routes as `path` or `path Authorization: …`.
struct AuthForge {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl AuthForge {
    fn new(routes: &[(&str, u16, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(k, s, b)| ((*k).to_owned(), *s, (*b).to_owned()))
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
                let mut auth = None;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.eq_ignore_ascii_case("authorization")
                    {
                        auth = Some(value.trim().to_owned());
                    }
                }
                let mut key = path.clone();
                if let Some(auth) = &auth {
                    key = format!("{path} Authorization: {auth}");
                }
                log.lock().expect("log").push(key.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(route, ..)| *route == key)
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

    fn keys(&self) -> Vec<String> {
        self.requests.lock().expect("log").clone()
    }
}

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("credentials")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write");
    }
    root
}

fn store_file(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("credentials")
        .join(name)
        .join("credentials.toml");
    let _ = std::fs::remove_file(&path);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    path
}

fn refpinner(args: &[&str], store: &Path, envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command.args(args).env("REFPINNER_CREDENTIALS_FILE", store);
    for proxy in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
        "GITLAB_TOKEN",
        "GITLAB_ACCESS_TOKEN",
        "FORGEJO_TOKEN",
        "GITEA_TOKEN",
    ] {
        command.env_remove(proxy);
    }
    for (key, value) in envs {
        command.env(key, value);
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
fn credentials_list_and_remove_never_print_token_values() {
    let store = store_file("list-remove");
    let secret = "ghs_should_never_appear_in_output_xyz";
    std::fs::write(
        &store,
        format!("[[entries]]\nhost = \"github.com\"\nservice = \"github\"\ntoken = \"{secret}\"\n"),
    )
    .expect("write store");

    let listed = refpinner(&["credentials", "list"], &store, &[]);
    assert_eq!(listed.status.code(), Some(0), "{}", stderr(&listed));
    let out = stdout(&listed);
    assert!(out.contains("github.com"), "{out}");
    assert!(out.contains("github"), "{out}");
    assert!(out.contains("file"), "{out}");
    assert!(!out.contains(secret), "{out}");
    assert!(!stderr(&listed).contains(secret), "{}", stderr(&listed));

    let removed = refpinner(
        &[
            "credentials",
            "remove",
            "--host",
            "github.com",
            "--service",
            "github",
        ],
        &store,
        &[],
    );
    assert_eq!(removed.status.code(), Some(0), "{}", stderr(&removed));
    assert!(!stdout(&removed).contains(secret));
    assert!(!stderr(&removed).contains(secret));

    let empty = refpinner(&["credentials", "list"], &store, &[]);
    assert_eq!(empty.status.code(), Some(0));
    assert!(!stdout(&empty).contains("github.com"));
}

#[test]
fn unattended_store_refuses_to_save_automatically() {
    let store = store_file("unattended-store");
    let output = Command::new(env!("CARGO_BIN_EXE_refpinner"))
        .args([
            "credentials",
            "store",
            "--host",
            "github.com",
            "--service",
            "github",
        ])
        .env("REFPINNER_CREDENTIALS_FILE", &store)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run store");
    assert_ne!(output.status.code(), Some(0));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("unattended") || err.contains("terminal") || err.contains("prompt"),
        "{err}"
    );
    assert!(
        !store.exists()
            || std::fs::read_to_string(&store)
                .unwrap_or_default()
                .is_empty()
    );
}

#[test]
fn private_github_action_resolves_with_file_credential_and_hides_token() {
    let secret = "ghs_file_token_for_private_action";
    let forge = AuthForge::new(&[
        (
            "/repos/acme/private/git/ref/tags/v1",
            404,
            r#"{"message":"Not Found"}"#,
        ),
        (
            concat!(
                "/repos/acme/private/git/ref/tags/v1",
                " Authorization: Bearer ghs_file_token_for_private_action"
            ),
            200,
            r#"{"ref":"refs/tags/v1","object":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","type":"commit"}}"#,
        ),
    ]);
    let host = forge
        .url
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap();
    let root = project(
        "private-github",
        &[(
            ".github/workflows/ci.yml",
            "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: acme/private@v1\n",
        )],
    );
    let store = store_file("private-github");
    std::fs::write(
        &store,
        format!("[[entries]]\nhost = \"{host}\"\nservice = \"github\"\ntoken = \"{secret}\"\n"),
    )
    .expect("write");

    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(["pin"])
        .current_dir(&root)
        .env("REFPINNER_CREDENTIALS_FILE", &store)
        .env("REFPINNER_GITHUB_API_URL", &forge.url);
    for proxy in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "GH_TOKEN",
        "GITHUB_TOKEN",
    ] {
        command.env_remove(proxy);
    }
    let output = command.output().expect("run");

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains(secret), "{combined}");
    assert!(
        stdout(&output).contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "{}",
        stdout(&output)
    );
    assert!(
        forge
            .keys()
            .iter()
            .any(|k| k.contains("Authorization: Bearer")),
        "{:?}",
        forge.keys()
    );
}

#[test]
fn rejected_credential_warns_and_falls_back_to_public_access() {
    let secret = "ghs_rejected_token_value";
    let forge = AuthForge::new(&[
        (
            concat!(
                "/repos/actions/checkout/git/ref/tags/v4",
                " Authorization: Bearer ghs_rejected_token_value"
            ),
            401,
            r#"{"message":"Bad credentials"}"#,
        ),
        (
            "/repos/actions/checkout/git/ref/tags/v4",
            200,
            r#"{"ref":"refs/tags/v4","object":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","type":"commit"}}"#,
        ),
    ]);
    let host = forge
        .url
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap();
    let root = project(
        "rejected-cred",
        &[(
            ".github/workflows/ci.yml",
            "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
        )],
    );
    let store = store_file("rejected-cred");
    std::fs::write(
        &store,
        format!("[[entries]]\nhost = \"{host}\"\nservice = \"github\"\ntoken = \"{secret}\"\n"),
    )
    .expect("write");

    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(["pin"])
        .current_dir(&root)
        .env("REFPINNER_CREDENTIALS_FILE", &store)
        .env("REFPINNER_GITHUB_API_URL", &forge.url);
    for proxy in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "GH_TOKEN",
        "GITHUB_TOKEN",
    ] {
        command.env_remove(proxy);
    }
    let output = command.output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("rejected") || err.contains("warning"), "{err}");
    assert!(!err.contains(secret), "{err}");
    assert!(stdout(&output).contains("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
}

#[test]
fn github_env_token_is_scoped_to_github_com_not_enterprise_host() {
    // Loopback API hosts must not receive github.com env tokens.
    let forge = AuthForge::new(&[(
        "/repos/actions/checkout/git/ref/tags/v4",
        200,
        r#"{"ref":"refs/tags/v4","object":{"sha":"cccccccccccccccccccccccccccccccccccccccc","type":"commit"}}"#,
    )]);
    let root = project(
        "env-scope",
        &[(
            ".github/workflows/ci.yml",
            "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
        )],
    );
    let store = store_file("env-scope");
    let secret = "ghs_env_must_not_go_to_ghes";
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(["pin"])
        .current_dir(&root)
        .env("REFPINNER_CREDENTIALS_FILE", &store)
        .env("REFPINNER_GITHUB_API_URL", &forge.url)
        .env("GITHUB_TOKEN", secret)
        .env("GH_TOKEN", secret);
    for proxy in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        command.env_remove(proxy);
    }
    let output = command.output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains(secret), "{combined}");
    assert!(
        forge.keys().iter().all(|k| !k.contains("Authorization")),
        "{:?}",
        forge.keys()
    );
}

#[test]
fn private_registry_image_uses_basic_auth_on_token_endpoint_without_leaking_secret() {
    use std::collections::HashMap;

    type Route = (
        &'static str,
        u16,
        Vec<(&'static str, &'static str)>,
        &'static str,
    );

    struct Registry {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl Registry {
        fn new(routes: Vec<Route>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let url = format!("http://{}", listener.local_addr().expect("addr"));
            let base = url.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let log = Arc::clone(&requests);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let mut reader = BufReader::new(&stream);
                    let mut request_line = String::new();
                    let _ = reader.read_line(&mut request_line);
                    let mut parts = request_line.split(' ');
                    let method = parts.next().unwrap_or("").to_owned();
                    let path = parts.next().unwrap_or("").to_owned();
                    let mut headers = HashMap::new();
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0
                            || header.trim().is_empty()
                        {
                            break;
                        }
                        if let Some((name, value)) = header.split_once(':') {
                            headers
                                .insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
                        }
                    }
                    let mut key = format!("{method} {path}");
                    if let Some(auth) = headers.get("authorization") {
                        key = format!("{key} {auth}");
                    }
                    log.lock().expect("log").push(key.clone());
                    let route = routes.iter().find(|(route, ..)| *route == key);
                    let (status, extra, body) =
                        route.map_or((404, Vec::new(), ""), |r| (r.1, r.2.clone(), r.3));
                    let body = body.replace("{url}", &base);
                    let mut response = format!("HTTP/1.1 {status} Mock\r\n");
                    for (name, value) in extra {
                        response
                            .push_str(&format!("{name}: {}\r\n", value.replace("{url}", &base)));
                    }
                    let sent = if method == "HEAD" { "" } else { body.as_str() };
                    response.push_str(&format!(
                        "Content-Length: {}\r\nConnection: close\r\n\r\n{sent}",
                        sent.len()
                    ));
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            Self { url, requests }
        }

        fn keys(&self) -> Vec<String> {
            self.requests.lock().expect("log").clone()
        }
    }

    let digest = "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    // Base64 encoding of user:secret for the mock Authorization key.
    let basic = "Basic dXNlcjpzZWNyZXQ=";
    let registry = Registry::new(vec![
        (
            "HEAD /v2/acme/app/manifests/1",
            401,
            vec![(
                "WWW-Authenticate",
                r#"Bearer realm="{url}/token",service="registry",scope="repository:acme/app:pull""#,
            )],
            "",
        ),
        (
            "GET /token?service=registry&scope=repository:acme/app:pull",
            401,
            Vec::new(),
            "",
        ),
        (
            concat!(
                "GET /token?service=registry&scope=repository:acme/app:pull",
                " Basic dXNlcjpzZWNyZXQ="
            ),
            200,
            Vec::new(),
            r#"{"token":"pull-token"}"#,
        ),
        (
            "HEAD /v2/acme/app/manifests/1 Bearer pull-token",
            200,
            vec![("Docker-Content-Digest", digest)],
            "",
        ),
    ]);
    let root = project(
        "private-registry",
        &[(".gitlab-ci.yml", "image: registry-1.docker.io/acme/app:1\n")],
    );
    let store = store_file("private-registry");
    // Credentials are scoped to the image registry host, not the mock URL override.
    std::fs::write(
        &store,
        "[[entries]]\nhost = \"registry-1.docker.io\"\nservice = \"registry\"\ntoken = \"user:secret\"\n",
    )
    .expect("write");

    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(["pin"])
        .current_dir(&root)
        .env("REFPINNER_CREDENTIALS_FILE", &store)
        .env("REFPINNER_REGISTRY_URL", &registry.url);
    for proxy in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        command.env_remove(proxy);
    }
    let output = command.output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains("user:secret"), "{combined}");
    assert!(!combined.contains(basic), "{combined}");
    assert!(stdout(&output).contains(digest), "{}", stdout(&output));
    assert!(
        registry.keys().iter().any(|k| k.contains(basic)),
        "{:?}",
        registry.keys()
    );
}

#[test]
fn authenticated_cli_token_is_tried_before_file() {
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("credentials")
        .join("fake-cli-bin");
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::create_dir_all(&bin).expect("mkdir");
    let gh = bin.join("gh");
    std::fs::write(
        &gh,
        "#!/bin/sh\n# fake gh for credential tests\nif [ \"$1\" = auth ] && [ \"$2\" = token ]; then\n  echo 'ghs_from_cli_not_file'\n  exit 0\nfi\nexit 1\n",
    )
    .expect("write gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&gh).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh, perms).unwrap();
    }

    let secret_file = "ghs_from_file_should_lose";
    let forge = AuthForge::new(&[(
        concat!(
            "/repos/acme/cli/git/ref/tags/v1",
            " Authorization: Bearer ghs_from_cli_not_file"
        ),
        200,
        r#"{"ref":"refs/tags/v1","object":{"sha":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","type":"commit"}}"#,
    )]);
    let host = forge
        .url
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap();
    let root = project(
        "cli-first",
        &[(
            ".github/workflows/ci.yml",
            "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: acme/cli@v1\n",
        )],
    );
    let store = store_file("cli-first");
    std::fs::write(
        &store,
        format!(
            "[[entries]]\nhost = \"{host}\"\nservice = \"github\"\ntoken = \"{secret_file}\"\n"
        ),
    )
    .expect("write");

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(["pin"])
        .current_dir(&root)
        .env("PATH", &path)
        .env("REFPINNER_CREDENTIALS_FILE", &store)
        .env("REFPINNER_GITHUB_API_URL", &forge.url);
    for proxy in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "GH_TOKEN",
        "GITHUB_TOKEN",
    ] {
        command.env_remove(proxy);
    }
    let output = command.output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains("ghs_from_cli_not_file"), "{combined}");
    assert!(!combined.contains(secret_file), "{combined}");
}
