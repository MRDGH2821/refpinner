# Project-level `.config` and dot-prefixed configuration files

Research date: 2026-11-13. Sources below are official project documentation or specifications; they describe
documented behavior at the time of research, not every historical release. Here `PROJECT` is the project
directory and "automatic" means discovery without an explicit configuration path or an environment variable
redirecting a global location.

## Finding

There is primary-source evidence that tools automatically discover configuration **inside a project-level
`.config/`**, but the verified examples use filenames without a leading dot: mise supports
`PROJECT/.config/mise.toml`, and ansible-lint supports `PROJECT/.config/ansible-lint.yml` and `.yaml`. I found
**no primary-source evidence in this research** for a tool automatically discovering a dot-prefixed filename
directly inside that directory, such as `PROJECT/.config/.tool.toml` or `PROJECT/.config/.toolrc`, when
invoked from the project root under normal defaults. This is a limit of the examples examined, not proof that
no such tool exists.

A dot-prefixed filename at `PROJECT/.tool.toml`, a dot-prefixed filename in a user-global directory, an
explicitly supplied path, and a project `.config` file with a non-dot filename are different cases. None alone
establishes automatic discovery of `PROJECT/.config/.tool.toml`.

## Verified project `.config` examples

### [mise](https://mise.jdx.dev/configuration.html)

Searches directory levels from the filesystem root to the current directory, subject to its ceiling settings.
Documented project filenames include `.config/mise.toml`, `.config/mise/mise.toml`, and
`.config/mise/config.toml`, as well as root-level `mise.toml` and other forms. Files in deeper directories
override parent files; the documentation gives within-directory precedence.

`.config/mise.toml` is directly inside the project `.config` but its basename is **not** dot-prefixed. Its
usual global configuration is `~/.config/mise/config.toml`, a different location. [mise
directories](https://mise.jdx.dev/directories.html) documents global vs project scope.

### [ansible-lint](https://docs.ansible.com/projects/lint/configuring/)

From the current directory it documents `.ansible-lint`, `.ansible-lint.yml`, `.ansible-lint.yaml`,
`.config/ansible-lint.yml`, and `.config/ansible-lint.yaml`; it can search parents but does not load
configuration outside the Git repository. It expects to be run from the project root for correct discovery and
dependency resolution.

Its supported dot-prefixed names are at the searched directory level; its documented `.config/` names are not
dot-prefixed. `-c` specifies another file explicitly.

## Counterexamples and easily confused cases

- **Ruff:** [Official configuration documentation](https://docs.astral.sh/ruff/configuration/) says it finds
  the closest `pyproject.toml` containing `[tool.ruff]`, `ruff.toml`, or `.ruff.toml` in the file's directory
  hierarchy; `.ruff.toml` wins within a directory. If none is found, it checks user-specific
  `${config_dir}/ruff/.ruff.toml`, then `ruff.toml`, then `pyproject.toml`. That dotfile is inside the **user
  configuration directory's `ruff/` subdirectory**, not evidence for `PROJECT/.config/.ruff.toml`. `--config`
  can explicitly select a path; it is not automatic discovery. An analyzed file actually inside
  `PROJECT/.config/` is a separate ancestor-search case: its configuration lookup may encounter a dotfile
  there, which does not make that directory a special project-wide search path for sibling source files.
- **yamllint:** [Official configuration
  documentation](https://github.com/adrienverge/yamllint/blob/master/docs/configuration.rst) specifies
  `.yamllint`, `.yamllint.yaml`, or `.yamllint.yml` in the working directory or a parent, stopping at home or
  filesystem root; then `$YAMLLINT_CONFIG_FILE`, then `$XDG_CONFIG_HOME/yamllint/config` or
  `~/.config/yamllint/config`. The last is user-level by default and has a non-dot basename. `-c` supplies a
  file explicitly. A working directory inside `PROJECT/.config/` is a normal working-directory search, not
  proof the project root searches that subdirectory.
- **Prettier:** [Official configuration documentation](https://prettier.io/docs/configuration) lists
  `.prettierrc` and `.prettierrc.toml` among supported names and searches upward from the file being
  formatted. Thus a `PROJECT/.config/.prettierrc` could be on the ancestor chain for files inside `.config`,
  but the documented search does not descend into it for ordinary sibling project files. [The CLI
  documentation](https://prettier.io/docs/cli) allows an explicit `--config` path for configuration in a
  directory that would otherwise not be found.
- **Hadolint:** [Its official README](https://github.com/hadolint/hadolint#configure) lists
  `$PWD/.hadolint.yaml` first, followed by `$XDG_CONFIG_HOME/hadolint.yaml`, `$HOME/.config/hadolint.yaml`,
  other home paths, and `$HOME/.hadolint.yaml`, using the first file found; `.yml` is also accepted.
  `$HOME/.config/hadolint.yaml` is user-global and its basename is not dot-prefixed. Setting `XDG_CONFIG_HOME`
  to a project path or using `--config` is an explicit redirection, not a default project search.
- **Git:** [Git's configuration documentation](https://git-scm.com/docs/git-config) distinguishes repository
  `$GIT_DIR/config` from user-global `~/.gitconfig` and `$XDG_CONFIG_HOME/git/config`. The latter defaults to
  a location under the user's `.config`, not `PROJECT/.config/.gitconfig`.

## Practical interpretation

If deciding which names a scanner should recognize, support for `PROJECT/.config/<tool>.toml` is demonstrably
useful for specific tools such as mise, but it does not justify assuming every CLI reads that location. For
`PROJECT/.config/.<tool>.toml` or `PROJECT/.config/.toolrc`, the research found no direct automatic-discovery
example; add such a pattern only with tool-specific primary evidence or label it as a heuristic rather than a
supported path. Check the tool's documented version, starting directory, precedence, and explicit overrides
before inferring which file it reads.
