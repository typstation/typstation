//! Cursor and selection model.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::Buffer;

/// A selection in the buffer, in byte offsets.
///
/// `anchor` is where the selection started and `head` is the side that moves;
/// the caret is drawn at `head`. When both are equal, the selection is just a
/// caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// The fixed end of the selection.
    pub anchor: usize,
    /// The moving end of the selection, where the caret is.
    pub head: usize,
    /// The column the caret "wants" during vertical movement, so that moving
    /// through short lines does not lose the horizontal position.
    goal_column: Option<usize>,
}

impl Selection {
    /// Creates a caret (an empty selection) at the given byte offset.
    pub fn caret(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    /// Creates a selection between the given byte offsets.
    pub fn new(anchor: usize, head: usize) -> Self {
        Self {
            anchor,
            head,
            goal_column: None,
        }
    }

    /// Returns the selected byte range, normalized so that start ≤ end.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// Returns whether the selection is just a caret.
    pub fn is_caret(&self) -> bool {
        self.anchor == self.head
    }

    fn with_goal(mut self, goal: Option<usize>) -> Self {
        self.goal_column = goal;
        self
    }

    /// Applies a [`Motion`] to the selection.
    ///
    /// With `extend`, the anchor stays in place and only the head moves;
    /// otherwise the selection collapses to the new caret position.
    /// `page_lines` is the viewport height in lines, used by
    /// [`Motion::PageUp`] and [`Motion::PageDown`].
    pub(crate) fn apply_motion(
        self,
        buffer: &Buffer,
        motion: Motion,
        extend: bool,
        page_lines: usize,
    ) -> Selection {
        let text = buffer.text();
        let head = buffer.clamp(self.head);

        // A plain horizontal motion on a non-empty selection collapses it to
        // one of its edges instead of moving the caret.
        if !extend && !self.is_caret() {
            match motion {
                Motion::Left => return Selection::caret(self.range().start),
                Motion::Right => return Selection::caret(self.range().end),
                _ => {}
            }
        }

        let (target, goal) = match motion {
            Motion::Left => (prev_grapheme_boundary(text, head), None),
            Motion::Right => (next_grapheme_boundary(text, head), None),
            Motion::WordLeft => (prev_word_boundary(text, head), None),
            Motion::WordRight => (next_word_boundary(text, head), None),
            Motion::Home => (
                buffer.line_content_range(buffer.byte_to_line(head)).start,
                None,
            ),
            Motion::End => (
                buffer.line_content_range(buffer.byte_to_line(head)).end,
                None,
            ),
            Motion::DocumentStart => (0, None),
            Motion::DocumentEnd => (text.len(), None),
            Motion::Up => vertical(buffer, head, self.goal_column, -1),
            Motion::Down => vertical(buffer, head, self.goal_column, 1),
            Motion::PageUp => vertical(
                buffer,
                head,
                self.goal_column,
                -(page_lines.max(1) as isize),
            ),
            Motion::PageDown => {
                vertical(buffer, head, self.goal_column, page_lines.max(1) as isize)
            }
        };

        let anchor = if extend { self.anchor } else { target };

        Selection::new(anchor, target).with_goal(goal)
    }
}

fn vertical(
    buffer: &Buffer,
    head: usize,
    goal: Option<usize>,
    lines: isize,
) -> (usize, Option<usize>) {
    let position = buffer.position_of(head);
    let goal = goal.unwrap_or(position.column);
    let target_line = position.line.saturating_add_signed(lines);

    // Moving past the edges of the document lands on its start or end.
    if lines < 0 && position.line == 0 {
        return (0, Some(goal));
    }

    if lines > 0 && target_line >= buffer.line_count() {
        return (buffer.len(), Some(goal));
    }

    let target = buffer.byte_of(crate::Position {
        line: target_line,
        column: goal,
    });

    (target, Some(goal))
}

/// A cursor movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Move to the previous grapheme.
    Left,
    /// Move to the next grapheme.
    Right,
    /// Move one line up.
    Up,
    /// Move one line down.
    Down,
    /// Move to the start of the previous word.
    WordLeft,
    /// Move past the end of the next word.
    WordRight,
    /// Move to the start of the line.
    Home,
    /// Move to the end of the line.
    End,
    /// Move one viewport up.
    PageUp,
    /// Move one viewport down.
    PageDown,
    /// Move to the start of the document.
    DocumentStart,
    /// Move to the end of the document.
    DocumentEnd,
}

impl Motion {
    /// Widens the motion, as done when a "jump" modifier (Ctrl) is held.
    pub fn widen(self) -> Self {
        match self {
            Self::Left => Self::WordLeft,
            Self::Right => Self::WordRight,
            Self::Home => Self::DocumentStart,
            Self::End => Self::DocumentEnd,
            motion => motion,
        }
    }
}

/// Returns the previous grapheme boundary before `from`.
pub(crate) fn prev_grapheme_boundary(text: &str, from: usize) -> usize {
    text[..from]
        .graphemes(true)
        .next_back()
        .map(|grapheme| from - grapheme.len())
        .unwrap_or(0)
}

/// Returns the next grapheme boundary after `from`.
pub(crate) fn next_grapheme_boundary(text: &str, from: usize) -> usize {
    text[from..]
        .graphemes(true)
        .next()
        .map(|grapheme| from + grapheme.len())
        .unwrap_or(text.len())
}

/// Returns the position of the start of the word before `from`, skipping any
/// whitespace in between.
pub(crate) fn prev_word_boundary(text: &str, from: usize) -> usize {
    let mut boundary = from;

    for (start, word) in text[..from].split_word_bound_indices().rev() {
        boundary = start;

        if !word.chars().all(char::is_whitespace) {
            break;
        }
    }

    boundary
}

/// Returns the position past the end of the word after `from`, skipping any
/// whitespace in between.
pub(crate) fn next_word_boundary(text: &str, from: usize) -> usize {
    let mut boundary = from;

    for (start, word) in text[from..].split_word_bound_indices() {
        boundary = from + start + word.len();

        if !word.chars().all(char::is_whitespace) {
            break;
        }
    }

    boundary
}

/// Returns the range of the word (or whitespace run) at the given offset.
///
/// An offset exactly at a word's right edge selects that word rather than
/// the whitespace after it, matching double-click expectations.
pub(crate) fn word_range_at(text: &str, offset: usize) -> Range<usize> {
    let offset = offset.min(text.len());
    let mut previous: Option<(Range<usize>, bool)> = None;

    for (start, word) in text.split_word_bound_indices() {
        let range = start..start + word.len();
        let is_word = !word.chars().all(char::is_whitespace);

        if range.start <= offset && offset < range.end {
            if is_word || offset > range.start {
                return range;
            }

            // Exactly on the boundary between a word and the whitespace
            // that follows it: the word wins.
            return match previous {
                Some((word, true)) => word,
                _ => range,
            };
        }

        previous = Some((range, is_word));
    }

    // Past every segment: the very end of the text.
    match previous {
        Some((word, true)) if word.end == offset => word,
        _ => offset..offset,
    }
}
