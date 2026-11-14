use saphyr::{LoadableYamlNode, MarkedYaml, ScanError};
use saphyr_parser::{Event, Parser};

use crate::github::{byte_offset, value_span, warning};
use crate::reference::{Kind, Reference, State};

/// Plans the pin of a selected project ref, keeping quotes, comments and track metadata.
pub fn plan(
    source: &str,
    reference: &Reference,
    gitlab: &mut crate::gitlab_resolve::GitLab,
) -> Result<crate::pin::Change, String> {
    let selection = reference.text.rsplit_once('@');
    let (project, revision) = selection
        .map_or((reference.text.as_str(), None), |(project, revision)| {
            (project, Some(revision))
        });
    let (flow, indent) = context(source, reference)?;
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if revision.is_none() {
        let raw = &source[reference.span.clone()];
        let quoted = raw.starts_with(['\"', '\'']);
        let inner = if quoted { &raw[1..raw.len() - 1] } else { raw };
        if inner != project {
            return Err("cannot edit a reference with escapes or line breaks".to_owned());
        }
        let (commit, track) = gitlab.resolve(project, None)?;
        let (at, text) = if flow {
            (
                reference.span.end,
                format!(", ref: {commit} # {track}{newline}{}", " ".repeat(indent)),
            )
        } else {
            (
                crate::pin::line_end(source, reference.span.end),
                format!("{newline}{}ref: {commit} # {track}", " ".repeat(indent)),
            )
        };
        return Ok(crate::pin::Change {
            summary: String::new(),
            value: commit,
            edits: vec![crate::edit::Edit {
                start: at,
                end: at,
                text,
            }],
        });
    }
    let revision = revision.unwrap();
    let raw = &source[reference.span.clone()];
    let quoted = raw.starts_with(['\"', '\'']);
    let inner = if quoted { &raw[1..raw.len() - 1] } else { raw };
    if inner != revision {
        return Err("cannot edit a reference with escapes or line breaks".to_owned());
    }
    let after = &source[reference.span.end..crate::pin::line_end(source, reference.span.end)];
    if !flow && !(after.trim().is_empty() || after.trim_start().starts_with('#')) {
        return Err("cannot annotate a reference followed by other content".to_owned());
    }
    let (commit, track) = gitlab.resolve(project, Some(revision))?;
    Ok(crate::pin::Change {
        summary: String::new(),
        value: commit.clone(),
        edits: vec![
            crate::edit::Edit {
                start: reference.span.start + usize::from(quoted),
                end: reference.span.end - usize::from(quoted),
                text: commit,
            },
            crate::edit::Edit {
                start: reference.span.end,
                end: reference.span.end,
                text: if flow {
                    format!(" # {track}{newline}{}", " ".repeat(indent))
                } else {
                    format!(" # {track}")
                },
            },
        ],
    })
}

/// Finds the containing include mapping and copies its key indentation.
fn context(source: &str, reference: &Reference) -> Result<(bool, usize), String> {
    for document in MarkedYaml::load_from_str(source).map_err(|e| e.to_string())? {
        let Some(include) = document.data.as_mapping_get("include") else {
            continue;
        };
        let entries = include
            .data
            .as_sequence()
            .map_or_else(|| vec![include], |seq| seq.iter().collect());
        for entry in entries {
            let Some(project) = entry.data.as_mapping_get("project") else {
                continue;
            };
            let node = entry.data.as_mapping_get("ref").unwrap_or(project);
            if node.span.start.line() == reference.line
                && node.span.start.col() + 1 == reference.column
            {
                let start = byte_offset(source, entry.span.start.index());
                let key = entry
                    .data
                    .as_mapping()
                    .and_then(|mapping| mapping.keys().next())
                    .ok_or("include has no keys")?;
                return Ok((source[start..].starts_with('{'), key.span.start.col()));
            }
        }
    }
    Err("cannot locate project include mapping".to_owned())
}

/// Finds project includes; local files need no pin and unsupported forms warn.
pub fn references(source: &str) -> Result<Vec<Reference>, ScanError> {
    let mut references = Vec::new();
    let mut aliases = Vec::new();
    let mut scalars = Vec::new();
    for event in Parser::new_from_str(source) {
        let (event, span) = event?;
        if let Event::Scalar(value, ..) = &event {
            scalars.push((span.start, value.clone()));
        }
        let message = match event {
            Event::Alias(_) => {
                aliases.push(span.start);
                "unsupported YAML alias"
            }
            Event::Scalar(_, _, anchor, _)
            | Event::MappingStart(anchor, _)
            | Event::SequenceStart(anchor, _)
                if anchor != 0 =>
            {
                "unsupported YAML anchor"
            }
            _ => continue,
        };
        references.push(warning(span.start, message.to_owned()));
    }
    for document in MarkedYaml::load_from_str(source)? {
        let Some(include) = document.data.as_mapping_get("include") else {
            continue;
        };
        let entries = include
            .data
            .as_sequence()
            .map_or_else(|| vec![include], |seq| seq.iter().collect());
        for entry in entries {
            if aliases.contains(&include.span.start) || aliases.contains(&entry.span.start) {
                continue;
            }
            if let Some(text) = entry.data.as_str() {
                if text.contains('$') {
                    references.push(warning(
                        entry.span.start,
                        format!("unsupported dynamic reference {text}"),
                    ));
                } else if text.starts_with("https://") || text.starts_with("http://") {
                    references.push(warning(
                        entry.span.start,
                        format!("unsupported remote include {text}"),
                    ));
                }
                continue;
            }
            for form in ["remote", "component", "template"] {
                if let Some(value) = entry
                    .data
                    .as_mapping_get(form)
                    .filter(|v| !aliases.contains(&v.span.start))
                {
                    let text = value.data.as_str().unwrap_or("non-scalar value");
                    references.push(warning(
                        value.span.start,
                        format!("unsupported {form} include {text}"),
                    ));
                }
            }
            let Some(project) = entry.data.as_mapping_get("project") else {
                continue;
            };
            let revision = entry.data.as_mapping_get("ref");
            if aliases.contains(&project.span.start)
                || revision.is_some_and(|r| aliases.contains(&r.span.start))
            {
                continue;
            }
            let Some(name) = project.data.as_str() else {
                references.push(warning(
                    project.span.start,
                    "unsupported project include: project must be a string".to_owned(),
                ));
                continue;
            };
            let node = revision.unwrap_or(project);
            let selected = scalars
                .iter()
                .find(|(at, _)| *at == node.span.start)
                .map(|(_, value)| value.as_ref());
            let text = match revision {
                Some(_) => match selected {
                    Some(revision) => format!("{name}@{revision}"),
                    None => {
                        references.push(warning(
                            node.span.start,
                            "unsupported project include: ref must be a string".to_owned(),
                        ));
                        continue;
                    }
                },
                None => name.to_owned(),
            };
            if text.contains('$') {
                references.push(warning(
                    if name.contains('$') {
                        project.span.start
                    } else {
                        node.span.start
                    },
                    format!("unsupported dynamic reference {text}"),
                ));
                continue;
            }
            let selected = selected.unwrap_or("");
            let pinned = revision.is_some()
                && selected.len() == 40
                && selected.bytes().all(|b| b.is_ascii_hexdigit());
            references.push(Reference {
                line: node.span.start.line(),
                column: node.span.start.col() + 1,
                span: value_span(
                    source,
                    byte_offset(source, node.span.start.index()),
                    selected,
                ),
                text,
                state: if pinned {
                    State::Pinned
                } else {
                    State::Unpinned
                },
                kind: Kind::GitLab,
                reusable: false,
            });
        }
    }
    references.sort_by_key(|r| (r.line, r.column));
    Ok(references)
}
