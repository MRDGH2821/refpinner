use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

/// Resolves GitLab include refs once per project and selection.
pub struct GitLab {
    base: String,
    agent: ureq::Agent,
    cache: HashMap<String, Result<Value, String>>,
}

impl GitLab {
    pub fn from_env() -> Self {
        Self {
            base: std::env::var("REFPINNER_GITLAB_API_URL")
                .unwrap_or_else(|_| "https://gitlab.com/api/v4".to_owned())
                .trim_end_matches('/')
                .to_owned(),
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(0)
                .timeout_global(Some(Duration::from_secs(30)))
                .build()
                .into(),
            cache: HashMap::new(),
        }
    }

    /// Returns the selected commit and original track, using the default branch when omitted.
    pub fn resolve(
        &mut self,
        project: &str,
        revision: Option<&str>,
    ) -> Result<(String, String), String> {
        if project.split('/').count() < 2
            || project
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("not a namespaced GitLab project".to_owned());
        }
        let path = format!("/projects/{}", encode(project));
        let track = match revision {
            Some(revision) if !revision.is_empty() => revision.to_owned(),
            Some(_) => return Err("empty GitLab ref".to_owned()),
            None => self.get(&path)?["default_branch"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or("GitLab returned no default branch")?
                .to_owned(),
        };
        let body = self.get(&format!(
            "{path}/repository/commits/{}?stats=false",
            encode(&track)
        ))?;
        let commit = body["id"]
            .as_str()
            .filter(|id| id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("GitLab returned no valid commit ID")?;
        Ok((commit.to_owned(), track))
    }

    fn get(&mut self, path: &str) -> Result<Value, String> {
        if let Some(cached) = self.cache.get(path) {
            return cached.clone();
        }
        let result = (|| {
            let mut response = self
                .agent
                .get(format!("{}{path}", self.base))
                .header("Accept", "application/json")
                .call()
                .map_err(|error| format!("cannot reach GitLab API: {error}"))?;
            if response.status().as_u16() != 200 {
                return Err(format!(
                    "GitLab API returned HTTP {} for {path}",
                    response.status().as_u16()
                ));
            }
            let body = response
                .body_mut()
                .read_to_string()
                .map_err(|error| format!("cannot read GitLab API response: {error}"))?;
            serde_json::from_str(&body)
                .map_err(|error| format!("invalid GitLab API response: {error}"))
        })();
        self.cache.insert(path.to_owned(), result.clone());
        result
    }
}

/// Encode a complete path parameter, including namespace and branch slashes.
fn encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
