/// A dependency reference found in a CI file, as written in the source.
pub struct Reference {
    /// 1-based line of the reference value.
    pub line: usize,
    /// 1-based column, in characters, of the reference value.
    pub column: usize,
    /// The reference text, such as `actions/checkout@v4`, or the warning message.
    pub text: String,
    /// How lint classifies the reference.
    pub state: State,
}

/// Lint classification of a [`Reference`].
#[derive(Clone, Copy, PartialEq)]
pub enum State {
    /// Names an immutable revision.
    Pinned,
    /// Names a revision that can move; fails lint.
    Unpinned,
    /// Unsupported form that refpinner does not resolve or edit; reported without failing.
    Warning,
}
