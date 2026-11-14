# Pin CI

[![Copier](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/copier-org/copier/refs/heads/master/img/badge/black-badge.json)](https://github.com/copier-org/copier)

## Forgejo Actions

`refpinner lint`, `pin`, and `update` read Forgejo workflow steps, reusable workflows, and composite actions.
Discovery includes `.forgejo/workflows` and `.gitea/workflows`, including nested workflow folders.
It also includes `action.yml` and `action.yaml` under `.forgejo/actions` and `.gitea/actions`.
Root action metadata uses the nearest project marker to select the platform.
Explicit `.github` paths keep GitHub behavior, even when the project also contains Forgejo workflows.

A shorthand reference is an action name without a host, such as `actions/checkout@v7`.
Forgejo shorthand uses `https://data.forgejo.org` by default.
Gitea shorthand uses GitHub by default.
Set `--forgejo-action-source` to `github`, `self`, or an HTTP(S) URL to match the instance configuration.
The environment variable `REFPINNER_FORGEJO_ACTION_SOURCE` sets the same value.
CLI values override environment values.

Reusable workflow shorthand uses the instance itself, not the action source.
Set `--forgejo-instance` or `REFPINNER_FORGEJO_INSTANCE` for reusable shorthand and the `self` source.
An unknown instance produces an error instead of an assumed host.
Absolute references such as `https://codeberg.org/team/repo@main` use their stated host.
GitHub sources use the GitHub API. Other sources use the Forgejo-compatible `/api/v1` API.
API redirects produce an error instead of changing the host or connection scheme.

```sh
refpinner pin --forgejo-instance https://codeberg.org --forgejo-action-source self
refpinner pin --write --forgejo-instance https://codeberg.org --forgejo-action-source self
refpinner update --forgejo-instance https://codeberg.org --forgejo-action-source self
```

`pin` and `update` preview changes unless you pass `--write`.
Writes preserve the original reference form, comments, quotes, and line endings, with numbered backups.
Version annotations and update limits follow the same rules as GitHub references.
Local `./` and `$/` references need no pin.
Dynamic references and YAML anchors or aliases retain the existing warning and safe-write rules.
The tool does not fetch remote workflows for recursive inspection.

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
