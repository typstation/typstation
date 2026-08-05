//! Navegação lateral de seleção simples baseada no Side Navigation do Spectrum.

use iced::{
    Alignment, Background, Border, Element, Font, Length, Theme,
    font::Weight,
    widget::{Space, button, column, container, row, svg, text},
};

use super::{
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

#[derive(Debug, Clone)]
pub struct SideNavigationItem<Message> {
    pub label: String,
    pub icon: WorkflowIcon,
    pub selected: bool,
    pub on_press: Option<Message>,
}

impl<Message> SideNavigationItem<Message> {
    pub fn new(label: impl Into<String>, icon: WorkflowIcon, on_press: Option<Message>) -> Self {
        Self {
            label: label.into(),
            icon,
            selected: false,
            on_press,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
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
        let mut items = column![].spacing(tokens::spacing::SIDE_NAVIGATION_ITEM_GAP);

        for item in self.items {
            items = items.push(side_navigation_item(item));
        }

        container(items)
            .width(Length::Fixed(
                tokens::dimension::SIDE_NAVIGATION_DEFAULT_WIDTH,
            ))
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
        on_press,
    } = item;
    let enabled = on_press.is_some();
    let indicator = container(Space::new())
        .width(Length::Fixed(
            tokens::dimension::SIDE_NAVIGATION_INDICATOR_WIDTH,
        ))
        .height(Length::Fixed(
            tokens::dimension::SIDE_NAVIGATION_INDICATOR_HEIGHT,
        ))
        .style(move |theme| selection_indicator_style(theme, selected && enabled));
    let icon = svg(icon.handle())
        .width(Length::Fixed(tokens::icon::SIDE_NAVIGATION_WORKFLOW_SIZE))
        .height(Length::Fixed(tokens::icon::SIDE_NAVIGATION_WORKFLOW_SIZE))
        .style(move |theme, status| navigation_icon_style(theme, status, selected, enabled));
    let label = text(label)
        .size(tokens::typography::FONT_SIZE_100)
        .line_height(tokens::typography::LINE_HEIGHT_100)
        .font(Font {
            weight: Weight::Medium,
            ..Font::DEFAULT
        })
        .width(Length::Fill)
        .wrapping(text::Wrapping::None)
        .align_y(Alignment::Center);
    let content = row![indicator, icon, label]
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::SIDE_NAVIGATION_CONTENT_GAP)
        .width(Length::Fill)
        .height(Length::Fill);

    button(content)
        .on_press_maybe(on_press)
        .width(Length::Fill)
        .height(Length::Fixed(
            tokens::dimension::SIDE_NAVIGATION_ITEM_HEIGHT,
        ))
        .padding([
            0.0,
            tokens::spacing::SIDE_NAVIGATION_ITEM_PADDING_HORIZONTAL,
        ])
        .style(move |theme, status| navigation_item_style(theme, status, selected))
        .into()
}

fn side_navigation_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style::default().background(colors.gray.gray_50)
}

fn navigation_item_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Pressed => Some(colors.gray.gray_200),
        button::Status::Hovered => Some(colors.gray.gray_100),
        button::Status::Active if selected => Some(colors.gray.gray_100),
        button::Status::Active | button::Status::Disabled => None,
    };
    let text_color = if status == button::Status::Disabled {
        colors.disabled_content
    } else if selected {
        colors.accent_background.default
    } else {
        colors.gray.gray_800
    };

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border::default().rounded(tokens::dimension::CORNER_RADIUS_100),
        ..button::Style::default()
    }
}

fn navigation_icon_style(
    theme: &Theme,
    _status: svg::Status,
    selected: bool,
    enabled: bool,
) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = if !enabled {
        colors.disabled_content
    } else if selected {
        colors.accent_background.default
    } else {
        colors.gray.gray_800
    };

    svg::Style { color: Some(color) }
}

fn selection_indicator_style(theme: &Theme, selected: bool) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    if selected {
        container::Style::default()
            .background(colors.accent_background.default)
            .border(Border::default().rounded(
                tokens::dimension::SIDE_NAVIGATION_INDICATOR_WIDTH
                    * tokens::dimension::CORNER_RADIUS_FULL_MULTIPLIER,
            ))
    } else {
        container::Style::default()
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
        assert_eq!(item.on_press, Some(7));
    }

    #[test]
    fn navigation_reports_its_item_count() {
        let navigation = SideNavigation::new(vec![
            SideNavigationItem::<()>::new("Arquivos", WorkflowIcon::FolderOpen, None),
            SideNavigationItem::new("Tópicos", WorkflowIcon::TextBulleted, None),
        ]);

        assert_eq!(navigation.len(), 2);
        assert!(!navigation.is_empty());
    }
}
