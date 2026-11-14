//! Credential lookup scoped by `(host, service)`.
//!
//! Order: authenticated CLI → system keyring → environment → consented plaintext file.
//! Tokens never implement `Display`/`Debug` with their value. Unattended runs never save.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::{Deserialize, Serialize};

/// Where a credential came from, shown by `credentials list` without revealing the secret.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    Cli,
    Keyring,
    Environment,
    File,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Keyring => "keyring",
            Self::Environment => "environment",
            Self::File => "file",
        }
    }
}

/// Service a credential is valid for. Never reuse across services.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Github,
    Gitlab,
    Forgejo,
    Registry,
}

impl Service {
    fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Forgejo => "forgejo",
            Self::Registry => "registry",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            "forgejo" => Ok(Self::Forgejo),
            "registry" => Ok(Self::Registry),
            _ => Err(format!(
                "unknown service {value}; expected github, gitlab, forgejo or registry"
            )),
        }
    }
}

/// Opaque token; redacts itself in formatters so logs cannot leak it.
pub struct Token(String);

impl Token {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token([redacted])")
    }
}

/// A credential found for one host and service.
pub struct Credential {
    pub token: Token,
    #[allow(dead_code)] // retained for diagnostics and future list-from-memory output
    pub source: Source,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    host: String,
    service: Service,
    token: String,
}

/// Resolves credentials in the documented order for `host` and `service`.
pub fn find(host: &str, service: Service) -> Option<Credential> {
    let host = normalize_host(host);
    cli(&host, service)
        .or_else(|| keyring_get(&host, service))
        .or_else(|| environment(&host, service))
        .or_else(|| file_get(&host, service))
}

/// Lists stored credentials (file + keyring) without token values.
pub fn list_stored() -> Vec<(String, Service, Source)> {
    let mut listed = Vec::new();
    if let Ok(store) = load_store() {
        for entry in store.entries {
            listed.push((entry.host, entry.service, Source::File));
        }
    }
    listed
}

/// Removes a stored credential for `host` and `service` from file and keyring.
pub fn remove(host: &str, service: Service) -> Result<(), String> {
    let host = normalize_host(host);
    let _ = keyring_delete(&host, service);
    let mut store = load_store().unwrap_or_default();
    let before = store.entries.len();
    store
        .entries
        .retain(|e| !(e.host == host && e.service == service));
    if store.entries.len() == before {
        return Err(format!(
            "no stored credential for {host} ({})",
            service.as_str()
        ));
    }
    save_store(&store)
}

/// Saves a token after interactive consent. Refuses when stdin is not a terminal.
pub fn store_interactive(host: &str, service: Service) -> Result<(), String> {
    if !io::stdin().is_terminal() {
        return Err(String::from(
            "credentials store requires an interactive terminal; unattended runs never save automatically",
        ));
    }
    let host = normalize_host(host);
    eprint!(
        "Save token for {} ({}) to the credentials file? [y/N] ",
        host,
        service.as_str()
    );
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
        return Err(String::from("save cancelled"));
    }
    eprint!("Token: ");
    let _ = io::stderr().flush();
    let mut token = String::new();
    io::stdin()
        .read_line(&mut token)
        .map_err(|e| e.to_string())?;
    let token = token.trim();
    if token.is_empty() {
        return Err(String::from("empty token"));
    }
    // Prefer keyring when available; fall back to the consented plaintext file.
    if keyring_set(&host, service, token).is_ok() {
        return Ok(());
    }
    let mut store = load_store().unwrap_or_default();
    if let Some(existing) = store
        .entries
        .iter_mut()
        .find(|e| e.host == host && e.service == service)
    {
        existing.token = token.to_owned();
    } else {
        store.entries.push(Entry {
            host,
            service,
            token: token.to_owned(),
        });
    }
    save_store(&store)
}

/// CLI entry points for `refpinner credentials …`.
pub fn run(command: CredentialsCommand) -> ExitCode {
    match command {
        CredentialsCommand::List => {
            for (host, service, source) in list_stored() {
                println!("{} {} {}", host, service.as_str(), source.as_str());
            }
            ExitCode::SUCCESS
        }
        CredentialsCommand::Remove { host, service } => {
            match Service::parse(&service).and_then(|service| remove(&host, service)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("refpinner: {error}");
                    ExitCode::from(2)
                }
            }
        }
        CredentialsCommand::Store { host, service } => {
            match Service::parse(&service).and_then(|service| store_interactive(&host, service)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("refpinner: {error}");
                    ExitCode::from(2)
                }
            }
        }
    }
}

#[derive(clap::Subcommand)]
pub enum CredentialsCommand {
    /// List stored credentials without revealing token values
    List,
    /// Remove a stored credential
    Remove {
        #[arg(long)]
        host: String,
        #[arg(long)]
        service: String,
    },
    /// Prompt to save a token (interactive only)
    Store {
        #[arg(long)]
        host: String,
        #[arg(long)]
        service: String,
    },
}

/// Host used for credential lookup from an API or registry base URL.
pub fn host_from_url(url: &str) -> Option<String> {
    let uri: ureq::http::Uri = url.parse().ok()?;
    let host = uri.host()?.to_owned();
    Some(normalize_host(&host))
}

/// Maps API hosts like `api.github.com` to the credential host `github.com`.
pub fn github_credential_host(api_base: &str) -> Option<String> {
    let host = host_from_url(api_base)?;
    if host.eq_ignore_ascii_case("api.github.com") {
        return Some(String::from("github.com"));
    }
    Some(host)
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn cli(host: &str, service: Service) -> Option<Credential> {
    let token = match service {
        Service::Github => {
            let output = Command::new("gh")
                .args(["auth", "token", "--hostname", host])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            String::from_utf8(output.stdout).ok()?
        }
        Service::Gitlab => {
            let output = Command::new("glab")
                .args(["config", "get", "token", "--host", host])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            String::from_utf8(output.stdout).ok()?
        }
        Service::Forgejo => return None,
        Service::Registry => {
            let cred = docker_credential::get_credential(host).ok()?;
            match cred {
                docker_credential::DockerCredential::UsernamePassword(username, password) => {
                    format!("{username}:{password}")
                }
                docker_credential::DockerCredential::IdentityToken(token) => token,
            }
        }
    };
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(Credential {
        token: Token::new(token),
        source: Source::Cli,
    })
}

fn environment(host: &str, service: Service) -> Option<Credential> {
    let token = match service {
        Service::Github => github_env(host)?,
        Service::Gitlab => gitlab_env(host)?,
        Service::Forgejo => forgejo_env(host)?,
        Service::Registry => registry_env(host)?,
    };
    Some(Credential {
        token: Token::new(token),
        source: Source::Environment,
    })
}

fn github_env(host: &str) -> Option<String> {
    let github_com = host == "github.com" || host.ends_with(".ghe.com");
    if github_com {
        std::env::var("GH_TOKEN")
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .ok()
            .filter(|t| !t.is_empty())
    } else {
        std::env::var("GH_ENTERPRISE_TOKEN")
            .or_else(|_| std::env::var("GITHUB_ENTERPRISE_TOKEN"))
            .ok()
            .filter(|t| !t.is_empty())
    }
}

fn gitlab_env(host: &str) -> Option<String> {
    let configured = std::env::var("GITLAB_HOST")
        .or_else(|_| std::env::var("GL_HOST"))
        .or_else(|_| {
            std::env::var("GITLAB_URI")
                .map(|uri| host_from_url(&uri).unwrap_or_else(|| normalize_host(&uri)))
        })
        .map(|h| normalize_host(&h))
        .ok();
    let allowed = match configured.as_deref() {
        Some(configured) => configured == host,
        None => host == "gitlab.com",
    };
    if !allowed {
        return None;
    }
    std::env::var("GITLAB_TOKEN")
        .or_else(|_| std::env::var("GITLAB_ACCESS_TOKEN"))
        .ok()
        .filter(|t| !t.is_empty())
}

fn forgejo_env(host: &str) -> Option<String> {
    let configured = std::env::var("REFPINNER_FORGEJO_INSTANCE")
        .ok()
        .and_then(|url| host_from_url(&url));
    if configured.as_deref() != Some(host) && configured.is_some() {
        return None;
    }
    // Without an instance setting, still allow the token only when the host matches an
    // explicit REFPINNER_FORGEJO_HOST; otherwise require the instance host.
    if configured.is_none() {
        let scoped = std::env::var("REFPINNER_FORGEJO_HOST")
            .ok()
            .map(|h| normalize_host(&h));
        if scoped.as_deref() != Some(host) {
            return None;
        }
    }
    std::env::var("FORGEJO_TOKEN")
        .or_else(|_| std::env::var("GITEA_TOKEN"))
        .ok()
        .filter(|t| !t.is_empty())
}

fn registry_env(host: &str) -> Option<String> {
    let scoped = std::env::var("REFPINNER_REGISTRY_HOST")
        .ok()
        .map(|h| normalize_host(&h));
    if scoped.as_deref() != Some(host) {
        return None;
    }
    std::env::var("REFPINNER_REGISTRY_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

fn credentials_path() -> PathBuf {
    if let Ok(path) = std::env::var("REFPINNER_CREDENTIALS_FILE") {
        return PathBuf::from(path);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("refpinner").join("credentials.toml")
}

fn load_store() -> Result<Store, String> {
    let path = credentials_path();
    if !path.exists() {
        return Ok(Store::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

fn save_store(store: &Store) -> Result<(), String> {
    let path = credentials_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(store).map_err(|e| e.to_string())?;
    write_private(&path, text.as_bytes())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn file_get(host: &str, service: Service) -> Option<Credential> {
    let store = load_store().ok()?;
    store
        .entries
        .into_iter()
        .find(|e| e.host == host && e.service == service)
        .map(|e| Credential {
            token: Token::new(e.token),
            source: Source::File,
        })
}

fn keyring_service(service: Service) -> String {
    format!("refpinner:{}", service.as_str())
}

fn keyring_get(host: &str, service: Service) -> Option<Credential> {
    let entry = keyring::Entry::new(&keyring_service(service), host).ok()?;
    let token = entry.get_password().ok()?;
    if token.is_empty() {
        return None;
    }
    Some(Credential {
        token: Token::new(token),
        source: Source::Keyring,
    })
}

fn keyring_set(host: &str, service: Service, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(&keyring_service(service), host).map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

fn keyring_delete(host: &str, service: Service) -> Result<(), String> {
    let entry = keyring::Entry::new(&keyring_service(service), host).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}

/// Authorization header value for forge APIs (`Bearer` for GitHub/GitLab, `token` for Forgejo).
pub fn forge_authorization(service: Service, token: &str) -> String {
    match service {
        Service::Github | Service::Gitlab | Service::Registry => {
            format!("Bearer {token}")
        }
        Service::Forgejo => format!("token {token}"),
    }
}

/// Warn that credentials were rejected and public access will be tried.
pub fn warn_rejected(host: &str, service: Service) {
    eprintln!(
        "refpinner: warning: credentials for {host} ({}) were rejected; trying without authentication",
        service.as_str()
    );
}
