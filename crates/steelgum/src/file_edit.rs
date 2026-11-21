use std::fs;
use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

pub(crate) fn remove_json(source: &str, start: usize, end: usize) -> String {
    let following = source[end..].trim_start();
    let preceding = source[..start].trim_end();
    let mut changed = source.to_owned();
    let range = if following.starts_with(',') {
        start..source.len() - following.len() + 1
    } else if preceding.ends_with(',') {
        preceding.len() - 1..end
    } else {
        start..end
    };
    changed.replace_range(range, "");
    changed
}

/// Insert a JSON entry before a collection's closing delimiter.
pub(crate) fn append_json(
    source: &mut String,
    range: std::ops::Range<usize>,
    entry: &str,
    comma: bool,
) {
    let inner = &source[range.start..range.end - 1];
    let insert_at = range.start + inner.trim_end().len();
    let trailing = &source[insert_at..range.end - 1];
    let prefix = if comma { "," } else { "" };
    let addition = match trailing.rsplit_once('\n') {
        Some((_, indent)) => format!("{prefix}\n{indent}  {entry}"),
        None => format!("{prefix} {entry}"),
    };
    source.insert_str(insert_at, &addition);
}

/// Publish a validated edit after checking the reviewed file and its path again.
pub(crate) fn replace_unchanged(
    requested: &Path,
    target: &Path,
    expected: &[u8],
    changed: &[u8],
) -> io::Result<bool> {
    let parent = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(changed)?;
    temporary
        .as_file()
        .set_permissions(fs::metadata(target)?.permissions())?;
    temporary.as_file().sync_all()?;
    if requested.canonicalize()? != target || fs::read(target)? != expected {
        return Ok(false);
    }
    temporary.persist(target).map_err(|error| error.error)?;
    Ok(true)
}
