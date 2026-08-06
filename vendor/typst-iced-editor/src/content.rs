//! The state of an editor, owned by the application.

use std::cell::{Ref, RefCell};
use std::collections::BTreeSet;
use std::ops::Range;

use iced_core::{Size, Vector};
use typst_syntax::{LinkedNode, SyntaxNode};

use crate::action::Action;
use crate::anchor::{Anchor, Anchors, Bias};
use crate::buffer::{Buffer, Position};
use crate::complete::{Completion, Hover};
use crate::cursor::{self, Selection};
use crate::diagnostic::{Diagnostic, Severity};
use crate::fold::{self, Fold};
use crate::history::{EditKind, History, Transaction};
use crate::pair;

/// The document state of a [`CodeEditor`](crate::CodeEditor).
///
/// A [`Content`] lives in your application state and is updated by calling
/// [`perform`](Self::perform) with the [`Action`]s published by the widget:
///
/// ```no_run
/// # use typst_iced_editor::{code_editor, Action, Content};
/// struct App {
///     content: Content,
/// }
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     Edit(Action),
/// }
///
/// impl App {
///     fn update(&mut self, message: Message) {
///         match message {
///             Message::Edit(action) => self.content.perform(action),
///         }
///     }
///
///     fn view(&self) -> iced::Element<'_, Message> {
///         code_editor(&self.content).on_action(Message::Edit).into()
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Content(pub(crate) RefCell<Internal>);

#[derive(Debug)]
pub(crate) struct Internal {
    pub buffer: Buffer,
    pub selection: Selection,
    pub history: History,
    pub anchors: Anchors,
    pub diagnostics: Vec<AnchoredDiagnostic>,
    /// Search matches delivered by the application, anchored through edits.
    pub search_matches: Vec<AnchoredRange>,
    /// The currently focused search match.
    pub current_search_match: Option<usize>,
    /// The latest delivered completions, tagged with their request id.
    pub completions: Option<(u64, Vec<Completion>)>,
    /// The latest delivered hover result, tagged with its request id.
    pub hover: Option<(u64, Option<Hover>)>,
    /// The start lines of currently collapsed fold ranges.
    pub folded: BTreeSet<usize>,
    /// Revision of the folding state, used by the widget's visible-line map.
    pub fold_revision: u64,
    /// The scroll offset of the viewport, in logical pixels.
    pub scroll: Vector,
    /// Whether the viewport should scroll to the caret on the next draw.
    pub needs_reveal: bool,
    /// Whether a fold just toggled, so the widget should keep the line at the
    /// top of the viewport in place across the relayout (instead of revealing
    /// the caret). Only the text below the fold shifts.
    pub fold_anchor: bool,
    /// Viewport geometry, last measured by the widget during `draw`.
    pub view: View,
    /// Buffer lines touched since the widget last rebuilt its wrap map.
    pub wrap_dirty: WrapDirty,
    /// Number of spaces per indentation level.
    pub tab_width: usize,
    /// Whether typing an opening delimiter inserts its closing partner.
    pub auto_pairs: bool,
    /// Whether Enter carries over the indentation of the current line.
    pub auto_indent: bool,
}

/// Viewport geometry owned by the widget but needed by [`Content::perform`]
/// (e.g. to size page motions and clamp scrolling).
#[derive(Debug, Default)]
pub(crate) struct View {
    /// The height of one visual row, in logical pixels.
    pub line_height: f32,
    /// The size of the text area, in logical pixels.
    pub size: Size,
    /// The width of the widest line shaped so far, used to bound horizontal
    /// scrolling. Zero while soft wrap is enabled.
    pub max_line_width: f32,
    /// The buffer revision `max_line_width` was measured against.
    pub revision: u64,
    /// Total number of visual rows (with soft wrap, ≥ the line count).
    /// Zero until the widget has measured the document.
    pub total_rows: usize,
}

impl View {
    /// The viewport height in whole rows.
    pub fn page_lines(&self) -> usize {
        if self.line_height <= 0.0 {
            1
        } else {
            ((self.size.height / self.line_height) as usize).max(1)
        }
    }
}

/// How stale the widget's wrap map is.
#[derive(Debug, Default)]
pub(crate) enum WrapDirty {
    /// No edits since the last sync.
    #[default]
    Clean,
    /// The recorded line splices, in the order they happened.
    Splices(Vec<LineSplice>),
    /// Too many edits accumulated: rebuild from scratch.
    Rebuild,
}

/// One edit expressed in buffer lines: `old_lines` lines starting at `start`
/// became `new_lines` lines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineSplice {
    pub start: usize,
    pub old_lines: usize,
    pub new_lines: usize,
}

/// A diagnostic whose endpoints are anchored, so it follows edits until
/// fresh diagnostics are set.
#[derive(Debug)]
pub(crate) struct AnchoredDiagnostic {
    pub start: Anchor,
    pub end: Anchor,
    pub severity: Severity,
    pub message: String,
}

/// A byte range tracked through edits.
#[derive(Debug)]
pub(crate) struct AnchoredRange {
    pub start: Anchor,
    pub end: Anchor,
}

impl WrapDirty {
    const MAX_SPLICES: usize = 128;

    fn push(&mut self, splice: LineSplice) {
        match self {
            Self::Clean => *self = Self::Splices(vec![splice]),
            Self::Splices(splices) if splices.len() >= Self::MAX_SPLICES => {
                *self = Self::Rebuild;
            }
            Self::Splices(splices) => splices.push(splice),
            Self::Rebuild => {}
        }
    }

    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

fn syntax_node_ranges(root: &SyntaxNode, current: Range<usize>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    collect_syntax_node_ranges(&LinkedNode::new(root), &current, &mut ranges);
    ranges
}

fn collect_syntax_node_ranges(
    node: &LinkedNode<'_>,
    current: &Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    let range = node.range();

    if range.start > current.start || current.end > range.end || range.is_empty() {
        return;
    }

    ranges.push(range);

    for child in node.children() {
        collect_syntax_node_ranges(&child, current, ranges);
    }
}

/// The length of the longest common prefix of `a` and `b`, on a char
/// boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    let mut len = a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count();

    while !a.is_char_boundary(len) {
        len -= 1;
    }

    len
}

/// The length of the longest common suffix of `a` and `b`, on a char
/// boundary. Call with any common prefix already removed, so the two
/// regions cannot overlap.
fn common_suffix(a: &str, b: &str) -> usize {
    let mut len = a
        .bytes()
        .rev()
        .zip(b.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();

    // The matched tail bytes are identical in both strings, so a boundary
    // in one is a boundary in the other.
    while !a.is_char_boundary(a.len() - len) {
        len -= 1;
    }

    len
}

impl Content {
    /// Creates an empty [`Content`].
    pub fn new() -> Self {
        Self::with_text("")
    }

    /// Creates a [`Content`] with the given initial text.
    pub fn with_text(text: &str) -> Self {
        Self(RefCell::new(Internal {
            buffer: Buffer::from_text(text),
            selection: Selection::caret(0),
            history: History::default(),
            anchors: Anchors::default(),
            diagnostics: Vec::new(),
            search_matches: Vec::new(),
            current_search_match: None,
            completions: None,
            hover: None,
            folded: BTreeSet::new(),
            fold_revision: 0,
            scroll: Vector::new(0.0, 0.0),
            needs_reveal: false,
            fold_anchor: false,
            view: View::default(),
            wrap_dirty: WrapDirty::default(),
            tab_width: 2,
            auto_pairs: true,
            auto_indent: true,
        }))
    }

    /// Applies an [`Action`] to the content.
    pub fn perform(&mut self, action: Action) {
        self.0.get_mut().perform(action);
    }

    /// Returns the full text of the document.
    pub fn text(&self) -> String {
        self.0.borrow().buffer.text().to_owned()
    }

    /// Returns read access to the underlying [`Buffer`], for position
    /// conversions and syntax tree access.
    pub fn buffer(&self) -> Ref<'_, Buffer> {
        Ref::map(self.0.borrow(), |internal| &internal.buffer)
    }

    /// Returns the number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.0.borrow().buffer.line_count()
    }

    /// Returns the current selection, in byte offsets.
    pub fn selection(&self) -> Range<usize> {
        self.0.borrow().selection.range()
    }

    /// Returns the selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        let internal = self.0.borrow();
        let range = internal.selection.range();

        (!range.is_empty()).then(|| internal.buffer.text()[range].to_owned())
    }

    /// Returns the position of the caret.
    pub fn cursor(&self) -> Position {
        let internal = self.0.borrow();
        internal.buffer.position_of(internal.selection.head)
    }

    /// Returns the current scroll offset of the viewport, in logical pixels.
    ///
    /// Useful for persisting and restoring the viewport across sessions.
    pub fn scroll_offset(&self) -> Vector {
        self.0.borrow().scroll
    }

    /// Creates an [`Anchor`] at the given byte offset. Its position follows
    /// every subsequent edit (including undo and redo).
    pub fn create_anchor(&mut self, offset: usize, bias: Bias) -> Anchor {
        let internal = self.0.get_mut();
        let offset = internal.buffer.clamp(offset);

        internal.anchors.create(offset, bias)
    }

    /// Returns the current byte offset of an [`Anchor`].
    pub fn anchor_position(&self, anchor: Anchor) -> Option<usize> {
        self.0.borrow().anchors.get(anchor)
    }

    /// Drops an [`Anchor`], returning its last position.
    pub fn remove_anchor(&mut self, anchor: Anchor) -> Option<usize> {
        self.0.get_mut().anchors.remove(anchor)
    }

    /// Returns the number of spaces per indentation level. Defaults to 2,
    /// the usual Typst indentation.
    pub fn tab_width(&self) -> usize {
        self.0.borrow().tab_width
    }

    /// Sets the number of spaces per indentation level.
    pub fn set_tab_width(&mut self, tab_width: usize) {
        self.0.get_mut().tab_width = tab_width.max(1);
    }

    /// Enables or disables automatic closing of `()`, `[]`, `{}`, `""`, and
    /// `$$`, and wrapping of selections in typed delimiters. On by default.
    pub fn set_auto_pairs(&mut self, auto_pairs: bool) {
        self.0.get_mut().auto_pairs = auto_pairs;
    }

    /// Enables or disables automatic indentation on Enter. On by default.
    pub fn set_auto_indent(&mut self, auto_indent: bool) {
        self.0.get_mut().auto_indent = auto_indent;
    }

    /// Replaces the set of diagnostics.
    ///
    /// Their ranges are anchored, so the underlines follow the text through
    /// later edits until the next call. Feed these from your compile loop
    /// or language server.
    pub fn set_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        let internal = self.0.get_mut();

        for diagnostic in std::mem::take(&mut internal.diagnostics) {
            internal.anchors.remove(diagnostic.start);
            internal.anchors.remove(diagnostic.end);
        }

        internal.diagnostics = diagnostics
            .into_iter()
            .map(|diagnostic| {
                let start = internal.buffer.clamp(diagnostic.range.start);
                let end = internal
                    .buffer
                    .clamp(diagnostic.range.end.max(diagnostic.range.start));

                AnchoredDiagnostic {
                    start: internal.anchors.create(start, Bias::Before),
                    end: internal.anchors.create(end, Bias::After),
                    severity: diagnostic.severity,
                    message: diagnostic.message,
                }
            })
            .collect();
    }

    /// Removes all diagnostics.
    pub fn clear_diagnostics(&mut self) {
        self.set_diagnostics([]);
    }

    /// Replaces the highlighted search matches.
    ///
    /// The search UI itself belongs to the application. The widget only
    /// stores the ranges, draws them, and can reveal/select the current one.
    pub fn set_search_matches(
        &mut self,
        matches: impl IntoIterator<Item = Range<usize>>,
        current: Option<usize>,
    ) {
        let internal = self.0.get_mut();

        for range in std::mem::take(&mut internal.search_matches) {
            internal.anchors.remove(range.start);
            internal.anchors.remove(range.end);
        }

        internal.search_matches = matches
            .into_iter()
            .map(|range| {
                let start = internal.buffer.clamp(range.start);
                let end = internal.buffer.clamp(range.end.max(range.start));

                AnchoredRange {
                    start: internal.anchors.create(start, Bias::Before),
                    end: internal.anchors.create(end, Bias::After),
                }
            })
            .collect();

        internal.current_search_match =
            current.filter(|index| *index < internal.search_matches.len());
    }

    /// Removes search highlights.
    pub fn clear_search_matches(&mut self) {
        self.set_search_matches([], None);
    }

    /// Returns the current search matches after resolving their anchors.
    pub fn search_matches(&self) -> Vec<Range<usize>> {
        self.0.borrow().resolved_search_matches()
    }

    /// Returns the index of the current search match, if any.
    pub fn current_search_match(&self) -> Option<usize> {
        self.0.borrow().current_search_match
    }

    /// Selects and reveals a search match.
    pub fn reveal_search_match(&mut self, index: usize) -> bool {
        let internal = self.0.get_mut();
        let Some(range) = internal.resolved_search_matches().get(index).cloned() else {
            return false;
        };

        internal.current_search_match = Some(index);
        internal.selection = Selection::new(range.start, range.end);
        internal.needs_reveal = true;

        true
    }

    /// Delivers completions requested with
    /// [`Action::RequestCompletions`](crate::Action::RequestCompletions).
    ///
    /// `id` must be the one from the request. Replies older than the latest
    /// one already delivered are ignored, so slow or out-of-order responses
    /// never override newer results.
    pub fn set_completions(&mut self, id: u64, completions: Vec<Completion>) {
        let internal = self.0.get_mut();

        if internal
            .completions
            .as_ref()
            .is_none_or(|(last, _)| id >= *last)
        {
            internal.completions = Some((id, completions));
        }
    }

    /// Returns the latest delivered completions and their request id.
    pub fn completions(&self) -> Option<(u64, Vec<Completion>)> {
        self.0.borrow().completions.clone()
    }

    /// Discards any delivered completions.
    pub fn clear_completions(&mut self) {
        self.0.get_mut().completions = None;
    }

    /// Delivers a hover result requested with
    /// [`Action::RequestHover`](crate::Action::RequestHover). Older replies
    /// are ignored, as with [`set_completions`](Self::set_completions).
    pub fn set_hover(&mut self, id: u64, hover: Option<Hover>) {
        let internal = self.0.get_mut();

        if internal.hover.as_ref().is_none_or(|(last, _)| id >= *last) {
            internal.hover = Some((id, hover));
        }
    }

    /// Returns the latest delivered hover result and its request id.
    pub fn hover(&self) -> Option<(u64, Option<Hover>)> {
        self.0.borrow().hover.clone()
    }

    /// Discards any delivered hover result.
    pub fn clear_hover(&mut self) {
        self.0.get_mut().hover = None;
    }

    /// Returns the foldable ranges currently discovered in the document.
    pub fn fold_ranges(&self) -> Vec<Fold> {
        fold::fold_ranges(&self.0.borrow().buffer)
    }

    /// Returns the fold ranges that are currently collapsed.
    pub fn folded_ranges(&self) -> Vec<Fold> {
        let internal = self.0.borrow();

        fold::fold_ranges(&internal.buffer)
            .into_iter()
            .filter(|fold| internal.folded.contains(&fold.start))
            .collect()
    }

    /// Returns whether the fold starting at `line` is currently collapsed.
    pub fn is_folded(&self, line: usize) -> bool {
        self.0.borrow().folded.contains(&line)
    }

    /// Collapses the fold starting at `line`, if any.
    pub fn collapse_fold(&mut self, line: usize) -> bool {
        self.0.get_mut().set_folded(line, true)
    }

    /// Expands the fold starting at `line`, if currently collapsed.
    pub fn expand_fold(&mut self, line: usize) -> bool {
        self.0.get_mut().set_folded(line, false)
    }

    /// Toggles the fold starting at `line`, if one exists.
    pub fn toggle_fold(&mut self, line: usize) -> bool {
        self.0.get_mut().toggle_fold(line)
    }

    /// Returns the current diagnostics, with ranges resolved through their
    /// anchors, most severe last (drawing order).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let internal = self.0.borrow();

        let mut diagnostics: Vec<Diagnostic> = internal
            .diagnostics
            .iter()
            .filter_map(|diagnostic| {
                Some(Diagnostic {
                    range: internal.resolve_anchor_range(diagnostic.start, diagnostic.end)?,
                    severity: diagnostic.severity,
                    message: diagnostic.message.clone(),
                })
            })
            .collect();

        diagnostics.sort_by_key(|diagnostic| diagnostic.severity);
        diagnostics
    }
}

impl Default for Content {
    fn default() -> Self {
        Self::new()
    }
}

impl Internal {
    pub fn perform(&mut self, action: Action) {
        // Any action that moves the caret or edits text should bring the
        // caret into view; scrolling is the only one that should not.
        self.needs_reveal = !matches!(action, Action::Scroll { .. } | Action::ScrollTo { .. });
        // Only a fold toggle anchors the viewport; clear a stale request so a
        // following action does not inherit it.
        self.fold_anchor = false;

        match action {
            Action::Move(motion) => {
                self.selection = self.selection.apply_motion(
                    &self.buffer,
                    motion,
                    false,
                    self.view.page_lines(),
                );
            }
            Action::Select(motion) => {
                self.selection =
                    self.selection
                        .apply_motion(&self.buffer, motion, true, self.view.page_lines());
            }
            Action::MoveTo(offset) => {
                self.selection = Selection::caret(self.buffer.clamp(offset));
            }
            Action::SelectTo(offset) => {
                self.selection = Selection::new(self.selection.anchor, self.buffer.clamp(offset));
            }
            Action::SelectWord(offset) => {
                let range = cursor::word_range_at(self.buffer.text(), self.buffer.clamp(offset));
                self.selection = Selection::new(range.start, range.end);
            }
            Action::SelectLine(offset) => {
                let line = self.buffer.byte_to_line(self.buffer.clamp(offset));
                let range = self.buffer.line_range(line);
                self.selection = Selection::new(range.start, range.end);
            }
            Action::SelectAll => {
                self.selection = Selection::new(0, self.buffer.len());
                self.needs_reveal = false;
            }
            Action::Insert(text) => self.insert(&text),
            Action::Enter => self.enter(),
            Action::Indent => self.indent(),
            Action::Unindent => self.unindent(),
            Action::Paste(text) => {
                self.replace(self.selection.range(), &text, EditKind::Other);
            }
            Action::Replace { range, text } => {
                let start = self.buffer.clamp(range.start.min(range.end));
                let end = self.buffer.clamp(range.start.max(range.end));
                self.replace(start..end, &text, EditKind::Other);
            }
            Action::ApplyEdits(edits) => self.apply_edits(edits),
            Action::MoveLineUp => self.move_lines(-1),
            Action::MoveLineDown => self.move_lines(1),
            Action::DuplicateLine => self.duplicate_lines(),
            Action::DeleteLine => self.delete_lines(),
            Action::JoinLines => self.join_lines(),
            Action::ToggleLineComment => self.toggle_line_comment(),
            Action::ToggleBlockComment => self.toggle_block_comment(),
            Action::ExpandSelection => self.expand_selection(),
            Action::Backspace => self.backspace(),
            Action::Delete => {
                if self.selection.is_caret() {
                    let head = self.buffer.clamp(self.selection.head);
                    let end = cursor::next_grapheme_boundary(self.buffer.text(), head);
                    self.replace(head..end, "", EditKind::DeleteForward);
                } else {
                    self.replace(self.selection.range(), "", EditKind::Other);
                }
            }
            Action::Scroll { x, y } => {
                self.scroll += Vector::new(x, y);
                self.clamp_scroll();
            }
            Action::ScrollTo { x, y } => {
                if let Some(x) = x {
                    self.scroll.x = x;
                }

                if let Some(y) = y {
                    self.scroll.y = y;
                }

                self.clamp_scroll();
            }
            Action::ToggleFold(line) => {
                // Folding is a viewport-structure change, not a caret move:
                // keep the top line in place rather than revealing the caret.
                self.needs_reveal = false;
                if self.toggle_fold(line) {
                    self.fold_anchor = true;
                }
            }
            // Intelligence requests are handled by the application, not the
            // document; performing them is a no-op.
            Action::RequestCompletions { .. } | Action::RequestHover { .. } => {
                self.needs_reveal = false;
            }
            Action::Undo => {
                if let Some(transaction) = self.history.undo() {
                    let start = transaction.at;
                    let end = start + transaction.inserted.len();
                    let removed = transaction.removed.clone();
                    let selection = transaction.selection_before;

                    self.apply_edit(start..end, &removed);
                    self.selection = selection;
                } else {
                    self.needs_reveal = false;
                }
            }
            Action::Redo => {
                if let Some(transaction) = self.history.redo() {
                    let start = transaction.at;
                    let end = start + transaction.removed.len();
                    let inserted = transaction.inserted.clone();
                    let selection = transaction.selection_after;

                    self.apply_edit(start..end, &inserted);
                    self.selection = selection;
                } else {
                    self.needs_reveal = false;
                }
            }
        }
    }

    /// Inserts typed text, applying pair-closing rules for single
    /// delimiters.
    fn insert(&mut self, text: &str) {
        let mut chars = text.chars();
        let (single, second) = (chars.next(), chars.next());

        let Some(typed) = single.filter(|_| second.is_none() && self.auto_pairs) else {
            self.replace(self.selection.range(), text, EditKind::Typing);
            return;
        };

        let range = self.selection.range();

        // Typing a delimiter over a selection wraps it instead of replacing.
        if !range.is_empty() {
            if let Some((open, close)) = pair::SURROUND_PAIRS
                .iter()
                .copied()
                .find(|(open, _)| *open == typed)
            {
                let inner = &self.buffer.text()[range.clone()];
                let replacement = format!("{open}{inner}{close}");
                let selection = Selection::new(
                    range.start + open.len_utf8(),
                    range.start + open.len_utf8() + inner.len(),
                );

                self.replace_with(range, &replacement, selection, EditKind::Other);
                return;
            }

            self.replace(range, text, EditKind::Typing);
            return;
        }

        let head = self.buffer.clamp(self.selection.head);
        let next = self.buffer.text()[head..].chars().next();
        let previous = self.buffer.text()[..head].chars().next_back();

        // Typing a closing delimiter right before the same character steps
        // over it instead of inserting a duplicate.
        if pair::is_auto_closing(typed) && next == Some(typed) {
            self.selection = Selection::caret(head + typed.len_utf8());
            return;
        }

        if let Some(close) = pair::auto_closing(typed) {
            // Do not auto-close right before a word, nor between characters
            // for identical pairs like `"` and `$` (e.g. after a word).
            let closes_naturally = next.is_none_or(|n| !n.is_alphanumeric());
            let identical = typed == close;
            let after_word =
                identical && previous.is_some_and(|p| p.is_alphanumeric() || p == typed);

            if closes_naturally && !after_word {
                let pair = format!("{typed}{close}");
                let caret = Selection::caret(head + typed.len_utf8());

                self.replace_with(head..head, &pair, caret, EditKind::Typing);
                return;
            }
        }

        self.replace(head..head, text, EditKind::Typing);
    }

    /// Breaks the line, carrying over indentation and opening a block when
    /// the caret sits between brackets.
    fn enter(&mut self) {
        let range = self.selection.range();

        if !self.auto_indent {
            self.replace(range, "\n", EditKind::Other);
            return;
        }

        let text = self.buffer.text();
        let line = self.buffer.byte_to_line(range.start);
        let line_start = self.buffer.line_content_range(line).start;

        let indent: String = text[line_start..range.start]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        let previous = text[..range.start].chars().next_back();
        let next = text[range.end..].chars().next();
        let opened = previous.and_then(|p| {
            pair::INDENT_BRACKETS
                .iter()
                .find(|(open, _)| *open == p)
                .map(|(_, close)| *close)
        });

        let tab = " ".repeat(self.tab_width);

        let (inserted, caret) = match opened {
            // `{|}` becomes an indented block with the closer on its own
            // line.
            Some(close) if next == Some(close) => {
                let inserted = format!("\n{indent}{tab}\n{indent}");
                let caret = range.start + 1 + indent.len() + tab.len();

                (inserted, caret)
            }
            // After an opening bracket, indent one level deeper.
            Some(_) => {
                let inserted = format!("\n{indent}{tab}");
                let caret = range.start + inserted.len();

                (inserted, caret)
            }
            // Otherwise, keep the current indentation.
            None => {
                let inserted = format!("\n{indent}");
                let caret = range.start + inserted.len();

                (inserted, caret)
            }
        };

        self.replace_with(range, &inserted, Selection::caret(caret), EditKind::Other);
    }

    /// Indents the selected lines by one level, or inserts spaces at the
    /// caret.
    fn indent(&mut self) {
        let tab = " ".repeat(self.tab_width);
        let range = self.selection.range();

        if self.selection.is_caret() {
            self.replace(range, &tab, EditKind::Typing);
            return;
        }

        let (region, lines) = self.selected_lines();

        let indented: String = lines
            .iter()
            .map(|line| {
                if line.trim_end_matches(['\n', '\r']).is_empty() {
                    line.to_string()
                } else {
                    format!("{tab}{line}")
                }
            })
            .collect();

        let selection = Selection::new(region.start, region.start + indented.len());
        self.replace_with(region, &indented, selection, EditKind::Other);
    }

    /// Removes up to one level of indentation from the selected lines.
    fn unindent(&mut self) {
        let tab_width = self.tab_width;
        let (region, lines) = self.selected_lines();

        let unindented: String = lines
            .iter()
            .map(|line| {
                let spaces = line.chars().take_while(|c| *c == ' ').count();
                &line[spaces.min(tab_width)..]
            })
            .collect();

        if unindented.len() == region.len() {
            self.needs_reveal = false;
            return;
        }

        let selection = Selection::new(region.start, region.start + unindented.len());
        self.replace_with(region, &unindented, selection, EditKind::Other);
    }

    fn selected_line_indices(&self) -> (usize, usize) {
        let range = self.selection.range();
        let first = self.buffer.byte_to_line(range.start);
        let mut last = self.buffer.byte_to_line(range.end);

        // A selection ending exactly at a line start does not include that
        // line (e.g. a triple-click selection with its trailing newline).
        if last > first && range.end == self.buffer.line_range(last).start {
            last -= 1;
        }

        (first, last)
    }

    fn line_span(&self, first: usize, last: usize) -> Range<usize> {
        self.buffer.line_range(first).start..self.buffer.line_range(last).end
    }

    fn move_lines(&mut self, direction: isize) {
        let (first, last) = self.selected_line_indices();

        if direction < 0 {
            if first == 0 {
                self.needs_reveal = false;
                return;
            }

            let previous = self.buffer.line_range(first - 1);
            let block = self.line_span(first, last);
            let previous_text = &self.buffer.text()[previous.clone()];
            let block_text = &self.buffer.text()[block.clone()];
            let replacement = format!("{block_text}{previous_text}");
            let selection = Selection::new(previous.start, previous.start + block.len());

            self.replace_with(
                previous.start..block.end,
                &replacement,
                selection,
                EditKind::Other,
            );
        } else {
            if last + 1 >= self.buffer.line_count() {
                self.needs_reveal = false;
                return;
            }

            let block = self.line_span(first, last);
            let next = self.buffer.line_range(last + 1);
            let block_text = &self.buffer.text()[block.clone()];
            let next_text = &self.buffer.text()[next.clone()];
            let replacement = format!("{next_text}{block_text}");
            let selection = Selection::new(block.start + next.len(), block.end + next.len());

            self.replace_with(
                block.start..next.end,
                &replacement,
                selection,
                EditKind::Other,
            );
        }
    }

    fn duplicate_lines(&mut self) {
        let (first, last) = self.selected_line_indices();
        let block = self.line_span(first, last);
        let text = self.buffer.text()[block.clone()].to_owned();
        let selection = Selection::new(block.end, block.end + text.len());

        self.replace_with(block.end..block.end, &text, selection, EditKind::Other);
    }

    fn delete_lines(&mut self) {
        let (first, last) = self.selected_line_indices();
        let block = self.line_span(first, last);
        let caret = Selection::caret(block.start.min(self.buffer.len()));

        self.replace_with(block, "", caret, EditKind::Other);
    }

    fn join_lines(&mut self) {
        let (first, mut last) = self.selected_line_indices();

        if first == last {
            if last + 1 >= self.buffer.line_count() {
                self.needs_reveal = false;
                return;
            }

            last += 1;
        }

        let span = self.line_span(first, last);
        let trailing = if self.buffer.text()[span.clone()].ends_with('\n') {
            "\n"
        } else {
            ""
        };

        let mut joined = String::new();

        for line in first..=last {
            let text = self.buffer.line_text(line);
            // The first line keeps its indentation; the joined ones shed
            // theirs. Blank lines vanish.
            let piece = if line == first {
                text.trim_end()
            } else {
                text.trim()
            };

            if piece.trim().is_empty() {
                continue;
            }

            if !joined.is_empty() {
                joined.push(' ');
            }

            joined.push_str(piece);
        }

        let replacement = format!("{joined}{trailing}");
        let caret = Selection::caret(span.start + joined.len());

        self.replace_with(span, &replacement, caret, EditKind::Other);
    }

    fn toggle_line_comment(&mut self) {
        let (first, last) = self.selected_line_indices();
        let span = self.line_span(first, last);
        let uncomment = (first..=last)
            .filter_map(|line| {
                let text = self.buffer.line_text(line);
                (!text.trim().is_empty()).then(|| {
                    let indent = text.len() - text.trim_start_matches([' ', '\t']).len();
                    text[indent..].starts_with("//")
                })
            })
            .all(|commented| commented);

        let mut replacement = String::new();

        for line in first..=last {
            let range = self.buffer.line_range(line);
            let line_text = &self.buffer.text()[range];
            let newline_len = line_text
                .len()
                .saturating_sub(line_text.trim_end_matches(['\n', '\r']).len());
            let (content, newline) = line_text.split_at(line_text.len() - newline_len);

            if content.trim().is_empty() {
                replacement.push_str(line_text);
                continue;
            }

            let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
            replacement.push_str(&content[..indent]);

            if uncomment {
                let rest = &content[indent..];
                let rest = rest
                    .strip_prefix("// ")
                    .or_else(|| rest.strip_prefix("//"))
                    .unwrap_or(rest);
                replacement.push_str(rest);
            } else {
                replacement.push_str("// ");
                replacement.push_str(&content[indent..]);
            }

            replacement.push_str(newline);
        }

        let selection = Selection::new(span.start, span.start + replacement.len());
        self.replace_with(span, &replacement, selection, EditKind::Other);
    }

    fn toggle_block_comment(&mut self) {
        let range = self.selection.range();
        let text = self.buffer.text();
        let selected = text[range.clone()].to_owned();

        // In `/*/` the markers overlap: only selections of at least `/**/`
        // count as already commented.
        if selected.len() >= 4 && selected.starts_with("/*") && selected.ends_with("*/") {
            let inner = selected[2..selected.len() - 2].to_owned();
            let selection = Selection::new(range.start, range.start + inner.len());
            self.replace_with(range, &inner, selection, EditKind::Other);
        } else if range.start >= 2
            // `get`, not indexing: the neighboring bytes can fall inside a
            // multi-byte character, where slicing would panic.
            && text.get(range.start - 2..range.start) == Some("/*")
            && text.get(range.end..range.end + 2) == Some("*/")
        {
            let replacement = selected;
            let selection = Selection::new(range.start - 2, range.start - 2 + replacement.len());
            self.replace_with(
                range.start - 2..range.end + 2,
                &replacement,
                selection,
                EditKind::Other,
            );
        } else {
            let replacement = format!("/*{selected}*/");
            let selection = Selection::new(range.start + 2, range.start + 2 + selected.len());
            self.replace_with(range, &replacement, selection, EditKind::Other);
        }
    }

    fn expand_selection(&mut self) {
        let current = self.selection.range();
        let mut candidates = self.selection_candidates();

        candidates.sort_by_key(|range| (range.len(), range.start));
        candidates.dedup();

        if let Some(next) = candidates.into_iter().find(|range| {
            range.start <= current.start && current.end <= range.end && *range != current
        }) {
            self.selection = Selection::new(next.start, next.end);
        } else {
            self.needs_reveal = false;
        }
    }

    fn selection_candidates(&self) -> Vec<Range<usize>> {
        let current = self.selection.range();
        let head = self.buffer.clamp(self.selection.head);
        let line = self.buffer.byte_to_line(head);
        let mut candidates = vec![
            cursor::word_range_at(self.buffer.text(), head),
            self.buffer.line_content_range(line),
            self.buffer.line_range(line),
            0..self.buffer.len(),
        ];

        candidates.extend(pair::enclosing_delimiters(self.buffer.text(), current));
        candidates.extend(syntax_node_ranges(
            self.buffer.root(),
            self.selection.range(),
        ));
        candidates
    }

    /// Returns the region of whole lines covered by the selection, and its
    /// text split into lines (with their newlines).
    fn selected_lines(&self) -> (Range<usize>, Vec<String>) {
        let (first, last) = self.selected_line_indices();
        let region = self.buffer.line_range(first).start..self.buffer.line_content_range(last).end;

        let lines = self.buffer.text()[region.clone()]
            .split_inclusive('\n')
            .map(str::to_owned)
            .collect();

        (region, lines)
    }

    /// Deletes backwards, removing both sides of an empty pair at once.
    fn backspace(&mut self) {
        if !self.selection.is_caret() {
            self.replace(self.selection.range(), "", EditKind::Other);
            return;
        }

        let head = self.buffer.clamp(self.selection.head);
        let text = self.buffer.text();
        let previous = text[..head].chars().next_back();
        let next = text[head..].chars().next();

        if self.auto_pairs {
            if let (Some(p), Some(n)) = (previous, next) {
                if pair::auto_closing(p) == Some(n) {
                    let range = head - p.len_utf8()..head + n.len_utf8();
                    self.replace(range, "", EditKind::Backspace);
                    return;
                }
            }
        }

        let start = cursor::prev_grapheme_boundary(text, head);
        self.replace(start..head, "", EditKind::Backspace);
    }

    /// Replaces `range` with `text`, leaving the caret after the inserted
    /// text.
    fn replace(&mut self, range: Range<usize>, text: &str, kind: EditKind) {
        let after = Selection::caret(range.start + text.len());
        self.replace_with(range, text, after, kind);
    }

    /// Replaces `range` with `text`, recording the edit in the history and
    /// setting the given selection.
    fn replace_with(
        &mut self,
        range: Range<usize>,
        text: &str,
        selection_after: Selection,
        kind: EditKind,
    ) {
        if range.is_empty() && text.is_empty() {
            self.needs_reveal = false;
            return;
        }

        let selection_before = self.selection;
        let removed = self.buffer.text()[range.clone()].to_owned();

        self.apply_edit(range.clone(), text);

        self.history.record(Transaction {
            at: range.start,
            removed,
            inserted: text.to_owned(),
            selection_before,
            selection_after,
            kind,
        });

        self.selection = selection_after;
    }

    fn apply_edits(&mut self, edits: Vec<(Range<usize>, String)>) {
        let mut edits = edits
            .into_iter()
            .map(|(range, text)| {
                let start = self.buffer.clamp(range.start.min(range.end));
                let end = self.buffer.clamp(range.start.max(range.end));
                (start..end, text)
            })
            .collect::<Vec<_>>();

        edits.sort_by_key(|(range, _)| range.start);

        let mut normalized = Vec::<(Range<usize>, String)>::new();
        let mut last_end = 0;

        for (range, text) in edits {
            if range.start < last_end {
                continue;
            }

            last_end = range.end;
            normalized.push((range, text));
        }

        if normalized.is_empty() {
            self.needs_reveal = false;
            return;
        }

        let before = self.buffer.text().to_owned();
        let mut after = String::with_capacity(before.len());
        let mut cursor = 0;
        let mut selection_after = Selection::caret(self.buffer.clamp(self.selection.head));

        for (range, text) in &normalized {
            after.push_str(&before[cursor..range.start]);
            let inserted_at = after.len();
            after.push_str(text);
            selection_after = Selection::caret(inserted_at + text.len());
            cursor = range.end;
        }

        after.push_str(&before[cursor..]);

        if after == before {
            self.needs_reveal = false;
            return;
        }

        // Replace only the region that actually changed: a whole-document
        // transaction would collapse every anchor to offset 0 and drop every
        // fold, besides storing the full text twice in the history.
        let prefix = common_prefix(&before, &after);
        let suffix = common_suffix(&before[prefix..], &after[prefix..]);

        self.replace_with(
            prefix..before.len() - suffix,
            &after[prefix..after.len() - suffix],
            selection_after,
            EditKind::Other,
        );
    }

    /// The single place every text mutation goes through: keeps the buffer,
    /// the anchors, and the wrap-map dirty log in sync.
    fn apply_edit(&mut self, range: Range<usize>, text: &str) {
        let start_line = self.buffer.byte_to_line(range.start);
        let old_lines = self.buffer.byte_to_line(range.end) - start_line + 1;

        self.buffer.edit(range.clone(), text);
        self.anchors.update(&range, text.len());

        let new_lines = self.buffer.byte_to_line(range.start + text.len()) - start_line + 1;

        self.shift_folds(start_line, old_lines, new_lines);

        self.wrap_dirty.push(LineSplice {
            start: start_line,
            old_lines,
            new_lines,
        });
    }

    /// Resolves an anchored pair to its current, normalized byte range.
    fn resolve_anchor_range(&self, start: Anchor, end: Anchor) -> Option<Range<usize>> {
        let start = self.anchors.get(start)?;
        let end = self.anchors.get(end)?;

        Some(start..end.max(start))
    }

    /// The diagnostics with ranges resolved through their anchors.
    pub fn resolved_diagnostics(&self) -> Vec<(Range<usize>, Severity)> {
        self.diagnostics
            .iter()
            .filter_map(|diagnostic| {
                Some((
                    self.resolve_anchor_range(diagnostic.start, diagnostic.end)?,
                    diagnostic.severity,
                ))
            })
            .collect()
    }

    pub fn resolved_search_matches(&self) -> Vec<Range<usize>> {
        self.search_matches
            .iter()
            .filter_map(|range| self.resolve_anchor_range(range.start, range.end))
            .collect()
    }

    /// The most severe diagnostic covering `offset`, as a range and message.
    pub fn diagnostic_at(&self, offset: usize) -> Option<(Range<usize>, String)> {
        self.diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let range = self.resolve_anchor_range(diagnostic.start, diagnostic.end)?;

                // Include the far edge so caret-width diagnostics still hit.
                (range.start <= offset && offset <= range.end)
                    .then(|| (diagnostic.severity, range, diagnostic.message.clone()))
            })
            .max_by_key(|(severity, _, _)| *severity)
            .map(|(_, range, message)| (range, message))
    }

    fn fold_at(&self, line: usize) -> Option<Fold> {
        fold::fold_ranges(&self.buffer)
            .into_iter()
            .find(|fold| fold.start == line)
    }

    fn set_folded(&mut self, line: usize, folded: bool) -> bool {
        let Some(fold) = self.fold_at(line) else {
            return false;
        };

        let changed = if folded {
            self.folded.insert(line)
        } else {
            self.folded.remove(&line)
        };

        if changed {
            if folded && self.selection_intersects(fold.hidden_lines()) {
                let caret = self.buffer.line_content_range(line).end;
                self.selection = Selection::caret(caret);
            }

            self.fold_revision += 1;
            self.clamp_scroll();
        }

        changed
    }

    fn toggle_fold(&mut self, line: usize) -> bool {
        let folded = !self.folded.contains(&line);
        self.set_folded(line, folded)
    }

    /// Keeps collapsed folds positioned through an edit that turned
    /// `old_lines` lines starting at `start` into `new_lines` lines.
    ///
    /// Folds above the edit are unaffected and folds below shift with the
    /// change in line count. A fold whose start line was itself edited stays
    /// collapsed while the line count is unchanged (typing on a heading), but
    /// is dropped when lines were added or removed there — the mapping is
    /// ambiguous, and expanding is the safe answer. Folds whose range no
    /// longer exists are ignored by `fold_ranges` filtering either way.
    fn shift_folds(&mut self, start: usize, old_lines: usize, new_lines: usize) {
        // With nothing collapsed there is nothing to remap, and no fold
        // resync signal is needed: no line can be hidden, and the cache
        // refreshes its stale (purely cosmetic) fold markers on its own
        // debounced schedule.
        if self.folded.is_empty() {
            return;
        }

        self.fold_revision += 1;

        let old_end = start + old_lines;
        let delta = new_lines as isize - old_lines as isize;
        let line_count = self.buffer.line_count();

        self.folded = std::mem::take(&mut self.folded)
            .into_iter()
            .filter_map(|line| {
                if line < start || old_lines == new_lines {
                    Some(line)
                } else if line >= old_end {
                    line.checked_add_signed(delta)
                } else {
                    None
                }
            })
            .filter(|line| *line < line_count)
            .collect();
    }

    fn selection_intersects(&self, lines: Range<usize>) -> bool {
        let range = self.selection.range();
        let first = self.buffer.byte_to_line(range.start);
        let last = self.buffer.byte_to_line(range.end);

        first < lines.end && lines.start <= last
    }

    /// Keeps the scroll offset within the bounds of the document.
    pub fn clamp_scroll(&mut self) {
        let max = self.max_scroll();

        self.scroll.x = self.scroll.x.clamp(0.0, max.x);
        self.scroll.y = self.scroll.y.clamp(0.0, max.y);
    }

    fn max_scroll(&self) -> Vector {
        if self.view.line_height <= 0.0 {
            // The widget has not been laid out yet.
            return Vector::new(f32::MAX, f32::MAX);
        }

        let rows = if self.view.total_rows > 0 {
            self.view.total_rows
        } else {
            self.buffer.line_count()
        };

        let content_height = rows as f32 * self.view.line_height;

        // Leave one line height of slack after the widest line so the caret
        // at the end of it stays comfortably visible.
        let content_width = if self.view.revision == self.buffer.revision() {
            self.view.max_line_width + self.view.line_height
        } else {
            f32::MAX
        };

        Vector::new(
            (content_width - self.view.size.width).max(0.0),
            (content_height - self.view.size.height).max(0.0),
        )
    }
}
