# Pin CI

[![Copier](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/copier-org/copier/refs/heads/master/img/badge/black-badge.json)](https://github.com/copier-org/copier)

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
