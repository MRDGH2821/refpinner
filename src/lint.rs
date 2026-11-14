use std::fs;
use std::path::Path;
use std::process::ExitCode;

use saphyr::ScanError;
use serde::Serialize;

use crate::config::Platform;
use crate::discover::Scan;
use crate::image::{self, is_digest};
use crate::reference::{Kind, Reference, State};
use crate::registry::Registry;
use crate::resolve::Resolver;
use crate::settings::Actions;
use crate::update::{self, Verify};
use crate::{github, image as images};

/// Exit status when every reference is pinned; warnings and updates alone do not change it.
const OK: u8 = 0;
/// Exit status when at least one reference is unpinned or an exact tag moved.
const FINDINGS: u8 = 1;
/// Exit status when the scan was incomplete: no files found, or a file could not be read.
const ERROR: u8 = 2;
/// Exit status when an online check could not finish for at least one reference.
const INCOMPLETE: u8 = 3;

/// Options for a lint run beyond the shared scan selection.
pub struct Options {
    pub online: bool,
    pub json: bool,
    pub fail_on_update: bool,
}

/// Lints the files selected by `scan` and prints one line per reference, or JSON when asked.
pub fn run(scan: &Scan, options: &Options, actions: &Actions) -> ExitCode {
    let root = &scan
        .path
        .canonicalize()
        .unwrap_or_else(|_| scan.path.clone());
    let discovery = match scan.files() {
        Ok(discovery) => discovery,
        Err(error) => return fail(&format!("cannot scan {}: {error}", root.display())),
    };
    let mut report = Report {
        status: OK,
        results: Vec::new(),
        warnings: discovery
            .warnings
            .iter()
            .map(|skip| Warning {
                file: skip.path.clone(),
                message: skip.message.to_owned(),
            })
            .collect(),
    };
    if !options.json {
        for warning in &report.warnings {
            println!("{}: warning: {}", warning.file, warning.message);
        }
    }
    if discovery.files.is_empty() {
        if options.json {
            report.status = ERROR;
            println!("{}", serde_json::to_string_pretty(&report).expect("json"));
            return ExitCode::from(ERROR);
        }
        return fail(&format!(
            "no supported CI files found in {}",
            root.display()
        ));
    }
    let mut resolver = options.online.then(Resolver::default);
    let mut registry = options.online.then(Registry::from_env);
    for input in &discovery.files {
        let shown = display(input.path.strip_prefix(root).unwrap_or(&input.path));
        let parsed = fs::read_to_string(&input.path)
            .map_err(|error| error.to_string())
            .and_then(|source| {
                let refs =
                    references(input.platform, &source).map_err(|error| error.to_string())?;
                Ok((source, refs))
            });
        let (source, refs) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("refpinner: {shown}: {error}");
                report.status = report.status.max(ERROR);
                continue;
            }
        };
        for reference in refs {
            if !input.settings.includes(&reference, scan.include_excluded) {
                continue;
            }
            let finding = match reference.state {
                State::Warning => Finding::new(&shown, &reference, "warning", None),
                State::Unpinned => {
                    report.status = report.status.max(FINDINGS);
                    Finding::new(&shown, &reference, "unpinned", None)
                }
                State::Pinned if !options.online => {
                    Finding::new(&shown, &reference, "pinned", None)
                }
                State::Pinned => online(OnlineCtx {
                    status: &mut report.status,
                    options,
                    input,
                    source: &source,
                    reference: &reference,
                    shown: &shown,
                    actions,
                    resolver: resolver.as_mut(),
                    registry: registry.as_mut(),
                }),
            };
            if !options.json {
                match &finding.detail {
                    Some(detail) => println!(
                        "{}:{}:{}: {} {}: {detail}",
                        finding.file, finding.line, finding.column, finding.state, finding.text
                    ),
                    None => println!(
                        "{}:{}:{}: {} {}",
                        finding.file, finding.line, finding.column, finding.state, finding.text
                    ),
                }
            }
            report.results.push(finding);
        }
    }
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report).expect("json"));
    }
    ExitCode::from(report.status)
}

struct OnlineCtx<'a> {
    status: &'a mut u8,
    options: &'a Options,
    input: &'a crate::discover::Input,
    source: &'a str,
    reference: &'a Reference,
    shown: &'a str,
    actions: &'a Actions,
    resolver: Option<&'a mut Resolver>,
    registry: Option<&'a mut Registry>,
}

fn online(ctx: OnlineCtx<'_>) -> Finding {
    let OnlineCtx {
        status,
        options,
        input,
        source,
        reference,
        shown,
        actions,
        resolver,
        registry,
    } = ctx;
    match reference.kind {
        Kind::Action => {
            let Some(resolver) = resolver else {
                return Finding::new(shown, reference, "pinned", None);
            };
            let (remote, repo) = match resolver.action(input, reference, actions) {
                Ok(remote) => remote,
                Err(error) => {
                    *status = (*status).max(INCOMPLETE);
                    return Finding::new(shown, reference, "incomplete", Some(error));
                }
            };
            match update::verify(source, reference, remote, &repo) {
                Ok(None) => Finding::new(shown, reference, "pinned", None),
                Ok(Some(summary)) => {
                    if options.fail_on_update {
                        *status = (*status).max(FINDINGS);
                    }
                    Finding::new(shown, reference, "update", Some(summary))
                }
                Err(Verify::Mismatch(detail)) => {
                    *status = (*status).max(FINDINGS);
                    Finding::new(shown, reference, "mismatch", Some(detail))
                }
                Err(Verify::Incomplete(detail)) => {
                    *status = (*status).max(INCOMPLETE);
                    Finding::new(shown, reference, "incomplete", Some(detail))
                }
            }
        }
        Kind::Image => {
            let Some(registry) = registry else {
                return Finding::new(shown, reference, "pinned", None);
            };
            match verify_image(reference, registry) {
                Ok(None) => Finding::new(shown, reference, "pinned", None),
                Ok(Some(detail)) => {
                    *status = (*status).max(FINDINGS);
                    Finding::new(shown, reference, "mismatch", Some(detail))
                }
                Err(detail) => {
                    *status = (*status).max(INCOMPLETE);
                    Finding::new(shown, reference, "incomplete", Some(detail))
                }
            }
        }
        Kind::GitLab => Finding::new(shown, reference, "pinned", None),
    }
}

/// Confirms a pinned image digest still matches its tag, or reports a mismatch / incomplete.
fn verify_image(reference: &Reference, registry: &mut Registry) -> Result<Option<String>, String> {
    let Some((name, digest)) = reference
        .text
        .rsplit_once('@')
        .filter(|(_, d)| is_digest(d))
    else {
        return Ok(None);
    };
    let name = image::parse(name)?;
    let current = registry.resolve(&name)?;
    if current == digest {
        Ok(None)
    } else {
        Ok(Some(format!(
            "tag {} moved to {current}",
            name.tag.as_deref().unwrap_or("latest")
        )))
    }
}

/// Extracts references using the file's platform.
pub fn references(platform: Platform, source: &str) -> Result<Vec<Reference>, ScanError> {
    if platform == Platform::Gitlab {
        let mut references = images::gitlab(source)?;
        references.extend(
            crate::gitlab::references(source)?
                .into_iter()
                .filter(|r| !r.text.starts_with("unsupported YAML ")),
        );
        references.sort_by_key(|r| (r.line, r.column));
        return Ok(references);
    }
    github::references(source)
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

#[derive(Serialize)]
struct Report {
    status: u8,
    results: Vec<Finding>,
    warnings: Vec<Warning>,
}

#[derive(Serialize)]
struct Finding {
    file: String,
    line: usize,
    column: usize,
    text: String,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl Finding {
    fn new(file: &str, reference: &Reference, state: &'static str, detail: Option<String>) -> Self {
        Self {
            file: file.to_owned(),
            line: reference.line,
            column: reference.column,
            text: reference.text.clone(),
            state,
            detail,
        }
    }
}

#[derive(Serialize)]
struct Warning {
    file: String,
    message: String,
}
