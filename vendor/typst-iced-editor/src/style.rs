//! Styling of the editor widget.

use iced_core::theme;
use iced_core::{Background, Border, Color, Theme};

use crate::diagnostic::Severity;
use crate::highlight::{SyntaxTheme, Tag};

/// The possible status of a [`CodeEditor`](crate::CodeEditor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The editor can be interacted with.
    Active,
    /// The editor is being hovered.
    Hovered,
    /// The editor is focused.
    Focused {
        /// Whether the editor is hovered, while focused.
        is_hovered: bool,
    },
    /// The editor cannot be interacted with.
    Disabled,
}

/// The appearance of a [`CodeEditor`](crate::CodeEditor).
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// The [`Background`] of the editor.
    pub background: Background,
    /// The [`Border`] of the editor.
    pub border: Border,
    /// The default text color, used where the syntax theme does not apply.
    pub text: Color,
    /// The background color of selected text.
    pub selection: Color,
    /// The background color of search matches.
    pub search_match: Color,
    /// The background color of the currently focused search match.
    pub current_search_match: Color,
    /// The background color used to mark matching delimiters.
    pub delimiter_match: Color,
    /// The color of the caret.
    pub cursor: Color,
    /// The background color of the line the caret is on, if any.
    pub current_line: Option<Color>,
    /// The text color of the line numbers in the gutter.
    pub gutter_text: Color,
    /// The text color of the current line's number in the gutter.
    pub gutter_current_text: Color,
    /// The color of the vertical guide showing a foldable block's extent.
    pub fold_guide: Color,
    /// The color of that guide for the block containing the caret.
    pub fold_guide_current: Color,
    /// The colors and font variants for Typst syntax highlighting.
    pub syntax: SyntaxTheme,
    /// The colors of the diagnostic squiggles, by severity.
    pub diagnostic: DiagnosticStyle,
    /// The appearance of the completion popup and hover tooltip.
    pub popup: PopupStyle,
    /// The appearance of the editor-owned scrollbar.
    pub scrollbar: ScrollbarStyle,
}

/// The appearance of the editor's scrollbar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarStyle {
    /// The color of the track behind the thumb.
    pub track: Color,
    /// The color of the thumb at rest.
    pub thumb: Color,
    /// The color of the thumb while the pointer is over the track.
    pub thumb_hovered: Color,
    /// The color of the thumb while it is being dragged.
    pub thumb_active: Color,
}

/// The squiggle colors of each diagnostic [`Severity`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiagnosticStyle {
    /// The color of error squiggles.
    pub error: Color,
    /// The color of warning squiggles.
    pub warning: Color,
    /// The color of info squiggles.
    pub info: Color,
    /// The color of hint squiggles.
    pub hint: Color,
}

impl DiagnosticStyle {
    /// The squiggle color for the given [`Severity`].
    pub fn color(&self, severity: Severity) -> Color {
        match severity {
            Severity::Error => self.error,
            Severity::Warning => self.warning,
            Severity::Info => self.info,
            Severity::Hint => self.hint,
        }
    }
}

/// The appearance of the completion popup and hover tooltip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupStyle {
    /// The background of the popup.
    pub background: Background,
    /// The border of the popup.
    pub border: Border,
    /// The text color.
    pub text: Color,
    /// A dimmed text color, for completion details.
    pub dim_text: Color,
    /// The background of the selected completion.
    pub selection: Color,
}

/// The theme catalog of a [`CodeEditor`](crate::CodeEditor).
pub trait Catalog: theme::Base {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

/// A styling function for a [`CodeEditor`](crate::CodeEditor).
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

/// The default style of a [`CodeEditor`](crate::CodeEditor).
///
/// It follows the palette of the application [`Theme`] and picks the dark or
/// light [`SyntaxTheme`] accordingly.
pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();
    let syntax = if palette.is_dark {
        SyntaxTheme::dark()
    } else {
        SyntaxTheme::light()
    };
    let comment = syntax
        .style(Tag::Comment)
        .color
        .unwrap_or(palette.background.strong.color);

    let active = Style {
        background: Background::Color(palette.background.base.color),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        text: palette.background.base.text,
        selection: palette.primary.weak.color,
        search_match: Color {
            a: 0.28,
            ..palette.primary.weak.color
        },
        current_search_match: Color {
            a: 0.55,
            ..palette.primary.strong.color
        },
        delimiter_match: Color {
            a: 0.35,
            ..palette.secondary.weak.color
        },
        cursor: palette.background.base.text,
        current_line: Some(Color {
            a: 0.5,
            ..palette.background.weak.color
        }),
        gutter_text: palette.background.strong.color,
        gutter_current_text: palette.background.base.text,
        fold_guide: Color {
            a: comment.a * 0.4,
            ..comment
        },
        fold_guide_current: Color {
            a: comment.a * 0.7,
            ..comment
        },
        syntax,
        diagnostic: DiagnosticStyle {
            error: palette.danger.base.color,
            warning: Color::from_rgb8(0xd1, 0x9a, 0x66),
            info: palette.primary.base.color,
            hint: palette.secondary.base.color,
        },
        popup: PopupStyle {
            background: Background::Color(palette.background.weak.color),
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            text: palette.background.base.text,
            dim_text: palette.secondary.base.color,
            selection: palette.primary.weak.color,
        },
        scrollbar: ScrollbarStyle {
            track: Color {
                a: 0.08,
                ..palette.background.strong.color
            },
            thumb: Color {
                a: 0.32,
                ..palette.background.base.text
            },
            thumb_hovered: Color {
                a: 0.48,
                ..palette.background.base.text
            },
            thumb_active: Color {
                a: 0.68,
                ..palette.background.base.text
            },
        },
    };

    match status {
        Status::Active | Status::Hovered => active,
        Status::Focused { .. } => Style {
            border: Border {
                color: palette.primary.strong.color,
                ..active.border
            },
            ..active
        },
        Status::Disabled => Style {
            background: Background::Color(palette.background.weak.color),
            text: palette.background.strong.color,
            ..active
        },
    }
}
