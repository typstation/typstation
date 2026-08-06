//! Diagnostics shown as squiggly underlines.
//!
//! Diagnostics are fed in from outside the editor — typically from your
//! Typst compile loop or a language server — with
//! [`Content::set_diagnostics`](crate::Content::set_diagnostics). Their
//! ranges are anchored, so the underlines keep tracking the right text as
//! the user edits, until fresh diagnostics arrive.

use std::ops::Range;

/// The severity of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// A hint, drawn most subtly.
    Hint,
    /// Informational.
    Info,
    /// A warning.
    Warning,
    /// An error, drawn most prominently. The highest severity.
    Error,
}

/// A diagnostic attached to a range of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The byte range the diagnostic applies to.
    ///
    /// An empty range still underlines a single caret-width slot, so
    /// zero-width diagnostics remain visible.
    pub range: Range<usize>,
    /// How severe the diagnostic is.
    pub severity: Severity,
    /// The message shown when hovering the range.
    pub message: String,
}

impl Diagnostic {
    /// Creates a new [`Diagnostic`].
    pub fn new(range: Range<usize>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            range,
            severity,
            message: message.into(),
        }
    }

    /// Creates an error diagnostic.
    pub fn error(range: Range<usize>, message: impl Into<String>) -> Self {
        Self::new(range, Severity::Error, message)
    }

    /// Creates a warning diagnostic.
    pub fn warning(range: Range<usize>, message: impl Into<String>) -> Self {
        Self::new(range, Severity::Warning, message)
    }
}
