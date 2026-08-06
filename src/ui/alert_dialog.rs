//! Alert Dialog modal baseado na anatomia e nas variantes do Adobe Spectrum.

use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Theme,
    font::Weight,
    widget::{Column, Row, Space, container, mouse_area, svg, text},
};

use super::{
    ButtonOptions, WorkflowIcon, spectrum_button,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertDialogVariant {
    Confirmation,
    Warning,
    Destructive,
    Error,
}

#[derive(Debug, Clone)]
pub struct AlertDialogAction<Message> {
    pub label: &'static str,
    pub on_press: Option<Message>,
    pub options: ButtonOptions,
}

impl<Message> AlertDialogAction<Message> {
    pub const fn new(
        label: &'static str,
        on_press: Option<Message>,
        options: ButtonOptions,
    ) -> Self {
        Self {
            label,
            on_press,
            options,
        }
    }
}

pub struct AlertDialog<Message> {
    variant: AlertDialogVariant,
    title: String,
    description: String,
    actions: Vec<AlertDialogAction<Message>>,
    blocked_message: Message,
}

impl<Message> AlertDialog<Message> {
    pub fn new(
        variant: AlertDialogVariant,
        title: impl Into<String>,
        description: impl Into<String>,
        actions: Vec<AlertDialogAction<Message>>,
        blocked_message: Message,
    ) -> Self {
        assert!(
            actions.len() <= 3,
            "an Alert Dialog must not contain more than three actions"
        );

        Self {
            variant,
            title: title.into(),
            description: description.into(),
            actions,
            blocked_message,
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.actions.len()
    }

    fn build<'a>(self) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        let mut heading = Row::new()
            .align_y(Alignment::Center)
            .spacing(tokens::spacing::ALERT_DIALOG_ICON_TO_TITLE);
        if let Some(icon) = variant_icon(self.variant) {
            heading = heading.push(
                svg(icon.handle())
                    .width(Length::Fixed(tokens::icon::ALERT_DIALOG_ICON_SIZE))
                    .height(Length::Fixed(tokens::icon::ALERT_DIALOG_ICON_SIZE))
                    .style(move |theme, status| alert_icon_style(theme, status, self.variant)),
            );
        }
        heading = heading.push(
            text(self.title)
                .size(tokens::typography::ALERT_DIALOG_TITLE)
                .line_height(tokens::typography::LINE_HEIGHT_100)
                .wrapping(text::Wrapping::WordOrGlyph)
                .font(Font {
                    weight: Weight::Bold,
                    ..Font::DEFAULT
                })
                .width(Length::Fill),
        );

        let mut actions = Row::new()
            .align_y(Alignment::Center)
            .spacing(tokens::spacing::ALERT_DIALOG_BUTTON_GAP)
            .push(Space::new().width(Length::Fill));
        for action in self.actions {
            actions = actions.push(spectrum_button(
                action.label,
                action.on_press,
                action.options,
            ));
        }

        let divider = container(Space::new())
            .width(Length::Fill)
            .height(Length::Fixed(
                tokens::dimension::ALERT_DIALOG_DIVIDER_HEIGHT,
            ))
            .style(divider_style);
        let content = Column::new()
            .width(Length::Fill)
            .push(heading)
            .push(Space::new().height(Length::Fixed(
                tokens::spacing::ALERT_DIALOG_TITLE_TO_DIVIDER,
            )))
            .push(divider)
            .push(Space::new().height(Length::Fixed(
                tokens::spacing::ALERT_DIALOG_DIVIDER_TO_DESCRIPTION,
            )))
            .push(
                text(self.description)
                    .size(tokens::typography::ALERT_DIALOG_DESCRIPTION)
                    .line_height(tokens::typography::LINE_HEIGHT_100)
                    .wrapping(text::Wrapping::WordOrGlyph),
            )
            .push(Space::new().height(Length::Fixed(
                tokens::spacing::ALERT_DIALOG_DESCRIPTION_TO_BUTTONS,
            )))
            .push(actions);
        let dialog = container(content)
            .width(Length::Fill)
            .max_width(tokens::dimension::ALERT_DIALOG_MAXIMUM_WIDTH)
            .padding(tokens::spacing::ALERT_DIALOG_PADDING)
            .style(dialog_style);
        let overlay = container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(tokens::spacing::BASE_PADDING_HORIZONTAL_EXTRA_LARGE)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(backdrop_style);

        mouse_area(overlay).on_press(self.blocked_message).into()
    }
}

impl<'a, Message> From<AlertDialog<Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(dialog: AlertDialog<Message>) -> Self {
        dialog.build()
    }
}

const fn variant_icon(variant: AlertDialogVariant) -> Option<WorkflowIcon> {
    match variant {
        AlertDialogVariant::Confirmation => None,
        AlertDialogVariant::Warning => Some(WorkflowIcon::Alert),
        AlertDialogVariant::Destructive | AlertDialogVariant::Error => {
            Some(WorkflowIcon::AlertCircleFilled)
        }
    }
}

fn alert_icon_style(
    theme: &Theme,
    _status: svg::Status,
    variant: AlertDialogVariant,
) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = match variant {
        AlertDialogVariant::Confirmation => colors.neutral_content.default,
        AlertDialogVariant::Warning => colors.notice,
        AlertDialogVariant::Destructive | AlertDialogVariant::Error => {
            colors.negative_background.default
        }
    };

    svg::Style { color: Some(color) }
}

fn backdrop_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(Color::from_rgba(0.0, 0.0, 0.0, 0.40))
}

fn dialog_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.gray.gray_50)),
        border: Border {
            color: colors.gray.gray_300,
            width: 1.0,
            radius: tokens::dimension::CORNER_RADIUS_500.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}

fn divider_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_300)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_preserves_actions_and_semantic_variant() {
        let dialog = AlertDialog::new(
            AlertDialogVariant::Destructive,
            "Excluir arquivo",
            "A ação não pode ser desfeita.",
            vec![
                AlertDialogAction::new("Cancelar", Some(1), ButtonOptions::SECONDARY),
                AlertDialogAction::new("Excluir", Some(2), ButtonOptions::NEGATIVE),
            ],
            0,
        );

        assert_eq!(dialog.variant, AlertDialogVariant::Destructive);
        assert_eq!(dialog.len(), 2);
        assert_eq!(
            variant_icon(dialog.variant),
            Some(WorkflowIcon::AlertCircleFilled)
        );
    }

    #[test]
    fn official_alert_dialog_widths_are_preserved() {
        assert_eq!(tokens::dimension::ALERT_DIALOG_MINIMUM_WIDTH, 288.0);
        assert_eq!(tokens::dimension::ALERT_DIALOG_MAXIMUM_WIDTH, 480.0);
    }
}
