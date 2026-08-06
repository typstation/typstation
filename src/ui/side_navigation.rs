//! Trilho lateral de ícones baseado em um Action Group vertical do Spectrum.

use iced::{
    Alignment, Background, Border, Element, Length, Theme,
    widget::{Column, Space, Stack, container, tooltip},
};

use super::{
    ActionButtonOptions,
    button::workflow_icon_action_button_with_tooltip,
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone)]
pub struct SideNavigationItem<Message> {
    pub label: String,
    pub icon: WorkflowIcon,
    pub selected: bool,
    pub notification: Option<SideNavigationNotification>,
    pub on_press: Option<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideNavigationNotification {
    Error,
    Warning,
}

impl<Message> SideNavigationItem<Message> {
    pub fn new(label: impl Into<String>, icon: WorkflowIcon, on_press: Option<Message>) -> Self {
        Self {
            label: label.into(),
            icon,
            selected: false,
            notification: None,
            on_press,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn notification(mut self, notification: Option<SideNavigationNotification>) -> Self {
        self.notification = notification;
        self
    }
}

pub struct SideNavigation<Message> {
    items: Vec<SideNavigationItem<Message>>,
}

impl<Message> SideNavigation<Message> {
    pub fn new(items: Vec<SideNavigationItem<Message>>) -> Self {
        Self { items }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn build<'a>(self) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        let mut items = Column::new()
            .spacing(tokens::spacing::SIDE_NAVIGATION_ITEM_GAP)
            .align_x(Alignment::Center)
            .width(Length::Fill);

        for item in self.items {
            items = items.push(side_navigation_item(item));
        }

        container(items)
            .width(Length::Fixed(tokens::dimension::SIDE_NAVIGATION_RAIL_WIDTH))
            .height(Length::Fill)
            .padding([
                tokens::spacing::SIDE_NAVIGATION_PADDING_VERTICAL,
                tokens::spacing::SIDE_NAVIGATION_PADDING_HORIZONTAL,
            ])
            .style(side_navigation_style)
            .into()
    }
}

impl<'a, Message> From<SideNavigation<Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(navigation: SideNavigation<Message>) -> Self {
        navigation.build()
    }
}

fn side_navigation_item<'a, Message>(item: SideNavigationItem<Message>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let SideNavigationItem {
        label,
        icon,
        selected,
        notification,
        on_press,
    } = item;

    let action = workflow_icon_action_button_with_tooltip(
        icon,
        label,
        on_press,
        ActionButtonOptions::QUIET
            .selected(selected)
            .emphasized(true),
        tooltip::Position::Right,
    );
    let Some(notification) = notification else {
        return action;
    };
    let dot = container(Space::new())
        .width(Length::Fixed(
            tokens::dimension::SIDE_NAVIGATION_NOTIFICATION_SIZE,
        ))
        .height(Length::Fixed(
            tokens::dimension::SIDE_NAVIGATION_NOTIFICATION_SIZE,
        ))
        .style(move |theme| notification_style(theme, notification));
    let overlay = container(dot)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(1.0)
        .align_x(Alignment::End)
        .align_y(Alignment::Start);

    Stack::new()
        .width(Length::Fixed(tokens::dimension::COMPONENT_HEIGHT_100))
        .height(Length::Fixed(tokens::dimension::COMPONENT_HEIGHT_100))
        .push(action)
        .push(overlay)
        .into()
}

fn side_navigation_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style::default().background(colors.gray.gray_50)
}

fn notification_style(theme: &Theme, notification: SideNavigationNotification) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match notification {
        SideNavigationNotification::Error => colors.negative_background.default,
        SideNavigationNotification::Warning => colors.notice,
    };

    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: colors.gray.gray_50,
            width: 2.0,
            radius: (tokens::dimension::SIDE_NAVIGATION_NOTIFICATION_SIZE
                * tokens::dimension::CORNER_RADIUS_FULL_MULTIPLIER)
                .into(),
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_builder_preserves_selection_and_action() {
        let item =
            SideNavigationItem::new("Arquivos", WorkflowIcon::FolderOpen, Some(7)).selected(true);

        assert_eq!(item.label, "Arquivos");
        assert_eq!(item.icon, WorkflowIcon::FolderOpen);
        assert!(item.selected);
        assert_eq!(item.notification, None);
        assert_eq!(item.on_press, Some(7));
    }

    #[test]
    fn item_builder_preserves_a_semantic_notification() {
        let item = SideNavigationItem::<()>::new(
            "Problemas: 1 erro",
            WorkflowIcon::AlertCircleFilled,
            None,
        )
        .notification(Some(SideNavigationNotification::Error));

        assert_eq!(item.notification, Some(SideNavigationNotification::Error));
    }

    #[test]
    fn navigation_reports_its_item_count() {
        let navigation = SideNavigation::new(vec![
            SideNavigationItem::<()>::new("Arquivos", WorkflowIcon::FolderOpen, None),
            SideNavigationItem::new("Sumário", WorkflowIcon::TextBulleted, None),
            SideNavigationItem::new("Problemas", WorkflowIcon::AlertCircleFilled, None),
        ]);

        assert_eq!(navigation.len(), 3);
        assert!(!navigation.is_empty());
    }
}
