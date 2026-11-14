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

## Image pin architecture

`src/image.rs` extracts image references from GitHub, Forgejo, Gitea, and GitLab YAML.
Offline lint does not create a registry client.
`src/registry.rs` resolves tags through OCI registry requests and requests multi-platform image indexes before individual manifests.
It supports anonymous Bearer authentication and hashes the manifest bytes when the digest header is absent.
`REFPINNER_REGISTRY_URL` overrides registry endpoints for local tests or a mirror.

Image pins retain the source tag as `name:tag@sha256:...`, with `latest` for an omitted tag.
Images and GitHub actions share `pin::apply` for previews, confirmation, and writes.
`write::replace` checks that only selected YAML values change and saves the original bytes in a numbered backup.
Image updates and private registry credentials belong to separate issues.
