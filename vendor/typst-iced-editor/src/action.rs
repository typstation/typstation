//! Interactions produced by the editor widget.

use std::ops::Range;

use crate::cursor::Motion;

/// An interaction with the editor.
///
/// The [`CodeEditor`](crate::CodeEditor) widget turns raw input events into
/// [`Action`]s and publishes them as messages; the application applies them
/// with [`Content::perform`](crate::Content::perform). Byte offsets are
/// produced by the widget's own hit-testing, so applications can also craft
/// actions programmatically to drive the editor.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Move the caret, collapsing any selection.
    Move(Motion),
    /// Move the caret, extending the selection.
    Select(Motion),
    /// Place the caret at the given byte offset.
    MoveTo(usize),
    /// Extend the selection to the given byte offset.
    SelectTo(usize),
    /// Select the word at the given byte offset.
    SelectWord(usize),
    /// Select the line at the given byte offset.
    SelectLine(usize),
    /// Select the whole document.
    SelectAll,
    /// Insert typed text at the caret, replacing any selection.
    ///
    /// Single delimiters follow the pair-closing rules of the [`Content`]
    /// (auto-close, type-over, surround the selection), unless disabled
    /// with [`Content::set_auto_pairs`].
    ///
    /// [`Content`]: crate::Content
    /// [`Content::set_auto_pairs`]: crate::Content::set_auto_pairs
    Insert(String),
    /// Break the current line, carrying over indentation (see
    /// [`Content::set_auto_indent`](crate::Content::set_auto_indent)).
    Enter,
    /// Indent the selected lines by one level, or insert spaces at the
    /// caret. Produced by Tab.
    Indent,
    /// Remove one indentation level from the selected lines. Produced by
    /// Shift+Tab.
    Unindent,
    /// Paste text at the caret, replacing any selection.
    Paste(String),
    /// Replace an explicit byte range with the given text, leaving the caret
    /// after it. Used to accept completions, but general-purpose.
    Replace {
        /// The byte range to replace.
        range: Range<usize>,
        /// The replacement text.
        text: String,
    },
    /// Apply many non-overlapping replacements as one undoable edit.
    ///
    /// Edits are byte ranges in the original document. They may be supplied
    /// in any order; overlapping ranges are ignored after the first
    /// normalized pass.
    ApplyEdits(Vec<(Range<usize>, String)>),
    /// Move the selected lines one line up.
    MoveLineUp,
    /// Move the selected lines one line down.
    MoveLineDown,
    /// Duplicate the selected lines below themselves.
    DuplicateLine,
    /// Delete the selected lines.
    DeleteLine,
    /// Join the selected line with the next one, or all selected lines.
    JoinLines,
    /// Toggle line comments (`//`) on the selected lines.
    ToggleLineComment,
    /// Toggle block comments (`/* ... */`) around the selection.
    ToggleBlockComment,
    /// Expand the selection to the next syntax or enclosing delimiter range.
    ExpandSelection,
    /// Delete the selection, or the grapheme before the caret.
    Backspace,
    /// Delete the selection, or the grapheme after the caret.
    Delete,
    /// Scroll the viewport by the given amount of logical pixels.
    Scroll {
        /// Horizontal delta, in logical pixels.
        x: f32,
        /// Vertical delta, in logical pixels.
        y: f32,
    },
    /// Scroll the viewport to an absolute logical-pixel offset.
    ///
    /// `None` keeps the current offset for that axis. This is used by
    /// absolute controls like the scrollbar thumb.
    ScrollTo {
        /// Horizontal offset, in logical pixels.
        x: Option<f32>,
        /// Vertical offset, in logical pixels.
        y: Option<f32>,
    },
    /// Collapse or expand the fold that starts on the given zero-based line.
    ToggleFold(usize),
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// The editor is asking for completions at `offset`.
    ///
    /// Emitted when the user opens or refines the completion popup. The
    /// application should compute completions (possibly asynchronously) and
    /// deliver them with
    /// [`Content::set_completions`](crate::Content::set_completions),
    /// passing back the same `id`. Stale replies (an older `id`) are ignored,
    /// so out-of-order responses never flicker the popup.
    RequestCompletions {
        /// The request id, echoed back to `set_completions`.
        id: u64,
        /// The caret byte offset the completions are for.
        offset: usize,
        /// Whether the user explicitly requested completion instead of
        /// opening it through a trigger character.
        explicit: bool,
    },
    /// The editor is asking for hover information at `offset`.
    ///
    /// Delivered back with [`Content::set_hover`](crate::Content::set_hover)
    /// using the same `id`. Diagnostics are resolved by the editor itself and
    /// do not produce this request.
    RequestHover {
        /// The request id, echoed back to `set_hover`.
        id: u64,
        /// The byte offset under the pointer.
        offset: usize,
    },
}

impl Action {
    /// Returns whether the action is an asynchronous intelligence request
    /// (completions or hover) rather than an edit or a caret movement.
    pub fn is_request(&self) -> bool {
        matches!(
            self,
            Self::RequestCompletions { .. } | Self::RequestHover { .. }
        )
    }

    /// Returns whether the action modifies the text of the document.
    ///
    /// Useful to know when to kick off recompilation, linting, etc.
    pub fn is_edit(&self) -> bool {
        matches!(
            self,
            Self::Insert(_)
                | Self::Enter
                | Self::Indent
                | Self::Unindent
                | Self::Paste(_)
                | Self::Replace { .. }
                | Self::ApplyEdits(_)
                | Self::MoveLineUp
                | Self::MoveLineDown
                | Self::DuplicateLine
                | Self::DeleteLine
                | Self::JoinLines
                | Self::ToggleLineComment
                | Self::ToggleBlockComment
                | Self::Backspace
                | Self::Delete
                | Self::Undo
                | Self::Redo
        )
    }
}
