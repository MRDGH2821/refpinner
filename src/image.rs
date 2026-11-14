use saphyr::{LoadableYamlNode, MarkedYaml, ScanError};

use crate::github::{byte_offset, value_span, warning, yaml_warnings};
use crate::reference::{Kind, Reference, State};

/// Builds the reference for the image named by the string `node`. Returns `None` when
/// `node` names no image: it is not a string, or it is empty, which disables a service.
///
/// Values with `${{ }}` expressions or `$` CI/CD variables are reported as warnings.
pub fn reference(source: &str, node: &MarkedYaml) -> Option<Reference> {
    let text = node.data.as_str().filter(|text| !text.is_empty())?;
    if text.contains('$') {
        let mut reference = warning(
            node.span.start,
            format!("unsupported dynamic reference {text}"),
        );
        reference.kind = Kind::Image;
        return Some(reference);
    }
    let pinned = text
        .rsplit_once('@')
        .is_some_and(|(_, digest)| is_digest(digest));
    Some(Reference {
        line: node.span.start.line(),
        column: node.span.start.col() + 1,
        text: text.to_owned(),
        state: if pinned {
            State::Pinned
        } else {
            State::Unpinned
        },
        span: value_span(source, byte_offset(source, node.span.start.index()), text),
        kind: Kind::Image,
        reusable: false,
    })
}

/// Extracts the job and service images in a GitLab CI file, sorted by position.
///
/// `image` is a string or a mapping with `name`; `services` lists strings or mappings with
/// `name`. Both are read at the top level (deprecated globals), under `default` and under
/// each job. YAML anchors and aliases are reported as warnings; aliased nodes are skipped.
pub fn gitlab(source: &str) -> Result<Vec<Reference>, ScanError> {
    let (mut references, aliases) = yaml_warnings(source)?;
    let aliased = |node: &MarkedYaml| aliases.contains(&node.span.start);
    fn name<'a>(node: &'a MarkedYaml<'a>) -> &'a MarkedYaml<'a> {
        node.data.as_mapping_get("name").unwrap_or(node)
    }
    let documents = MarkedYaml::load_from_str(source)?;
    for document in &documents {
        // `variables` maps names to values, so its `image` key is not an image.
        let jobs = document
            .data
            .as_mapping()
            .into_iter()
            .flatten()
            .filter(|(key, _)| key.data.as_str() != Some("variables"))
            .map(|(_, job)| job);
        for job in std::iter::once(document).chain(jobs) {
            if aliased(job) {
                continue;
            }
            let image = job
                .data
                .as_mapping_get("image")
                .filter(|i| !aliased(i))
                .map(name);
            let services = job
                .data
                .as_mapping_get("services")
                .filter(|s| !aliased(s))
                .and_then(|s| s.data.as_sequence())
                .into_iter()
                .flatten()
                .filter(|service| !aliased(service))
                .map(name);
            for node in image.into_iter().chain(services) {
                if !aliased(node) {
                    references.extend(reference(source, node));
                }
            }
        }
    }
    references.sort_by_key(|r| (r.line, r.column));
    Ok(references)
}

/// An image name split into the parts a registry request needs.
pub struct Name {
    /// Registry API host, such as `registry-1.docker.io` or `ghcr.io:443`.
    pub host: String,
    /// Repository path in the registry, such as `library/alpine`.
    pub repository: String,
    /// The tag, or `None` when the image omits it and `latest` applies.
    pub tag: Option<String>,
}

/// Parses `[docker://][host[:port]/]path[:tag]` with Docker's normalization: a first
/// segment is a host only if it holds `.` or `:` or is `localhost`, Docker Hub names gain
/// `library/`, and Docker Hub is reached at `registry-1.docker.io`.
pub fn parse(text: &str) -> Result<Name, String> {
    let name = text.strip_prefix("docker://").unwrap_or(text);
    let (path, tag) = match name.rsplit_once(':') {
        Some((path, tag)) if !tag.contains('/') => (path, Some(tag)),
        _ => (name, None),
    };
    let (host, repository) = match path.split_once('/') {
        Some((host, rest)) if host.contains(['.', ':']) || host == "localhost" => (host, rest),
        _ => ("docker.io", path),
    };
    let hub = matches!(host, "docker.io" | "index.docker.io");
    let repository = if hub && !repository.contains('/') {
        format!("library/{repository}")
    } else {
        repository.to_owned()
    };
    // These rules keep each part from changing the request URL.
    if !is_host(host) {
        return Err(format!("{host} is not a valid registry host"));
    }
    if !repository.split('/').all(is_path_component) {
        return Err(format!("{repository} is not a valid image repository"));
    }
    if let Some(tag) = tag.filter(|tag| !is_tag(tag)) {
        return Err(format!("{tag} is not a valid image tag"));
    }
    Ok(Name {
        host: if hub {
            String::from("registry-1.docker.io")
        } else {
            host.to_owned()
        },
        repository,
        tag: tag.map(str::to_owned),
    })
}

/// A `sha256:` digest with 64 lowercase hex characters.
pub fn is_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn is_host(host: &str) -> bool {
    let (name, port) = host.split_once(':').unwrap_or((host, "1"));
    !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && name.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// A repository path component per the distribution spec: lowercase alphanumerics joined
/// by `.`, `_` or `-`.
fn is_path_component(part: &str) -> bool {
    let lower_or_digit = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    part.bytes().next().is_some_and(lower_or_digit)
        && part.bytes().last().is_some_and(lower_or_digit)
        && part
            .bytes()
            .all(|b| lower_or_digit(b) || matches!(b, b'.' | b'_' | b'-'))
        && !part.contains("..")
}

fn is_tag(tag: &str) -> bool {
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    tag.len() <= 128
        && tag.bytes().next().is_some_and(word)
        && tag.bytes().all(|b| word(b) || matches!(b, b'.' | b'-'))
}
