/// A replacement of the bytes `start..end` of a file's original text.
pub struct Edit {
    /// Byte offset where the replaced text starts.
    pub start: usize,
    /// Byte offset just after the replaced text; equal to `start` for an insertion.
    pub end: usize,
    /// The replacement text.
    pub text: String,
}

/// Applies non-overlapping edits to `source`, leaving every other byte unchanged.
pub fn apply(source: &str, edits: &[Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|edit| (edit.start, edit.end));
    let mut result = String::with_capacity(source.len());
    let mut copied = 0;
    for edit in sorted {
        result.push_str(&source[copied..edit.start]);
        result.push_str(&edit.text);
        copied = edit.end;
    }
    result.push_str(&source[copied..]);
    result
}
