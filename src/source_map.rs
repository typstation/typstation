use std::{cmp::Ordering, ops::Range, path::PathBuf};

use typst::{
    layout::{Frame, FrameItem, Point, Rect, Transform},
    syntax::Span,
};
use typst_layout::Page;
use typstation::world::TypstationWorld;

const MIN_REGION_SIZE: f32 = 1.5;

/// Identifies the source file that produced a visual preview region.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SourceTarget {
    Main,
    ProjectFile(PathBuf),
}

/// Axis-aligned bounds in Typst points, relative to the page's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SourceBounds {
    pub fn contains(self, x: f32, y: f32, tolerance: f32) -> bool {
        x >= self.x - tolerance
            && x <= self.x + self.width + tolerance
            && y >= self.y - tolerance
            && y <= self.y + self.height + tolerance
    }

    pub fn distance_squared(self, x: f32, y: f32) -> f32 {
        let dx = if x < self.x {
            self.x - x
        } else if x > self.x + self.width {
            x - (self.x + self.width)
        } else {
            0.0
        };
        let dy = if y < self.y {
            self.y - y
        } else if y > self.y + self.height {
            y - (self.y + self.height)
        } else {
            0.0
        };

        dx * dx + dy * dy
    }

    pub fn area(self) -> f32 {
        self.width * self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRegionKind {
    Text,
    Image,
    Shape,
}

impl SourceRegionKind {
    pub fn hit_priority(self) -> u8 {
        match self {
            Self::Text => 0,
            Self::Image => 1,
            Self::Shape => 2,
        }
    }
}

/// Connects a source byte range to the visual area it produced on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRegion {
    pub target: SourceTarget,
    pub range: Range<usize>,
    pub bounds: SourceBounds,
    pub kind: SourceRegionKind,
}

/// Builds the source map for one compiled page.
pub fn page_regions(world: &TypstationWorld, page: &Page) -> Vec<SourceRegion> {
    let mut regions = Vec::new();
    let page_width = page.frame.width().to_pt() as f32;
    let page_height = page.frame.height().to_pt() as f32;

    collect_frame(
        world,
        &page.frame,
        Transform::identity(),
        page_width,
        page_height,
        &mut regions,
    );

    regions
}

/// Resolves a Typst span to the source file and byte range that produced it.
pub fn span_source_range(
    world: &TypstationWorld,
    span: Span,
) -> Option<(SourceTarget, Range<usize>)> {
    source_location(world, span, 0, usize::MAX)
}

fn collect_frame(
    world: &TypstationWorld,
    frame: &Frame,
    parent_transform: Transform,
    page_width: f32,
    page_height: f32,
    regions: &mut Vec<SourceRegion>,
) {
    for (position, item) in frame.items() {
        let transform = parent_transform.pre_concat(Transform::translate(position.x, position.y));

        match item {
            FrameItem::Group(group) => collect_frame(
                world,
                &group.frame,
                transform.pre_concat(group.transform),
                page_width,
                page_height,
                regions,
            ),
            FrameItem::Text(text) => {
                let mut cursor = Point::zero();

                for glyph in &text.glyphs {
                    let advance =
                        Point::new(glyph.x_advance.at(text.size), glyph.y_advance.at(text.size));
                    let offset =
                        Point::new(glyph.x_offset.at(text.size), glyph.y_offset.at(text.size));
                    let visual_x = cursor.x + offset.x;
                    let visual_y = -(cursor.y + offset.y);
                    let mut min_x = cursor.x.min(cursor.x + advance.x);
                    let mut max_x = cursor.x.max(cursor.x + advance.x);

                    if (max_x - min_x).to_pt().abs() < f64::from(MIN_REGION_SIZE) {
                        let half_width = text.size * 0.25;
                        min_x = visual_x - half_width;
                        max_x = visual_x + half_width;
                    }

                    let local = Rect::new(
                        Point::new(min_x, visual_y - text.size * 0.85),
                        Point::new(max_x, visual_y + text.size * 0.25),
                    );

                    if let Some((target, range)) = source_location(
                        world,
                        glyph.span.0,
                        usize::from(glyph.span.1),
                        glyph.range().len(),
                    ) && let Some(bounds) =
                        transformed_bounds(local, transform, page_width, page_height)
                    {
                        regions.push(SourceRegion {
                            target,
                            range,
                            bounds,
                            kind: SourceRegionKind::Text,
                        });
                    }

                    cursor += advance;
                }
            }
            FrameItem::Shape(shape, span) => push_region(
                world,
                *span,
                shape.bbox(true),
                transform,
                page_width,
                page_height,
                SourceRegionKind::Shape,
                regions,
            ),
            FrameItem::Image(_, size, span) => push_region(
                world,
                *span,
                Rect::from_pos_size(Point::zero(), *size),
                transform,
                page_width,
                page_height,
                SourceRegionKind::Image,
                regions,
            ),
            FrameItem::Link(_, _) | FrameItem::Tag(_) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_region(
    world: &TypstationWorld,
    span: Span,
    local: Rect,
    transform: Transform,
    page_width: f32,
    page_height: f32,
    kind: SourceRegionKind,
    regions: &mut Vec<SourceRegion>,
) {
    let Some((target, range)) = source_location(world, span, 0, usize::MAX) else {
        return;
    };
    let Some(bounds) = transformed_bounds(local, transform, page_width, page_height) else {
        return;
    };

    regions.push(SourceRegion {
        target,
        range,
        bounds,
        kind,
    });
}

fn source_location(
    world: &TypstationWorld,
    span: Span,
    offset: usize,
    rendered_len: usize,
) -> Option<(SourceTarget, Range<usize>)> {
    let (id, base) = world.span_range(span.into())?;
    let target = if world.is_main(id) {
        SourceTarget::Main
    } else {
        SourceTarget::ProjectFile(world.project_path(id)?)
    };

    if offset >= base.len() || rendered_len == usize::MAX {
        return Some((target, base));
    }

    let start = base.start + offset;
    let end = start.saturating_add(rendered_len.max(1)).min(base.end);
    let range = if start < end { start..end } else { base };

    Some((target, range))
}

fn transformed_bounds(
    rect: Rect,
    transform: Transform,
    page_width: f32,
    page_height: f32,
) -> Option<SourceBounds> {
    let corners = [
        rect.min,
        Point::new(rect.max.x, rect.min.y),
        rect.max,
        Point::new(rect.min.x, rect.max.y),
    ]
    .map(|point| point.transform(transform));

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for point in corners {
        let x = point.x.to_pt() as f32;
        let y = point.y.to_pt() as f32;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    if ![min_x, min_y, max_x, max_y]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }

    min_x = min_x.clamp(0.0, page_width);
    min_y = min_y.clamp(0.0, page_height);
    max_x = max_x.clamp(0.0, page_width);
    max_y = max_y.clamp(0.0, page_height);

    let width = max_x - min_x;
    let height = max_y - min_y;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some(SourceBounds {
        x: min_x,
        y: min_y,
        width: width.max(MIN_REGION_SIZE).min(page_width - min_x),
        height: height.max(MIN_REGION_SIZE).min(page_height - min_y),
    })
}

pub fn source_distance(range: &Range<usize>, offset: usize) -> usize {
    if offset < range.start {
        range.start - offset
    } else {
        offset.saturating_sub(range.end)
    }
}

pub fn compare_source_candidates(
    left: &SourceRegion,
    right: &SourceRegion,
    offset: usize,
) -> Ordering {
    let left_contains = left.range.start <= offset && offset < left.range.end;
    let right_contains = right.range.start <= offset && offset < right.range.end;

    right_contains
        .cmp(&left_contains)
        .then_with(|| {
            source_distance(&left.range, offset).cmp(&source_distance(&right.range, offset))
        })
        .then_with(|| left.kind.hit_priority().cmp(&right.kind.hit_priority()))
        .then_with(|| left.range.len().cmp(&right.range.len()))
        .then_with(|| {
            left.bounds
                .area()
                .partial_cmp(&right.bounds.area())
                .unwrap_or(Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_measure_hit_and_distance() {
        let bounds = SourceBounds {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 10.0,
        };

        assert!(bounds.contains(15.0, 25.0, 0.0));
        assert!(bounds.contains(8.0, 25.0, 2.0));
        assert!(!bounds.contains(7.0, 25.0, 2.0));
        assert_eq!(bounds.distance_squared(45.0, 25.0), 25.0);
    }

    #[test]
    fn source_distance_is_zero_inside_a_range() {
        assert_eq!(source_distance(&(10..20), 15), 0);
        assert_eq!(source_distance(&(10..20), 5), 5);
        assert_eq!(source_distance(&(10..20), 25), 5);
    }
}
