use std::time::Duration;

use iced::{
    Alignment, Background, Border, Element, Font, Length, Theme,
    font::Weight,
    widget::{Button, button, svg, text, tooltip},
};

use super::{
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors, StateColors},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Accent,
    Primary,
    Secondary,
    Negative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Fill,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    Small,
    Medium,
    Large,
    ExtraLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonOptions {
    pub variant: ButtonVariant,
    pub style: ButtonStyle,
    pub size: ButtonSize,
}

impl ButtonOptions {
    pub const ACCENT: Self = Self {
        variant: ButtonVariant::Accent,
        style: ButtonStyle::Fill,
        size: ButtonSize::Medium,
    };

    pub const PRIMARY: Self = Self {
        variant: ButtonVariant::Primary,
        style: ButtonStyle::Fill,
        size: ButtonSize::Medium,
    };

    pub const SECONDARY: Self = Self {
        variant: ButtonVariant::Secondary,
        style: ButtonStyle::Outline,
        size: ButtonSize::Medium,
    };

    pub const NEGATIVE: Self = Self {
        variant: ButtonVariant::Negative,
        style: ButtonStyle::Fill,
        size: ButtonSize::Medium,
    };

    pub const fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionButtonSize {
    ExtraSmall,
    Small,
    Medium,
    Large,
    ExtraLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionButtonOptions {
    pub size: ActionButtonSize,
    pub quiet: bool,
    pub selected: bool,
    pub emphasized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActionButtonPosition {
    Standalone,
    First,
    Middle,
    Last,
}

impl ActionButtonPosition {
    pub(super) const fn in_group(index: usize, len: usize) -> Self {
        match (index, len) {
            (_, 0 | 1) => Self::Standalone,
            (0, _) => Self::First,
            (index, len) if index + 1 == len => Self::Last,
            _ => Self::Middle,
        }
    }
}

impl ActionButtonOptions {
    pub const STANDARD: Self = Self {
        size: ActionButtonSize::Medium,
        quiet: false,
        selected: false,
        emphasized: false,
    };

    pub const QUIET: Self = Self {
        quiet: true,
        ..Self::STANDARD
    };

    pub const fn size(mut self, size: ActionButtonSize) -> Self {
        self.size = size;
        self
    }

    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub const fn emphasized(mut self, emphasized: bool) -> Self {
        self.emphasized = emphasized;
        self
    }
}

pub fn spectrum_button<'a, Message>(
    label: &'a str,
    on_press: Option<Message>,
    options: ButtonOptions,
) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    let metrics = button_metrics(options.size);
    let label = text(label)
        .size(metrics.font_size)
        .font(Font {
            weight: Weight::Bold,
            ..Font::DEFAULT
        })
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    button(label)
        .on_press_maybe(on_press)
        .height(Length::Fixed(metrics.height))
        .padding([0.0, metrics.horizontal_padding])
        .style(move |theme, status| spectrum_button_style(theme, status, options))
}

pub fn action_button<'a, Message>(
    label: impl text::IntoFragment<'a>,
    on_press: Option<Message>,
    options: ActionButtonOptions,
) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    let metrics = action_button_metrics(options.size);
    let label = text(label)
        .size(metrics.font_size)
        .font(Font {
            weight: Weight::Medium,
            ..Font::DEFAULT
        })
        .align_y(Alignment::Center);

    button(label)
        .on_press_maybe(on_press)
        .height(Length::Fixed(metrics.height))
        .padding([0.0, metrics.horizontal_padding])
        .style(move |theme, status| {
            action_button_style(theme, status, options, ActionButtonPosition::Standalone)
        })
}

pub fn icon_action_button<'a, Message>(
    symbol: &'a str,
    label: &'a str,
    on_press: Option<Message>,
    options: ActionButtonOptions,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    icon_action_button_at(
        symbol,
        label,
        on_press,
        options,
        ActionButtonPosition::Standalone,
    )
}

pub(super) fn grouped_icon_action_button<'a, Message>(
    symbol: &'a str,
    label: &'a str,
    on_press: Option<Message>,
    options: ActionButtonOptions,
    position: ActionButtonPosition,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    icon_action_button_at(symbol, label, on_press, options, position)
}

fn icon_action_button_at<'a, Message>(
    symbol: &'a str,
    label: &'a str,
    on_press: Option<Message>,
    options: ActionButtonOptions,
    position: ActionButtonPosition,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let metrics = action_button_metrics(options.size);
    let symbol = text(symbol)
        .size(metrics.icon_size)
        .width(Length::Fixed(metrics.icon_size))
        .height(Length::Fixed(metrics.icon_size))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);
    let control = button(symbol)
        .on_press_maybe(on_press)
        .width(Length::Fixed(metrics.height))
        .height(Length::Fixed(metrics.height))
        .padding([metrics.icon_edge_vertical, metrics.icon_edge_horizontal])
        .style(move |theme, status| action_button_style(theme, status, options, position));

    tooltip(
        control,
        text(label).size(tokens::typography::FONT_SIZE_75),
        tooltip::Position::Bottom,
    )
    .gap(tokens::spacing::BASE_GAP_SMALL)
    .padding(8)
    .delay(Duration::from_millis(500))
    .style(tooltip_style)
    .into()
}

pub fn workflow_icon_action_button<'a, Message>(
    icon: WorkflowIcon,
    label: &'a str,
    on_press: Option<Message>,
    options: ActionButtonOptions,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    workflow_icon_action_button_at(
        icon,
        label,
        on_press,
        options,
        ActionButtonPosition::Standalone,
    )
}

pub(super) fn grouped_workflow_icon_action_button<'a, Message>(
    icon: WorkflowIcon,
    label: &'a str,
    on_press: Option<Message>,
    options: ActionButtonOptions,
    position: ActionButtonPosition,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    workflow_icon_action_button_at(icon, label, on_press, options, position)
}

fn workflow_icon_action_button_at<'a, Message>(
    icon: WorkflowIcon,
    label: &'a str,
    on_press: Option<Message>,
    options: ActionButtonOptions,
    position: ActionButtonPosition,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let metrics = action_button_metrics(options.size);
    let enabled = on_press.is_some();
    let icon = svg(icon.handle())
        .width(Length::Fixed(metrics.icon_size))
        .height(Length::Fixed(metrics.icon_size))
        .style(move |theme, status| workflow_icon_style(theme, status, enabled, options));
    let control = button(icon)
        .on_press_maybe(on_press)
        .width(Length::Fixed(metrics.height))
        .height(Length::Fixed(metrics.height))
        .padding([metrics.icon_edge_vertical, metrics.icon_edge_horizontal])
        .style(move |theme, status| action_button_style(theme, status, options, position));

    tooltip(
        control,
        text(label).size(tokens::typography::FONT_SIZE_75),
        tooltip::Position::Bottom,
    )
    .gap(tokens::spacing::BASE_GAP_SMALL)
    .padding(8)
    .delay(Duration::from_millis(500))
    .style(tooltip_style)
    .into()
}

#[derive(Debug, Clone, Copy)]
struct ButtonMetrics {
    height: f32,
    font_size: f32,
    horizontal_padding: f32,
}

#[derive(Debug, Clone, Copy)]
struct ActionButtonMetrics {
    height: f32,
    font_size: f32,
    icon_size: f32,
    icon_edge_horizontal: f32,
    icon_edge_vertical: f32,
    horizontal_padding: f32,
    radius: f32,
}

const fn button_metrics(size: ButtonSize) -> ButtonMetrics {
    match size {
        ButtonSize::Small => ButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_75,
            font_size: tokens::typography::FONT_SIZE_75,
            horizontal_padding: tokens::spacing::BUTTON_HORIZONTAL_SMALL,
        },
        ButtonSize::Medium => ButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_100,
            font_size: tokens::typography::FONT_SIZE_100,
            horizontal_padding: tokens::spacing::BUTTON_HORIZONTAL_MEDIUM,
        },
        ButtonSize::Large => ButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_200,
            font_size: tokens::typography::FONT_SIZE_200,
            horizontal_padding: tokens::spacing::BUTTON_HORIZONTAL_LARGE,
        },
        ButtonSize::ExtraLarge => ButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_300,
            font_size: tokens::typography::FONT_SIZE_300,
            horizontal_padding: tokens::spacing::BUTTON_HORIZONTAL_EXTRA_LARGE,
        },
    }
}

const fn action_button_metrics(size: ActionButtonSize) -> ActionButtonMetrics {
    match size {
        ActionButtonSize::ExtraSmall => ActionButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_50,
            font_size: tokens::typography::FONT_SIZE_50,
            icon_size: tokens::icon::WORKFLOW_SIZE_50,
            icon_edge_horizontal: tokens::spacing::COMPONENT_EDGE_TO_VISUAL_ONLY_50,
            icon_edge_vertical: tokens::spacing::COMPONENT_TOP_TO_WORKFLOW_ICON_50,
            horizontal_padding: 8.0,
            radius: tokens::dimension::CORNER_RADIUS_300,
        },
        ActionButtonSize::Small => ActionButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_75,
            font_size: tokens::typography::FONT_SIZE_75,
            icon_size: tokens::icon::WORKFLOW_SIZE_75,
            icon_edge_horizontal: tokens::spacing::COMPONENT_EDGE_TO_VISUAL_ONLY_75,
            icon_edge_vertical: tokens::spacing::COMPONENT_TOP_TO_WORKFLOW_ICON_75,
            horizontal_padding: tokens::spacing::BASE_PADDING_HORIZONTAL_SMALL,
            radius: tokens::dimension::CORNER_RADIUS_400,
        },
        ActionButtonSize::Medium => ActionButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_100,
            font_size: tokens::typography::FONT_SIZE_100,
            icon_size: tokens::icon::WORKFLOW_SIZE_100,
            icon_edge_horizontal: tokens::spacing::COMPONENT_EDGE_TO_VISUAL_ONLY_100,
            icon_edge_vertical: tokens::spacing::COMPONENT_TOP_TO_WORKFLOW_ICON_100,
            horizontal_padding: tokens::spacing::BASE_PADDING_HORIZONTAL_MEDIUM,
            radius: tokens::dimension::CORNER_RADIUS_500,
        },
        ActionButtonSize::Large => ActionButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_200,
            font_size: tokens::typography::FONT_SIZE_200,
            icon_size: tokens::icon::WORKFLOW_SIZE_200,
            icon_edge_horizontal: tokens::spacing::COMPONENT_EDGE_TO_VISUAL_ONLY_200,
            icon_edge_vertical: tokens::spacing::COMPONENT_TOP_TO_WORKFLOW_ICON_200,
            horizontal_padding: tokens::spacing::BASE_PADDING_HORIZONTAL_LARGE,
            radius: tokens::dimension::CORNER_RADIUS_600,
        },
        ActionButtonSize::ExtraLarge => ActionButtonMetrics {
            height: tokens::dimension::COMPONENT_HEIGHT_300,
            font_size: tokens::typography::FONT_SIZE_300,
            icon_size: tokens::icon::WORKFLOW_SIZE_300,
            icon_edge_horizontal: tokens::spacing::COMPONENT_EDGE_TO_VISUAL_ONLY_300,
            icon_edge_vertical: tokens::spacing::COMPONENT_TOP_TO_WORKFLOW_ICON_300,
            horizontal_padding: tokens::spacing::BASE_PADDING_HORIZONTAL_EXTRA_LARGE,
            radius: tokens::dimension::CORNER_RADIUS_700,
        },
    }
}

fn spectrum_button_style(
    theme: &Theme,
    status: button::Status,
    options: ButtonOptions,
) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let metrics = button_metrics(options.size);
    let radius = metrics.height * tokens::dimension::CORNER_RADIUS_FULL_MULTIPLIER;

    if status == button::Status::Disabled {
        return button::Style {
            background: (options.style == ButtonStyle::Fill)
                .then_some(Background::Color(colors.disabled_background)),
            text_color: colors.disabled_content,
            border: Border {
                color: colors.disabled_border,
                width: if options.style == ButtonStyle::Outline {
                    tokens::dimension::BORDER_WIDTH_200
                } else {
                    0.0
                },
                radius: radius.into(),
            },
            ..button::Style::default()
        };
    }

    let state = match options.variant {
        ButtonVariant::Accent => colors.accent_background,
        ButtonVariant::Negative => colors.negative_background,
        ButtonVariant::Primary => colors.neutral_background,
        ButtonVariant::Secondary => StateColors::secondary(&colors),
    };
    let state_color = state.for_status(status);
    let content_color = match options.variant {
        ButtonVariant::Accent | ButtonVariant::Negative => iced::Color::WHITE,
        ButtonVariant::Primary if options.style == ButtonStyle::Fill => colors.gray.gray_25,
        ButtonVariant::Primary | ButtonVariant::Secondary => {
            colors.neutral_content.for_status(status)
        }
    };

    match options.style {
        ButtonStyle::Fill => button::Style {
            background: Some(Background::Color(state_color)),
            text_color: content_color,
            border: Border {
                radius: radius.into(),
                ..Border::default()
            },
            ..button::Style::default()
        },
        ButtonStyle::Outline => button::Style {
            background: match status {
                button::Status::Active => None,
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(colors.gray.gray_100))
                }
                button::Status::Disabled => None,
            },
            text_color: content_color,
            border: Border {
                color: state_color,
                width: tokens::dimension::BORDER_WIDTH_200,
                radius: radius.into(),
            },
            ..button::Style::default()
        },
    }
}

fn action_button_style(
    theme: &Theme,
    status: button::Status,
    options: ActionButtonOptions,
    position: ActionButtonPosition,
) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let metrics = action_button_metrics(options.size);
    let border = Border {
        radius: action_button_radius(metrics.radius, position),
        ..Border::default()
    };

    if status == button::Status::Disabled {
        return button::Style {
            background: (!options.quiet).then_some(Background::Color(colors.disabled_background)),
            text_color: colors.disabled_content,
            border,
            ..button::Style::default()
        };
    }

    if options.selected {
        let background = if options.emphasized {
            colors.accent_background.for_status(status)
        } else {
            colors.neutral_background.for_status(status)
        };

        return button::Style {
            background: Some(Background::Color(background)),
            text_color: if options.emphasized {
                iced::Color::WHITE
            } else {
                colors.gray.gray_25
            },
            border,
            ..button::Style::default()
        };
    }

    let background = match (options.quiet, status) {
        (true, button::Status::Active) => None,
        (true, button::Status::Hovered) => Some(colors.gray.gray_100),
        (true, button::Status::Pressed) => Some(colors.gray.gray_200),
        (false, button::Status::Active) => Some(colors.gray.gray_100),
        (false, button::Status::Hovered | button::Status::Pressed) => Some(colors.gray.gray_200),
        (_, button::Status::Disabled) => None,
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: colors.neutral_content.for_status(status),
        border,
        ..button::Style::default()
    }
}

fn action_button_radius(radius: f32, position: ActionButtonPosition) -> iced::border::Radius {
    let group_radius = tokens::dimension::ACTION_GROUP_COMPACT_RADIUS;

    match position {
        ActionButtonPosition::Standalone => iced::border::Radius::new(radius),
        ActionButtonPosition::First => iced::border::Radius {
            top_left: group_radius,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: group_radius,
        },
        ActionButtonPosition::Middle => iced::border::Radius::new(0.0),
        ActionButtonPosition::Last => iced::border::Radius {
            top_left: 0.0,
            top_right: group_radius,
            bottom_right: group_radius,
            bottom_left: 0.0,
        },
    }
}

fn workflow_icon_style(
    theme: &Theme,
    status: svg::Status,
    enabled: bool,
    options: ActionButtonOptions,
) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = if !enabled {
        colors.disabled_content
    } else if options.selected {
        if options.emphasized {
            iced::Color::WHITE
        } else {
            colors.gray.gray_25
        }
    } else {
        match status {
            svg::Status::Idle => colors.neutral_content.default,
            svg::Status::Hovered => colors.neutral_content.hover,
        }
    };

    svg::Style { color: Some(color) }
}

fn tooltip_style(theme: &Theme) -> iced::widget::container::Style {
    let colors = SpectrumColors::from_theme(theme);

    iced::widget::container::Style::default()
        .background(colors.gray.gray_900)
        .color(colors.gray.gray_25)
        .border(Border::default().rounded(tokens::dimension::CORNER_RADIUS_300))
}

trait StateColorExt {
    fn for_status(self, status: button::Status) -> iced::Color;
}

impl StateColorExt for tokens::StateColors {
    fn for_status(self, status: button::Status) -> iced::Color {
        match status {
            button::Status::Active => self.default,
            button::Status::Hovered => self.hover,
            button::Status::Pressed => self.down,
            button::Status::Disabled => self.default,
        }
    }
}

impl tokens::StateColors {
    fn secondary(colors: &SpectrumColors) -> Self {
        Self {
            default: colors.gray.gray_300,
            hover: colors.gray.gray_400,
            down: colors.gray.gray_400,
            key_focus: colors.gray.gray_400,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_button_uses_spectrum_dimensions() {
        let metrics = button_metrics(ButtonSize::Medium);

        assert_eq!(metrics.height, 32.0);
        assert_eq!(metrics.font_size, 14.0);
        assert_eq!(metrics.horizontal_padding, 16.0);
    }

    #[test]
    fn medium_action_button_uses_spectrum_dimensions() {
        let metrics = action_button_metrics(ActionButtonSize::Medium);

        assert_eq!(metrics.height, 32.0);
        assert_eq!(metrics.icon_size, 20.0);
        assert_eq!(metrics.radius, 8.0);
    }

    #[test]
    fn small_action_button_uses_spectrum_dimensions() {
        let metrics = action_button_metrics(ActionButtonSize::Small);

        assert_eq!(metrics.height, 24.0);
        assert_eq!(metrics.icon_size, 16.0);
        assert_eq!(metrics.icon_edge_horizontal, 4.0);
        assert_eq!(metrics.icon_edge_vertical, 4.0);
        assert_eq!(
            metrics.icon_size + 2.0 * metrics.icon_edge_horizontal,
            metrics.height
        );
        assert_eq!(
            metrics.icon_size + 2.0 * metrics.icon_edge_vertical,
            metrics.height
        );
        assert_eq!(metrics.radius, 7.0);
    }

    #[test]
    fn icon_only_padding_fills_every_action_button_size() {
        for size in [
            ActionButtonSize::ExtraSmall,
            ActionButtonSize::Small,
            ActionButtonSize::Medium,
            ActionButtonSize::Large,
            ActionButtonSize::ExtraLarge,
        ] {
            let metrics = action_button_metrics(size);

            assert_eq!(
                metrics.icon_size + 2.0 * metrics.icon_edge_horizontal,
                metrics.height
            );
            assert_eq!(
                metrics.icon_size + 2.0 * metrics.icon_edge_vertical,
                metrics.height
            );
        }
    }

    #[test]
    fn compact_positions_only_round_the_outer_edges() {
        let radius = 7.0;
        let group_radius = tokens::dimension::ACTION_GROUP_COMPACT_RADIUS;

        assert_eq!(
            action_button_radius(radius, ActionButtonPosition::First),
            iced::border::Radius {
                top_left: group_radius,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: group_radius,
            }
        );
        assert_eq!(
            action_button_radius(radius, ActionButtonPosition::Middle),
            iced::border::Radius::new(0.0)
        );
        assert_eq!(
            action_button_radius(radius, ActionButtonPosition::Last),
            iced::border::Radius {
                top_left: 0.0,
                top_right: group_radius,
                bottom_right: group_radius,
                bottom_left: 0.0,
            }
        );
    }
}
