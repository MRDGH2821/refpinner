use std::cmp::Ordering;

/// A numeric version with optional SemVer prerelease identifiers. Metadata has no precedence.
#[derive(Debug, Eq, PartialEq)]
pub struct Version {
    pub core: Vec<u64>,
    pre: Vec<String>,
}

impl Version {
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => self
                    .pre
                    .iter()
                    .zip(&other.pre)
                    .map(|(a, b)| match (numeric(a), numeric(b)) {
                        (true, true) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (false, false) => a.cmp(b),
                    })
                    .find(|order| *order != Ordering::Equal)
                    .unwrap_or_else(|| self.pre.len().cmp(&other.pre.len())),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn numeric(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn identifiers(text: &str) -> Option<Vec<String>> {
    text.split('.')
        .map(|part| {
            (!part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
                .then(|| part.to_owned())
        })
        .collect()
}

/// Parses numeric tags, including abbreviated cores, prereleases and build metadata.
pub fn parse(tag: &str) -> Option<Version> {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    let tag = if let Some((version, metadata)) = tag.split_once('+') {
        identifiers(metadata)?;
        version
    } else {
        tag
    };
    let (core, pre) = match tag.split_once('-') {
        Some((core, pre)) => {
            let pre = identifiers(pre)?;
            if pre
                .iter()
                .any(|part| numeric(part) && part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            (core, pre)
        }
        None => (tag, Vec::new()),
    };
    let core = core
        .split('.')
        .map(|part| numeric(part).then(|| part.parse().ok()).flatten())
        .collect::<Option<Vec<u64>>>()?;
    Some(Version { core, pre })
}

/// Which release classes are eligible before comparing versions.
#[derive(Clone, Copy)]
pub enum Releases {
    Stable,
    Pre,
    Rc,
}

impl Releases {
    pub fn allows(self, current: &Version, candidate: &Version) -> bool {
        candidate.pre.is_empty()
            || match self {
                Self::Pre => true,
                Self::Stable => !current.pre.is_empty() && current.core == candidate.core,
                Self::Rc => match candidate.pre.as_slice() {
                    [label] => {
                        label.eq_ignore_ascii_case("rc")
                            || (label
                                .get(..2)
                                .is_some_and(|head| head.eq_ignore_ascii_case("rc"))
                                && label.get(2..).is_some_and(numeric))
                    }
                    [label, number] => label.eq_ignore_ascii_case("rc") && numeric(number),
                    _ => false,
                },
            }
    }
}

/// How far an update may move from the current version.
#[derive(Clone, Copy, PartialEq, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Limit {
    /// Any higher version.
    Major,
    /// A higher version with the same major.
    Minor,
    /// A higher version with the same major and minor.
    Patch,
}

impl Limit {
    /// Whether `candidate` is an upgrade from `current` within this limit.
    ///
    /// Both must have the same precision, so `v4.11` is never compared with `v4.10.0`. A
    /// current version too short for the limit (`v4` under `Patch`) allows nothing.
    pub fn allows(self, current: &Version, candidate: &Version) -> bool {
        let kept = match self {
            Limit::Major => 0,
            Limit::Minor => 1,
            Limit::Patch => 2,
        };
        kept <= current.core.len()
            && candidate.core.len() == current.core.len()
            && candidate > current
            && candidate.core[..kept] == current.core[..kept]
    }
}

#[cfg(test)]
mod tests {
    use super::{Limit, Releases, parse};

    #[test]
    fn malformed_versions_are_not_candidates() {
        for invalid in [
            "",
            "v",
            "1..2",
            "v1.2.",
            "v1.a",
            "v1.2.3-",
            "v1.2.3-rc..1",
            "v1.2.3-01",
            "v1.2.3-rc.01",
            "v1.2.3-rc!",
            "v1.2.3+",
            "v1.2.3+a+b",
            "v1.2.3+build..1",
            "v1.2.3-é",
            "18446744073709551616.0.0",
        ] {
            assert!(parse(invalid).is_none(), "{invalid}");
        }
        assert!(parse("v1.2-rc.1").unwrap().is_prerelease());
        assert!(
            parse("v1.2.3-rc.18446744073709551616").unwrap()
                > parse("v1.2.3-rc.18446744073709551615").unwrap()
        );
        assert!(parse("v1.2.3-1").unwrap() < parse("v1.2.3-alpha").unwrap());
    }

    #[test]
    fn release_eligibility_does_not_replace_bounds_or_precedence() {
        let current = parse("v4.2.1-beta.1").unwrap();
        for (tag, stable, pre, rc, patch) in [
            ("v4.2.1-beta.2", true, true, false, true),
            ("v4.2.1-rc.1", true, true, true, true),
            ("v4.2.1", true, true, true, true),
            ("v4.2.2-beta", false, true, false, true),
            ("v4.3.0-rc.1", false, true, true, false),
            ("v5.0.0", true, true, true, false),
            ("v4.2.1-alpha", true, true, false, false),
            ("v4.2-rc.2", false, true, true, false),
        ] {
            let candidate = parse(tag).unwrap();
            assert_eq!(
                Releases::Stable.allows(&current, &candidate),
                stable,
                "{tag}"
            );
            assert_eq!(Releases::Pre.allows(&current, &candidate), pre, "{tag}");
            assert_eq!(Releases::Rc.allows(&current, &candidate), rc, "{tag}");
            assert_eq!(Limit::Patch.allows(&current, &candidate), patch, "{tag}");
        }
    }

    #[test]
    fn limits_allow_only_upgrades_of_the_same_precision() {
        let current = parse("v4.2.1").unwrap();
        let allows = |limit: Limit, tag| limit.allows(&current, &parse(tag).unwrap());
        assert!(allows(Limit::Patch, "v4.2.10"));
        assert!(!allows(Limit::Patch, "v4.3.0"));
        assert!(allows(Limit::Minor, "v4.10.0"));
        assert!(!allows(Limit::Minor, "v5.0.0"));
        assert!(allows(Limit::Major, "v5.0.0"));
        assert!(!allows(Limit::Major, "v4.2.1"));
        assert!(!allows(Limit::Major, "v4.2.0"));
        assert!(!allows(Limit::Major, "v5"));
        assert!(!Limit::Patch.allows(&parse("v4").unwrap(), &parse("v5").unwrap()));
    }

    #[test]
    fn versions_order_prereleases_by_semver_not_release_class() {
        let ordered = [
            "v1.9.9",
            "v2.0.0-alpha",
            "v2.0.0-alpha.1",
            "v2.0.0-beta",
            "v2.0.0-rc.2",
            "v2.0.0-rc.10",
            "v2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(
                parse(pair[0]).unwrap() < parse(pair[1]).unwrap(),
                "{pair:?}"
            );
        }
        assert_eq!(parse("v2.0.0+build.1"), parse("v2.0.0+other"));
        assert_eq!(parse("v2.0.0-rc.1+build"), parse("v2.0.0-rc.1"));
        // RC recognition is case-insensitive; SemVer identifier ordering is not.
        assert!(parse("v2.0.0-RC.1").unwrap() < parse("v2.0.0-beta").unwrap());
        assert!(parse("v2.0.0-rc10").unwrap() < parse("v2.0.0-rc2").unwrap());
    }
}
