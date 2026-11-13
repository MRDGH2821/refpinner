# Prerelease and release candidate selection

## Version ordering

[Semantic Versioning 2.0.0](https://semver.org/) specifies ordering but does not prescribe which versions a tool should select.
A newer version's prerelease can sort above an older stable release: `2.0.0-rc.1 > 1.9.9`. For the same version, the final release sorts after its prereleases: `2.0.0 > 2.0.0-rc.1`.
Build metadata does not affect precedence. SemVer does not define `rc` as a special release phase, so a tool must define how it recognizes release candidates.

## Existing tools

- [npm's semver range rules](https://docs.npmjs.com/cli/v6/using-npm/semver/#prerelease-tags) normally exclude prereleases unless the range explicitly opts in; node-semver provides an `includePrerelease` option.
- [pip's install documentation](https://pip.pypa.io/en/stable/cli/pip_install/#pre-release-versions) describes `--pre` as allowing prerelease and development versions. Python uses its own [version specification](https://packaging.python.org/en/latest/specifications/version-specifiers/), including normalization of several release-candidate spellings.
- [Cargo](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#pre-releases) normally excludes prereleases unless a requirement explicitly requests one. An explicit prerelease can advance within its release and to compatible final versions.
- [Renovate](https://docs.renovatebot.com/configuration-options/#ignoreunstable) ignores unstable versions by default, but supports updates within the same release of an already unstable dependency and allows graduation to stable versions. Disabling `ignoreUnstable` allows broader unstable updates.

## Recommendations

- Filter eligible versions before ordering them. Keep stable releases eligible with either flag.
- Let `--pre` allow all recognized prereleases, and `--rc` allow only stable releases and recognized release candidates. The highest eligible version wins subject to the selected major, minor, or patch limit.
- Make the flags mutually exclusive to avoid implying that `--rc` restricts `--pre`.
- Define RC recognition explicitly; SemVer permits arbitrary prerelease labels. Recognize common `rc`, `rc.1`, and `rc1` forms without matching unrelated words. Apply other aliases only when supported by the applicable version scheme.
- For a dependency already on a prerelease, follow its existing release's prerelease sequence and allow its final version by default; require an opt-in to move to prereleases of another release. Confirm this policy with the user.
- Test a newer-core RC against an older final, an RC against its final, numeric RC ordering, beta versus RC, unrecognized labels, and metadata-only differences.
