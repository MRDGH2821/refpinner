use std::collections::HashMap;
use std::time::Duration;

use crate::image::{Name, is_digest};

/// Environment variable that sends every registry request to one base URL, for tests or a
/// pull-through mirror. Paths stay `/v2/<repository>/...`.
const URL_VAR: &str = "REFPINNER_REGISTRY_URL";

/// Index types come first so a multi-platform image resolves to its index digest, not to
/// the manifest of one platform.
const ACCEPT: &str = concat!(
    "application/vnd.oci.image.index.v1+json, ",
    "application/vnd.docker.distribution.manifest.list.v2+json, ",
    "application/vnd.oci.image.manifest.v1+json, ",
    "application/vnd.docker.distribution.manifest.v2+json",
);

/// Resolves image tags to manifest digests through the OCI distribution API, once per
/// registry, repository and tag.
pub struct Registry {
    base: Option<String>,
    agent: ureq::Agent,
    cache: HashMap<String, Result<String, String>>,
}

impl Registry {
    /// Uses `REFPINNER_REGISTRY_URL` for every registry when set, otherwise
    /// `https://<registry host>`.
    pub fn from_env() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // Never follow redirects with Authorization; blob CDNs are out of scope here.
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            base: std::env::var(URL_VAR)
                .ok()
                .map(|url| url.trim_end_matches('/').to_owned()),
            agent,
            cache: HashMap::new(),
        }
    }

    /// Returns the `sha256:` digest the tag of `name` (`latest` when omitted) points to now.
    pub fn resolve(&mut self, name: &Name) -> Result<String, String> {
        let tag = name.tag.as_deref().unwrap_or("latest");
        let key = format!("{}/{}:{tag}", name.host, name.repository);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let resolved = self.lookup(name, tag);
        self.cache.insert(key, resolved.clone());
        resolved
    }

    /// Lists every tag of `name`'s repository, following pagination.
    pub fn tags(&mut self, name: &Name) -> Result<Vec<String>, String> {
        let key = format!("{}/{}:tags", name.host, name.repository);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone().map(|body| {
                serde_json::from_str::<TagList>(&body)
                    .map(|list| list.tags)
                    .unwrap_or_default()
            });
        }
        let mut tags = Vec::new();
        let mut url = {
            let base = self
                .base
                .clone()
                .unwrap_or_else(|| format!("https://{}", name.host));
            format!("{base}/v2/{}/tags/list?n=1000", name.repository)
        };
        let mut token = None;
        loop {
            let mut response = self.get(&url, token.as_deref())?;
            if response.status().as_u16() == 401 && token.is_none() {
                let challenge = header(&response, "www-authenticate").unwrap_or_default();
                token = Some(self.token(&challenge, &name.host)?);
                response = self.get(&url, token.as_deref())?;
            }
            match response.status().as_u16() {
                200 => {}
                status => {
                    return Err(format!(
                        "registry {} returned HTTP {status} listing tags of {}",
                        name.host, name.repository
                    ));
                }
            }
            let link = header(&response, "link");
            let body = response
                .body_mut()
                .read_to_string()
                .map_err(|error| format!("cannot read tag list of {}: {error}", name.repository))?;
            let page: TagList = serde_json::from_str(&body)
                .map_err(|error| format!("invalid tag list for {}: {error}", name.repository))?;
            tags.extend(page.tags);
            let Some(next) = link.as_deref().and_then(next_link) else {
                break;
            };
            url = if next.starts_with("http://") || next.starts_with("https://") {
                next.to_owned()
            } else {
                let base = self
                    .base
                    .clone()
                    .unwrap_or_else(|| format!("https://{}", name.host));
                format!("{base}{next}")
            };
        }
        self.cache.insert(
            key,
            Ok(serde_json::to_string(&TagList { tags: tags.clone() }).unwrap()),
        );
        Ok(tags)
    }

    fn get(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<ureq::http::Response<ureq::Body>, String> {
        let request = self.agent.get(url);
        let request = match token {
            Some(token) => request.header("Authorization", format!("Bearer {token}")),
            None => request,
        };
        request
            .call()
            .map_err(|error| format!("cannot reach registry: {error}"))
    }

    fn lookup(&self, name: &Name, tag: &str) -> Result<String, String> {
        let base = self
            .base
            .clone()
            .unwrap_or_else(|| format!("https://{}", name.host));
        let url = format!("{base}/v2/{}/manifests/{tag}", name.repository);
        let mut token = None;
        let mut response = self.manifest(&url, true, None)?;
        if response.status().as_u16() == 401 {
            let challenge = header(&response, "www-authenticate").unwrap_or_default();
            token = Some(self.token(&challenge, &name.host)?);
            response = self.manifest(&url, true, token.as_deref())?;
        }
        let repository = &name.repository;
        match response.status().as_u16() {
            200 => {}
            404 => return Err(format!("no tag {tag} in {} image {repository}", name.host)),
            status => {
                return Err(format!(
                    "registry {} returned HTTP {status} for {repository}:{tag}",
                    name.host
                ));
            }
        }
        if let Some(digest) = header(&response, "docker-content-digest").filter(|d| is_digest(d)) {
            return Ok(digest);
        }
        // The header is optional in older registries; the digest is the hash of the exact bytes.
        let mut response = self.manifest(&url, false, token.as_deref())?;
        if response.status().as_u16() != 200 {
            return Err(format!(
                "registry {} returned HTTP {} for {repository}:{tag}",
                name.host,
                response.status().as_u16()
            ));
        }
        let bytes = response
            .body_mut()
            .read_to_vec()
            .map_err(|error| format!("cannot read manifest of {repository}:{tag}: {error}"))?;
        let hash = ring::digest::digest(&ring::digest::SHA256, &bytes);
        let hex: String = hash.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        Ok(format!("sha256:{hex}"))
    }

    /// Sends `HEAD` (no pull counted on Docker Hub) or `GET` for a manifest.
    fn manifest(
        &self,
        url: &str,
        head: bool,
        token: Option<&str>,
    ) -> Result<ureq::http::Response<ureq::Body>, String> {
        let request = if head {
            self.agent.head(url)
        } else {
            self.agent.get(url)
        };
        let request = request.header("Accept", ACCEPT);
        let request = match token {
            Some(token) => request.header("Authorization", format!("Bearer {token}")),
            None => request,
        };
        request
            .call()
            .map_err(|error| format!("cannot reach registry: {error}"))
    }

    /// Gets a Bearer token from the realm named by a challenge.
    ///
    /// Credentials are looked up by the **registry** host (`registry_host`), never by the
    /// realm host, and are sent only on this direct token request (redirects are disabled).
    fn token(&self, challenge: &str, registry_host: &str) -> Result<String, String> {
        let params = challenge
            .split_once(' ')
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, params)| parameters(params))
            .ok_or("registry requires authentication")?;
        let get = |key: &str| {
            params
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        let realm = get("realm")
            .filter(|realm| realm.starts_with("https://") || realm.starts_with("http://"))
            .ok_or("registry sent a token challenge without an HTTP realm")?;
        let mut url = realm.to_owned();
        for (key, value) in [("service", get("service")), ("scope", get("scope"))] {
            if let Some(value) = value {
                url.push(if url.contains('?') { '&' } else { '?' });
                url.push_str(&format!("{key}={}", encode(value)));
            }
        }
        let cred = (!registry_host.is_empty())
            .then(|| crate::credentials::find(registry_host, crate::credentials::Service::Registry))
            .flatten();
        let mut request = self.agent.get(&url);
        if let Some(cred) = &cred {
            request = request.header("Authorization", basic_auth(cred.token.expose()));
        }
        let mut response = request
            .call()
            .map_err(|error| format!("cannot reach registry token service: {error}"))?;
        if response.status().as_u16() == 401 && cred.is_some() {
            crate::credentials::warn_rejected(registry_host, crate::credentials::Service::Registry);
            response = self
                .agent
                .get(&url)
                .call()
                .map_err(|error| format!("cannot reach registry token service: {error}"))?;
        }
        if response.status().as_u16() != 200 {
            return Err(format!(
                "registry token service returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| format!("cannot read registry token: {error}"))?;
        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|error| format!("invalid registry token response: {error}"))?;
        json["token"]
            .as_str()
            .or(json["access_token"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| String::from("registry token response has no token"))
    }
}

/// Basic auth for a registry token request. `user:pass` or a bare token (empty user).
fn basic_auth(secret: &str) -> String {
    let raw = if secret.contains(':') {
        secret.to_owned()
    } else {
        format!(":{secret}")
    };
    format!("Basic {}", base64_encode(raw.as_bytes()))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Parses the `key="value"` pairs of an RFC 6750 challenge; values hold no quotes.
fn parameters(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut rest = text.trim();
    while let Some((key, after)) = rest.split_once('=') {
        let key = key.trim().trim_start_matches(',').trim().to_owned();
        let (value, tail) = match after.strip_prefix('"') {
            Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
            None => after.split_once(',').unwrap_or((after, "")),
        };
        pairs.push((key, value.to_owned()));
        rest = tail.trim_start_matches(',').trim();
    }
    pairs
}

#[derive(serde::Deserialize, serde::Serialize)]
struct TagList {
    #[serde(default)]
    tags: Vec<String>,
}

/// Reads the `rel="next"` target from an RFC 5988 `Link` header.
fn next_link(header: &str) -> Option<&str> {
    header.split(',').find_map(|part| {
        let (url, params) = part.trim().split_once('>')?;
        let url = url.trim().strip_prefix('<')?;
        params
            .split(';')
            .any(|param| param.trim() == "rel=\"next\"")
            .then_some(url)
    })
}

/// Percent-encodes a query value; `:` and `/` are valid in a query (RFC 3986) and stay readable.
fn encode(value: &str) -> String {
    let mut encoded = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b':' | b'/') {
            encoded.push(char::from(b));
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    encoded
}
