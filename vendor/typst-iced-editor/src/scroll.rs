//! Viewport scrolling: the smooth-scroll animation and the editor-owned
//! scrollbars.
//!
//! The scrollbars are drawn as an overlay over the viewport, so they never
//! participate in layout. Thumb drags freeze the geometry captured at press
//! time ([`ScrollbarDrag`]), which keeps the hot mouse-move path O(1) and
//! prevents lazily measured rows from shifting the thumb under the pointer.

use iced_core::renderer::Quad;
use iced_core::text;
use iced_core::time::Instant;
use iced_core::{Color, Font, Point, Rectangle, Vector};

use std::ops::Range;

use crate::buffer::Buffer;
use crate::diagnostic::Severity;
use crate::line_cache::LineCache;
use crate::style::Style;
use crate::widget::Metrics;

pub(crate) const SCROLLBAR_WIDTH: f32 = 12.0;
pub(crate) const SCROLLBAR_PADDING: f32 = 3.0;
pub(crate) const SCROLLBAR_MIN_THUMB: f32 = 24.0;
pub(crate) const SCROLLBAR_EPSILON: f32 = 0.5;
/// Height of a diagnostic/search mark drawn on the scrollbar track.
pub(crate) const SCROLLBAR_TICK_HEIGHT: f32 = 2.0;

pub(crate) struct SmoothScroll {
    display: Vector,
    target: Vector,
    last_tick: Option<Instant>,
    initialized: bool,
}

impl SmoothScroll {
    const EPSILON: f32 = 0.5;
    const MAX_DT: f32 = 0.05;
    const TIME_CONSTANT: f32 = 0.055;

    pub(crate) fn new() -> Self {
        Self {
            display: Vector::new(0.0, 0.0),
            target: Vector::new(0.0, 0.0),
            last_tick: None,
            initialized: false,
        }
    }

    pub(crate) fn current(&mut self, target: Vector) -> Vector {
        if !self.initialized {
            self.jump_to(target);
        } else {
            self.target = target;
        }

        self.display
    }

    pub(crate) fn advance(&mut self, target: Vector, now: Instant) -> (Vector, bool) {
        if !self.initialized {
            self.jump_to(target);
            return (self.display, false);
        }

        self.target = target;

        let dt = self
            .last_tick
            .map(|last_tick| now.saturating_duration_since(last_tick).as_secs_f32())
            .unwrap_or(1.0 / 60.0)
            .min(Self::MAX_DT);

        self.last_tick = Some(now);

        let alpha = 1.0 - (-dt / Self::TIME_CONSTANT).exp();
        self.display.x += (self.target.x - self.display.x) * alpha;
        self.display.y += (self.target.y - self.display.y) * alpha;

        if (self.target.x - self.display.x).abs() <= Self::EPSILON
            && (self.target.y - self.display.y).abs() <= Self::EPSILON
        {
            self.display = self.target;
            self.last_tick = None;
            (self.display, false)
        } else {
            (self.display, true)
        }
    }

    pub(crate) fn jump_to(&mut self, target: Vector) {
        self.display = target;
        self.target = target;
        self.last_tick = None;
        self.initialized = true;
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ScrollbarDrag {
    /// The scrollbar geometry frozen at the start of the drag, so mouse moves
    /// need no cache sync and lazily measured content cannot shift the thumb
    /// under the pointer — the hot drag path is O(1).
    pub geometry: ScrollbarGeometry,
    /// Pointer offset within the thumb along the drag axis, at grab time.
    pub grab: f32,
}

/// Which scrollbar: the vertical one on the right edge, or the horizontal one
/// along the bottom (shown only when soft wrap is off).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy)]
pub(crate) struct ScrollbarGeometry {
    pub axis: Axis,
    pub track: Rectangle,
    pub thumb_area: Rectangle,
    pub thumb: Rectangle,
    /// The maximum scroll offset along the axis.
    pub max_scroll: f32,
    /// The content length along the axis.
    pub content_extent: f32,
}

impl ScrollbarGeometry {
    /// The pointer coordinate along this scrollbar's axis.
    pub(crate) fn along(&self, point: Point) -> f32 {
        match self.axis {
            Axis::Vertical => point.y,
            Axis::Horizontal => point.x,
        }
    }

    /// The start of the thumb along the axis.
    pub(crate) fn thumb_start(&self) -> f32 {
        match self.axis {
            Axis::Vertical => self.thumb.y,
            Axis::Horizontal => self.thumb.x,
        }
    }
}

/// Builds the geometry of one scrollbar. `content_extent` is the document
/// length along the axis and `offset` the current scroll along it;
/// `cross_inset` leaves room at the far end for the other scrollbar.
pub(crate) fn scrollbar_geometry(
    bounds: Rectangle,
    metrics: &Metrics,
    axis: Axis,
    content_extent: f32,
    offset: f32,
    cross_inset: f32,
) -> Option<ScrollbarGeometry> {
    let viewport = match axis {
        Axis::Vertical => metrics.text_area.height,
        Axis::Horizontal => metrics.text_area.width,
    };
    let content_extent = content_extent.max(viewport);
    let max_scroll = (content_extent - viewport).max(0.0);

    let track = match axis {
        Axis::Vertical => Rectangle {
            x: bounds.x + bounds.width - SCROLLBAR_WIDTH,
            y: bounds.y,
            width: SCROLLBAR_WIDTH,
            height: (bounds.height - cross_inset).max(0.0),
        },
        Axis::Horizontal => Rectangle {
            // The horizontal scrollbar belongs to the scrollable text
            // viewport. Starting at the widget edge would cover the gutter
            // and turn clicks below line numbers into horizontal scrolls.
            x: metrics.text_area.x,
            y: bounds.y + bounds.height - SCROLLBAR_WIDTH,
            width: (bounds.x + bounds.width - cross_inset - metrics.text_area.x).max(0.0),
            height: SCROLLBAR_WIDTH,
        },
    };

    let track_len = match axis {
        Axis::Vertical => track.height,
        Axis::Horizontal => track.width,
    };

    if max_scroll <= SCROLLBAR_EPSILON || track_len <= SCROLLBAR_MIN_THUMB {
        return None;
    }

    let thumb_area = Rectangle {
        x: track.x + SCROLLBAR_PADDING,
        y: track.y + SCROLLBAR_PADDING,
        width: (track.width - SCROLLBAR_PADDING * 2.0).max(1.0),
        height: (track.height - SCROLLBAR_PADDING * 2.0).max(1.0),
    };
    let area_len = match axis {
        Axis::Vertical => thumb_area.height,
        Axis::Horizontal => thumb_area.width,
    };

    let thumb_len = (area_len * viewport / content_extent)
        .max(SCROLLBAR_MIN_THUMB)
        .min(area_len);
    let travel = (area_len - thumb_len).max(0.0);
    let start = travel * (offset / max_scroll).clamp(0.0, 1.0);

    let thumb = match axis {
        Axis::Vertical => Rectangle {
            x: thumb_area.x,
            y: thumb_area.y + start,
            width: thumb_area.width,
            height: thumb_len,
        },
        Axis::Horizontal => Rectangle {
            x: thumb_area.x + start,
            y: thumb_area.y,
            width: thumb_len,
            height: thumb_area.height,
        },
    };

    Some(ScrollbarGeometry {
        axis,
        track,
        thumb_area,
        thumb,
        max_scroll,
        content_extent,
    })
}

/// The scroll offset the given pointer maps to, when the thumb was grabbed
/// `grab` pixels from its leading edge.
pub(crate) fn scrollbar_target(geometry: &ScrollbarGeometry, pointer: Point, grab: f32) -> f32 {
    let (area_start, area_len, thumb_len) = match geometry.axis {
        Axis::Vertical => (
            geometry.thumb_area.y,
            geometry.thumb_area.height,
            geometry.thumb.height,
        ),
        Axis::Horizontal => (
            geometry.thumb_area.x,
            geometry.thumb_area.width,
            geometry.thumb.width,
        ),
    };

    let travel = (area_len - thumb_len).max(0.0);
    if travel <= SCROLLBAR_EPSILON {
        return 0.0;
    }

    let start = (geometry.along(pointer) - grab).clamp(area_start, area_start + travel);
    let progress = ((start - area_start) / travel).clamp(0.0, 1.0);

    geometry.max_scroll * progress
}

/// The vertical and horizontal scrollbars for the current content, each
/// shortened to leave the corner free when both are shown.
pub(crate) fn scrollbar_geometries(
    bounds: Rectangle,
    metrics: &Metrics,
    wrap: bool,
    content_height: f32,
    content_width: f32,
    scroll: Vector,
) -> (Option<ScrollbarGeometry>, Option<ScrollbarGeometry>) {
    let shows_vertical = content_height - metrics.text_area.height > SCROLLBAR_EPSILON;
    let shows_horizontal = !wrap && content_width - metrics.text_area.width > SCROLLBAR_EPSILON;

    let vertical = scrollbar_geometry(
        bounds,
        metrics,
        Axis::Vertical,
        content_height,
        scroll.y,
        if shows_horizontal {
            SCROLLBAR_WIDTH
        } else {
            0.0
        },
    );
    let horizontal = if wrap {
        None
    } else {
        scrollbar_geometry(
            bounds,
            metrics,
            Axis::Horizontal,
            content_width,
            scroll.x,
            if shows_vertical { SCROLLBAR_WIDTH } else { 0.0 },
        )
    };

    (vertical, horizontal)
}

/// Starts a drag on the given scrollbar, returning the drag state and the
/// scroll offset the initial press maps to.
pub(crate) fn begin_scrollbar_drag(
    geometry: ScrollbarGeometry,
    pointer: Point,
) -> (ScrollbarDrag, f32) {
    let thumb_len = match geometry.axis {
        Axis::Vertical => geometry.thumb.height,
        Axis::Horizontal => geometry.thumb.width,
    };
    let grab = if geometry.thumb.contains(pointer) {
        geometry.along(pointer) - geometry.thumb_start()
    } else {
        thumb_len / 2.0
    };

    let target = scrollbar_target(&geometry, pointer, grab);
    (ScrollbarDrag { geometry, grab }, target)
}

pub(crate) fn scrollbar_color(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// An overview mark on the scrollbar track.
pub(crate) struct ScrollbarTick {
    y: f32,
    color: Color,
}

/// The overview marks for the track: one per diagnostic (colored by severity)
/// and per search match, mapped from its line to a position on the track.
/// Deduplicated per pixel row and category so a dense document stays cheap.
pub(crate) fn scrollbar_ticks<P: text::Paragraph<Font = Font>>(
    geometry: &ScrollbarGeometry,
    buffer: &Buffer,
    cache: &LineCache<P>,
    style: &Style,
    diagnostics: &[(Range<usize>, Severity)],
    search_matches: &[Range<usize>],
    current_search_match: Option<usize>,
) -> Vec<ScrollbarTick> {
    let total_rows = cache.total_rows().max(1) as f32;
    let top = geometry.thumb_area.y;
    let span = geometry.thumb_area.height;

    let y_of = |offset: usize| {
        let line = buffer.byte_to_line(offset);
        let fraction = (cache.first_row(line) as f32 / total_rows).clamp(0.0, 1.0);
        top + fraction * span
    };

    let mut ticks = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (range, severity) in diagnostics {
        let y = y_of(range.start);
        if seen.insert((*severity as u8, y.round() as i32)) {
            ticks.push(ScrollbarTick {
                y,
                color: scrollbar_color(style.diagnostic.color(*severity), 0.85),
            });
        }
    }

    for (index, range) in search_matches.iter().enumerate() {
        let (category, color) = if Some(index) == current_search_match {
            (5u8, style.current_search_match)
        } else {
            (4u8, style.search_match)
        };
        let y = y_of(range.start);
        if seen.insert((category, y.round() as i32)) {
            ticks.push(ScrollbarTick {
                y,
                color: scrollbar_color(color, 0.9),
            });
        }
    }

    ticks
}

pub(crate) fn draw_scrollbar<Renderer: iced_core::Renderer>(
    renderer: &mut Renderer,
    geometry: &ScrollbarGeometry,
    style: &Style,
    hovered: bool,
    dragging: bool,
    ticks: &[ScrollbarTick],
) {
    renderer.fill_quad(
        Quad {
            bounds: geometry.track,
            ..Quad::default()
        },
        style.scrollbar.track,
    );

    // Overview marks sit on the track, beneath the translucent thumb, so a
    // diagnostic or match outside the viewport is visible at a glance.
    for tick in ticks {
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: geometry.track.x + 1.0,
                    y: tick.y - SCROLLBAR_TICK_HEIGHT / 2.0,
                    width: (geometry.track.width - 2.0).max(1.0),
                    height: SCROLLBAR_TICK_HEIGHT,
                },
                ..Quad::default()
            },
            tick.color,
        );
    }

    let thumb_color = if dragging {
        style.scrollbar.thumb_active
    } else if hovered {
        style.scrollbar.thumb_hovered
    } else {
        style.scrollbar.thumb
    };

    renderer.fill_quad(
        Quad {
            bounds: geometry.thumb,
            border: iced_core::Border {
                radius: (geometry.thumb.width / 2.0).into(),
                ..Default::default()
            },
            ..Quad::default()
        },
        thumb_color,
    );
}
