use std::io::{self, IsTerminal};
use std::process::ExitCode;

use crate::discover::Scan;
use crate::edit::Edit;
use crate::pin::{self, Change, line_end};
use crate::reference::{Reference, State};
use crate::resolve::GitHub;
use crate::version::{self, Limit};

/// Previews updates of the pinned references in the files selected by `scan`.
///
/// Each pin follows its annotation: an exact version moves to the highest version tag that
/// `limit` allows, and a branch or floating tag moves to the commit it names now. An
/// unannotated pin follows the default branch only with `follow_default_branch`, or after
/// the user confirms the previewed update in an interactive write. With `write`, updates are
/// applied as `pin --write` applies pins.
pub fn run(scan: &Scan, write: bool, limit: Limit, follow_default_branch: bool) -> ExitCode {
    let mut github = GitHub::from_env();
    let follow = follow_default_branch || (write && io::stdin().is_terminal());
    pin::apply(scan, write, State::Pinned, "update", |source, reference| {
        plan(source, reference, &mut github, limit, follow)
    })
}

/// Computes the update of one pinned reference, or `None` when it is up to date.
fn plan(
    source: &str,
    reference: &Reference,
    github: &mut GitHub,
    limit: Limit,
    follow: bool,
) -> Result<Option<Change>, String> {
    let (action, commit) = reference
        .text
        .rsplit_once('@')
        .ok_or("reference has no @revision")?;
    let raw = &source[reference.span.clone()];
    let quoted = raw.starts_with(['"', '\'']);
    if raw.get(usize::from(quoted)..raw.len() - usize::from(quoted)) != Some(&reference.text) {
        return Err(String::from(
            "cannot edit a reference with escapes or line breaks",
        ));
    }
    let revision_end = reference.span.end - usize::from(quoted);
    let change = |summary: String, new: String, at: (usize, usize), annotation: String| Change {
        summary,
        value: format!("{action}@{new}"),
        edits: vec![
            Edit {
                start: revision_end - commit.len(),
                end: revision_end,
                text: new,
            },
            Edit {
                start: at.0,
                end: at.1,
                text: annotation,
            },
        ],
    };
    let Some(note) = annotation(source, reference.span.end) else {
        if !follow {
            return Err(String::from(
                "pin has no annotation; pass --follow-default-branch to follow the default branch",
            ));
        }
        let branch = github.default_branch(action)?;
        let tip = github.resolve(action, &branch)?.commit;
        let at = (reference.span.end, reference.span.end);
        return Ok((tip != commit).then(|| {
            change(
                format!("default branch {branch}"),
                tip,
                at,
                format!(" # {branch}"),
            )
        }));
    };
    let at = (note.start, note.end);
    let Some(current) = version::parse(&note.version) else {
        // A branch or other moving ref: its new commit is the update.
        let tip = github.resolve(action, &note.version)?.commit;
        let summary = format!("{} moved", note.version);
        return Ok((tip != commit).then(|| change(summary, tip, at, note.version.clone())));
    };
    let (exact, current, track) = if current.len() >= 3 || note.track.is_some() {
        let tagged = github.resolve(action, &note.version)?.commit;
        if tagged != commit {
            return Err(format!(
                "tag {} moved to {tagged}; review it before updating",
                note.version
            ));
        }
        (note.version.clone(), current, note.track.clone())
    } else if let Some(exact) = github.exact(action, &note.version, commit)? {
        let parsed = version::parse(&exact).ok_or("exact version is not a version")?;
        (exact, parsed, Some(note.version.clone()))
    } else if limit == Limit::Patch {
        return Err(format!(
            "cannot find the exact version of {commit} on track {}",
            note.version
        ));
    } else {
        // Without an exact version the floating tag itself is the track to follow.
        let moved = github.resolve(action, &note.version)?;
        let summary = format!("track {} moved", note.version);
        let text = match moved.version.filter(|exact| *exact != note.version) {
            Some(exact) => format!("{exact} (track: {})", note.version),
            None => note.version.clone(),
        };
        return Ok((moved.commit != commit).then(|| change(summary, moved.commit, at, text)));
    };
    let prefix = &exact[..exact.len()
        - exact
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .len()];
    let best = github
        .tags(action, prefix)?
        .into_iter()
        .filter(|name| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        })
        .filter_map(|name| Some((version::parse(&name)?, name)))
        .filter(|(candidate, _)| limit.allows(&current, candidate))
        .max();
    let Some((chosen, name)) = best else {
        return Ok(None);
    };
    let tip = github.resolve(action, &name)?.commit;
    // A track keeps its precision and follows the version, so a major upgrade moves `v4` to `v5`.
    let track = track.map(|track| match version::parse(&track) {
        Some(parts) if parts.len() <= chosen.len() => {
            let kept: Vec<String> = chosen[..parts.len()].iter().map(u64::to_string).collect();
            format!("{prefix}{}", kept.join("."))
        }
        _ => track,
    });
    let text = match track {
        Some(track) => format!("{name} (track: {track})"),
        None => name.clone(),
    };
    Ok(Some(change(format!("{exact} -> {name}"), tip, at, text)))
}

/// A pin annotation such as `# v4.2.1 (track: v4)` or `# main`.
struct Annotation {
    /// Byte range in the source of the version and track, after `# `.
    start: usize,
    end: usize,
    /// The exact version, floating tag or branch.
    version: String,
    track: Option<String>,
}

/// Parses the annotation in the comment after a value that ends at `end`.
///
/// The annotation is the first word of the comment, optionally followed by
/// ` (track: <track>)`. A version word may be followed by any text, as in
/// `# v8.1.3, 2026-11-10`. Any other word must end the line or come before another ` #`
/// comment, so a comment such as `# build step` is not an annotation.
fn annotation(source: &str, end: usize) -> Option<Annotation> {
    let after = &source[end..line_end(source, end)];
    let comment = after.trim_start().strip_prefix('#')?;
    let body = comment.trim_start_matches([' ', '\t']);
    let start = end + after.len() - body.len();
    let word = body
        .split([' ', '\t', ','])
        .next()
        .filter(|w| !w.is_empty())?;
    let mut rest = &body[word.len()..];
    let mut track = None;
    if let Some((name, tail)) = rest
        .strip_prefix(" (track: ")
        .and_then(|tail| tail.split_once(')'))
    {
        track = Some(name.to_owned());
        rest = tail;
    }
    let trimmed = rest.trim_start_matches([' ', '\t']);
    if version::parse(word).is_none() && !(trimmed.is_empty() || trimmed.starts_with('#')) {
        return None;
    }
    Some(Annotation {
        start,
        end: start + body.len() - rest.len(),
        version: word.to_owned(),
        track,
    })
}
