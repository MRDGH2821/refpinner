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
            for uses in job.data.as_mapping_get("uses").into_iter().chain(steps) {
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
    }
}

/// A full 40-character Git commit ID; anything shorter or symbolic can move.
fn is_commit(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit())
}
