use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::edit::{self, Edit};
use crate::lint::{display, fail};
use crate::reference::{Reference, State};
use crate::resolve::{GitHub, Resolution};
use crate::{discover, github};

/// Exit status when every reference resolved.
const OK: u8 = 0;
/// Exit status when a file or reference could not be processed.
const ERROR: u8 = 2;

/// Previews pins for the unpinned references under `root` without writing files.
pub fn run(root: &Path) -> ExitCode {
    let files = match discover::files(root) {
        Ok(files) => files,
        Err(error) => return fail(&format!("cannot scan {}: {error}", root.display())),
    };
    let mut github = GitHub::from_env();
    let mut status = OK;
    for file in files {
        let shown = display(file.strip_prefix(root).unwrap_or(&file));
        let parsed = fs::read_to_string(&file)
            .map_err(|error| error.to_string())
            .and_then(|source| {
                let references = github::references(&source).map_err(|error| error.to_string())?;
                Ok((source, references))
            });
        let (source, references) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("refpinner: {shown}: {error}");
                status = ERROR;
                continue;
            }
        };
        for reference in &references {
            let location = format!("{shown}:{}:{}", reference.line, reference.column);
            if reference.state != State::Unpinned {
                continue;
            }
            match edits(&source, reference, &mut github) {
                Ok(edits) => preview(&location, &source, reference, &edits),
                Err(error) => {
                    eprintln!("refpinner: {location}: {}: {error}", reference.text);
                    status = ERROR;
                }
            }
        }
    }
    ExitCode::from(status)
}

/// Computes the edits that pin `reference` to the commit its revision names now.
///
/// The value keeps its quoting. The annotation goes after the value, before any comment.
pub fn edits(
    source: &str,
    reference: &Reference,
    github: &mut GitHub,
) -> Result<Vec<Edit>, String> {
    let raw = &source[reference.span.clone()];
    let quoted = raw.starts_with(['"', '\'']);
    let inner = if quoted { &raw[1..raw.len() - 1] } else { raw };
    if inner != reference.text {
        return Err(String::from(
            "cannot edit a reference with escapes or line breaks",
        ));
    }
    let after = &source[reference.span.end..line_end(source, reference.span.end)];
    if !(after.trim().is_empty() || after.trim_start().starts_with('#')) {
        return Err(String::from(
            "cannot annotate a reference followed by other content",
        ));
    }
    let (action, revision) = reference
        .text
        .rsplit_once('@')
        .ok_or("reference has no @revision")?;
    let resolution = github.resolve(action, revision)?;
    let revision_end = reference.span.end - usize::from(quoted);
    Ok(vec![
        Edit {
            start: revision_end - revision.len(),
            end: revision_end,
            text: resolution.commit.clone(),
        },
        Edit {
            start: reference.span.end,
            end: reference.span.end,
            text: format!(" # {}", annotation(revision, &resolution)),
        },
    ])
}

/// Records the exact version when known and the source track when it differs.
fn annotation(revision: &str, resolution: &Resolution) -> String {
    match &resolution.version {
        Some(version) if version == revision => version.clone(),
        Some(version) => format!("{version} (track: {revision})"),
        None => revision.to_owned(),
    }
}

/// Prints the reference location and its line before and after the edits.
fn preview(location: &str, source: &str, reference: &Reference, edits: &[Edit]) {
    let start = source[..reference.span.start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let end = line_end(source, reference.span.end);
    let shifted: Vec<Edit> = edits
        .iter()
        .map(|edit| Edit {
            start: edit.start - start,
            end: edit.end - start,
            text: edit.text.clone(),
        })
        .collect();
    let line = &source[start..end];
    println!("{location}: {}", reference.text);
    println!("-{line}");
    println!("+{}", edit::apply(line, &shifted));
}

/// Byte offset of the end of the line holding `offset`, before any `\r\n` or `\n`.
fn line_end(source: &str, offset: usize) -> usize {
    let end = source[offset..]
        .find('\n')
        .map_or(source.len(), |newline| offset + newline);
    if source[..end].ends_with('\r') {
        end - 1
    } else {
        end
    }
}
