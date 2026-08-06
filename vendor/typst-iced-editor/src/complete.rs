//! Autocomplete and hover data, plus built-in providers.
//!
//! The editor is decoupled from any particular source of intelligence: it
//! emits [`Action::RequestCompletions`] / [`Action::RequestHover`] and the
//! application answers asynchronously with
//! [`Content::set_completions`] / [`Content::set_hover`]. A provider is thus
//! just a function from the current [`Buffer`] and a byte offset to some
//! results — a Typst compile [`World`], a language server, or the built-in
//! [`document_words`] all fit that shape.
//!
//! [`Action::RequestCompletions`]: crate::Action::RequestCompletions
//! [`Action::RequestHover`]: crate::Action::RequestHover
//! [`Content::set_completions`]: crate::Content::set_completions
//! [`Content::set_hover`]: crate::Content::set_hover
//! [`World`]: https://docs.rs/typst

use std::collections::BTreeSet;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::Buffer;

/// A single completion candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// The text shown in the popup.
    pub label: String,
    /// Extra text shown dimmed after the label (a type, a signature…).
    pub detail: Option<String>,
    /// The byte range replaced when the completion is accepted.
    pub replace: Range<usize>,
    /// The text inserted in place of `replace`.
    pub insert: String,
}

impl Completion {
    /// Creates a completion that replaces `replace` with `label`.
    pub fn new(replace: Range<usize>, label: impl Into<String>) -> Self {
        let label = label.into();

        Self {
            replace,
            insert: label.clone(),
            label,
            detail: None,
        }
    }

    /// Sets the inserted text, when it differs from the label.
    pub fn insert(mut self, insert: impl Into<String>) -> Self {
        self.insert = insert.into();
        self
    }

    /// Sets the detail shown after the label.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// The contents of a hover tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    /// The byte range the tooltip describes.
    pub range: Range<usize>,
    /// The tooltip text. May contain newlines.
    pub content: String,
}

/// The word being typed just before `offset`: its range and text.
///
/// A word is a run of alphanumeric characters, `_`, or `-`. Returns an empty
/// range at the caret when there is no word.
pub fn word_before(buffer: &Buffer, offset: usize) -> (Range<usize>, &str) {
    let text = buffer.text();
    let offset = buffer.clamp(offset);

    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';

    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(offset);

    (start..offset, &text[start..offset])
}

/// A completion provider that suggests words already present in the document.
///
/// A dependency-free default: it needs no compiler or language server, and
/// is handy for prose. Completions are the distinct words sharing the prefix
/// under the caret, minus the prefix itself.
pub fn document_words(buffer: &Buffer, offset: usize) -> Vec<Completion> {
    let (range, prefix) = word_before(buffer, offset);

    if prefix.len() < 2 {
        return Vec::new();
    }

    let words: BTreeSet<&str> = buffer
        .text()
        .unicode_words()
        .filter(|word| word.len() > prefix.len() && word.starts_with(prefix))
        .collect();

    words
        .into_iter()
        .take(50)
        .map(|word| Completion::new(range.clone(), word))
        .collect()
}
