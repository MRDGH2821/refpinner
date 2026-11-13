/// A dependency reference found in a CI file, as written in the source.
pub struct Reference {
    /// 1-based line of the reference value.
    pub line: usize,
    /// 1-based column, in characters, of the reference value.
    pub column: usize,
    /// The reference text, such as `actions/checkout@v4`.
    pub text: String,
    /// Whether the reference names an immutable revision.
    pub pinned: bool,
}
