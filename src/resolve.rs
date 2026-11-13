use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

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

/// Resolves GitHub action references through the REST API, once per repository and revision.
pub struct GitHub {
    base: String,
    agent: ureq::Agent,
    cache: HashMap<String, Result<Resolution, String>>,
}

impl GitHub {
    /// Uses `REFPINNER_GITHUB_API_URL` when set, otherwise `https://api.github.com`.
    pub fn from_env() -> Self {
        let base = std::env::var(API_URL_VAR).unwrap_or_else(|_| DEFAULT_API_URL.to_owned());
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_owned(),
            agent,
            cache: HashMap::new(),
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

    fn lookup(&self, repo: &str, revision: &str) -> Result<Resolution, String> {
        // Git ref names cannot hold these, and they would change the API path.
        if revision.is_empty() || revision.contains("..") || revision.starts_with('/') {
            return Err(format!("{revision} is not a valid Git ref name"));
        }
        let encoded = encode(revision);
        if let Some(tag) = self.get(&format!("/repos/{repo}/git/ref/tags/{encoded}"))? {
            let commit = self.peel(repo, &tag["object"])?;
            let version = self.exact_version(repo, revision, &commit)?;
            return Ok(Resolution { commit, version });
        }
        if let Some(branch) = self.get(&format!("/repos/{repo}/git/ref/heads/{encoded}"))? {
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
        let Some(track) = version(tag) else {
            return Ok(None);
        };
        if track.len() >= 3 {
            return Ok(Some(tag.to_owned()));
        }
        let path = format!("/repos/{repo}/git/matching-refs/tags/{}.", encode(tag));
        let refs = self.get(&path)?.unwrap_or(Value::Null);
        let mut candidates: Vec<(Vec<u64>, &str, &Value)> = refs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let name = entry["ref"].as_str()?.strip_prefix("refs/tags/")?;
                let parsed = version(name)?;
                (parsed.len() > track.len() && parsed.starts_with(&track)).then_some((
                    parsed,
                    name,
                    &entry["object"],
                ))
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

    /// Follows annotated tag objects until it reaches a commit.
    fn peel(&self, repo: &str, object: &Value) -> Result<String, String> {
        let mut object = object.clone();
        for _ in 0..MAX_PEEL {
            let sha = object["sha"]
                .as_str()
                .filter(|sha| is_commit_id(sha))
                .ok_or("GitHub returned a reference without a valid object ID")?;
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
        let mut response = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|error| format!("cannot reach GitHub API: {error}"))?;
        match response.status().as_u16() {
            200 => {}
            404 => return Ok(None),
            status => return Err(format!("GitHub API returned HTTP {status} for {path}")),
        }
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| format!("cannot read GitHub API response: {error}"))?;
        serde_json::from_str(&body)
            .map(Some)
            .map_err(|error| format!("invalid GitHub API response for {path}: {error}"))
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

/// Parses `v1`, `1.2` or `v1.2.3` into numeric components; anything else is not a version.
fn version(tag: &str) -> Option<Vec<u64>> {
    tag.strip_prefix('v')
        .unwrap_or(tag)
        .split('.')
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        })
        .collect()
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
