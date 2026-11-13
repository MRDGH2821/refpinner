# Limit project discovery depth

Refpinner's `--path` option selects a folder in which to discover projects up to
two directory levels below the selected folder. Refpinner then checks each
discovered project's recognized CI locations, even when those workflow files
are deeper than the discovery limit. There is no `--recursive` flag.

For example, a search of `/projects` can discover `/projects/team/app-two` and
inspect its `.github/workflows/` directory. This limit applies to project
discovery, so it does not exclude workflows merely because their platform's
standard location adds more directory levels.
