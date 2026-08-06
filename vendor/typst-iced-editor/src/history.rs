//! Undo/redo history.

use std::collections::VecDeque;

use crate::cursor::Selection;

/// How many undo steps are kept; the oldest fall off beyond this, so a long
/// session cannot grow the history without bound.
const MAX_UNDO: usize = 1_000;

/// The undo/redo stacks of a document.
///
/// Each entry is a [`Transaction`]: a single text replacement plus the
/// selections before and after it. Consecutive compatible transactions (e.g.
/// typing a word, holding backspace) are merged so that undo works in
/// human-sized steps rather than per keystroke.
#[derive(Debug, Default)]
pub(crate) struct History {
    undo: VecDeque<Transaction>,
    redo: Vec<Transaction>,
}

/// A single undoable step: `removed` was replaced by `inserted` at byte
/// offset `at`.
#[derive(Debug)]
pub(crate) struct Transaction {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
    pub selection_before: Selection,
    pub selection_after: Selection,
    pub kind: EditKind,
}

/// The gesture that produced an edit, used to decide whether consecutive
/// transactions can be merged into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    /// Text typed by the user.
    Typing,
    /// Deletion with backspace.
    Backspace,
    /// Deletion with the delete key.
    DeleteForward,
    /// Anything else (paste, cut, replacing a selection, ...).
    Other,
}

impl History {
    /// Records a transaction, merging it into the previous one when they form
    /// a single logical gesture. Recording clears the redo stack and drops
    /// the oldest step beyond [`MAX_UNDO`].
    pub fn record(&mut self, transaction: Transaction) {
        self.redo.clear();

        if let Some(last) = self.undo.back_mut() {
            if merge(last, &transaction) {
                return;
            }
        }

        self.undo.push_back(transaction);

        if self.undo.len() > MAX_UNDO {
            let _ = self.undo.pop_front();
        }
    }

    /// Takes the most recent transaction for undoing.
    ///
    /// The caller must apply the returned transaction in reverse to the
    /// buffer; it is already moved onto the redo stack.
    pub fn undo(&mut self) -> Option<&Transaction> {
        let transaction = self.undo.pop_back()?;
        self.redo.push(transaction);
        self.redo.last()
    }

    /// Takes the most recent undone transaction for redoing.
    pub fn redo(&mut self) -> Option<&Transaction> {
        let transaction = self.redo.pop()?;
        self.undo.push_back(transaction);
        self.undo.back()
    }
}

/// Tries to merge `next` into `last`. Returns whether it happened.
fn merge(last: &mut Transaction, next: &Transaction) -> bool {
    if last.kind != next.kind {
        return false;
    }

    match next.kind {
        // Typing extends the previous insertion, as long as the caret did not
        // move away and we are not crossing a word or line boundary.
        EditKind::Typing => {
            let continues = next.removed.is_empty() && next.at == last.at + last.inserted.len();

            let starts_word = next.inserted.chars().any(char::is_whitespace)
                && !last.inserted.ends_with(char::is_whitespace);

            if !continues || starts_word || next.inserted.contains('\n') {
                return false;
            }

            last.inserted.push_str(&next.inserted);
            last.selection_after = next.selection_after;
            true
        }
        // Consecutive backspaces grow the removed text backwards.
        EditKind::Backspace => {
            let continues = next.inserted.is_empty()
                && last.inserted.is_empty()
                && next.at + next.removed.len() == last.at
                && !next.removed.contains('\n');

            if !continues {
                return false;
            }

            last.at = next.at;
            last.removed = format!("{}{}", next.removed, last.removed);
            last.selection_after = next.selection_after;
            true
        }
        // Consecutive deletes grow the removed text forwards.
        EditKind::DeleteForward => {
            let continues = next.inserted.is_empty()
                && last.inserted.is_empty()
                && next.at == last.at
                && !next.removed.contains('\n');

            if !continues {
                return false;
            }

            last.removed.push_str(&next.removed);
            last.selection_after = next.selection_after;
            true
        }
        EditKind::Other => false,
    }
}
