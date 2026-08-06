//! Typst syntax highlighting.
//!
//! Highlighting comes straight from the syntax tree that the [`Buffer`]
//! already maintains: [`typst_syntax::highlight`] assigns a semantic [`Tag`]
//! to each node, and a [`SyntaxTheme`] maps tags to colors and font
//! variants.
//!
//! [`Buffer`]: crate::Buffer

use std::ops::Range;

use iced_core::font;
use iced_core::Color;
use typst_syntax::{LinkedNode, SyntaxNode};

pub use typst_syntax::Tag;

/// How text with a given [`Tag`] should look.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SyntaxStyle {
    /// The text color. `None` keeps the editor's default text color.
    pub color: Option<Color>,
    /// The font weight. `None` keeps the editor font's weight.
    pub weight: Option<font::Weight>,
    /// Whether the text is rendered in italics.
    pub italic: bool,
}

impl SyntaxStyle {
    /// A style that only changes the text color.
    pub fn color(color: Color) -> Self {
        Self {
            color: Some(color),
            ..Self::default()
        }
    }

    /// Makes the style bold.
    pub fn bold(mut self) -> Self {
        self.weight = Some(font::Weight::Bold);
        self
    }

    /// Makes the style italic.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}

/// A mapping from syntax [`Tag`]s to [`SyntaxStyle`]s.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxTheme {
    styles: [SyntaxStyle; Tag::LIST.len()],
}

/// The accent colors shared by the built-in One Dark / One Light themes.
struct Palette {
    cyan: Color,
    blue: Color,
    green: Color,
    orange: Color,
    purple: Color,
    red: Color,
    gray: Color,
}

impl SyntaxTheme {
    /// Creates a theme that leaves every tag unstyled.
    pub fn plain() -> Self {
        Self {
            styles: [SyntaxStyle::default(); Tag::LIST.len()],
        }
    }

    /// The default theme for dark backgrounds, based on the One Dark
    /// palette.
    pub fn dark() -> Self {
        Self::one_palette(Palette {
            cyan: Color::from_rgb8(0x56, 0xb6, 0xc2),
            blue: Color::from_rgb8(0x61, 0xaf, 0xef),
            green: Color::from_rgb8(0x98, 0xc3, 0x79),
            orange: Color::from_rgb8(0xd1, 0x9a, 0x66),
            purple: Color::from_rgb8(0xc6, 0x78, 0xdd),
            red: Color::from_rgb8(0xe0, 0x6c, 0x75),
            gray: Color::from_rgb8(0x5c, 0x63, 0x70),
        })
    }

    /// The default theme for light backgrounds, based on the One Light
    /// palette.
    pub fn light() -> Self {
        Self::one_palette(Palette {
            cyan: Color::from_rgb8(0x01, 0x84, 0xbc),
            blue: Color::from_rgb8(0x40, 0x78, 0xf2),
            green: Color::from_rgb8(0x50, 0xa1, 0x4f),
            orange: Color::from_rgb8(0x98, 0x68, 0x01),
            purple: Color::from_rgb8(0xa6, 0x26, 0xa4),
            red: Color::from_rgb8(0xe4, 0x56, 0x49),
            gray: Color::from_rgb8(0xa0, 0xa1, 0xa7),
        })
    }

    /// Maps tags to accents the same way for One Dark and One Light, so the
    /// two themes only differ in their [`Palette`].
    fn one_palette(palette: Palette) -> Self {
        let Palette {
            cyan,
            blue,
            green,
            orange,
            purple,
            red,
            gray,
        } = palette;

        Self::plain()
            .with(Tag::Comment, SyntaxStyle::color(gray).italic())
            .with(Tag::Escape, SyntaxStyle::color(cyan))
            .with(Tag::Strong, SyntaxStyle::default().bold())
            .with(Tag::Emph, SyntaxStyle::default().italic())
            .with(Tag::Link, SyntaxStyle::color(blue))
            .with(Tag::Raw, SyntaxStyle::color(green))
            .with(Tag::Label, SyntaxStyle::color(blue))
            .with(Tag::Ref, SyntaxStyle::color(blue))
            .with(Tag::Heading, SyntaxStyle::color(blue).bold())
            .with(Tag::ListMarker, SyntaxStyle::color(orange))
            .with(Tag::ListTerm, SyntaxStyle::default().bold())
            .with(Tag::MathDelimiter, SyntaxStyle::color(cyan))
            .with(Tag::MathOperator, SyntaxStyle::color(cyan))
            .with(Tag::Keyword, SyntaxStyle::color(purple))
            .with(Tag::Operator, SyntaxStyle::color(cyan))
            .with(Tag::Number, SyntaxStyle::color(orange))
            .with(Tag::String, SyntaxStyle::color(green))
            .with(Tag::Function, SyntaxStyle::color(blue))
            .with(Tag::Interpolated, SyntaxStyle::color(red))
            .with(Tag::Error, SyntaxStyle::color(red))
    }

    /// Sets the style of a tag.
    pub fn with(mut self, tag: Tag, style: SyntaxStyle) -> Self {
        self.styles[tag as usize] = style;
        self
    }

    /// Returns the style of a tag.
    pub fn style(&self, tag: Tag) -> SyntaxStyle {
        self.styles[tag as usize]
    }
}

/// Collects the highlighted runs of one line.
///
/// Walks the syntax tree pruned to `line_range` and emits, in order, one run
/// per leaf that carries a tag (its own or inherited from an ancestor). The
/// ranges are clipped to the line and do not overlap.
pub(crate) fn line_highlights(
    root: &SyntaxNode,
    line_range: Range<usize>,
    runs: &mut Vec<(Range<usize>, Tag)>,
) {
    runs.clear();
    walk(&LinkedNode::new(root), None, &line_range, runs);
}

fn walk(
    node: &LinkedNode<'_>,
    inherited: Option<Tag>,
    line_range: &Range<usize>,
    runs: &mut Vec<(Range<usize>, Tag)>,
) {
    let range = node.range();

    if range.start >= line_range.end || range.end <= line_range.start {
        return;
    }

    let tag = typst_syntax::highlight(node).or(inherited);

    if node.children().len() == 0 {
        if let Some(tag) = tag {
            let start = range.start.max(line_range.start);
            let end = range.end.min(line_range.end);

            if start < end {
                runs.push((start..end, tag));
            }
        }

        return;
    }

    // Children are ordered by position, so the scan can stop at the first
    // child past the line — and enter from whichever end of the child list
    // is closer. This matters at the root, which has one child per paragraph:
    // shaping a line must not pay for every paragraph before (or after) it.
    if line_range.start <= range.start + (range.end - range.start) / 2 {
        for child in node.children() {
            if child.offset() >= line_range.end {
                break;
            }

            walk(&child, tag, line_range, runs);
        }
    } else {
        let mut intersecting = Vec::new();

        for child in node.children().rev() {
            if child.range().end <= line_range.start {
                break;
            }

            if child.offset() < line_range.end {
                intersecting.push(child);
            }
        }

        for child in intersecting.iter().rev() {
            walk(child, tag, line_range, runs);
        }
    }
}
