//! Delimiter pairs: the canonical tables and the matching scanners.
//!
//! Auto-closing while typing, surrounding a selection, expanding a selection
//! to an enclosing pair, and highlighting the pair at the caret all consult
//! these tables, so a delimiter behaves consistently across features.

use std::ops::Range;

use typst_syntax::{LinkedNode, Side, SyntaxKind};

use crate::buffer::Buffer;

/// The delimiter pairs that auto-close when typed.
pub(crate) const AUTO_PAIRS: &[(char, char)] =
    &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('$', '$')];

/// The delimiters that wrap a selection when typed over it. A superset of
/// [`AUTO_PAIRS`] with the Typst markup delimiters.
pub(crate) const SURROUND_PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('"', '"'),
    ('$', '$'),
    ('*', '*'),
    ('_', '_'),
    ('`', '`'),
];

/// The brackets that increase indentation when a line breaks after them.
pub(crate) const INDENT_BRACKETS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];

/// The auto-closing partner of `open`, if it has one.
pub(crate) fn auto_closing(open: char) -> Option<char> {
    AUTO_PAIRS
        .iter()
        .find(|(o, _)| *o == open)
        .map(|(_, close)| *close)
}

/// Whether `c` closes one of the [`AUTO_PAIRS`].
pub(crate) fn is_auto_closing(c: char) -> bool {
    AUTO_PAIRS.iter().any(|(_, close)| *close == c)
}

/// Whether `c` closes one of the [`INDENT_BRACKETS`].
pub(crate) fn is_close_bracket(c: char) -> bool {
    INDENT_BRACKETS.iter().any(|(_, close)| *close == c)
}

/// The [`AUTO_PAIRS`] ranges that enclose `current`, innermost first,
/// scanning outward from its start.
pub(crate) fn enclosing_delimiters(text: &str, current: Range<usize>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();

    for (open_at, open) in text[..current.start].char_indices().rev() {
        let Some(close) = auto_closing(open) else {
            continue;
        };

        let close_at = if open == close {
            // Identical pairs (quotes, `$`) cannot nest: the first partner
            // after the opener closes it.
            text[open_at + open.len_utf8()..]
                .char_indices()
                .find(|(_, ch)| *ch == close)
                .map(|(relative, _)| open_at + open.len_utf8() + relative)
        } else {
            find_closing_delimiter(text, open_at, open, close)
        };

        if let Some(close_at) = close_at {
            let end = close_at + close.len_utf8();
            if current.end <= end {
                ranges.push(open_at..end);
            }
        }
    }

    ranges
}

/// The ranges of the matched delimiter pair at the caret, for highlighting:
/// a bracket right before or after the caret together with its partner, or
/// the `$` pair of the equation whose delimiter touches the caret.
pub(crate) fn matching_delimiter_ranges(buffer: &Buffer, caret: usize) -> Vec<Range<usize>> {
    let text = buffer.text();
    let caret = buffer.clamp(caret);

    let before = text[..caret]
        .char_indices()
        .next_back()
        .map(|(index, ch)| (index, ch, true));
    let after = text[caret..]
        .char_indices()
        .next()
        .map(|(index, ch)| (caret + index, ch, false));

    for (at, ch, prefer_before) in [before, after].into_iter().flatten() {
        // `$` both opens and closes an equation: only the syntax tree can
        // tell which one this is — or whether it is math at all (the same
        // character appears inside strings and raw blocks).
        if ch == '$' {
            if let Some(pair) = dollar_pair(buffer, at) {
                return pair;
            }

            continue;
        }

        if let Some(close) = bracket_closing(ch) {
            if let Some(other) = find_closing_delimiter(text, at, ch, close) {
                return vec![at..at + ch.len_utf8(), other..other + close.len_utf8()];
            }
        } else if let Some(open) = bracket_opening(ch) {
            if let Some(other) = find_opening_delimiter(text, at, open, ch) {
                return if prefer_before {
                    vec![other..other + open.len_utf8(), at..at + ch.len_utf8()]
                } else {
                    vec![at..at + ch.len_utf8(), other..other + open.len_utf8()]
                };
            }
        }
    }

    Vec::new()
}

/// The two delimiter ranges of the equation whose `$` sits at `at`, if any.
fn dollar_pair(buffer: &Buffer, at: usize) -> Option<Vec<Range<usize>>> {
    let root = LinkedNode::new(buffer.root());
    let leaf = root.leaf_at(at + 1, Side::Before)?;

    if leaf.kind() != SyntaxKind::Dollar || leaf.offset() != at {
        return None;
    }

    let equation = leaf.parent()?;
    if equation.kind() != SyntaxKind::Equation {
        return None;
    }

    let mut dollars = equation
        .children()
        .filter(|child| child.kind() == SyntaxKind::Dollar);
    let open = dollars.next()?.range();
    let close = dollars.next_back()?.range();

    Some(vec![open, close])
}

/// The closing partner of an [`INDENT_BRACKETS`] opener, for matching.
fn bracket_closing(open: char) -> Option<char> {
    INDENT_BRACKETS
        .iter()
        .find(|(o, _)| *o == open)
        .map(|(_, close)| *close)
}

/// The opening partner of an [`INDENT_BRACKETS`] closer, for matching.
fn bracket_opening(close: char) -> Option<char> {
    INDENT_BRACKETS
        .iter()
        .find(|(_, c)| *c == close)
        .map(|(open, _)| *open)
}

/// The offset of the closer matching the `open` at `open_at`, scanning
/// forward and skipping nested pairs.
fn find_closing_delimiter(text: &str, open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;

    for (relative, ch) in text[open_at..].char_indices() {
        let at = open_at + relative;

        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);

            if depth == 0 && at != open_at {
                return Some(at);
            }
        }
    }

    None
}

/// The offset of the opener matching the `close` at `close_at`, scanning
/// backward and skipping nested pairs.
fn find_opening_delimiter(text: &str, close_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;

    for (at, ch) in text[..=close_at].char_indices().rev() {
        if ch == close {
            depth += 1;
        } else if ch == open {
            depth = depth.saturating_sub(1);

            if depth == 0 && at != close_at {
                return Some(at);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_match_by_depth() {
        let buffer = Buffer::from_text("#f((a), b)");

        // Caret after the inner opener pairs it with the inner closer.
        assert_eq!(matching_delimiter_ranges(&buffer, 4), vec![3..4, 5..6]);

        // Caret after the outer opener skips the nested pair.
        assert_eq!(matching_delimiter_ranges(&buffer, 3), vec![2..3, 9..10]);
    }

    #[test]
    fn dollar_matches_its_own_equation() {
        let buffer = Buffer::from_text("$x$ and $y$");

        // Caret after the first closing `$`: it pairs with the first opener,
        // not with the opener of the following equation.
        assert_eq!(matching_delimiter_ranges(&buffer, 3), vec![0..1, 2..3]);

        // Caret before the second opening `$`.
        assert_eq!(matching_delimiter_ranges(&buffer, 8), vec![8..9, 10..11]);
    }

    #[test]
    fn dollar_inside_a_string_does_not_match() {
        let buffer = Buffer::from_text("#str(\"a$b\")");

        // The `$` at offset 7 is string content, not a math delimiter.
        assert!(matching_delimiter_ranges(&buffer, 8).is_empty());
    }

    #[test]
    fn enclosing_delimiters_grow_outward() {
        let ranges = enclosing_delimiters("a(b[c]d)e", 4..5);

        assert_eq!(ranges, vec![3..6, 1..8]);
    }
}
