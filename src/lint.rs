use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::reference::State;
use crate::{discover, github};

/// Exit status when every reference is pinned; warnings alone do not change it.
const OK: u8 = 0;
/// Exit status when at least one reference is unpinned.
const UNPINNED: u8 = 1;
/// Exit status when the scan was incomplete: no files found, or a file could not be read.
const ERROR: u8 = 2;

/// Lints the CI files under `root` offline and prints one line per reference.
pub fn run(root: &Path) -> ExitCode {
    let files = match discover::files(root) {
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
    for file in files {
        let shown = display(file.strip_prefix(root).unwrap_or(&file));
        let parsed = fs::read_to_string(&file)
            .map_err(|error| error.to_string())
            .and_then(|source| github::references(&source).map_err(|error| error.to_string()));
        let references = match parsed {
            Ok(references) => references,
            Err(error) => {
                eprintln!("refpinner: {shown}: {error}");
                status = ERROR;
                continue;
            }
        };
        for reference in references {
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
