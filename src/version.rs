/// Parses `v1`, `1.2` or `v1.2.3` into numeric components; anything else is not a version.
///
/// shortcut: prerelease and build suffixes are not versions yet, so only stable tags are
/// compared; #7 adds prerelease ordering here.
pub fn parse(tag: &str) -> Option<Vec<u64>> {
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

/// How far an update may move from the current version.
#[derive(Clone, Copy, PartialEq)]
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
    pub fn allows(self, current: &[u64], candidate: &[u64]) -> bool {
        let kept = match self {
            Limit::Major => 0,
            Limit::Minor => 1,
            Limit::Patch => 2,
        };
        kept <= current.len()
            && candidate.len() == current.len()
            && candidate > current
            && candidate[..kept] == current[..kept]
    }
}

#[cfg(test)]
mod tests {
    use super::{Limit, parse};

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
        assert!(!Limit::Patch.allows(&[4], &[5]));
        assert_eq!(parse("v4.2-rc.1"), None);
    }
}
