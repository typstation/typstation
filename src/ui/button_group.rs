//! Grupo de botões relacionados baseado no Button Group do Adobe Spectrum.

use iced::{
    Element,
    widget::{Column, Row},
};

use super::{ButtonOptions, spectrum_button, tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonGroupOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub struct ButtonGroupItem<'a, Message> {
    pub label: &'a str,
    pub on_press: Option<Message>,
    pub options: ButtonOptions,
}

impl<'a, Message> ButtonGroupItem<'a, Message> {
    pub const fn new(label: &'a str, on_press: Option<Message>, options: ButtonOptions) -> Self {
        Self {
            label,
            on_press,
            options,
        }
    }
}

pub struct ButtonGroup<'a, Message> {
    items: Vec<ButtonGroupItem<'a, Message>>,
    trailing: Option<Element<'a, Message>>,
    orientation: ButtonGroupOrientation,
}

impl<'a, Message> ButtonGroup<'a, Message> {
    pub fn new(items: Vec<ButtonGroupItem<'a, Message>>) -> Self {
        Self {
            items,
            trailing: None,
            orientation: ButtonGroupOrientation::Horizontal,
        }
    }

    pub fn trailing(mut self, content: impl Into<Element<'a, Message>>) -> Self {
        self.trailing = Some(content.into());
        self
    }

    pub fn vertical(mut self) -> Self {
        self.orientation = ButtonGroupOrientation::Vertical;
        self
    }

    pub fn len(&self) -> usize {
        self.items.len() + usize::from(self.trailing.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.trailing.is_none()
    }

    fn build(self) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        match self.orientation {
            ButtonGroupOrientation::Horizontal => {
                let group = self.items.into_iter().fold(
                    Row::new().spacing(tokens::spacing::BUTTON_GROUP_GAP),
                    |group, item| {
                        group.push(spectrum_button(item.label, item.on_press, item.options))
                    },
                );
                match self.trailing {
                    Some(trailing) => group.push(trailing),
                    None => group,
                }
                .into()
            }
            ButtonGroupOrientation::Vertical => {
                let group = self.items.into_iter().fold(
                    Column::new().spacing(tokens::spacing::BUTTON_GROUP_GAP),
                    |group, item| {
                        group.push(spectrum_button(item.label, item.on_press, item.options))
                    },
                );
                match self.trailing {
                    Some(trailing) => group.push(trailing),
                    None => group,
                }
                .into()
            }
        }
    }
}

impl<'a, Message> From<ButtonGroup<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(group: ButtonGroup<'a, Message>) -> Self {
        group.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_group_defaults_to_horizontal_and_preserves_items() {
        let group = ButtonGroup::new(vec![
            ButtonGroupItem::new("SVG", Some(1), ButtonOptions::SECONDARY),
            ButtonGroupItem::new("PDF", Some(2), ButtonOptions::PRIMARY),
        ]);

        assert_eq!(group.orientation, ButtonGroupOrientation::Horizontal);
        assert_eq!(group.len(), 2);
        assert!(!group.is_empty());
    }

    #[test]
    fn button_group_can_stack_vertically() {
        let group = ButtonGroup::<()>::new(Vec::new()).vertical();

        assert_eq!(group.orientation, ButtonGroupOrientation::Vertical);
    }

    #[test]
    fn button_group_accepts_a_trailing_custom_action() {
        let group = ButtonGroup::new(vec![ButtonGroupItem::new(
            "Exportar PDF",
            Some(1),
            ButtonOptions::PRIMARY,
        )])
        .trailing(iced::widget::text("…"));

        assert_eq!(group.len(), 2);
        assert!(!group.is_empty());
    }
}
