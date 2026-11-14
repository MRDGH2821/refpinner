# Pin CI

[![Copier](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/copier-org/copier/refs/heads/master/img/badge/black-badge.json)](https://github.com/copier-org/copier)

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
`{ pattern = "reference glob", regex = "tag regex" }` tables. A regex must contain a named `major` capture.
Validation checks these patterns offline. Image tag interpretation and component selection belong to issue #12 and are not implemented here.

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
