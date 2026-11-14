use std::path::Path;

/// Forge action sources shared by CLI, environment, and project configuration.
#[derive(clap::Args, Clone, Default)]
pub struct Actions {
    /// Forgejo instance URL, used by reusable workflows and the `self` action source
    #[arg(long, global = true, env = "REFPINNER_FORGEJO_INSTANCE")]
    pub forgejo_instance: Option<String>,
    /// Action source: self, github, or a URL (Forgejo default: https://data.forgejo.org)
    #[arg(long, global = true, env = "REFPINNER_FORGEJO_ACTION_SOURCE")]
    pub forgejo_action_source: Option<String>,
}

impl Actions {
    /// Source of shorthand references; reusable workflows belong to the instance itself.
    pub fn source(&self, file: &Path, reusable: bool) -> Result<String, String> {
        let platform = platform(file);
        if platform == ".github" {
            return Ok(String::from("https://github.com"));
        }
        if reusable {
            return self.instance();
        }
        match self
            .forgejo_action_source
            .as_deref()
            .unwrap_or(if platform == ".gitea" {
                "github"
            } else {
                "https://data.forgejo.org"
            }) {
            "self" => self.instance(),
            "github" => Ok(String::from("https://github.com")),
            url => Ok(url.to_owned()),
        }
    }

    fn instance(&self) -> Result<String, String> {
        self.forgejo_instance.clone().ok_or_else(|| {
            String::from(
                "Forgejo instance is unknown; set --forgejo-instance or REFPINNER_FORGEJO_INSTANCE",
            )
        })
    }
}

/// Directory names take precedence; root action metadata uses the nearest project marker.
fn platform(file: &Path) -> &'static str {
    const MARKERS: [&str; 3] = [".forgejo", ".gitea", ".github"];
    for component in file.components().rev() {
        if let Some(marker) = MARKERS
            .iter()
            .find(|marker| component.as_os_str() == **marker)
        {
            return marker;
        }
    }
    for parent in file.ancestors().skip(1) {
        if let Some(marker) = MARKERS.iter().find(|marker| parent.join(marker).is_dir()) {
            return marker;
        }
        if parent.join(".git").exists() {
            break;
        }
    }
    ".github"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_defaults_follow_the_platform_not_the_instance_host() {
        let settings = Actions::default();
        assert_eq!(
            settings
                .source(Path::new(".forgejo/workflows/ci.yml"), false)
                .unwrap(),
            "https://data.forgejo.org"
        );
        assert_eq!(
            settings
                .source(Path::new(".gitea/workflows/ci.yml"), false)
                .unwrap(),
            "https://github.com"
        );
        assert_eq!(
            settings
                .source(Path::new(".github/workflows/ci.yml"), true)
                .unwrap(),
            "https://github.com"
        );
        assert!(
            settings
                .source(Path::new(".forgejo/workflows/ci.yml"), true)
                .is_err()
        );
        let settings = Actions {
            forgejo_instance: None,
            forgejo_action_source: Some("self".into()),
        };
        assert!(
            settings
                .source(Path::new(".forgejo/workflows/ci.yml"), false)
                .is_err()
        );
    }
}
