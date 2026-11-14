use std::ops::Range;

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
    /// Byte range of the value in the source, including any quotes; empty for warnings.
    pub span: Range<usize>,
    /// Job-level reusable workflows resolve shorthand on the current forge instance.
    pub reusable: bool,
    /// What the reference names, which selects how it is resolved.
    pub kind: Kind,
}

/// The kind of dependency a [`Reference`] names.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// A Git-hosted action or reusable workflow, pinned to a commit.
    Action,
    /// A container image, pinned to a digest.
    Image,
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
