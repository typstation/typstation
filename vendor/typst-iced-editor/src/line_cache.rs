//! The line cache: virtualization and the soft-wrap row map.
//!
//! The document can be huge, so only visible lines are shaped (turned into
//! renderer paragraphs). But scrolling and the scrollbar need the height of
//! the whole document — and with soft wrap a line can occupy several visual
//! rows. [`LineCache`] resolves that tension with lazily measured row counts
//! (see [`RowCount`]) and prefix sums mapping visual rows ↔ buffer lines.
//!
//! This module also owns the byte ↔ pixel geometry built on top of shaped
//! paragraphs: caret position, hit-testing, and viewport reveal. With hanging
//! indentation, the first visual row of a line can wrap at the full text width
//! while continuation rows wrap at the reduced width left after their indent,
//! so the cache stores both whole-line paragraphs and per-row fragments.

use std::collections::HashMap;
use std::ops::Range;

use iced_core::text::{self, Span, Text};
use iced_core::time::{Duration, Instant};
use iced_core::{alignment, font};
use iced_core::{Font, Pixels, Point, Size, Vector};
use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::Buffer;
use crate::content::{Internal, WrapDirty};
use crate::fold::{self, Fold};
use crate::highlight::{self, SyntaxTheme, Tag};
use crate::widget::Metrics;

/// The visual-row count of a buffer line.
///
/// Lines start with an estimate based on their byte length and are measured
/// exactly (shaped with the current wrap geometry) only when they come into
/// view. This is what keeps large documents cheap: neither a big paste nor a
/// long scrollbar jump has to shape thousands of lines up front. The estimate
/// is stored, not recomputed, so summing the wrap map never touches the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowCount {
    Estimated(u32),
    Measured(u32),
}

impl RowCount {
    pub(crate) fn get(self) -> u32 {
        match self {
            Self::Estimated(rows) | Self::Measured(rows) => rows,
        }
    }
}

/// Shaped paragraphs of individual lines plus the wrap map: how many visual
/// rows each buffer line occupies, with prefix sums for row ↔ line lookups.
pub(crate) struct LineCache<P> {
    revision: u64,
    fold_revision: u64,
    font: Font,
    size: f32,
    line_height: f32,
    char_width: f32,
    /// The width lines wrap at, or `None` when soft wrap is disabled.
    wrap_width: Option<f32>,
    /// Whether wrapped rows are indented under their line's indentation.
    wrap_indent: bool,
    syntax: SyntaxTheme,
    entries: HashMap<usize, P>,
    /// Individually shaped visual rows.
    ///
    /// They are the source of truth for row-local x positions and hit-testing
    /// when wrapped rows have a hanging indent. They also avoid drawing one
    /// wrapped paragraph through adjoining clips, which caused seams and
    /// clipped glyphs at fractional scales.
    row_entries: HashMap<usize, Vec<P>>,
    /// The byte ranges of each visual row.
    ///
    /// For plain wrap, these are probed from a single shaped paragraph. For
    /// hanging-indented wrap, row 0 is discovered at the full wrap width and
    /// the remaining text is discovered at the reduced continuation width.
    row_spans: HashMap<usize, Vec<Range<usize>>>,
    runs: Vec<(Range<usize>, Tag)>,
    /// Visual rows per buffer line, estimated until measured (see
    /// [`RowCount`]).
    rows: Vec<RowCount>,
    /// Whether the next [`measure_visible`](Self::measure_visible) may spend
    /// the overscan budget. Armed once per frame, so the many cache syncs a
    /// frame performs (update, draw, hit tests) only measure ahead once.
    overscan_allowed: bool,
    /// Whether the overscan band still has unmeasured lines, so the widget
    /// keeps requesting frames until the band is filled.
    overscan_pending: bool,
    /// Fold ranges currently available in the document.
    foldable: Vec<Fold>,
    /// The buffer revision `foldable` was discovered at.
    foldable_revision: u64,
    /// When the deferred fold discovery is due, while `foldable` is stale.
    fold_walk_due: Option<Instant>,
    /// The timestamp of the frame being synced; consumed by the fold-walk
    /// debounce so only the once-per-frame sync advances its clock.
    frame: Option<Instant>,
    /// Fold ranges that are currently collapsed.
    folds: Vec<Fold>,
    /// Whether a buffer line is hidden by a collapsed fold.
    hidden: Vec<bool>,
    /// `offsets[i]` = total rows before line `i` (length: lines + 1).
    offsets: Vec<u64>,
    /// Estimated pixel width of the widest line, for the horizontal scrollbar.
    /// Zero while soft wrap is on (no horizontal scrolling then).
    doc_width: f32,
}

/// Once the shaped-paragraph cache grows past this, entries far from the
/// viewport are pruned (see [`LineCache::prune_entries`]) — a windowed prune
/// instead of a full clear, so scrolling never re-shapes the whole view.
const CACHE_CAPACITY: usize = 512;
/// How many not-yet-measured lines each frame may shape beyond the visible
/// ones (see [`LineCache::measure_visible`]).
const OVERSCAN_BUDGET: usize = 24;
/// How long edits must pause before stale fold ranges are re-discovered
/// (see [`LineCache::should_walk_folds`]).
const FOLD_WALK_DEBOUNCE: Duration = Duration::from_millis(200);

impl<P: text::Paragraph<Font = Font>> LineCache<P> {
    pub(crate) fn new() -> Self {
        Self {
            revision: 0,
            fold_revision: 0,
            font: Font::MONOSPACE,
            size: 0.0,
            line_height: 0.0,
            char_width: 1.0,
            wrap_width: None,
            wrap_indent: true,
            syntax: SyntaxTheme::plain(),
            entries: HashMap::new(),
            row_entries: HashMap::new(),
            row_spans: HashMap::new(),
            runs: Vec::new(),
            rows: Vec::new(),
            foldable: Vec::new(),
            // A sentinel no buffer revision takes, so the first sync walks.
            foldable_revision: u64::MAX,
            fold_walk_due: None,
            frame: None,
            folds: Vec::new(),
            hidden: Vec::new(),
            offsets: vec![0],
            doc_width: 0.0,
            overscan_allowed: false,
            overscan_pending: false,
        }
    }

    /// Arms the overscan budget for the next sync and stamps it with the
    /// frame time. Called once per frame.
    pub(crate) fn allow_overscan(&mut self, now: Instant) {
        self.overscan_allowed = true;
        self.frame = Some(now);
    }

    /// Whether the overscan band still has unmeasured lines.
    pub(crate) fn overscan_pending(&self) -> bool {
        self.overscan_pending
    }

    /// When the deferred fold discovery is due, if a walk is pending.
    pub(crate) fn fold_walk_due(&self) -> Option<Instant> {
        self.fold_walk_due
    }

    /// Brings the cache and the wrap map up to date with the content.
    pub(crate) fn sync(
        &mut self,
        internal: &mut Internal,
        metrics: &Metrics,
        wrap: bool,
        wrap_indent: bool,
    ) {
        let wrap_width = wrap.then_some(metrics.text_area.width);

        let shape_changed = self.font != metrics.font
            || self.size != metrics.size
            || self.line_height != metrics.line_height
            || self.char_width != metrics.digit_width
            || self.wrap_width != wrap_width
            || self.wrap_indent != wrap_indent;
        let buffer_changed = self.revision != internal.buffer.revision();
        let fold_changed = self.fold_revision != internal.fold_revision;

        if shape_changed {
            self.font = metrics.font;
            self.size = metrics.size;
            self.line_height = metrics.line_height;
            self.char_width = metrics.digit_width;
            self.wrap_width = wrap_width;
            self.wrap_indent = wrap_indent;

            let _ = internal.wrap_dirty.take();
            self.rebuild(&internal.buffer);
        } else if buffer_changed {
            self.entries.clear();
            self.row_entries.clear();
            self.row_spans.clear();

            match internal.wrap_dirty.take() {
                WrapDirty::Splices(splices) => {
                    // Inserted lines start as estimates; only the visible ones
                    // are shaped below. This is what keeps a large paste from
                    // shaping thousands of lines at once.
                    for splice in splices {
                        let fill: Vec<RowCount> = (0..splice.new_lines)
                            .map(|i| self.initial_rows(&internal.buffer, splice.start + i))
                            .collect();
                        let end = (splice.start + splice.old_lines).min(self.rows.len());
                        self.rows.splice(splice.start.min(end)..end, fill);
                    }
                }
                WrapDirty::Clean | WrapDirty::Rebuild => self.rebuild(&internal.buffer),
            }
        }

        self.revision = internal.buffer.revision();
        self.fold_revision = internal.fold_revision;

        let walk = self.should_walk_folds(internal, shape_changed, fold_changed);

        if shape_changed
            || buffer_changed
            || fold_changed
            || walk
            || self.hidden.len() != internal.buffer.line_count()
        {
            self.sync_folds(internal, walk);
            self.rebuild_offsets();
        }

        if shape_changed || buffer_changed {
            self.doc_width = self.estimate_doc_width(&internal.buffer);
        }

        self.measure_visible(
            &internal.buffer,
            internal.scroll.y,
            metrics.rows_in_view(),
            metrics.line_height,
        );
        self.prune_entries(
            internal.scroll.y,
            metrics.rows_in_view(),
            metrics.line_height,
        );
    }

    /// Invalidates the cache if the syntax theme changed.
    ///
    /// The theme affects layout (bold and italic runs have different
    /// widths), so hit-testing must use the same theme as drawing.
    pub(crate) fn set_syntax(&mut self, syntax: &SyntaxTheme, buffer: &Buffer) {
        if &self.syntax != syntax {
            self.syntax = syntax.clone();
            self.entries.clear();
            self.row_entries.clear();
            self.row_spans.clear();

            // Different run widths can change where lines wrap.
            if self.wrap_width.is_some() {
                self.rebuild(buffer);
                // Nudge the fold revision so the next sync re-runs the fold
                // and offset pass against the reshaped rows, even though
                // neither the buffer nor the folds changed.
                self.fold_revision = self.fold_revision.wrapping_sub(1);
            }
        }
    }

    /// Resets the wrap map to estimates. With wrap on, lines are measured
    /// lazily as they become visible ([`measure_visible`](Self::measure_visible));
    /// without wrap every line is exactly one row.
    pub(crate) fn rebuild(&mut self, buffer: &Buffer) {
        self.entries.clear();
        self.row_entries.clear();
        self.row_spans.clear();
        self.rows = (0..buffer.line_count())
            .map(|line| self.initial_rows(buffer, line))
            .collect();
        self.rebuild_offsets();
    }

    /// Recomputes the row prefix sums. A pure sum over the stored counts, so
    /// it is cheap enough to run whenever a measurement lands.
    pub(crate) fn rebuild_offsets(&mut self) {
        self.offsets.clear();
        self.offsets.push(0);

        let mut total = 0;
        for (line, rows) in self.rows.iter().enumerate() {
            if !self.is_hidden(line) {
                total += u64::from(rows.get());
            }
            self.offsets.push(total);
        }
    }

    /// Estimates the pixel width of the widest line for the horizontal
    /// scrollbar. Zero when wrapping (no horizontal scroll then). Uses the
    /// widest line's byte length — exact for the monospace ASCII of most
    /// code, and an over-estimate is harmless (a little extra scroll room).
    /// One O(lines) pass per edit, like the offsets rebuild beside it.
    pub(crate) fn estimate_doc_width(&self, buffer: &Buffer) -> f32 {
        if self.wrap_width.is_some() {
            return 0.0;
        }

        let widest = (0..buffer.line_count())
            .map(|line| buffer.line_content_range(line).len())
            .max()
            .unwrap_or(0);

        widest as f32 * self.char_width
    }

    /// The content width used to size the horizontal scrollbar.
    pub(crate) fn content_width(&self) -> f32 {
        self.doc_width
    }

    /// The pixel indent applied to the *wrapped* rows of a line: the width of
    /// its leading whitespace, capped so content still has room. Zero when
    /// soft wrap or the feature is off. Purely visual — the text is unchanged.
    pub(crate) fn line_indent(&self, buffer: &Buffer, line: usize) -> f32 {
        let Some(wrap_width) = self.wrap_width else {
            return 0.0;
        };
        if !self.wrap_indent {
            return 0.0;
        }

        let text = buffer.line_text(line);
        let leading = text.len() - text.trim_start().len();

        (leading as f32 * self.char_width).min(wrap_width * 0.5)
    }

    pub(crate) fn estimated_rows(&self, buffer: &Buffer, line: usize) -> u32 {
        let Some(wrap_width) = self.wrap_width else {
            return 1;
        };

        let char_width = self.char_width.max(1.0);
        let columns_per_row = (wrap_width / char_width).floor().max(1.0) as usize;
        let columns = buffer.line_text(line).len().max(1);

        columns.div_ceil(columns_per_row).min(u32::MAX as usize) as u32
    }

    /// Whether this sync should run the O(document) fold discovery.
    ///
    /// With nothing collapsed, stale fold ranges are purely cosmetic (gutter
    /// chevrons and guides), so the walk runs at most once per
    /// [`FOLD_WALK_DEBOUNCE`] during bursts of edits instead of once per
    /// keystroke. Collapsed folds need exact ranges to hide the right lines,
    /// so any staleness walks immediately.
    fn should_walk_folds(
        &mut self,
        internal: &Internal,
        shape_changed: bool,
        fold_changed: bool,
    ) -> bool {
        let stale = self.foldable_revision != internal.buffer.revision();
        // Only the once-per-frame armed sync advances the debounce clock;
        // hit-test syncs between frames leave it untouched.
        let frame = self.frame.take();

        if !stale {
            self.fold_walk_due = None;
            return false;
        }

        if shape_changed || fold_changed || !internal.folded.is_empty() {
            self.fold_walk_due = None;
            return true;
        }

        let Some(now) = frame else {
            return false;
        };

        match self.fold_walk_due {
            Some(due) if now >= due => {
                self.fold_walk_due = None;
                true
            }
            Some(_) => false,
            None => {
                self.fold_walk_due = Some(now + FOLD_WALK_DEBOUNCE);
                false
            }
        }
    }

    fn sync_folds(&mut self, internal: &Internal, walk: bool) {
        if walk {
            self.foldable = fold::fold_ranges(&internal.buffer);
            self.foldable_revision = internal.buffer.revision();
        }

        self.hidden = vec![false; internal.buffer.line_count()];
        self.folds = self
            .foldable
            .iter()
            .copied()
            .filter(|fold| internal.folded.contains(&fold.start))
            .collect();

        for fold in &self.folds {
            for line in fold.hidden_lines() {
                if let Some(hidden) = self.hidden.get_mut(line) {
                    *hidden = true;
                }
            }
        }
    }

    /// The starting row count of a line: exact (one row) without wrap,
    /// estimated from the line's length with it.
    pub(crate) fn initial_rows(&self, buffer: &Buffer, line: usize) -> RowCount {
        if self.wrap_width.is_none() {
            RowCount::Measured(1)
        } else if line < buffer.line_count() {
            RowCount::Estimated(self.estimated_rows(buffer, line))
        } else {
            // A later splice in the same batch shortens the document again;
            // the estimate is refined once the line is visible anyway.
            RowCount::Estimated(1)
        }
    }

    /// Measures a line if it has not been measured yet, updating its row
    /// count. Returns whether the count changed (so offsets need rebuilding).
    pub(crate) fn ensure_measured(&mut self, buffer: &Buffer, line: usize) -> bool {
        if self.wrap_width.is_none() || matches!(self.rows.get(line), Some(RowCount::Measured(_))) {
            return false;
        }

        let spans = self.visual_row_ranges(buffer, line);
        let rows = spans.len().max(1).min(u32::MAX as usize) as u32;

        let paragraph = self.shape(buffer, line);
        let _ = self.entries.insert(line, paragraph);
        let _ = self.row_spans.insert(line, spans);
        let _ = self.row_entries.remove(&line);

        let changed = self.rows.get(line).copied().map(RowCount::get) != Some(rows);
        if let Some(slot) = self.rows.get_mut(line) {
            *slot = RowCount::Measured(rows);
        }
        changed
    }

    /// Measures the lines currently in view, so their wrapped heights are
    /// exact even though the rest of the document is only estimated. Bounded
    /// to the visible lines plus a budgeted overscan, so a frame stays cheap
    /// regardless of document size — even when a scrollbar drag lands every
    /// frame on territory never measured before.
    pub(crate) fn measure_visible(
        &mut self,
        buffer: &Buffer,
        scroll_y: f32,
        view_rows: u64,
        line_height: f32,
    ) {
        if self.wrap_width.is_none() {
            return;
        }

        // The rows on screen must be exact. Measuring shifts the offsets,
        // which can change what is visible, so iterate to settle the row map
        // before drawing.
        for _ in 0..4 {
            let (first, last) = self.line_band(scroll_y, view_rows, line_height, 0);

            let mut changed = false;
            for line in first..=last {
                if self.is_hidden(line) {
                    continue;
                }

                changed |= self.ensure_measured(buffer, line);
            }

            if changed {
                self.rebuild_offsets();
            } else {
                break;
            }
        }

        // Measure a band around the viewport too, reducing pop-in while
        // scrolling — but on a budget, spent at most once per frame: the
        // band fills over the following frames (the widget keeps requesting
        // them while `overscan_pending`) instead of multiplying the cost of
        // any single one.
        if !self.overscan_allowed {
            return;
        }

        self.overscan_allowed = false;

        let overscan = view_rows.saturating_mul(2).max(1);
        let (first, last) = self.line_band(scroll_y, view_rows, line_height, overscan);

        let mut budget = OVERSCAN_BUDGET;
        let mut changed = false;
        let mut pending = false;

        for line in first..=last {
            if self.is_hidden(line) || matches!(self.rows.get(line), Some(RowCount::Measured(_))) {
                continue;
            }

            if budget == 0 {
                pending = true;
                break;
            }

            changed |= self.ensure_measured(buffer, line);
            budget -= 1;
        }

        self.overscan_pending = pending;

        if changed {
            self.rebuild_offsets();
        }
    }

    /// The buffer lines covering the viewport at the given scroll offset,
    /// extended by `overscan` rows on both sides.
    pub(crate) fn line_band(
        &self,
        scroll_y: f32,
        view_rows: u64,
        line_height: f32,
        overscan: u64,
    ) -> (usize, usize) {
        let total = self.total_rows().max(1);
        let visible_first = ((scroll_y / line_height).floor().max(0.0) as u64).min(total - 1);
        let first_row = visible_first.saturating_sub(overscan);
        let last_row = (visible_first + view_rows + overscan + 1).min(total - 1);

        (self.line_at_row(first_row), self.line_at_row(last_row))
    }

    pub(crate) fn prune_entries(&mut self, scroll_y: f32, view_rows: u64, line_height: f32) {
        if self.entries.len() <= CACHE_CAPACITY {
            return;
        }

        let total = self.total_rows().max(1);
        let visible_first = ((scroll_y / line_height).floor().max(0.0) as u64).min(total - 1);
        let keep_rows = view_rows.saturating_mul(6).max(64);
        let first_line = self.line_at_row(visible_first.saturating_sub(keep_rows));
        let last_line = self.line_at_row((visible_first + view_rows + keep_rows).min(total - 1));

        self.entries
            .retain(|line, _| first_line <= *line && *line <= last_line);
        self.row_entries
            .retain(|line, _| first_line <= *line && *line <= last_line);
        self.row_spans
            .retain(|line, _| first_line <= *line && *line <= last_line);

        if self.entries.len() > CACHE_CAPACITY {
            let center = first_line + (last_line.saturating_sub(first_line) / 2);
            let mut lines = self.entries.keys().copied().collect::<Vec<_>>();
            lines.sort_by_key(|line| line.abs_diff(center));

            for line in lines.into_iter().skip(CACHE_CAPACITY) {
                let _ = self.entries.remove(&line);
                let _ = self.row_entries.remove(&line);
                let _ = self.row_spans.remove(&line);
            }
        }
    }

    /// The total number of visual rows in the document.
    pub(crate) fn total_rows(&self) -> u64 {
        self.offsets.last().copied().unwrap_or(0)
    }

    /// The first visual row of the given line.
    pub(crate) fn first_row(&self, line: usize) -> u64 {
        self.offsets.get(line).copied().unwrap_or(0)
    }

    /// The number of visual rows of the given line.
    pub(crate) fn rows(&self, line: usize) -> u32 {
        self.rows.get(line).copied().map(RowCount::get).unwrap_or(1)
    }

    pub(crate) fn is_hidden(&self, line: usize) -> bool {
        self.hidden.get(line).copied().unwrap_or(false)
    }

    pub(crate) fn is_folded(&self, line: usize) -> bool {
        self.folds.iter().any(|fold| fold.start == line)
    }

    /// The foldable ranges of the document (collapsed or not).
    pub(crate) fn foldable(&self) -> &[Fold] {
        &self.foldable
    }

    pub(crate) fn has_fold(&self, line: usize) -> bool {
        self.foldable.iter().any(|fold| fold.start == line)
    }

    /// The line containing the given visual row.
    pub(crate) fn line_at_row(&self, row: u64) -> usize {
        let mut line = self
            .offsets
            .partition_point(|&offset| offset <= row)
            .saturating_sub(1)
            .min(self.rows.len().saturating_sub(1));

        while line < self.rows.len() && self.is_hidden(line) {
            line += 1;
        }

        if line < self.rows.len() {
            line
        } else {
            self.rows
                .iter()
                .enumerate()
                .rev()
                .find_map(|(line, _)| (!self.is_hidden(line)).then_some(line))
                .unwrap_or(0)
        }
    }

    /// Returns the shaped paragraph of the given line.
    pub(crate) fn paragraph(&mut self, buffer: &Buffer, line: usize) -> &P {
        if !self.entries.contains_key(&line) {
            let paragraph = self.shape(buffer, line);
            let _ = self.entries.insert(line, paragraph);
            let _ = self.row_spans.remove(&line);
        }

        &self.entries[&line]
    }

    /// The shaped whole-line paragraph together with the byte ranges of its
    /// visual rows, both cached.
    ///
    /// The paragraph is useful for drawing unindented lines as one unit. The
    /// ranges are the shared row map used by drawing, hit-testing, selection,
    /// diagnostics, and caret geometry.
    pub(crate) fn line_geometry(&mut self, buffer: &Buffer, line: usize) -> (&P, &[Range<usize>]) {
        if !self.entries.contains_key(&line) {
            let paragraph = self.shape(buffer, line);
            let _ = self.entries.insert(line, paragraph);
            let _ = self.row_spans.remove(&line);
        }

        if !self.row_spans.contains_key(&line) {
            let spans = self.visual_row_ranges(buffer, line);
            let _ = self.row_spans.insert(line, spans);
        }

        (&self.entries[&line], &self.row_spans[&line])
    }

    /// Returns separately shaped visual rows for drawing and row-local
    /// geometry.
    ///
    /// These no-wrap fragments are also the per-row geometry source for
    /// hanging indentation: the first row can use the full wrap width while
    /// continuation rows are measured in the reduced width left after their
    /// visual indent.
    pub(crate) fn row_paragraphs(&mut self, buffer: &Buffer, line: usize) -> &[P] {
        if !self.row_entries.contains_key(&line) {
            let ranges = self.line_geometry(buffer, line).1.to_vec();
            let paragraphs = ranges
                .into_iter()
                .map(|range| {
                    self.shape_range(buffer, line, range, Size::INFINITE, text::Wrapping::None)
                })
                .collect();

            let _ = self.row_entries.insert(line, paragraphs);
        }

        &self.row_entries[&line]
    }

    /// The x coordinate of the caret placed at `byte` within the given
    /// visual row, including the hanging indent applied to wrapped rows.
    pub(crate) fn x_in_row(
        &mut self,
        buffer: &Buffer,
        line: usize,
        row: usize,
        byte: usize,
    ) -> f32 {
        let text = buffer.line_text(line);
        let ranges = self.line_geometry(buffer, line).1.to_vec();
        let Some(row_range) = ranges.get(row) else {
            return 0.0;
        };

        let byte = byte.clamp(row_range.start, row_range.end);
        let shift = if row >= 1 {
            self.line_indent(buffer, line)
        } else {
            0.0
        };

        if byte == row_range.start {
            return shift;
        }

        let grapheme = text[row_range.start..byte].graphemes(true).count();
        let paragraph = self.row_paragraphs(buffer, line).get(row);

        shift
            + paragraph
                .and_then(|paragraph| paragraph.grapheme_position(0, grapheme))
                .map(|point| point.x)
                .unwrap_or(0.0)
    }

    /// The visual row and x position of a caret placed at `byte` within a
    /// line. A byte on a wrap boundary belongs to the following row.
    pub(crate) fn caret_in_line(
        &mut self,
        buffer: &Buffer,
        line: usize,
        byte: usize,
    ) -> (u32, f32) {
        let ranges = self.line_geometry(buffer, line).1.to_vec();
        let row = ranges
            .iter()
            .rposition(|range| range.start <= byte)
            .unwrap_or(0);
        let x = self.x_in_row(buffer, line, row, byte);

        (row as u32, x)
    }

    pub(crate) fn shape(&mut self, buffer: &Buffer, line: usize) -> P {
        let text = buffer.line_text(line);

        let (bounds, wrapping) = match self.wrap_width {
            Some(width) => (Size::new(width, f32::INFINITY), text::Wrapping::WordOrGlyph),
            None => (Size::INFINITE, text::Wrapping::None),
        };

        self.shape_range(buffer, line, 0..text.len(), bounds, wrapping)
    }

    /// Computes the byte ranges of a line's visual rows.
    ///
    /// `iced`/cosmic-text wraps a paragraph at one width. Hanging indentation
    /// needs two widths: the first row gets the full text area, while the
    /// continuation rows must fit after being shifted right by the visual
    /// indent. We therefore shape the full line once to find row 0, then shape
    /// the remainder at the reduced width and splice the ranges together.
    fn visual_row_ranges(&mut self, buffer: &Buffer, line: usize) -> Vec<Range<usize>> {
        let text = buffer.line_text(line);
        let content_len = text.len();
        let Some(width) = self.wrap_width else {
            return std::iter::once(0..content_len).collect();
        };

        let indent = self.line_indent(buffer, line);
        if indent <= 0.0 {
            let paragraph = self.shape_range(
                buffer,
                line,
                0..content_len,
                Size::new(width, f32::INFINITY),
                text::Wrapping::WordOrGlyph,
            );
            let rows = rows_of(&paragraph, self.line_height);

            return row_ranges(&paragraph, rows, content_len, self.line_height);
        }

        let first = self.shape_range(
            buffer,
            line,
            0..content_len,
            Size::new(width, f32::INFINITY),
            text::Wrapping::WordOrGlyph,
        );
        let first_rows = rows_of(&first, self.line_height);
        let mut first_ranges = row_ranges(&first, first_rows, content_len, self.line_height);
        let first_end = first_ranges
            .drain(..1)
            .next()
            .map(|range| range.end)
            .unwrap_or(content_len)
            .min(content_len);

        if first_end >= content_len {
            return std::iter::once(0..content_len).collect();
        }

        let usable = (width - indent).max(self.char_width);
        let rest = self.shape_range(
            buffer,
            line,
            first_end..content_len,
            Size::new(usable, f32::INFINITY),
            text::Wrapping::WordOrGlyph,
        );
        let rest_rows = rows_of(&rest, self.line_height);
        let mut ranges: Vec<Range<usize>> = std::iter::once(0..first_end).collect();

        ranges.extend(
            row_ranges(&rest, rest_rows, content_len - first_end, self.line_height)
                .into_iter()
                .map(|range| first_end + range.start..first_end + range.end),
        );

        ranges
    }

    fn shape_range(
        &mut self,
        buffer: &Buffer,
        line: usize,
        range: Range<usize>,
        bounds: Size,
        wrapping: text::Wrapping,
    ) -> P {
        let content = buffer.line_content_range(line);
        let text = buffer.line_text(line);
        let absolute = content.start + range.start..content.start + range.end;

        highlight::line_highlights(buffer.root(), absolute, &mut self.runs);

        let mut spans: Vec<Span<'_, (), Font>> = Vec::new();
        let mut cursor = range.start;

        for (highlight, tag) in &self.runs {
            let start = highlight.start - content.start;
            let end = highlight.end - content.start;

            if start > cursor {
                spans.push(Span::new(&text[cursor..start]));
            }

            spans.push(styled_span(
                &text[start..end],
                self.syntax.style(*tag),
                self.font,
            ));
            cursor = end;
        }

        if cursor < range.end {
            spans.push(Span::new(&text[cursor..range.end]));
        }

        P::with_spans(Text {
            content: spans.as_slice(),
            bounds,
            size: Pixels(self.size),
            line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping,
        })
    }
}

/// How many visual rows a shaped paragraph occupies.
pub(crate) fn rows_of<P: text::Paragraph>(paragraph: &P, line_height: f32) -> u32 {
    ((paragraph.min_bounds().height / line_height).round() as u32).max(1)
}

pub(crate) fn styled_span<'a>(
    text: &'a str,
    style: crate::highlight::SyntaxStyle,
    base_font: Font,
) -> Span<'a, (), Font> {
    let mut span = Span::new(text);

    if let Some(color) = style.color {
        span = span.color(color);
    }

    if style.weight.is_some() || style.italic {
        span = span.font(Font {
            weight: style.weight.unwrap_or(base_font.weight),
            style: if style.italic {
                font::Style::Italic
            } else {
                base_font.style
            },
            ..base_font
        });
    }

    span
}

/// The byte ranges of each visual row of a line, discovered by probing the
/// paragraph at the start of every row.
fn row_ranges<P: text::Paragraph>(
    paragraph: &P,
    rows: u32,
    content_len: usize,
    line_height: f32,
) -> Vec<Range<usize>> {
    let starts: Vec<usize> = (0..rows)
        .map(|row| {
            paragraph
                .hit_test(Point::new(0.0, (row as f32 + 0.5) * line_height))
                .map(|hit| hit.cursor().min(content_len))
                .unwrap_or(0)
        })
        .collect();

    (0..rows as usize)
        .map(|row| {
            let start = starts[row];
            let end = starts.get(row + 1).copied().unwrap_or(content_len);

            start..end.max(start)
        })
        .collect()
}

/// Finds the byte offset at the given window position.
pub(crate) fn offset_at<P: text::Paragraph<Font = Font>>(
    position: Point,
    metrics: &Metrics,
    internal: &Internal,
    cache: &mut LineCache<P>,
    scroll: Vector,
) -> usize {
    let x = position.x - metrics.text_area.x + scroll.x;
    let y = position.y - metrics.text_area.y + scroll.y;

    let total = cache.total_rows().max(1);
    let row = ((y / metrics.line_height).floor().max(0.0) as u64).min(total - 1);

    offset_at_row(Point::new(x, 0.0), row, metrics, &internal.buffer, cache)
}

/// Finds the byte offset strictly under the pointer.
///
/// Unlike [`offset_at`], this does not clamp positions in the empty space
/// before or after a visual row to its nearest caret. That distinction lets
/// clicks keep their editor-friendly behavior without producing hover
/// requests for text that is not actually under the pointer.
pub(crate) fn hover_offset_at<P: text::Paragraph<Font = Font>>(
    position: Point,
    metrics: &Metrics,
    internal: &Internal,
    cache: &mut LineCache<P>,
    scroll: Vector,
) -> Option<usize> {
    let x = position.x - metrics.text_area.x + scroll.x;
    let y = position.y - metrics.text_area.y + scroll.y;
    let total = cache.total_rows().max(1);

    if x < 0.0 || y < 0.0 || y >= total as f32 * metrics.line_height {
        return None;
    }

    let row = (y / metrics.line_height).floor() as u64;
    let line = cache.line_at_row(row);
    let row_in_line = row.saturating_sub(cache.first_row(line)) as usize;
    let content = internal.buffer.line_content_range(line);
    let ranges = cache.line_geometry(&internal.buffer, line).1.to_vec();
    let range = ranges.get(row_in_line)?;

    if range.is_empty() {
        return None;
    }

    let start_x = if row_in_line == 0 {
        0.0
    } else {
        cache.line_indent(&internal.buffer, line)
    };
    let end_x = cache.x_in_row(&internal.buffer, line, row_in_line, range.end);

    if x < start_x || x > end_x {
        return None;
    }

    let local = Point::new(x - start_x, 0.5 * metrics.line_height);
    let hit = cache
        .row_paragraphs(&internal.buffer, line)
        .get(row_in_line)?
        .hit_test(local)?;

    Some(content.start + range.start + hit.cursor().min(range.end - range.start))
}

/// Finds the byte offset at horizontal position `point.x` on the given
/// visual row.
pub(crate) fn offset_at_row<P: text::Paragraph<Font = Font>>(
    point: Point,
    row: u64,
    metrics: &Metrics,
    buffer: &Buffer,
    cache: &mut LineCache<P>,
) -> usize {
    let line = cache.line_at_row(row);
    let row_in_line = row.saturating_sub(cache.first_row(line)) as usize;

    let content = buffer.line_content_range(line);
    let ranges = cache.line_geometry(buffer, line).1.to_vec();
    let Some(range) = ranges.get(row_in_line) else {
        return content.end;
    };
    let shift = if row_in_line >= 1 {
        cache.line_indent(buffer, line)
    } else {
        0.0
    };

    let local = Point::new((point.x - shift).max(0.0), 0.5 * metrics.line_height);
    let hit = cache
        .row_paragraphs(buffer, line)
        .get(row_in_line)
        .and_then(|paragraph| paragraph.hit_test(local));

    match hit {
        Some(hit) => content.start + range.start + hit.cursor().min(range.end - range.start),
        None if point.x <= shift => content.start + range.start,
        None => content.start + range.end,
    }
}

/// The caret's line, visual row within the line, and x position.
pub(crate) fn caret_geometry<P: text::Paragraph<Font = Font>>(
    internal: &Internal,
    cache: &mut LineCache<P>,
) -> (usize, u32, f32) {
    let head = internal.buffer.clamp(internal.selection.head);
    let line = internal.buffer.byte_to_line(head);
    let content = internal.buffer.line_content_range(line);

    let (row, x) = cache.caret_in_line(&internal.buffer, line, head - content.start);

    (line, row, x)
}

/// Scrolls the viewport just enough to bring the caret into view.
pub(crate) fn reveal_caret<P: text::Paragraph<Font = Font>>(
    internal: &mut Internal,
    metrics: &Metrics,
    cache: &mut LineCache<P>,
    wrap: bool,
) {
    let (line, row, caret_x) = caret_geometry(internal, cache);

    let row_top = (cache.first_row(line) + u64::from(row)) as f32 * metrics.line_height;
    let view = metrics.text_area;

    if row_top < internal.scroll.y {
        internal.scroll.y = row_top;
    } else if row_top + metrics.line_height > internal.scroll.y + view.height {
        internal.scroll.y = row_top + metrics.line_height - view.height;
    }

    if wrap {
        return;
    }

    let margin = metrics.size.min(view.width / 4.0);

    if caret_x < internal.scroll.x + margin {
        internal.scroll.x = (caret_x - margin).max(0.0);
    } else if caret_x > internal.scroll.x + view.width - margin {
        internal.scroll.x = caret_x - view.width + margin;
    }

    // Make sure clamping does not undo the horizontal reveal.
    internal.view.max_line_width = internal.view.max_line_width.max(caret_x);
}
