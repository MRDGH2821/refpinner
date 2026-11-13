use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

use crate::discover::Scan;
use crate::edit::{self, Edit};
use crate::lint::{display, fail};
use crate::reference::{Kind, Reference, State};
use crate::registry::Registry;
use crate::resolve::{GitHub, Resolution};
use crate::{image, lint, write};

/// Exit status when every reference resolved.
const OK: u8 = 0;
/// Exit status when a file or reference could not be processed.
const ERROR: u8 = 2;

/// Previews pins for the unpinned references in the files selected by `scan`.
///
/// With `write`, each file's pins are applied with a backup. When stdin is a terminal the
/// user picks each pin; otherwise every pin that resolved is applied.
pub fn run(scan: &Scan, write: bool) -> ExitCode {
    let root = &scan.path;
    let files = match scan.files() {
        Ok(files) => files,
        Err(error) => return fail(&format!("cannot scan {}: {error}", root.display())),
    };
    let mut github = GitHub::from_env();
    let mut registry = Registry::from_env();
    let interactive = write && io::stdin().is_terminal();
    let mut status = OK;
    for file in files {
        let shown = display(file.strip_prefix(root).unwrap_or(&file));
        let parsed = fs::read_to_string(&file)
            .map_err(|error| error.to_string())
            .and_then(|source| {
                let references =
                    lint::references(&file, &source).map_err(|error| error.to_string())?;
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
        // Editing a file with anchors or aliases could change other nodes that share them.
        let aliased = references
            .iter()
            .any(|r| r.state == State::Warning && r.text.starts_with("unsupported YAML "));
        let mut chosen = Vec::new();
        let mut values = Vec::new();
        for reference in &references {
            let location = format!("{shown}:{}:{}", reference.line, reference.column);
            match reference.state {
                State::Pinned => continue,
                State::Warning => {
                    println!("{location}: warning {}", reference.text);
                    continue;
                }
                State::Unpinned if aliased => {
                    eprintln!(
                        "refpinner: {location}: {}: not edited because the file uses YAML anchors or aliases",
                        reference.text
                    );
                    status = ERROR;
                    continue;
                }
                State::Unpinned => {}
            }
            let result = match reference.kind {
                Kind::Action => edits(&source, reference, &mut github),
                Kind::Image => image_edits(&source, reference, &mut registry),
            };
            match result {
                Ok(edits) => {
                    preview(&location, &source, reference, &edits);
                    if write && (!interactive || confirm()) {
                        let pinned = match reference.kind {
                            // `edits` succeeded, so the text has an `@` and edit 0 is the commit.
                            Kind::Action => {
                                let action = reference.text.rsplit_once('@').map_or("", |(a, _)| a);
                                format!("{action}@{}", edits[0].text)
                            }
                            Kind::Image => format!("{}{}", reference.text, edits[0].text),
                        };
                        values.push((reference.line, reference.column, pinned));
                        chosen.extend(edits);
                    }
                }
                Err(error) => {
                    eprintln!("refpinner: {location}: {}: {error}", reference.text);
                    status = ERROR;
                }
            }
        }
        if chosen.is_empty() {
            continue;
        }
        match write::replace(&file, &source, &chosen, &values) {
            Ok(backup) => println!(
                "wrote {shown} (backup {})",
                display(backup.strip_prefix(root).unwrap_or(&backup))
            ),
            Err(error) => {
                eprintln!("refpinner: {shown}: not written: {error}");
                status = ERROR;
            }
        }
    }
    ExitCode::from(status)
}

/// Asks whether to apply the pin just previewed; anything but `y` or `yes` declines.
fn confirm() -> bool {
    eprint!("apply this pin? [y/N] ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).is_ok()
        && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Computes the edit that pins an image `reference` to the digest its tag names now.
///
/// The tag stays in the value as the annotation, `name:tag@sha256:...`; an implicit tag is
/// written out as `:latest`. The value keeps its quoting and any comment.
pub fn image_edits(
    source: &str,
    reference: &Reference,
    registry: &mut Registry,
) -> Result<Vec<Edit>, String> {
    let raw = &source[reference.span.clone()];
    let quoted = raw.starts_with(['"', '\'']);
    let inner = if quoted { &raw[1..raw.len() - 1] } else { raw };
    if inner != reference.text {
        return Err(String::from(
            "cannot edit a reference with escapes or line breaks",
        ));
    }
    let name = image::parse(&reference.text)?;
    let digest = registry.resolve(&name)?;
    let tag = if name.tag.is_none() { ":latest" } else { "" };
    let end = reference.span.end - usize::from(quoted);
    Ok(vec![Edit {
        start: end,
        end,
        text: format!("{tag}@{digest}"),
    }])
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
