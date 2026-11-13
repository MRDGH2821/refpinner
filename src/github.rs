use std::ops::Range;

use saphyr::{LoadableYamlNode, MarkedYaml, ScanError};

use crate::reference::Reference;

/// Extracts the action references used by steps in a GitHub workflow.
///
/// Local actions (`./...`) need no pin and are skipped.
/// shortcut: `docker://` images and `${{ }}` expressions are skipped silently; the image
/// and unsupported-reference tickets report them.
pub fn references(source: &str) -> Result<Vec<Reference>, ScanError> {
    let mut references = Vec::new();
    for document in MarkedYaml::load_from_str(source)? {
        let Some(jobs) = document
            .data
            .as_mapping_get("jobs")
            .and_then(|j| j.data.as_mapping())
        else {
            continue;
        };
        for job in jobs.values() {
            let Some(steps) = job
                .data
                .as_mapping_get("steps")
                .and_then(|s| s.data.as_sequence())
            else {
                continue;
            };
            for uses in steps
                .iter()
                .filter_map(|step| step.data.as_mapping_get("uses"))
            {
                let Some(text) = uses.data.as_str() else {
                    continue;
                };
                if text.starts_with("./") || text.starts_with("docker://") || text.contains("${{") {
                    continue;
                }
                references.push(Reference {
                    line: uses.span.start.line(),
                    column: uses.span.start.col() + 1,
                    text: text.to_owned(),
                    pinned: is_commit(text.rsplit_once('@').map_or("", |(_, rev)| rev)),
                    span: value_span(source, byte_offset(source, uses.span.start.index()), text),
                });
            }
        }
    }
    Ok(references)
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
