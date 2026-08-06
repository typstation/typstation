//! Foldable line ranges discovered from Typst source.

use std::ops::Range;

use typst_syntax::{LinkedNode, SyntaxKind};

use crate::buffer::Buffer;

/// A foldable region in the document, expressed in zero-based buffer lines.
///
/// The `start` line remains visible when the range is collapsed. Lines from
/// `start + 1` through `end` are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    /// The visible line that owns the fold.
    pub start: usize,
    /// The last line hidden by the fold, inclusive.
    pub end: usize,
}

impl Fold {
    pub(crate) fn hidden_lines(self) -> Range<usize> {
        self.start + 1..self.end + 1
    }
}

/// Returns the foldable ranges in `buffer`.
///
/// Headings fold until the next heading of the same or higher level. Raw
/// fences, bracketed blocks, calls, collections, parameter lists, and import
/// lists fold from their opener through the matching closer when multi-line.
pub fn fold_ranges(buffer: &Buffer) -> Vec<Fold> {
    let mut headings = Vec::new();
    let mut folds = Vec::new();

    collect_ast_folds(
        LinkedNode::new(buffer.root()),
        buffer,
        &mut headings,
        &mut folds,
    );
    folds.extend(heading_folds(buffer, headings));

    folds.sort_by_key(|fold| (fold.start, fold.end));
    folds.dedup();
    folds
}

fn collect_ast_folds(
    node: LinkedNode<'_>,
    buffer: &Buffer,
    headings: &mut Vec<(usize, usize)>,
    folds: &mut Vec<Fold>,
) {
    match node.kind() {
        SyntaxKind::Heading => {
            if let Some(level) = heading_level(&node.get().full_text()) {
                headings.push((buffer.byte_to_line(node.range().start), level));
            }
        }
        SyntaxKind::Raw
        | SyntaxKind::CodeBlock
        | SyntaxKind::ContentBlock
        | SyntaxKind::Args
        | SyntaxKind::Array
        | SyntaxKind::Dict
        | SyntaxKind::Params
        // `ImportItems` excludes its surrounding parentheses, which would
        // leave the first item and closing delimiter outside the fold. The
        // parent module node gives the expected complete range instead.
        | SyntaxKind::ModuleImport => {
            if let Some(fold) = fold_from_byte_range(buffer, node.range()) {
                folds.push(fold);
            }
        }
        _ => {}
    }

    for child in node.children() {
        collect_ast_folds(child, buffer, headings, folds);
    }
}

fn fold_from_byte_range(buffer: &Buffer, range: Range<usize>) -> Option<Fold> {
    if range.is_empty() {
        return None;
    }

    let start = buffer.byte_to_line(range.start);
    let end = buffer.byte_to_line(range.end.saturating_sub(1));

    (end > start).then_some(Fold { start, end })
}

fn heading_folds(buffer: &Buffer, mut headings: Vec<(usize, usize)>) -> Vec<Fold> {
    headings.sort_unstable();
    headings.dedup();

    let mut folds = Vec::new();

    for (index, &(line, level)) in headings.iter().enumerate() {
        let end = headings[index + 1..]
            .iter()
            .find_map(|&(next, next_level)| (next_level <= level).then_some(next))
            .unwrap_or_else(|| buffer.line_count())
            .saturating_sub(1);

        if end > line {
            folds.push(Fold { start: line, end });
        }
    }

    folds
}

fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '=').count();

    if level == 0 {
        return None;
    }

    trimmed[level..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
        .then_some(level)
}
