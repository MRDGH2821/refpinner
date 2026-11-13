use saphyr::{LoadableYamlNode, MarkedYaml, ScanError};

use crate::reference::Reference;

/// Extracts the action references used by steps and reusable-workflow jobs in a GitHub
/// workflow.
///
/// Local actions and workflows (`./...`, `$/...`) need no pin and are skipped.
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
            let steps = job
                .data
                .as_mapping_get("steps")
                .and_then(|s| s.data.as_sequence())
                .into_iter()
                .flatten()
                .filter_map(|step| step.data.as_mapping_get("uses"));
            for uses in job.data.as_mapping_get("uses").into_iter().chain(steps) {
                let Some(text) = uses.data.as_str() else {
                    continue;
                };
                if text.starts_with("./")
                    || text.starts_with("$/")
                    || text.starts_with("docker://")
                    || text.contains("${{")
                {
                    continue;
                }
                references.push(Reference {
                    line: uses.span.start.line(),
                    column: uses.span.start.col() + 1,
                    text: text.to_owned(),
                    pinned: is_commit(text.rsplit_once('@').map_or("", |(_, rev)| rev)),
                });
            }
        }
    }
    Ok(references)
}

/// A full 40-character Git commit ID; anything shorter or symbolic can move.
fn is_commit(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit())
}
