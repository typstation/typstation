//! The text buffer backing the editor.
//!
//! [`Buffer`] wraps a [`typst_syntax::Source`]: the text and its Typst syntax
//! tree are kept in sync incrementally on every edit, and all position
//! conversions (byte offset ↔ line/column ↔ UTF-16) come from that single
//! source of truth.

use std::ops::Range;

use typst_syntax::{Source, SyntaxNode};

/// A position in the buffer expressed as line and column.
///
/// Both are zero-based. The column counts `char`s from the start of the
/// line, matching [`typst_syntax::Lines`] conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    /// The zero-based line index.
    pub line: usize,
    /// The zero-based column, in `char`s.
    pub column: usize,
}

/// A text buffer with an always up-to-date Typst syntax tree.
///
/// Edits go through [`Buffer::edit`], which reparses only the affected part
/// of the document. Every mutation bumps [`Buffer::revision`], which callers
/// can use to invalidate caches derived from the text.
#[derive(Debug, Clone)]
pub struct Buffer {
    source: Source,
    revision: u64,
}

impl Buffer {
    /// Creates an empty [`Buffer`].
    pub fn new() -> Self {
        Self::from_text("")
    }

    /// Creates a [`Buffer`] with the given initial text.
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            source: Source::detached(text.into()),
            revision: 0,
        }
    }

    /// Returns the whole text of the buffer.
    pub fn text(&self) -> &str {
        self.source.text()
    }

    /// Returns the length of the text in bytes.
    pub fn len(&self) -> usize {
        self.source.text().len()
    }

    /// Returns whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.source.text().is_empty()
    }

    /// Returns the current revision of the buffer.
    ///
    /// The revision changes on every edit.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the root of the Typst syntax tree.
    pub fn root(&self) -> &SyntaxNode {
        self.source.root()
    }

    /// Replaces the given byte range with `with`, reparsing incrementally.
    pub fn edit(&mut self, replace: Range<usize>, with: &str) {
        let _ = self.source.edit(replace, with);
        self.revision += 1;
    }

    /// Returns the number of lines.
    ///
    /// An empty buffer has one (empty) line, and a trailing newline starts a
    /// new (empty) last line.
    pub fn line_count(&self) -> usize {
        self.source.lines().len_lines()
    }

    /// Returns the byte range of the given line, including its trailing
    /// newline (if any). Out-of-range lines are clamped to the last line.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let lines = self.source.lines();
        let line = line.min(self.line_count() - 1);
        lines.line_to_range(line).unwrap_or(0..0)
    }

    /// Returns the byte range of the given line, excluding its trailing
    /// newline.
    pub fn line_content_range(&self, line: usize) -> Range<usize> {
        let range = self.line_range(line);
        let text = &self.text()[range.clone()];
        range.start..range.start + text.trim_end_matches(['\n', '\r']).len()
    }

    /// Returns the text of the given line, excluding its trailing newline.
    pub fn line_text(&self, line: usize) -> &str {
        &self.text()[self.line_content_range(line)]
    }

    /// Returns the line containing the given byte offset.
    pub fn byte_to_line(&self, byte: usize) -> usize {
        self.source
            .lines()
            .byte_to_line(byte.min(self.len()))
            .unwrap_or(0)
    }

    /// Returns the [`Position`] of the given byte offset.
    pub fn position_of(&self, byte: usize) -> Position {
        let (line, column) = self
            .source
            .lines()
            .byte_to_line_column(self.clamp(byte))
            .unwrap_or((0, 0));

        Position { line, column }
    }

    /// Returns the byte offset of the given [`Position`].
    ///
    /// Out-of-range lines and columns are clamped.
    pub fn byte_of(&self, position: Position) -> usize {
        let line = position.line.min(self.line_count() - 1);
        let content = self.line_content_range(line);
        let text = &self.text()[content.clone()];

        let column_offset = text
            .char_indices()
            .nth(position.column)
            .map(|(i, _)| i)
            .unwrap_or(text.len());

        content.start + column_offset
    }

    /// Converts a byte offset to a UTF-16 code unit offset.
    pub fn byte_to_utf16(&self, byte: usize) -> usize {
        self.source
            .lines()
            .byte_to_utf16(self.clamp(byte))
            .unwrap_or(0)
    }

    /// Converts a UTF-16 code unit offset to a byte offset.
    pub fn utf16_to_byte(&self, utf16: usize) -> usize {
        self.source
            .lines()
            .utf16_to_byte(utf16)
            .unwrap_or_else(|| self.len())
    }

    /// Returns the LSP-style position (line, UTF-16 column) of a byte offset.
    pub fn lsp_position(&self, byte: usize) -> (usize, usize) {
        let byte = self.clamp(byte);
        let line = self.byte_to_line(byte);
        let line_start = self.line_range(line).start;

        (
            line,
            self.byte_to_utf16(byte) - self.byte_to_utf16(line_start),
        )
    }

    /// Returns the byte offset of an LSP-style position (line, UTF-16
    /// column). Out-of-range positions are clamped.
    pub fn lsp_position_to_byte(&self, line: usize, utf16_column: usize) -> usize {
        let line = line.min(self.line_count() - 1);
        let content = self.line_content_range(line);
        let line_utf16 = self.byte_to_utf16(content.start);
        let max_utf16 = self.byte_to_utf16(content.end) - line_utf16;

        self.utf16_to_byte(line_utf16 + utf16_column.min(max_utf16))
    }

    /// Clamps a byte offset to the buffer, snapping down to a `char`
    /// boundary.
    pub fn clamp(&self, byte: usize) -> usize {
        let text = self.text();
        let mut byte = byte.min(text.len());

        while !text.is_char_boundary(byte) {
            byte -= 1;
        }

        byte
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}
