//! The painting helpers of the editor widget.
//!
//! [`Widget::draw`](crate::CodeEditor) orchestrates the passes; the byte ↔
//! pixel segment walk and the individual marks (highlights, squiggles, fold
//! guides, preedit) live here.

use std::ops::Range;
use std::sync::OnceLock;

use iced_core::input_method;
use iced_core::renderer::Quad;
use iced_core::svg;
use iced_core::text::{self, Paragraph as _, Text};
use iced_core::{alignment, Color, Font, Pixels, Point, Rectangle, Size, Vector};
use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::Buffer;
use crate::fold::Fold;
use crate::line_cache::LineCache;
use crate::pair::is_close_bracket;
use crate::style::Style;
use crate::widget::Metrics;

const SQUIGGLY: &[u8] = include_bytes!("../assets/squiggly.svg");

static SQUIGGLY_HANDLE: OnceLock<svg::Handle> = OnceLock::new();

/// The end of the grapheme starting at `from`, clamped to `max`.
pub(crate) fn next_grapheme_end(buffer: &Buffer, from: usize, max: usize) -> usize {
    crate::cursor::next_grapheme_boundary(buffer.text(), from).min(max)
}

/// Draws a wavy underline between `x0` and `x1` at the given baseline.
///
/// A six-pixel SVG tile repeats without stretching. The first and last tile
/// are clipped to the exact diagnostic range.
pub(crate) fn draw_squiggle<Renderer: svg::Renderer>(
    renderer: &mut Renderer,
    x0: f32,
    x1: f32,
    y: f32,
    color: iced_core::Color,
) {
    const TILE_WIDTH: f32 = 6.0;
    const TILE_HEIGHT: f32 = 3.0;

    if x1 <= x0 {
        return;
    }

    let mut x = x0.floor();
    let top = y.round() - 2.0;
    let handle = SQUIGGLY_HANDLE
        .get_or_init(|| svg::Handle::from_memory(SQUIGGLY))
        .clone();

    while x < x1 {
        let end = (x + TILE_WIDTH).min(x1);
        let visible_start = x.max(x0);
        let tile = Rectangle {
            x,
            y: top,
            width: TILE_WIDTH,
            height: TILE_HEIGHT,
        };

        renderer.draw_svg(
            svg::Svg::new(handle.clone()).color(color),
            tile,
            Rectangle {
                x: visible_start,
                width: end - visible_start,
                ..tile
            },
        );

        x += TILE_WIDTH;
    }
}

/// The per-frame inputs shared by the text-area drawing passes.
pub(crate) struct Frame<'a> {
    pub metrics: &'a Metrics,
    pub scroll: Vector,
    pub visible: &'a [usize],
}

/// Calls `emit` with `(row, x0, x1)` — in line-local coordinates — for each
/// visual-row segment that `range` covers on `line`.
///
/// The range is clipped to the line. On the last row, a range reaching past
/// the content (a selected newline) extends `newline_extension` pixels beyond
/// the text. An empty range produces one zero-width segment at the caret
/// position of its offset; callers give it a minimum width.
pub(crate) fn for_each_range_segment<P: text::Paragraph<Font = Font>>(
    buffer: &Buffer,
    cache: &mut LineCache<P>,
    line: usize,
    range: &Range<usize>,
    newline_extension: f32,
    mut emit: impl FnMut(u32, f32, f32),
) {
    let line_range = buffer.line_range(line);
    let content = buffer.line_content_range(line);

    let start = range.start.max(line_range.start);
    let end = range.end.min(line_range.end);

    if start > end {
        return;
    }

    let ranges = cache.line_geometry(buffer, line).1.to_vec();

    if range.is_empty() {
        let byte = start.clamp(content.start, content.end) - content.start;
        let (row, x) = cache.caret_in_line(buffer, line, byte);
        emit(row, x, x);
        return;
    }

    for (row, row_range) in ranges.iter().enumerate() {
        let row_start = content.start + row_range.start;
        let row_end = content.start + row_range.end;
        let is_last_row = row == ranges.len() - 1;

        let seg_start = start.max(row_start);
        let seg_end = end.min(if is_last_row { line_range.end } else { row_end });

        if seg_start >= seg_end {
            continue;
        }

        let x0 = cache.x_in_row(buffer, line, row, seg_start - content.start);
        let x1 = if seg_end > content.end {
            cache.x_in_row(buffer, line, row, row_range.end) + newline_extension
        } else {
            cache.x_in_row(buffer, line, row, seg_end - content.start)
        };

        emit(row as u32, x0, x1);
    }
}

pub(crate) fn draw_range_highlights<'a, Renderer, P, I>(
    renderer: &mut Renderer,
    buffer: &Buffer,
    cache: &mut LineCache<P>,
    frame: &Frame<'_>,
    ranges: I,
    color: Color,
) where
    Renderer: iced_core::Renderer,
    P: text::Paragraph<Font = Font>,
    I: IntoIterator<Item = &'a Range<usize>>,
{
    let Frame {
        metrics,
        scroll,
        visible,
    } = *frame;

    for range in ranges {
        let first = buffer.byte_to_line(range.start);
        let last = buffer.byte_to_line(range.end);

        for line in (first..=last).filter(|line| visible.binary_search(line).is_ok()) {
            let top =
                metrics.text_area.y + cache.first_row(line) as f32 * metrics.line_height - scroll.y;

            for_each_range_segment(
                buffer,
                cache,
                line,
                range,
                metrics.size * 0.5,
                |row, x0, x1| {
                    let width = (x1 - x0).max(if range.is_empty() { 2.0 } else { 1.0 });

                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: metrics.text_area.x + x0 - scroll.x,
                                y: top + row as f32 * metrics.line_height,
                                width,
                                height: metrics.line_height,
                            },
                            ..Quad::default()
                        },
                        color,
                    );
                },
            );
        }
    }
}

/// The visible buffer lines whose rows intersect the viewport.
pub(crate) fn visible_lines<P: text::Paragraph<Font = Font>>(
    cache: &LineCache<P>,
    scroll: Vector,
    metrics: &Metrics,
    line_count: usize,
) -> Vec<usize> {
    let total = cache.total_rows();

    if total == 0 {
        return Vec::new();
    }

    let first_row = ((scroll.y / metrics.line_height).floor().max(0.0) as u64).min(total - 1);
    let last_row = (first_row + metrics.rows_in_view() + 1).min(total - 1);

    let first = cache.line_at_row(first_row);
    let last = cache.line_at_row(last_row);

    (first..(last + 1).min(line_count))
        .filter(|line| !cache.is_hidden(*line))
        .collect()
}

/// Draws a straight vertical guide for a foldable block.
///
/// Like VS Code's bracket-pair guides, the line is split by visual row and a
/// segment is omitted whenever text occupies the guide's horizontal position.
/// This keeps the guide out of delimiters, prose, and wrapped continuations.
pub(crate) fn draw_fold_guide<Renderer, P>(
    renderer: &mut Renderer,
    cache: &mut LineCache<P>,
    buffer: &Buffer,
    metrics: &Metrics,
    scroll: Vector,
    fold: Fold,
    color: Color,
) where
    Renderer: iced_core::Renderer,
    P: text::Paragraph<Font = Font>,
{
    let body_start = fold.start + 1;
    if body_start > fold.end {
        return;
    }

    const WIDTH: f32 = 1.0;

    // The guide belongs to the indentation level where the fold opens, not
    // to the delimiter's inline column or the body's final indentation cell.
    // Thus an opener at column 0 with a two-space body draws at column 0, and
    // a nested opener indented by two spaces draws at column 2.
    let indentation = buffer
        .line_text(fold.start)
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .count();
    let body_indentation = (body_start..=fold.end)
        .map(|line| buffer.line_text(line))
        .filter(|text| {
            let trimmed = text.trim_start();
            !trimmed.is_empty() && !is_fold_closer(trimmed)
        })
        .map(|text| text.chars().take_while(|ch| ch.is_whitespace()).count())
        .min();

    // Without a deeper body indentation there is no whitespace lane for this
    // guide. This suppresses isolated ticks in blank rows of heading folds.
    if body_indentation.is_none_or(|body| body <= indentation) {
        return;
    }

    let guide_x = indentation as f32 * metrics.digit_width;
    let delimited = is_fold_closer(buffer.line_text(fold.end).trim_start());
    let first_row = cache.first_row(fold.start);
    let last_row = cache.first_row(fold.end) + u64::from(cache.rows(fold.end)) - 1;
    let top = (first_row as f32 + 0.5) * metrics.line_height;
    let bottom = (last_row as f32 + 0.5) * metrics.line_height;

    let x = metrics.text_area.x + guide_x - scroll.x;

    for visual_row in first_row..=last_row {
        let line = cache.line_at_row(visual_row);
        let row = visual_row.saturating_sub(cache.first_row(line)) as u32;

        if delimited && (line == fold.start || line == fold.end) {
            continue;
        }

        if guide_intersects_text(cache, buffer, line, row, guide_x) {
            continue;
        }

        let row_top = visual_row as f32 * metrics.line_height;
        let row_bottom = row_top + metrics.line_height;
        let segment_top = row_top.max(top);
        let segment_bottom = row_bottom.min(bottom);

        if segment_bottom <= segment_top {
            continue;
        }

        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x,
                    y: metrics.text_area.y + segment_top - scroll.y,
                    width: WIDTH,
                    height: segment_bottom - segment_top,
                },
                ..Quad::default()
            },
            color,
        );
    }
}

fn guide_intersects_text<P: text::Paragraph<Font = Font>>(
    cache: &mut LineCache<P>,
    buffer: &Buffer,
    line: usize,
    row: u32,
    guide_x: f32,
) -> bool {
    const GLYPH_MARGIN: f32 = 1.0;

    let text = buffer.line_text(line);
    let ranges = cache.line_geometry(buffer, line).1.to_vec();
    let Some(range) = ranges.get(row as usize) else {
        return false;
    };

    let visual = &text[range.clone()];
    let trimmed_start = visual.len() - visual.trim_start().len();
    let trimmed_end = visual.trim_end().len();

    if trimmed_start >= trimmed_end {
        return false;
    }

    let start = range.start + trimmed_start;
    let end = range.start + trimmed_end;
    let x0 = cache.x_in_row(buffer, line, row as usize, start);
    let x1 = cache.x_in_row(buffer, line, row as usize, end);

    guide_x >= x0.min(x1) - GLYPH_MARGIN && guide_x <= x0.max(x1) + GLYPH_MARGIN
}

fn is_fold_closer(text: &str) -> bool {
    text.starts_with(is_close_bracket) || text.starts_with("```")
}

/// Draws the IME composition text at the caret, over the line, with an
/// underline and the preedit caret.
pub(crate) fn draw_preedit<Renderer: text::Renderer<Font = Font>>(
    renderer: &mut Renderer,
    preedit: &input_method::Preedit,
    position: Point,
    metrics: &Metrics,
    style: &Style,
    clip: Rectangle,
) {
    let paragraph = Renderer::Paragraph::with_text(Text {
        content: &preedit.content,
        bounds: Size::INFINITE,
        size: Pixels(metrics.size),
        line_height: text::LineHeight::Absolute(Pixels(metrics.line_height)),
        font: metrics.font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::None,
    });

    let width = paragraph.min_bounds().width;

    // An opaque background so the composition covers the text underneath.
    renderer.fill_quad(
        Quad {
            bounds: Rectangle {
                x: position.x,
                y: position.y,
                width,
                height: metrics.line_height,
            },
            ..Quad::default()
        },
        style.background,
    );

    renderer.fill_quad(
        Quad {
            bounds: Rectangle {
                x: position.x,
                y: position.y + metrics.line_height - 1.0,
                width,
                height: 1.0,
            },
            ..Quad::default()
        },
        style.text,
    );

    renderer.fill_paragraph(&paragraph, position, style.text, clip);

    // The caret within the composition.
    let caret_byte = preedit
        .selection
        .as_ref()
        .map(|selection| selection.start.min(preedit.content.len()))
        .unwrap_or(preedit.content.len());

    let graphemes = preedit.content[..caret_byte].graphemes(true).count();
    let caret_x = if graphemes == 0 {
        0.0
    } else {
        paragraph
            .grapheme_position(0, graphemes)
            .map(|point| point.x)
            .unwrap_or(width)
    };

    renderer.fill_quad(
        Quad {
            bounds: Rectangle {
                x: position.x + caret_x,
                y: position.y,
                width: 2.0,
                height: metrics.line_height,
            },
            ..Quad::default()
        },
        style.cursor,
    );
}
