use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

const PINNED_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PINNED_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// One mock response: request key, status, headers and body. `{url}` in a header or the
/// body is replaced with the mock's base URL.
type Route = (
    &'static str,
    u16,
    Vec<(&'static str, &'static str)>,
    &'static str,
);

/// A local stand-in for container registries and their token service.
///
/// Each request is keyed as `<METHOD> <path>`, plus ` <Authorization>` when that header is
/// sent. Unknown keys get 404. Requests are recorded with their `Accept` header.
struct Registry {
    url: String,
    requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl Registry {
    fn new(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock registry");
        let url = format!("http://{}", listener.local_addr().expect("mock address"));
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
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':') {
                        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
                    }
                }
                let mut key = format!("{method} {path}");
                if let Some(auth) = headers.get("authorization") {
                    key = format!("{key} {auth}");
                }
                let accept = headers.get("accept").cloned().unwrap_or_default();
                log.lock().expect("request log").push((key.clone(), accept));
                let route = routes.iter().find(|(route, ..)| *route == key);
                let (status, extra, body) =
                    route.map_or((404, Vec::new(), ""), |r| (r.1, r.2.clone(), r.3));
                let body = body.replace("{url}", &base);
                let mut response = format!("HTTP/1.1 {status} Mock\r\n");
                for (name, value) in extra {
                    response.push_str(&format!("{name}: {}\r\n", value.replace("{url}", &base)));
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
        let log = self.requests.lock().expect("request log");
        log.iter().map(|(key, _)| key.clone()).collect()
    }

    fn accepts(&self) -> Vec<String> {
        let log = self.requests.lock().expect("request log");
        log.iter().map(|(_, accept)| accept.clone()).collect()
    }
}

/// Builds a fresh project under the Cargo test tmpdir; paths are relative to the project root.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("image")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (file, body) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create project");
        std::fs::write(path, body).expect("write file");
    }
    root
}

fn refpinner(dir: &Path, registry: &Registry, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_refpinner"));
    command
        .args(args)
        .current_dir(dir)
        .env("REFPINNER_REGISTRY_URL", &registry.url)
        .env("REFPINNER_GITHUB_API_URL", &registry.url);
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
fn lint_reports_job_service_and_docker_action_images_offline() {
    let workflow = format!(
        concat!(
            "jobs:\n",
            "  build:\n",
            "    container: node:18\n",
            "    services:\n",
            "      db:\n",
            "        image: \"postgres:16\"\n",
            "      cache:\n",
            "        image: \"\"\n",
            "      dyn:\n",
            "        image: ${{{{ matrix.db }}}}\n",
            "    steps:\n",
            "      - uses: docker://alpine:3.20\n",
            "      - uses: docker://ghcr.io/owner/tool@{a}\n",
            "  test:\n",
            "    container:\n",
            "      image: alpine\n",
        ),
        a = PINNED_A
    );
    let gitlab = format!(
        concat!(
            "image: ruby:3.3\n",
            "variables:\n",
            "  image: not-an-image\n",
            "default:\n",
            "  services:\n",
            "    - redis:7\n",
            "build:\n",
            "  image:\n",
            "    name: registry.gitlab.com/group/project/builder:1.0\n",
            "    entrypoint: [\"\"]\n",
            "  services:\n",
            "    - name: postgres:16@{b}\n",
            "      alias: db\n",
            "    - $CI_REGISTRY_IMAGE/cache:latest\n",
        ),
        b = PINNED_B
    );
    let root = project(
        "lint",
        &[
            (".github/workflows/ci.yml", &workflow),
            (".gitlab-ci.yml", &gitlab),
            (
                "action.yml",
                "runs:\n  using: docker\n  image: docker://debian:stable-slim\n",
            ),
            (
                ".github/actions/local/action.yml",
                "runs:\n  using: docker\n  image: Dockerfile\n",
            ),
        ],
    );
    let registry = Registry::new(Vec::new());

    let output = refpinner(&root, &registry, &["lint"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:3:16: unpinned node:18\n",
                ".github/workflows/ci.yml:6:16: unpinned postgres:16\n",
                ".github/workflows/ci.yml:10:16: warning unsupported dynamic reference ${{{{ matrix.db }}}}\n",
                ".github/workflows/ci.yml:12:15: unpinned docker://alpine:3.20\n",
                ".github/workflows/ci.yml:13:15: pinned docker://ghcr.io/owner/tool@{a}\n",
                ".github/workflows/ci.yml:16:14: unpinned alpine\n",
                ".gitlab-ci.yml:1:8: unpinned ruby:3.3\n",
                ".gitlab-ci.yml:6:7: unpinned redis:7\n",
                ".gitlab-ci.yml:9:11: unpinned registry.gitlab.com/group/project/builder:1.0\n",
                ".gitlab-ci.yml:12:13: pinned postgres:16@{b}\n",
                ".gitlab-ci.yml:14:7: warning unsupported dynamic reference $CI_REGISTRY_IMAGE/cache:latest\n",
                "action.yml:3:10: unpinned docker://debian:stable-slim\n",
            ),
            a = PINNED_A,
            b = PINNED_B
        )
    );
    assert!(registry.keys().is_empty(), "lint must not use the network");
}

const NODE: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const POSTGRES: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const ALPINE: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
/// `sha256` of the body `{}`, served for `ruby:3.3` without a digest header.
const RUBY: &str = "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a";

/// Routes for `node:18` (anonymous), `postgres:16` (anonymous Bearer token),
/// `alpine:latest`, and `ruby:3.3` (no digest header, so the client hashes the manifest).
fn images() -> Vec<Route> {
    vec![
        (
            "HEAD /v2/library/node/manifests/18",
            200,
            vec![("Docker-Content-Digest", NODE)],
            "",
        ),
        (
            "HEAD /v2/library/postgres/manifests/16",
            401,
            vec![(
                "WWW-Authenticate",
                r#"Bearer realm="{url}/token",service="registry.docker.io",scope="repository:library/postgres:pull""#,
            )],
            "",
        ),
        (
            "GET /token?service=registry.docker.io&scope=repository:library/postgres:pull",
            200,
            Vec::new(),
            r#"{"token":"anonymous"}"#,
        ),
        (
            "HEAD /v2/library/postgres/manifests/16 Bearer anonymous",
            200,
            vec![("Docker-Content-Digest", POSTGRES)],
            "",
        ),
        (
            "HEAD /v2/library/alpine/manifests/latest",
            200,
            vec![("Docker-Content-Digest", ALPINE)],
            "",
        ),
        ("HEAD /v2/library/ruby/manifests/3.3", 200, Vec::new(), ""),
        ("GET /v2/library/ruby/manifests/3.3", 200, Vec::new(), "{}"),
    ]
}

#[test]
fn gitlab_include_and_image_share_one_safe_write() {
    let source =
        "include:\n  - project: group/project\n    ref: main\n    file: /ci.yml\nimage: node:18\n";
    let root = project("gitlab-mixed", &[(".gitlab-ci.yml", source)]);
    let mut routes = images();
    routes.push((
        "GET /projects/group%2Fproject/repository/commits/main?stats=false",
        200,
        Vec::new(),
        r#"{"id":"1111111111111111111111111111111111111111"}"#,
    ));
    let registry = Registry::new(routes);
    let output = Command::new(env!("CARGO_BIN_EXE_refpinner"))
        .current_dir(&root)
        .args(["pin", "--write"])
        .env("REFPINNER_REGISTRY_URL", &registry.url)
        .env("REFPINNER_GITLAB_API_URL", &registry.url)
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .output()
        .expect("run refpinner");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let result = std::fs::read_to_string(root.join(".gitlab-ci.yml")).expect("read file");
    assert!(
        result.contains("ref: 1111111111111111111111111111111111111111 # main"),
        "{result}"
    );
    assert!(
        result.contains(&format!("image: node:18@{NODE}")),
        "{result}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".gitlab-ci.yml.bkp")).unwrap(),
        source
    );
}

#[test]
fn preview_pins_images_to_index_digests_keeping_tags_and_leaves_files_unchanged() {
    let workflow = concat!(
        "jobs:\n",
        "  build:\n",
        "    container: node:18 # runtime\n",
        "    services:\n",
        "      db:\n",
        "        image: 'postgres:16'\n",
        "    steps:\n",
        "      - uses: docker://alpine\n",
    );
    let gitlab = "image: ruby:3.3\nservices:\n  - name: node:18\n";
    let root = project(
        "preview",
        &[
            (".github/workflows/ci.yml", workflow),
            (".gitlab-ci.yml", gitlab),
        ],
    );
    let registry = Registry::new(images());

    let output = refpinner(&root, &registry, &["pin"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                ".github/workflows/ci.yml:3:16: node:18\n",
                "-    container: node:18 # runtime\n",
                "+    container: node:18@{node} # runtime\n",
                ".github/workflows/ci.yml:6:16: postgres:16\n",
                "-        image: 'postgres:16'\n",
                "+        image: 'postgres:16@{postgres}'\n",
                ".github/workflows/ci.yml:8:15: docker://alpine\n",
                "-      - uses: docker://alpine\n",
                "+      - uses: docker://alpine:latest@{alpine}\n",
                ".gitlab-ci.yml:1:8: ruby:3.3\n",
                "-image: ruby:3.3\n",
                "+image: ruby:3.3@{ruby}\n",
                ".gitlab-ci.yml:3:11: node:18\n",
                "-  - name: node:18\n",
                "+  - name: node:18@{node}\n",
            ),
            node = NODE,
            postgres = POSTGRES,
            alpine = ALPINE,
            ruby = RUBY
        )
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read workflow"),
        workflow
    );
    let keys = registry.keys();
    let node = "HEAD /v2/library/node/manifests/18";
    assert_eq!(
        keys.iter().filter(|key| *key == node).count(),
        1,
        "{keys:?}"
    );
    let accepts = registry.accepts();
    assert_eq!(accepts.iter().filter(|a| a.contains("manifest")).count(), 6);
    for accept in accepts.iter().filter(|a| a.contains("manifest")) {
        assert!(
            accept.starts_with("application/vnd.oci.image.index.v1+json"),
            "{accept}"
        );
        assert!(accept.contains("application/vnd.docker.distribution.manifest.list.v2+json"));
    }
}

#[test]
fn write_pins_images_in_each_location_with_backups_and_reports_registry_failures() {
    let forgejo = concat!(
        "jobs:\n",
        "  a:\n",
        "    container:\n",
        "      image: \"node:18\"\n",
        "    services:\n",
        "      db:\n",
        "        image: registry.example.com:5000/team/db:9\n",
        "    steps:\n",
        "      - uses: ./local-action\n",
    );
    let action = "runs:\n  using: docker\n  image: docker://alpine # base\n";
    let gitlab = concat!(
        "default:\n",
        "  image: ruby:3.3\n",
        "test:\n",
        "  services:\n",
        "    - postgres:16\n",
        "    - Bad/Name:1\n",
    );
    let root = project(
        "write",
        &[
            (".forgejo/workflows/ci.yml", forgejo),
            (".github/actions/base/action.yml", action),
            (".gitlab-ci.yml", gitlab),
        ],
    );
    let mut routes = images();
    routes.push(("HEAD /v2/team/db/manifests/9", 500, Vec::new(), ""));
    let registry = Registry::new(routes);

    let output = refpinner(&root, &registry, &["pin", "--write"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        concat!(
            "refpinner: .forgejo/workflows/ci.yml:7:16: registry.example.com:5000/team/db:9: ",
            "registry registry.example.com:5000 returned HTTP 500 for team/db:9\n",
            "refpinner: .gitlab-ci.yml:6:7: Bad/Name:1: Bad/Name is not a valid image repository\n",
        )
    );
    let out = stdout(&output);
    for file in [
        ".forgejo/workflows/ci.yml",
        ".github/actions/base/action.yml",
        ".gitlab-ci.yml",
    ] {
        assert!(
            out.contains(&format!("wrote {file} (backup {file}.bkp)\n")),
            "{out}"
        );
        let backup = std::fs::read_to_string(root.join(format!("{file}.bkp"))).expect("backup");
        let original = [forgejo, action, gitlab]
            .into_iter()
            .find(|body| *body == backup);
        assert!(
            original.is_some(),
            "{file} backup differs from the original"
        );
    }
    assert_eq!(
        std::fs::read_to_string(root.join(".forgejo/workflows/ci.yml")).expect("read"),
        format!(
            concat!(
                "jobs:\n",
                "  a:\n",
                "    container:\n",
                "      image: \"node:18@{node}\"\n",
                "    services:\n",
                "      db:\n",
                "        image: registry.example.com:5000/team/db:9\n",
                "    steps:\n",
                "      - uses: ./local-action\n",
            ),
            node = NODE
        )
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".github/actions/base/action.yml")).expect("read"),
        format!("runs:\n  using: docker\n  image: docker://alpine:latest@{ALPINE} # base\n")
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".gitlab-ci.yml")).expect("read"),
        format!(
            concat!(
                "default:\n",
                "  image: ruby:3.3@{ruby}\n",
                "test:\n",
                "  services:\n",
                "    - postgres:16@{postgres}\n",
                "    - Bad/Name:1\n",
            ),
            ruby = RUBY,
            postgres = POSTGRES
        )
    );
    assert!(
        !registry.keys().iter().any(|key| key.contains("checkout")),
        "Forgejo action shorthand must not be resolved against GitHub"
    );

    let lint = refpinner(&root, &registry, &["lint"]);

    assert_eq!(lint.status.code(), Some(1));
    assert!(stdout(&lint).contains(&format!(
        ".github/actions/base/action.yml:3:10: pinned docker://alpine:latest@{ALPINE}\n"
    )));
}

#[test]
fn registry_auth_scheme_is_case_insensitive_and_accepts_access_token() {
    let original = "image: alpine\n";
    let root = project("auth-scheme", &[(".gitlab-ci.yml", original)]);
    let registry = Registry::new(vec![
        (
            "HEAD /v2/library/alpine/manifests/latest",
            401,
            vec![("WWW-Authenticate", r#"bearer realm="{url}/token""#)],
            "",
        ),
        (
            "GET /token",
            200,
            Vec::new(),
            r#"{"access_token":"public"}"#,
        ),
        (
            "HEAD /v2/library/alpine/manifests/latest Bearer public",
            200,
            vec![("Docker-Content-Digest", ALPINE)],
            "",
        ),
    ]);

    let output = refpinner(&root, &registry, &["pin", "--write"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(root.join(".gitlab-ci.yml")).expect("read image"),
        format!("image: alpine:latest@{ALPINE}\n")
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".gitlab-ci.yml.bkp")).expect("read backup"),
        original
    );
}

#[test]
fn mixed_pins_preserve_flow_yaml_and_bytes_take_next_backup_and_skip_image_updates() {
    let original = concat!(
        "# café\r\n",
        "jobs:\r\n",
        "  build:\r\n",
        "    container: {image: \"node:18\"}\r\n",
        "    services: {db: {image: 'node:18'}} # keep\r\n",
        "    steps:\r\n",
        "      - uses: owner/tool@main # tool\r\n",
    );
    let file = ".github/workflows/ci.yml";
    let root = project("mixed-bytes", &[(file, original)]);
    let path = root.join(file);
    std::fs::write(root.join(format!("{file}.bkp")), "existing backup").expect("occupy backup");
    let mut routes = images();
    routes.push((
        "GET /repos/owner/tool/git/ref/tags/main",
        200,
        Vec::new(),
        r#"{"object":{"type":"commit","sha":"1111111111111111111111111111111111111111"}}"#,
    ));
    let registry = Registry::new(routes);

    let output = refpinner(&root, &registry, &["pin", "--write"]);

    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains(&format!("wrote {file} (backup {file}.bkp2)\n")));
    let expected = original
        .replace("node:18", &format!("node:18@{NODE}"))
        .replace(
            "owner/tool@main",
            "owner/tool@1111111111111111111111111111111111111111 # main",
        );
    assert_eq!(
        std::fs::read(&path).expect("read workflow"),
        expected.as_bytes()
    );
    assert_eq!(
        std::fs::read(root.join(format!("{file}.bkp2"))).expect("read exact backup"),
        original.as_bytes()
    );
    assert_eq!(
        std::fs::read_to_string(root.join(format!("{file}.bkp"))).expect("read old backup"),
        "existing backup"
    );
    assert_eq!(
        registry
            .keys()
            .iter()
            .filter(|key| key.contains("/manifests/"))
            .count(),
        1
    );
    let before = registry.keys().len();

    let repeat = refpinner(&root, &registry, &["pin", "--write"]);

    assert_eq!(repeat.status.code(), Some(0));
    assert_eq!(stderr(&repeat), "");
    assert_eq!(stdout(&repeat), "");
    assert_eq!(registry.keys().len(), before);

    let update = refpinner(&root, &registry, &["update", "--write"]);

    assert_eq!(update.status.code(), Some(0));
    assert_eq!(stderr(&update), "");
    assert_eq!(stdout(&update), "");
    assert_eq!(
        registry.keys().len(),
        before + 1,
        "only the action should be checked"
    );
    assert_eq!(
        std::fs::read(&path).expect("read unchanged file"),
        expected.as_bytes()
    );
    assert!(!root.join(format!("{file}.bkp3")).exists());
}

#[test]
fn unsafe_image_yaml_is_not_resolved_or_written_and_safe_files_continue() {
    let aliased = "image: &base node:18\nservices: [*base]\n";
    let escaped = "jobs:\n  build:\n    container: \"node\\x3a18\"\n";
    let root = project(
        "unsafe-yaml",
        &[
            (".gitlab-ci.yml", aliased),
            (".github/workflows/ci.yml", escaped),
            (
                ".gitea/workflows/ci.yaml",
                "jobs:\n  build:\n    container: alpine\n",
            ),
        ],
    );
    let registry = Registry::new(images());

    let output = refpinner(&root, &registry, &["pin", "--write"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("cannot edit a reference with escapes or line breaks"));
    assert!(stderr(&output).contains("not edited because the file uses YAML anchors or aliases"));
    assert!(stdout(&output).contains("warning unsupported YAML alias"));
    for (file, original) in [
        (".gitlab-ci.yml", aliased),
        (".github/workflows/ci.yml", escaped),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.join(file)).expect("read unsafe file"),
            original
        );
        assert!(!root.join(format!("{file}.bkp")).exists());
    }
    assert_eq!(
        std::fs::read_to_string(root.join(".gitea/workflows/ci.yaml")).expect("read safe file"),
        format!("jobs:\n  build:\n    container: alpine:latest@{ALPINE}\n")
    );
    assert_eq!(
        registry.keys(),
        ["HEAD /v2/library/alpine/manifests/latest"]
    );
}

#[test]
fn registry_failures_leave_originals_without_backups_and_keep_safe_changes() {
    let original = concat!(
        "image: missing:1\n",
        "services:\n",
        "  - limited:1\n",
        "  - private:1\n",
        "  - unreadable:1\n",
    );
    let root = project(
        "registry-failures",
        &[
            (".gitlab-ci.yml", original),
            (
                ".github/workflows/ci.yml",
                "jobs:\n  a:\n    container: alpine\n",
            ),
        ],
    );
    let mut routes = images();
    routes.extend([
        ("HEAD /v2/library/limited/manifests/1", 429, Vec::new(), ""),
        (
            "HEAD /v2/library/private/manifests/1",
            401,
            vec![("WWW-Authenticate", r#"Bearer realm="{url}/bad-token""#)],
            "",
        ),
        ("GET /bad-token", 200, Vec::new(), "not JSON"),
        (
            "HEAD /v2/library/unreadable/manifests/1",
            200,
            Vec::new(),
            "",
        ),
        (
            "GET /v2/library/unreadable/manifests/1",
            503,
            Vec::new(),
            "",
        ),
    ]);
    let registry = Registry::new(routes);

    let output = refpinner(&root, &registry, &["pin", "--write"]);

    assert_eq!(output.status.code(), Some(2));
    let errors = stderr(&output);
    for message in [
        "no tag 1 in registry-1.docker.io image library/missing",
        "returned HTTP 429 for library/limited:1",
        "invalid registry token response:",
        "returned HTTP 503 for library/unreadable:1",
    ] {
        assert!(errors.contains(message), "{errors}");
    }
    assert_eq!(
        std::fs::read_to_string(root.join(".gitlab-ci.yml")).expect("read failed file"),
        original
    );
    assert!(!root.join(".gitlab-ci.yml.bkp").exists());
    assert!(stdout(&output).contains("wrote .github/workflows/ci.yml"));
}
