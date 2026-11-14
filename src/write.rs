use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use saphyr_parser::{Event, Parser};

use crate::edit::{self, Edit};

/// A scalar the edits must change: its 1-based line and column in the original, and the
/// value it must hold afterwards.
pub enum Value {
    /// Replace one scalar without changing its style or surrounding structure.
    Scalar(usize, usize, String),
    /// Add a plain ref key/value immediately after an existing GitLab project value.
    GitLabRef(usize, usize, String),
}

/// Applies `edits` to `original`, the text read from `path`, and replaces the file.
///
/// The replacement must parse to the same YAML events as `original`, except that each
/// scalar listed in `values` must hold its new value. Before the file changes, its exact
/// bytes go to the first free `<name>.bkp`, `<name>.bkp2`, ... with the same permissions.
/// The new text goes to a temporary file that is renamed over the original. Returns the
/// backup path. On any error the original file is unchanged and no backup is left.
pub fn replace(
    path: &Path,
    original: &str,
    edits: &[Edit],
    values: &[Value],
) -> Result<PathBuf, String> {
    let replacement = edit::apply(original, edits);
    validate(original, &replacement, values)?;
    let meta = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !meta.is_file() {
        return Err(String::from("not a regular file"));
    }
    // Another process may have changed the file after it was read.
    if fs::read(path).map_err(|error| error.to_string())? != original.as_bytes() {
        return Err(String::from("file changed while refpinner was running"));
    }
    let backup = backup(path, original.as_bytes(), &meta.permissions())
        .map_err(|error| format!("cannot create backup: {error}"))?;
    let temp = sibling(path, &format!(".{}.tmp", std::process::id()), true);
    let written = create(&temp, replacement.as_bytes(), &meta.permissions())
        .and_then(|()| fs::rename(&temp, path));
    if let Err(error) = written {
        // Backups exist only for files that changed.
        let _ = fs::remove_file(&temp);
        let _ = fs::remove_file(&backup);
        return Err(format!("cannot replace file: {error}"));
    }
    Ok(backup)
}

/// Saves `bytes` to the first backup name that does not exist; symlinks count as existing.
fn backup(path: &Path, bytes: &[u8], permissions: &fs::Permissions) -> io::Result<PathBuf> {
    for number in 1.. {
        let suffix = if number == 1 {
            String::from(".bkp")
        } else {
            format!(".bkp{number}")
        };
        let candidate = sibling(path, &suffix, false);
        match create(&candidate, bytes, permissions) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            result => return result.map(|()| candidate),
        }
    }
    unreachable!("backup numbers are unbounded")
}

/// Creates a new file holding `bytes`. `create_new` fails on any existing name, including
/// a dangling symlink, so no symlink is followed.
fn create(path: &Path, bytes: &[u8], permissions: &fs::Permissions) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    // Set permissions before writing so a private file is never readable by others.
    let result = file
        .set_permissions(permissions.clone())
        .and_then(|()| file.write_all(bytes))
        .and_then(|()| file.sync_all());
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

/// `path` with `suffix` appended to its file name, optionally hidden with a leading `.`.
fn sibling(path: &Path, suffix: &str, hidden: bool) -> PathBuf {
    let mut name = std::ffi::OsString::from(if hidden { "." } else { "" });
    name.push(path.file_name().unwrap_or_default());
    name.push(suffix);
    path.with_file_name(name)
}

/// Checks that `replacement` differs from `original` only in the scalars named by `values`.
pub fn validate(original: &str, replacement: &str, values: &[Value]) -> Result<(), String> {
    let invalid =
        |error: saphyr_parser::ScanError| format!("edited file is not valid YAML: {error}");
    let before: Vec<_> = Parser::new_from_str(original)
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    let after: Vec<_> = Parser::new_from_str(replacement)
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    let changed = || String::from("edits changed more than the references");
    let mut after = after.iter();
    let mut matched = 0;
    for (index, (old, span)) in before.iter().enumerate() {
        let Some((new, _)) = after.next() else {
            return Err(changed());
        };
        let expected = values.iter().find(|value| {
            let (line, column) = match value {
                Value::Scalar(line, column, _) | Value::GitLabRef(line, column, _) => {
                    (line, column)
                }
            };
            (*line, *column) == (span.start.line(), span.start.col() + 1)
        });
        if let Some(Value::GitLabRef(_, _, commit)) = expected {
            use saphyr_parser::ScalarStyle::Plain;
            let project_key = matches!(before.get(index.wrapping_sub(1)), Some((Event::Scalar(key, _, 0, None), _)) if key == "project");
            if !project_key || old != new || !matches!(old, Event::Scalar(_, _, 0, None)) {
                return Err(changed());
            }
            match (after.next(), after.next()) {
                (
                    Some((Event::Scalar(key, Plain, 0, None), _)),
                    Some((Event::Scalar(value, Plain, 0, None), _)),
                ) if key == "ref" && value == commit => {
                    matched += 1;
                    continue;
                }
                _ => return Err(changed()),
            }
        }
        match (old, new, expected) {
            (
                Event::Scalar(_, style, anchor, tag),
                Event::Scalar(value, new_style, new_anchor, new_tag),
                Some(Value::Scalar(_, _, expected)),
            ) if value == expected && (style, anchor, tag) == (new_style, new_anchor, new_tag) => {
                matched += 1;
            }
            (_, _, None) if old == new => {}
            _ => return Err(changed()),
        }
    }
    if matched == values.len() && after.next().is_none() {
        Ok(())
    } else {
        Err(changed())
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, validate};

    #[test]
    fn accepts_only_the_expected_scalar_changes() {
        let original = "a: x@v1 # c\nb: y\n";
        let values = [Value::Scalar(1, 4, String::from("x@abc"))];
        assert_eq!(
            validate(original, "a: x@abc # v1 # c\nb: y\n", &values),
            Ok(())
        );
        assert!(validate(original, "a: x@abc\nb: z\n", &values).is_err());
        assert!(validate(original, "a: x@abd\nb: y\n", &values).is_err());
        assert!(validate(original, "a: x@abc\nb: y\nc: d\n", &values).is_err());
        assert!(validate(original, "a: [x@abc\n", &values).is_err());
        assert!(validate(original, original, &values).is_err());
    }
}
