//! Positions that survive edits.
//!
//! An [`Anchor`] tracks a byte offset through every edit of the document:
//! text inserted before it pushes it forward, deletions spanning it collapse
//! it to the start of the deletion, and so on. Anchors are the foundation
//! for anything that must stay attached to a piece of text — diagnostics,
//! search matches, bookmarks. Folds use separate line-based remapping.

use std::collections::HashMap;
use std::ops::Range;

/// The side an anchor sticks to when text is inserted exactly at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    /// The anchor stays before the inserted text.
    Before,
    /// The anchor moves after the inserted text.
    After,
}

/// A handle to an anchored position, created with
/// [`Content::create_anchor`](crate::Content::create_anchor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Anchor(u64);

/// The set of live anchors of a document.
#[derive(Debug, Default)]
pub(crate) struct Anchors {
    next: u64,
    positions: HashMap<Anchor, (usize, Bias)>,
}

impl Anchors {
    /// Registers a new anchor at the given byte offset.
    pub fn create(&mut self, offset: usize, bias: Bias) -> Anchor {
        let anchor = Anchor(self.next);
        self.next += 1;

        self.positions.insert(anchor, (offset, bias));
        anchor
    }

    /// Returns the current byte offset of an anchor.
    pub fn get(&self, anchor: Anchor) -> Option<usize> {
        self.positions.get(&anchor).map(|(offset, _)| *offset)
    }

    /// Drops an anchor, returning its last position.
    pub fn remove(&mut self, anchor: Anchor) -> Option<usize> {
        self.positions.remove(&anchor).map(|(offset, _)| offset)
    }

    /// Adjusts every anchor after `replaced` was substituted by `new_len`
    /// bytes of text.
    pub fn update(&mut self, replaced: &Range<usize>, new_len: usize) {
        for (offset, bias) in self.positions.values_mut() {
            *offset = adjust(*offset, *bias, replaced, new_len);
        }
    }
}

fn adjust(offset: usize, bias: Bias, replaced: &Range<usize>, new_len: usize) -> usize {
    let is_insertion_at_anchor = replaced.is_empty() && offset == replaced.start;

    if offset < replaced.start
        || (offset == replaced.start && !(is_insertion_at_anchor && bias == Bias::After))
    {
        // Entirely before the edit (an insertion exactly at the anchor only
        // pushes it when it prefers the `After` side).
        offset
    } else if offset >= replaced.end {
        // Entirely after the edit: shift by the change in length.
        offset - replaced.len() + new_len
    } else {
        // Inside the replaced region: collapse to its start.
        replaced.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjust_around_edits() {
        // Insertion before, at (both biases), and after the anchor.
        assert_eq!(adjust(5, Bias::Before, &(0..0), 3), 8);
        assert_eq!(adjust(5, Bias::Before, &(5..5), 3), 5);
        assert_eq!(adjust(5, Bias::After, &(5..5), 3), 8);
        assert_eq!(adjust(5, Bias::Before, &(7..7), 3), 5);

        // Deletion before, spanning, and after the anchor.
        assert_eq!(adjust(5, Bias::Before, &(0..2), 0), 3);
        assert_eq!(adjust(5, Bias::Before, &(3..8), 0), 3);
        assert_eq!(adjust(5, Bias::Before, &(6..8), 0), 5);

        // Replacement spanning the anchor collapses to its start.
        assert_eq!(adjust(5, Bias::After, &(4..6), 10), 4);
    }
}
