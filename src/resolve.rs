use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use crate::reference::Reference;
use crate::settings::Actions;
use crate::version;

/// Routes each action to a host-specific API client; all forges share version and edit logic.
#[derive(Default)]
pub struct Resolver {
    remotes: HashMap<String, Remote>,
}

impl Resolver {
    pub fn action<'a>(
        &'a mut self,
        input: &crate::discover::Input,
        reference: &Reference,
        settings: &Actions,
    ) -> Result<(&'a mut Remote, String), String> {
        let action = reference
            .text
            .rsplit_once('@')
            .ok_or("reference has no @revision")?
            .0;
        let url = if action.contains("://") {
            action.to_owned()
        } else {
            format!(
                "{}/{}",
                Actions {
                    forgejo_instance: settings
                        .forgejo_instance
                        .clone()
                        .or_else(|| input.settings.forgejo_instance.clone()),
                    forgejo_action_source: settings
                        .forgejo_action_source
                        .clone()
                        .or_else(|| input.settings.forgejo_action_source.clone()),
                }
                .source_for(
                    match input.platform {
                        crate::config::Platform::Forgejo => ".forgejo",
                        crate::config::Platform::Gitea => ".gitea",
                        _ => ".github",
                    },
                    reference.reusable
                )?
                .trim_end_matches('/'),
                action
            )
        };
        let uri: ureq::http::Uri = url.parse().map_err(|_| "invalid action source URL")?;
        if !matches!(uri.scheme_str(), Some("http" | "https"))
            || uri.authority().is_none_or(|a| a.as_str().contains('@'))
            || uri.query().is_some()
            || url.contains('#')
        {
            return Err(String::from(
                "action source must be an HTTP(S) URL without credentials, query, or fragment",
            ));
        }
        let authority = uri.authority().ok_or("action source URL has no host")?;
        let repo = uri.path().trim_start_matches('/').to_owned();
        repository(&repo)?;
        let github = uri
            .host()
            .is_some_and(|host| host.eq_ignore_ascii_case("github.com"));
        let base = if github {
            std::env::var(API_URL_VAR).unwrap_or_else(|_| DEFAULT_API_URL.to_owned())
        } else {
            format!(
                "{}://{authority}/api/v1",
                uri.scheme_str().unwrap_or("https")
            )
        };
        let key = format!("{github}:{base}");
        let remote = self
            .remotes
            .entry(key)
            .or_insert_with(|| Remote::new(&base, github));
        Ok((remote, repo))
    }
}

/// Environment variable that overrides the GitHub REST API base URL, for GHES or tests.
const API_URL_VAR: &str = "REFPINNER_GITHUB_API_URL";
const DEFAULT_API_URL: &str = "https://api.github.com";
/// Git allows tags of tags; stop peeling after this many hops.
const MAX_PEEL: usize = 5;
/// Candidate exact-version tags checked per floating tag before giving up on an exact version.
const MAX_CANDIDATES: usize = 10;

/// The commit a Git reference points to now.
#[derive(Clone)]
pub struct Resolution {
    /// The 40-character commit ID.
    pub commit: String,
    /// The most specific version tag at `commit`, when one is known.
    pub version: Option<String>,
}

/// Resolves GitHub and Forgejo refs through one shared engine, once per repository and revision.
pub struct Remote {
    base: String,
    github: bool,
    agent: ureq::Agent,
    cache: HashMap<String, Result<Resolution, String>>,
    /// Responses of listing requests, by API path.
    listings: HashMap<String, Result<Option<Value>, String>>,
}

impl Remote {
    fn new(base: &str, github: bool) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // Never let an API redirect choose a different scheme or host.
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_owned(),
            github,
            agent,
            cache: HashMap::new(),
            listings: HashMap::new(),
        }
    }

    /// Resolves `revision` of `action` (`owner/repo[/path]`) to a commit.
    ///
    /// Tags take precedence over branches, as they do for GitHub reusable workflows.
    pub fn resolve(&mut self, action: &str, revision: &str) -> Result<Resolution, String> {
        let repo = repository(action)?;
        let key = format!("{repo}@{revision}");
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let resolved = self.lookup(&repo, revision);
        self.cache.insert(key, resolved.clone());
        resolved
    }

    /// Finds the exact version tag at `commit` for a floating version tag like `v4`.
    pub fn exact(&self, action: &str, track: &str, commit: &str) -> Result<Option<String>, String> {
        self.exact_version(&repository(action)?, track, commit)
    }

    /// Lists the names of the tags of `action` that start with `prefix`.
    pub fn tags(&mut self, action: &str, prefix: &str) -> Result<Vec<String>, String> {
        let path = self.refs_path(
            &repository(action)?,
            &format!("tags/{}", encode(prefix)),
            true,
        );
        let refs = self.cached(&path)?.unwrap_or(Value::Null);
        Ok(refs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry["ref"].as_str()?.strip_prefix("refs/tags/"))
            .map(str::to_owned)
            .collect())
    }

    /// Returns the name of the default branch of `action`'s repository.
    pub fn default_branch(&mut self, action: &str) -> Result<String, String> {
        let repo = repository(action)?;
        self.cached(&format!("/repos/{repo}"))?
            .and_then(|body| body["default_branch"].as_str().map(str::to_owned))
            .ok_or_else(|| format!("cannot find the default branch of {repo}"))
    }

    /// [`Self::get`] once per path and run.
    fn cached(&mut self, path: &str) -> Result<Option<Value>, String> {
        if let Some(cached) = self.listings.get(path) {
            return cached.clone();
        }
        let response = self.get(path);
        self.listings.insert(path.to_owned(), response.clone());
        response
    }

    fn lookup(&self, repo: &str, revision: &str) -> Result<Resolution, String> {
        // Git ref names cannot hold these, and they would change the API path.
        if revision.is_empty() || revision.contains("..") || revision.starts_with('/') {
            return Err(format!("{revision} is not a valid Git ref name"));
        }
        let encoded = encode(revision);
        if let Some(tag) = self.exact_ref(
            repo,
            &format!("tags/{encoded}"),
            &format!("refs/tags/{revision}"),
        )? {
            let commit = self.peel(repo, &tag["object"])?;
            let version = self.exact_version(repo, revision, &commit)?;
            return Ok(Resolution { commit, version });
        }
        if let Some(branch) = self.exact_ref(
            repo,
            &format!("heads/{encoded}"),
            &format!("refs/heads/{revision}"),
        )? {
            let commit = self.peel(repo, &branch["object"])?;
            return Ok(Resolution {
                commit,
                version: None,
            });
        }
        Err(format!("no tag or branch named {revision} in {repo}"))
    }

    /// Finds the most specific version tag at `commit` for a version tag like `v4` or `v4.2`.
    fn exact_version(&self, repo: &str, tag: &str, commit: &str) -> Result<Option<String>, String> {
        let Some(track) = version::parse(tag) else {
            return Ok(None);
        };
        if track.core.len() >= 3 || track.is_prerelease() {
            return Ok(Some(tag.to_owned()));
        }
        let path = self.refs_path(repo, &format!("tags/{}.", encode(tag)), true);
        let refs = self.get(&path)?.unwrap_or(Value::Null);
        let mut candidates: Vec<(version::Version, &str, &Value)> = refs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let name = entry["ref"].as_str()?.strip_prefix("refs/tags/")?;
                let parsed = version::parse(name)?;
                (parsed.core.len() > track.core.len() && parsed.core.starts_with(&track.core))
                    .then_some((parsed, name, &entry["object"]))
            })
            .collect();
        // Highest first; a longer version sorts above its own prefix, so `v4.2.0` beats `v4.2`.
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, name, object) in candidates.into_iter().take(MAX_CANDIDATES) {
            if self.peel(repo, object)? == commit {
                return Ok(Some(name.to_owned()));
            }
        }
        Ok(None)
    }

    fn refs_path(&self, repo: &str, revision: &str, matching: bool) -> String {
        let endpoint = match (self.github, matching) {
            (true, true) => "matching-refs",
            (true, false) => "ref",
            (false, _) => "refs",
        };
        format!("/repos/{repo}/git/{endpoint}/{revision}")
    }

    /// Forgejo returns prefix matches, even for an exact lookup.
    fn exact_ref(&self, repo: &str, revision: &str, name: &str) -> Result<Option<Value>, String> {
        let response = self.get(&self.refs_path(repo, revision, false))?;
        if self.github {
            Ok(response)
        } else {
            Ok(response.and_then(|body| {
                body.as_array()?
                    .iter()
                    .find(|entry| entry["ref"].as_str() == Some(name))
                    .cloned()
            }))
        }
    }

    fn service(&self) -> &str {
        if self.github { "GitHub" } else { "Forgejo" }
    }

    /// Follows annotated tag objects until it reaches a commit.
    fn peel(&self, repo: &str, object: &Value) -> Result<String, String> {
        let mut object = object.clone();
        for _ in 0..MAX_PEEL {
            let sha = object["sha"]
                .as_str()
                .filter(|sha| is_commit_id(sha))
                .ok_or_else(|| {
                    format!(
                        "{} returned a reference without a valid object ID",
                        self.service()
                    )
                })?;
            match object["type"].as_str() {
                Some("commit") => return Ok(sha.to_owned()),
                Some("tag") => {
                    object = self
                        .get(&format!("/repos/{repo}/git/tags/{sha}"))?
                        .ok_or_else(|| format!("tag object {sha} not found in {repo}"))?["object"]
                        .clone();
                }
                _ => return Err(format!("{repo} reference does not point to a commit")),
            }
        }
        Err(format!("{repo} tag nests more than {MAX_PEEL} levels"))
    }

    /// Returns the JSON body, or `None` for 404.
    fn get(&self, path: &str) -> Result<Option<Value>, String> {
        let url = format!("{}{path}", self.base);
        let service = self.service();
        let mut request = self.agent.get(&url).header("Accept", "application/json");
        if self.github {
            request = request
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");
        }
        let mut response = request
            .call()
            .map_err(|error| format!("cannot reach {service} API: {error}"))?;
        match response.status().as_u16() {
            200 => {}
            404 => return Ok(None),
            status => return Err(format!("{service} API returned HTTP {status} for {path}")),
        }
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| format!("cannot read {service} API response: {error}"))?;
        serde_json::from_str(&body)
            .map(Some)
            .map_err(|error| format!("invalid {service} API response for {path}: {error}"))
    }
}

/// Returns `owner/repo` from `owner/repo[/path]`.
fn repository(action: &str) -> Result<String, String> {
    let mut parts = action.split('/');
    match (parts.next(), parts.next()) {
        (Some(owner), Some(repo)) if [owner, repo].iter().all(|part| is_name(part)) => {
            Ok(format!("{owner}/{repo}"))
        }
        _ => Err(format!("{action} is not an owner/repo action")),
    }
}

fn is_name(part: &str) -> bool {
    // `.` and `..` would change the API path.
    !part.bytes().all(|b| b == b'.')
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn is_commit_id(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Percent-encodes a revision for a URL path, keeping `/` so branch names like `release/v1` work.
fn encode(revision: &str) -> String {
    let mut encoded = String::new();
    for b in revision.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            encoded.push(char::from(b));
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    encoded
}
