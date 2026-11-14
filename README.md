# Pin CI

[![Copier](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/copier-org/copier/refs/heads/master/img/badge/black-badge.json)](https://github.com/copier-org/copier)

## Prerelease updates

Use `refpinner update --pre` to admit all prereleases, or `refpinner update --rc`
to admit only release candidates. Both options also admit stable releases. They
conflict with each other and respect `--major`, `--minor`, and `--patch` limits.

Without either option, stable releases are eligible. An existing prerelease can
advance within the same numeric version and graduate to a stable release, but it
cannot move to another version's prereleases without opting in.

Release candidates use `rc`, `rc.1`, or `rc1` suffixes, without regard to case.
Classification does not change SemVer ordering: `rc.10` follows `rc.2`, while
`rc10` sorts before `rc2` as text. Identifier ordering remains case-sensitive.
Build metadata does not affect precedence, and metadata-only changes are not
updates. Tags with different numeric precision are not compared.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for setup, checks, and the contribution workflow.

## Recommendations

### Configuration directory

If this project is a tool, CLI, or library that reads its own configuration,
support resolving it from a project-level `.config/` directory
(e.g. `.config/pin-ci.toml`) alongside
any other locations you accept. It keeps consumers' repo roots tidy and follows
an emerging cross-ecosystem convention:

- <https://github.com/numtide/prj-spec> — project directory specification
- <https://dot-config.github.io/> — the `.config/` directory convention
- <https://github.com/pi0/config-dir> — reference implementation for resolving it
