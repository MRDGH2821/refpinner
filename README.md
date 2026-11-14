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

## Configuration

`refpinner` reads local TOML configuration. It searches each discovered project in this order and reads only the first existing file:

1. `refpinner.toml`
2. `.refpinner.toml`
3. `.config/refpinner.toml`
4. `.config/.refpinner.toml`
5. `.config/refpinner/config.toml`

Use `--config PATH` to select another file. `REFPINNER_CONFIG` selects a file when the flag is absent.
The selected explicit file applies to every discovered project. Without an explicit file, nested projects use their own configuration.
They do not inherit a parent project's configuration automatically.

```sh
refpinner config validate
refpinner config validate --config .config/refpinner.toml
refpinner lint --config .config/refpinner.toml
refpinner update --patch --write
```

Validation runs offline. Missing parents, inheritance cycles, invalid TOML, unknown fields and invalid patterns stop commands before any writes.
Errors identify the file but do not quote configuration values.

```toml
extends = ["../shared/base.toml", "../shared/team.toml"]
platforms = ["github", "forgejo", "gitea", "gitlab"]
exclude = ["generated.yml", "vendor/"]
no_symlink_warnings = true
suppress = ["unsupported dynamic reference *"]
update_limit = "minor"

# Additional files bypass discovery ignores and file exclusions.
files = [{ path = "../ci/build.yml", platform = "github" }]

# The last matching reference rule wins. Match the name without @revision.
references = [
  { pattern = "actions/*", update_limit = "patch" },
  { pattern = "owner/legacy", exclude = true }
]

# Configure the source of shorthand Forgejo action references.
forgejo_instance = "https://forgejo.example.org"
forgejo_action_source = "self"
```

Parents load in `extends` order. Later parents override earlier parents, and the child overrides all parents.
Lists replace earlier lists, including an empty list. Paths in `extends` and `files` are relative to the file that declares them.
File exclusion globs are relative to the project root. Reference and suppression patterns use glob syntax.
Use `refpinner lint --include-excluded` to audit excluded files and references.
Suppression hides warning output but does not make YAML aliases safe to edit.

Explicit CLI values override environment values, which override configuration values and built-in defaults.
An explicit `--minor` also overrides a configured patch limit. The default update limit is `minor`.
The supported environment variables are `REFPINNER_CONFIG`, `REFPINNER_PATH`, `REFPINNER_EXCLUDE` (comma-separated), `REFPINNER_PLATFORM`,
`REFPINNER_NO_SYMLINK_WARNINGS`, `REFPINNER_INCLUDE_EXCLUDED`, and `REFPINNER_UPDATE_LIMIT` (`major`, `minor`, or `patch`).
Use `--no-symlink-warnings=false` to override a true environment or configuration value.
Existing forge API environment variables remain available.

`version_rules` accepts a list of `{ pattern = "reference glob", glob = "tag glob" }` or
`{ pattern = "reference glob", regex = "tag regex" }` tables. Match the image name without a tag
(for example `node` or `ghcr.io/owner/tool`). A regex must contain a named `major` capture; optional
`minor`, `patch`, `prerelease`, `suffix`, and other named groups become version components. Glob
patterns may use `{name}` placeholders for the same captures. The last matching rule replaces
built-in Docker-style tag interpretation. Unattended updates keep named components fixed; changing
them requires an interactive write or a rule that makes the desired tag the primary match.

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
`refpinner update` advances comparable image tags within the selected limits; private registry credentials belong to a separate issue.
