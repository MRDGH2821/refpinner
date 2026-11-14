use std::ops::Range;

use saphyr::{LoadableYamlNode, MarkedYaml, Marker, ScanError};
use saphyr_parser::{Event, Parser};

use crate::reference::{Reference, State};

/// Extracts the action references used by steps and reusable-workflow jobs in a GitHub
/// workflow, or by `runs.steps` in a composite action, sorted by position.
///
/// Local actions and workflows (`./...`, `$/...`) need no pin and are skipped.
/// `${{ }}` expressions, YAML anchors and aliases are reported as warnings; aliased nodes
/// are not resolved.
/// shortcut: `docker://` images are skipped silently; the image ticket reports them.
pub fn references(source: &str) -> Result<Vec<Reference>, ScanError> {
    let mut references = Vec::new();
    let mut aliases = Vec::new();
    for event in Parser::new_from_str(source) {
        let (event, span) = event?;
        let message = match event {
            Event::Alias(_) => {
                aliases.push(span.start);
                "unsupported YAML alias"
            }
            Event::Scalar(_, _, anchor, _)
            | Event::SequenceStart(anchor, _)
            | Event::MappingStart(anchor, _)
                if anchor != 0 =>
            {
                "unsupported YAML anchor"
            }
            _ => continue,
        };
        references.push(warning(span.start, message.to_owned()));
    }
    // The loader expands aliases into copies that keep the alias's own start marker.
    let aliased = |node: &MarkedYaml| aliases.contains(&node.span.start);
    for document in MarkedYaml::load_from_str(source)? {
        let jobs = document
            .data
            .as_mapping_get("jobs")
            .filter(|j| !aliased(j))
            .and_then(|j| j.data.as_mapping())
            .into_iter()
            .flat_map(|jobs| jobs.values());
        // A composite action's `runs` holds `steps` the same way a job does.
        for job in jobs.chain(document.data.as_mapping_get("runs")) {
            if aliased(job) {
                continue;
            }
            let steps = job
                .data
                .as_mapping_get("steps")
                .filter(|s| !aliased(s))
                .and_then(|s| s.data.as_sequence())
                .into_iter()
                .flatten()
                .filter(|step| !aliased(step))
                .filter_map(|step| step.data.as_mapping_get("uses"));
            let reusable = job.data.as_mapping_get("uses").map(|uses| (uses, true));
            for (uses, reusable) in reusable.into_iter().chain(steps.map(|uses| (uses, false))) {
                let Some(text) = uses.data.as_str().filter(|_| !aliased(uses)) else {
                    continue;
                };
                if text.starts_with("./") || text.starts_with("$/") || text.starts_with("docker://")
                {
                    continue;
                }
                if text.contains("${{") {
                    let message = format!("unsupported dynamic reference {text}");
                    references.push(warning(uses.span.start, message));
                    continue;
                }
                let pinned = is_commit(text.rsplit_once('@').map_or("", |(_, rev)| rev));
                references.push(Reference {
                    line: uses.span.start.line(),
                    column: uses.span.start.col() + 1,
                    text: text.to_owned(),
                    state: if pinned {
                        State::Pinned
                    } else {
                        State::Unpinned
                    },
                    span: value_span(source, byte_offset(source, uses.span.start.index()), text),
                    reusable,
                });
            }
        }
    }
    references.sort_by_key(|r| (r.line, r.column));
    Ok(references)
}

fn warning(at: Marker, text: String) -> Reference {
    Reference {
        line: at.line(),
        column: at.col() + 1,
        text,
        state: State::Warning,
        span: 0..0,
        reusable: false,
    }
}

/// Converts saphyr's character index into a byte offset into `source`.
fn byte_offset(source: &str, chars: usize) -> usize {
    source
        .char_indices()
        .nth(chars)
        .map_or(source.len(), |(offset, _)| offset)
}

/// Byte range of a plain or quoted `text` that starts at `start`, including its quotes.
///
/// saphyr's end marker can run past a quoted scalar into its trailing comment, so the end
/// comes from the text instead. Returns an empty range when the source spells `text`
/// differently, such as with escapes or line folding.
fn value_span(source: &str, start: usize, text: &str) -> Range<usize> {
    let rest = &source[start..];
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''));
    let length = match quote {
        Some(quote)
            if rest[1..]
                .strip_prefix(text)
                .is_some_and(|tail| tail.starts_with(quote)) =>
        {
            text.len() + 2
        }
        None if rest.starts_with(text) => text.len(),
        _ => 0,
    };
    start..start + length
}

/// A full 40-character Git commit ID; anything shorter or symbolic can move.
fn is_commit(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit())
}
