use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use saphyr::ScanError;

use crate::discover::{self, Scan};
use crate::reference::{Kind, Reference, State};
use crate::{github, image};

/// Exit status when every reference is pinned; warnings alone do not change it.
const OK: u8 = 0;
/// Exit status when at least one reference is unpinned.
const UNPINNED: u8 = 1;
/// Exit status when the scan was incomplete: no files found, or a file could not be read.
const ERROR: u8 = 2;

/// Lints the files selected by `scan` offline and prints one line per reference.
pub fn run(scan: &Scan) -> ExitCode {
    let root = &scan
        .path
        .canonicalize()
        .unwrap_or_else(|_| scan.path.clone());
    let files = match scan.files() {
        Ok(files) if !files.is_empty() => files,
        Ok(_) => {
            return fail(&format!(
                "no supported CI files found in {}",
                root.display()
            ));
        }
        Err(error) => return fail(&format!("cannot scan {}: {error}", root.display())),
    };
    let mut status = OK;
    for input in files {
        let file = input.path;
        let shown = display(file.strip_prefix(root).unwrap_or(&file));
        let parsed = fs::read_to_string(&file)
            .map_err(|error| error.to_string())
            .and_then(|source| references(&file, &source).map_err(|error| error.to_string()));
        let references = match parsed {
            Ok(references) => references,
            Err(error) => {
                eprintln!("refpinner: {shown}: {error}");
                status = ERROR;
                continue;
            }
        };
        for reference in references {
            if !input.settings.includes(&reference, scan.include_excluded) {
                continue;
            }
            let state = match reference.state {
                State::Pinned => "pinned",
                State::Unpinned => "unpinned",
                State::Warning => "warning",
            };
            println!(
                "{shown}:{}:{}: {state} {}",
                reference.line, reference.column, reference.text
            );
            if reference.state == State::Unpinned {
                status = status.max(UNPINNED);
            }
        }
    }
    ExitCode::from(status)
}

/// Extracts the references in `source`, read from `file`, with the parser for its platform.
///
/// shortcut: Forgejo and Gitea workflows report only images and YAML warnings, because
/// their action shorthand resolves against the Forgejo instance; the Forgejo ticket (#9)
/// adds their actions.
pub fn references(file: &Path, source: &str) -> Result<Vec<Reference>, ScanError> {
    if file.file_name() == Some(OsStr::new(discover::GITLAB_CI)) {
        return image::gitlab(source);
    }
    let mut references = github::references(source)?;
    if discover::is_forgejo(file) {
        references.retain(|r| r.kind == Kind::Image || r.state == State::Warning);
    }
    Ok(references)
}

pub fn fail(message: &str) -> ExitCode {
    eprintln!("refpinner: {message}");
    ExitCode::from(ERROR)
}

/// Formats a relative path with `/` separators so output is identical on every platform.
pub fn display(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
