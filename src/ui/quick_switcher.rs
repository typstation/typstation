//! Lançador rápido composto com primitivas do Adobe Spectrum.

use iced::{
    Alignment, Background, Border, Element, Font, Length, Theme,
    font::Weight,
    widget::{Space, button, column, container, mouse_area, row, scrollable, svg, text},
};

use super::{
    WorkflowIcon, elevated_dialog_style, metadata_text_style, search_field,
    tokens::{self, SpectrumColors},
};

pub struct QuickSwitcherItem<Message> {
    icon: WorkflowIcon,
    label: String,
    detail: Option<String>,
    shortcut: Option<String>,
    selected: bool,
    on_press: Message,
    on_focus: Message,
}

impl<Message> QuickSwitcherItem<Message> {
    pub fn new(icon: WorkflowIcon, label: impl Into<String>, on_press: Message) -> Self
    where
        Message: Clone,
    {
        Self {
            icon,
            label: label.into(),
            detail: None,
            shortcut: None,
            selected: false,
            on_focus: on_press.clone(),
            on_press,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn shortcut(mut self, shortcut: Option<String>) -> Self {
        self.shortcut = shortcut;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_focus(mut self, on_focus: Message) -> Self {
        self.on_focus = on_focus;
        self
    }
}

#[allow(clippy::too_many_arguments)]
pub fn quick_switcher<'a, Message>(
    title: &'a str,
    prompt: &'a str,
    query: &'a str,
    input_id: iced::widget::Id,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Message,
    on_clear: Option<Message>,
    items: Vec<QuickSwitcherItem<Message>>,
    empty_label: &'a str,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let field = search_field(
        prompt,
        query,
        input_id,
        on_input,
        on_submit,
        on_clear,
        Length::Fill,
    );
    let results: Element<'a, Message> = if items.is_empty() {
        container(
            text(empty_label)
                .size(tokens::typography::FONT_SIZE_100)
                .style(metadata_text_style),
        )
        .width(Length::Fill)
        .height(Length::Fixed(
            tokens::dimension::QUICK_SWITCHER_RESULTS_HEIGHT,
        ))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        let list = items
            .into_iter()
            .fold(column![].width(Length::Fill), |list, item| {
                list.push(quick_switcher_item(item))
            });

        scrollable(list)
            .width(Length::Fill)
            .height(Length::Fixed(
                tokens::dimension::QUICK_SWITCHER_RESULTS_HEIGHT,
            ))
            .into()
    };
    let dialog = container(
        column![
            text(title)
                .size(tokens::typography::FONT_SIZE_200)
                .font(Font {
                    weight: Weight::Bold,
                    ..Font::DEFAULT
                }),
            field,
            results,
        ]
        .width(Length::Fill)
        .spacing(tokens::spacing::QUICK_SWITCHER_CONTENT_GAP),
    )
    .width(Length::Fill)
    .max_width(tokens::dimension::QUICK_SWITCHER_WIDTH)
    .padding(tokens::spacing::QUICK_SWITCHER_EDGE_TO_CONTENT)
    .style(elevated_dialog_style);

    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([
            tokens::spacing::QUICK_SWITCHER_TOP_MARGIN,
            tokens::spacing::SPACING_400,
        ])
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

fn quick_switcher_item<'a, Message>(item: QuickSwitcherItem<Message>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let icon = svg(item.icon.handle())
        .width(Length::Fixed(tokens::icon::WORKFLOW_SIZE_100))
        .height(Length::Fixed(tokens::icon::WORKFLOW_SIZE_100))
        .style(quick_switcher_icon_style);
    let label = text(item.label)
        .size(tokens::typography::FONT_SIZE_100)
        .font(Font {
            weight: Weight::Medium,
            ..Font::DEFAULT
        })
        .wrapping(text::Wrapping::None);
    let description: Element<'a, Message> = item.detail.map_or_else(
        || Space::new().height(Length::Shrink).into(),
        |detail| {
            text(detail)
                .size(tokens::typography::FONT_SIZE_75)
                .wrapping(text::Wrapping::None)
                .style(metadata_text_style)
                .into()
        },
    );
    let labels = container(column![label, description].spacing(tokens::spacing::SPACING_50))
        .width(Length::Fill)
        .clip(true);
    let shortcut: Element<'a, Message> = item.shortcut.map_or_else(
        || Space::new().width(Length::Shrink).into(),
        |shortcut| {
            text(shortcut)
                .size(tokens::typography::FONT_SIZE_75)
                .wrapping(text::Wrapping::None)
                .style(metadata_text_style)
                .into()
        },
    );
    let content = row![icon, labels, shortcut]
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::QUICK_SWITCHER_ITEM_GAP);
    let control = button(content)
        .on_press(item.on_press)
        .width(Length::Fill)
        .height(Length::Fixed(tokens::dimension::QUICK_SWITCHER_ITEM_HEIGHT))
        .padding([0.0, tokens::spacing::QUICK_SWITCHER_ITEM_EDGE_TO_CONTENT])
        .style(move |theme, status| quick_switcher_item_style(theme, status, item.selected));

    mouse_area(control).on_enter(item.on_focus).into()
}

fn quick_switcher_item_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Pressed => Some(colors.gray.gray_200),
        button::Status::Hovered => Some(colors.gray.gray_100),
        button::Status::Active if selected => Some(colors.gray.gray_100),
        button::Status::Active | button::Status::Disabled => None,
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: colors.neutral_content.default,
        border: Border::default().rounded(tokens::dimension::CORNER_RADIUS_100),
        ..button::Style::default()
    }
}

fn quick_switcher_icon_style(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(SpectrumColors::from_theme(theme).gray.gray_700),
    }
}
