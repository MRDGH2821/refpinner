use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::image::TagParts;
use crate::version;

const NAMES: [&str; 5] = [
    "refpinner.toml",
    ".refpinner.toml",
    ".config/refpinner.toml",
    ".config/.refpinner.toml",
    ".config/refpinner/config.toml",
];

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    extends: Vec<PathBuf>,
    pub exclude: Option<Vec<String>>,
    pub files: Option<Vec<File>>,
    pub version_rules: Option<Vec<VersionRule>>,
    pub platforms: Option<Vec<Platform>>,
    pub suppress: Option<Vec<String>>,
    pub no_symlink_warnings: Option<bool>,
    pub references: Option<Vec<ReferenceRule>>,
    pub update_limit: Option<crate::version::Limit>,
    pub forgejo_instance: Option<String>,
    pub forgejo_action_source: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceRule {
    pattern: String,
    exclude: Option<bool>,
    suppress: Option<bool>,
    update_limit: Option<crate::version::Limit>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRule {
    pattern: String,
    glob: Option<String>,
    regex: Option<String>,
}

impl VersionRule {
    /// Parses `tag` with this rule's glob or regex named components.
    pub fn parse(&self, tag: &str) -> Option<TagParts> {
        let captures = if let Some(regex) = &self.regex {
            let compiled = regex::Regex::new(regex).ok()?;
            let caps = compiled.captures(tag)?;
            let mut pairs = Vec::new();
            for name in compiled.capture_names().flatten() {
                if let Some(value) = caps.name(name) {
                    pairs.push((name.to_owned(), value.as_str().to_owned()));
                }
            }
            pairs
        } else if let Some(glob) = &self.glob {
            named_glob_captures(glob, tag)?
        } else {
            return None;
        };
        parts_from_captures(&captures)
    }
}

/// Converts `{name}` placeholders into captures; other glob syntax becomes a regex.
fn named_glob_captures(glob: &str, tag: &str) -> Option<Vec<(String, String)>> {
    let mut pattern = String::from("^");
    let mut names = Vec::new();
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
                if name.is_empty() {
                    return None;
                }
                names.push(name.clone());
                pattern.push_str(&format!("(?P<{name}>[^/]+)"));
            }
            '*' => pattern.push_str(".*"),
            '?' => pattern.push('.'),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '[' | ']' | '\\' => {
                pattern.push('\\');
                pattern.push(c);
            }
            c => pattern.push(c),
        }
    }
    pattern.push('$');
    let compiled = regex::Regex::new(&pattern).ok()?;
    let caps = compiled.captures(tag)?;
    Some(
        names
            .into_iter()
            .filter_map(|name| {
                caps.name(&name)
                    .map(|value| (name, value.as_str().to_owned()))
            })
            .collect(),
    )
}

fn parts_from_captures(captures: &[(String, String)]) -> Option<TagParts> {
    let mut major = None;
    let mut minor = None;
    let mut patch = None;
    let mut pre = Vec::new();
    let mut components = Vec::new();
    let mut suffix = String::new();
    for (name, value) in captures {
        match name.as_str() {
            "major" => major = Some(value.parse().ok()?),
            "minor" => minor = Some(value.parse().ok()?),
            "patch" => patch = Some(value.parse().ok()?),
            "prerelease" => {
                pre = value
                    .split('.')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            "suffix" => suffix = value.clone(),
            _ => {
                if let Some(version) = version::parse(value) {
                    components.push((name.clone(), version));
                } else {
                    suffix = value.clone();
                }
            }
        }
    }
    let major = major?;
    let mut core = vec![major];
    if let Some(minor) = minor {
        core.push(minor);
        if let Some(patch) = patch {
            core.push(patch);
        }
    } else if patch.is_some() {
        return None;
    }
    Some(TagParts {
        version: version::Version { core, pre },
        suffix,
        components,
    })
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: PathBuf,
    pub platform: Platform,
}

#[derive(Clone, Copy, Deserialize, clap::ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Github,
    Forgejo,
    Gitea,
    Gitlab,
}

impl Settings {
    fn merge(&mut self, child: Self) {
        if child.exclude.is_some() {
            self.exclude = child.exclude;
        }
        if child.files.is_some() {
            self.files = child.files;
        }
        if child.version_rules.is_some() {
            self.version_rules = child.version_rules;
        }
        if child.platforms.is_some() {
            self.platforms = child.platforms;
        }
        if child.suppress.is_some() {
            self.suppress = child.suppress;
        }
        if child.no_symlink_warnings.is_some() {
            self.no_symlink_warnings = child.no_symlink_warnings;
        }
        if child.references.is_some() {
            self.references = child.references;
        }
        if child.update_limit.is_some() {
            self.update_limit = child.update_limit;
        }
        if child.forgejo_instance.is_some() {
            self.forgejo_instance = child.forgejo_instance;
        }
        if child.forgejo_action_source.is_some() {
            self.forgejo_action_source = child.forgejo_action_source;
        }
    }

    pub fn limit(&self, reference: &crate::reference::Reference) -> Option<crate::version::Limit> {
        self.rule(reference)
            .and_then(|rule| rule.update_limit)
            .or(self.update_limit)
    }

    fn rule(&self, reference: &crate::reference::Reference) -> Option<&ReferenceRule> {
        let name = reference
            .text
            .rsplit_once('@')
            .map_or(reference.text.as_str(), |(name, _)| name);
        self.references
            .as_deref()
            .unwrap_or_default()
            .iter()
            .rev()
            .find(|rule| matches(&rule.pattern, name))
    }

    pub fn includes(&self, reference: &crate::reference::Reference, audit: bool) -> bool {
        let rule = self.rule(reference);
        if reference.state == crate::reference::State::Warning {
            return !rule.and_then(|rule| rule.suppress).unwrap_or(false)
                && !self
                    .suppress
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|pattern| matches(pattern, &reference.text));
        }
        audit || !rule.and_then(|rule| rule.exclude).unwrap_or(false)
    }

    /// Last matching version rule for `reference`, if any. A matching rule replaces built-in
    /// tag interpretation even when the tag itself does not match the rule's pattern.
    pub fn version_rule(&self, reference: &str) -> Option<&VersionRule> {
        let name = reference_name(reference);
        self.version_rules
            .as_deref()
            .unwrap_or_default()
            .iter()
            .rev()
            .find(|rule| matches(&rule.pattern, name))
    }

    pub fn validate(&self, root: &Path) -> Result<(), String> {
        let mut excludes = ignore::overrides::OverrideBuilder::new(root);
        for pattern in self.exclude.as_deref().unwrap_or_default() {
            excludes
                .add(&format!("!{pattern}"))
                .map_err(|_| "invalid exclusion pattern")?;
        }
        for pattern in self.suppress.as_deref().unwrap_or_default().iter().chain(
            self.references
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|rule| &rule.pattern),
        ) {
            globset::Glob::new(pattern).map_err(|_| "invalid reference or suppression pattern")?;
        }
        let named = regex::Regex::new(r"\{[A-Za-z_][A-Za-z0-9_]*\}").unwrap();
        for rule in self.version_rules.as_deref().unwrap_or_default() {
            globset::Glob::new(&rule.pattern).map_err(|_| "invalid reference pattern")?;
            if rule.glob.is_some() == rule.regex.is_some() {
                return Err("version pattern needs exactly one glob or regex".into());
            }
            if let Some(glob) = &rule.glob {
                let stripped = named.replace_all(glob, "*");
                globset::Glob::new(&stripped).map_err(|_| "invalid version glob pattern")?;
            }
            if let Some(regex) = &rule.regex {
                let compiled =
                    regex::Regex::new(regex).map_err(|_| "invalid version regex pattern")?;
                if !compiled
                    .capture_names()
                    .flatten()
                    .any(|name| name == "major")
                {
                    return Err("version regex pattern needs a named major component".into());
                }
            }
        }
        Ok(())
    }
}

fn matches(pattern: &str, text: &str) -> bool {
    globset::Glob::new(pattern).is_ok_and(|glob| glob.compile_matcher().is_match(text))
}

/// Image or action name without digest or tag, for version rule matching.
fn reference_name(reference: &str) -> &str {
    let name = reference
        .rsplit_once('@')
        .map_or(reference, |(name, _)| name);
    let name = name.strip_prefix("docker://").unwrap_or(name);
    match name.rsplit_once(':') {
        Some((path, tag)) if !tag.contains('/') => path,
        _ => name,
    }
}

pub fn load(root: &Path, explicit: Option<&Path>) -> Result<Settings, String> {
    let selected = explicit.map(Path::to_path_buf).or_else(|| {
        NAMES
            .iter()
            .map(|name| root.join(name))
            .find(|path| path.exists())
    });
    match selected {
        Some(path) => read(&path, &mut Vec::new()),
        None => Ok(Settings::default()),
    }
}

fn read(path: &Path, stack: &mut Vec<PathBuf>) -> Result<Settings, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("cannot read config {}: {error}", path.display()))?;
    if stack.contains(&canonical) {
        return Err(format!("config inheritance cycle at {}", path.display()));
    }
    stack.push(canonical);
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read config {}: {error}", path.display()))?;
    // Parser errors can quote configuration values, including credentials.
    let mut child: Settings =
        toml::from_str(&source).map_err(|_| format!("invalid config {}", path.display()))?;
    let folder = path.parent().unwrap_or(Path::new("."));
    child
        .validate(folder)
        .map_err(|error| format!("config {}: {error}", path.display()))?;
    let mut result = Settings::default();
    for parent in &child.extends {
        result.merge(read(&folder.join(parent), stack)?);
    }
    if let Some(files) = &mut child.files {
        for file in files {
            file.path = folder.join(&file.path);
        }
    }
    result.merge(child);
    stack.pop();
    Ok(result)
}

#[derive(clap::Args)]
pub struct Options {
    /// Read this TOML configuration instead of searching the project
    #[arg(long, env = "REFPINNER_CONFIG", global = true)]
    pub config: Option<PathBuf>,
}
