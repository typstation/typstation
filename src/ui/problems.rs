//! Painel de problemas inspirado em Table e Status Light do Adobe Spectrum 2.

use std::time::Duration;

use iced::{
    Alignment, Background, Border, Element, Font, Length, Padding, Theme,
    font::Weight,
    widget::{
        Column, Row, Space, button, column, container, responsive, row, scrollable, svg, text,
        tooltip,
    },
};

use super::{
    icons::WorkflowIcon,
    tokens::{self, SpectrumColors},
};

const COMPACT_SUMMARY_BREAKPOINT: f32 = 260.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl ProblemSeverity {
    const ALL: [Self; 4] = [Self::Error, Self::Warning, Self::Information, Self::Hint];

    const fn priority(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Information => 2,
            Self::Hint => 3,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "Erro",
            Self::Warning => "Aviso",
            Self::Information => "Informação",
            Self::Hint => "Dica",
        }
    }

    const fn short_label(self) -> &'static str {
        match self {
            Self::Error => "E",
            Self::Warning => "A",
            Self::Information => "I",
            Self::Hint => "D",
        }
    }

    fn count_label(self, count: usize) -> String {
        let noun = match (self, count) {
            (Self::Error, 1) => "erro",
            (Self::Error, _) => "erros",
            (Self::Warning, 1) => "aviso",
            (Self::Warning, _) => "avisos",
            (Self::Information, 1) => "informação",
            (Self::Information, _) => "informações",
            (Self::Hint, 1) => "dica",
            (Self::Hint, _) => "dicas",
        };

        format!("{count} {noun}")
    }

    const fn indicator_icon(self) -> WorkflowIcon {
        match self {
            Self::Error => WorkflowIcon::AlertCircleFilled,
            Self::Warning => WorkflowIcon::Alert,
            Self::Information | Self::Hint => WorkflowIcon::AlertCircleFilled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProblemItem<Message> {
    pub severity: ProblemSeverity,
    pub message: String,
    pub source: String,
    pub on_press: Option<Message>,
}

impl<Message> ProblemItem<Message> {
    pub fn new(
        severity: ProblemSeverity,
        message: impl Into<String>,
        source: impl Into<String>,
        on_press: Option<Message>,
    ) -> Self {
        Self {
            severity,
            message: message.into(),
            source: source.into(),
            on_press,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ProblemCounts {
    errors: usize,
    warnings: usize,
    information: usize,
    hints: usize,
}

impl ProblemCounts {
    fn from_items<Message>(items: &[ProblemItem<Message>]) -> Self {
        let mut counts = Self::default();

        for item in items {
            match item.severity {
                ProblemSeverity::Error => counts.errors += 1,
                ProblemSeverity::Warning => counts.warnings += 1,
                ProblemSeverity::Information => counts.information += 1,
                ProblemSeverity::Hint => counts.hints += 1,
            }
        }

        counts
    }

    const fn get(self, severity: ProblemSeverity) -> usize {
        match severity {
            ProblemSeverity::Error => self.errors,
            ProblemSeverity::Warning => self.warnings,
            ProblemSeverity::Information => self.information,
            ProblemSeverity::Hint => self.hints,
        }
    }
}

pub struct Problems<Message> {
    items: Vec<ProblemItem<Message>>,
    height: Length,
    show_header: bool,
}

impl<Message> Problems<Message> {
    pub fn new(mut items: Vec<ProblemItem<Message>>) -> Self {
        items.sort_by_key(|item| item.severity.priority());

        Self {
            items,
            height: Length::Shrink,
            show_header: true,
        }
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn show_header(mut self, show_header: bool) -> Self {
        self.show_header = show_header;
        self
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
        let counts = ProblemCounts::from_items(&self.items);
        let body: Element<'a, Message> = if self.items.is_empty() {
            container(status_light(
                "Nenhum problema",
                ProblemSeverity::Hint,
                false,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            let mut rows = Column::new().width(Length::Fill);

            for (index, item) in self.items.into_iter().enumerate() {
                if index > 0 {
                    rows = rows.push(divider());
                }
                rows = rows.push(problem_row(item));
            }

            scrollable(rows)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let mut panel = Column::new().width(Length::Fill).height(self.height);

        if self.show_header {
            panel = panel
                .push(divider())
                .push(problems_header(counts))
                .push(divider());
        }

        panel.push(body).into()
    }
}

pub fn problem_count_indicator<'a, Message>(
    severity: ProblemSeverity,
    count: usize,
    on_press: Option<Message>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let accessible_label = format!("Abrir Problemas: {}", severity.count_label(count));
    let icon = svg(severity.indicator_icon().handle())
        .width(Length::Fixed(tokens::icon::STATUS_BAR_SEVERITY_SIZE))
        .height(Length::Fixed(tokens::icon::STATUS_BAR_SEVERITY_SIZE))
        .style(move |theme, status| severity_icon_style(theme, status, severity));
    let content = row![
        icon,
        text(count.to_string())
            .size(tokens::typography::FONT_SIZE_75)
            .wrapping(text::Wrapping::None),
    ]
    .align_y(Alignment::Center)
    .spacing(tokens::spacing::STATUS_BAR_ICON_TO_COUNT)
    .height(Length::Fill);
    let control = button(content)
        .on_press_maybe(on_press)
        .height(Length::Fixed(
            tokens::dimension::STATUS_BAR_INDICATOR_HEIGHT,
        ))
        .padding([0.0, tokens::spacing::STATUS_BAR_INDICATOR_GAP])
        .style(problem_count_indicator_style);

    tooltip(
        control,
        text(accessible_label).size(tokens::typography::FONT_SIZE_75),
        tooltip::Position::Top,
    )
    .gap(tokens::spacing::BASE_GAP_SMALL)
    .padding(8)
    .delay(Duration::from_millis(500))
    .style(tooltip_style)
    .into()
}

impl<'a, Message> From<Problems<Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(problems: Problems<Message>) -> Self {
        problems.build()
    }
}

fn problems_header<'a, Message>(counts: ProblemCounts) -> Element<'a, Message>
where
    Message: 'a,
{
    responsive(move |size| {
        let compact = size.width < COMPACT_SUMMARY_BREAKPOINT;
        let title = text("Problemas")
            .size(tokens::typography::FONT_SIZE_100)
            .font(Font {
                weight: Weight::Bold,
                ..Font::DEFAULT
            });
        let mut summary = Row::new()
            .align_y(Alignment::Center)
            .spacing(tokens::spacing::PROBLEMS_SUMMARY_GAP);

        for severity in ProblemSeverity::ALL {
            let count = counts.get(severity);
            if count > 0 {
                summary = summary.push(problem_count(severity, count, compact));
            }
        }

        container(row![title, Space::new().width(Length::Fill), summary])
            .width(Length::Fill)
            .height(Length::Fixed(tokens::dimension::PROBLEMS_HEADER_HEIGHT))
            .padding([0.0, tokens::spacing::PROBLEMS_EDGE_TO_CONTENT])
            .align_y(Alignment::Center)
            .style(header_style)
            .into()
    })
    .height(Length::Fixed(tokens::dimension::PROBLEMS_HEADER_HEIGHT))
    .into()
}

fn problem_count<'a, Message>(
    severity: ProblemSeverity,
    count: usize,
    compact: bool,
) -> Element<'a, Message>
where
    Message: 'a,
{
    let full_label = severity.count_label(count);
    let visible_label = if compact {
        format!("{} {count}", severity.short_label())
    } else {
        full_label.clone()
    };
    let light = status_light(visible_label, severity, true);

    if compact {
        tooltip(
            light,
            text(full_label).size(tokens::typography::FONT_SIZE_75),
            tooltip::Position::Bottom,
        )
        .gap(tokens::spacing::BASE_GAP_SMALL)
        .padding(8)
        .delay(Duration::from_millis(500))
        .style(tooltip_style)
        .into()
    } else {
        light
    }
}

fn problem_row<'a, Message>(item: ProblemItem<Message>) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let ProblemItem {
        severity,
        message,
        source,
        on_press,
    } = item;
    let message = text(message)
        .size(tokens::typography::FONT_SIZE_100)
        .line_height(tokens::typography::LINE_HEIGHT_100)
        .wrapping(text::Wrapping::WordOrGlyph)
        .width(Length::Fill);
    let message_line = row![status_dot(severity), message]
        .align_y(Alignment::Center)
        .spacing(tokens::spacing::STATUS_LIGHT_DOT_TO_LABEL)
        .width(Length::Fill);
    let metadata = row![
        Space::new().width(Length::Fixed(
            tokens::dimension::STATUS_LIGHT_DOT_SMALL + tokens::spacing::STATUS_LIGHT_DOT_TO_LABEL,
        )),
        text(format!("{} | {source}", severity.label()))
            .size(tokens::typography::FONT_SIZE_75)
            .line_height(tokens::typography::LINE_HEIGHT_100)
            .wrapping(text::Wrapping::WordOrGlyph)
            .width(Length::Fill)
            .style(metadata_style),
    ]
    .width(Length::Fill);
    let content = column![message_line, metadata]
        .spacing(tokens::spacing::PROBLEMS_MESSAGE_TO_METADATA)
        .width(Length::Fill);

    button(content)
        .on_press_maybe(on_press)
        .width(Length::Fill)
        .padding(Padding {
            top: tokens::spacing::TABLE_ROW_TOP_TO_TEXT_MEDIUM_COMPACT,
            right: tokens::spacing::PROBLEMS_EDGE_TO_CONTENT,
            bottom: tokens::spacing::TABLE_ROW_BOTTOM_TO_TEXT_MEDIUM_COMPACT,
            left: tokens::spacing::PROBLEMS_EDGE_TO_CONTENT,
        })
        .style(problem_row_style)
        .into()
}

fn status_light<'a, Message>(
    label: impl text::IntoFragment<'a>,
    severity: ProblemSeverity,
    compact: bool,
) -> Element<'a, Message>
where
    Message: 'a,
{
    row![
        status_dot(severity),
        text(label)
            .size(tokens::typography::FONT_SIZE_75)
            .wrapping(text::Wrapping::None),
    ]
    .align_y(Alignment::Center)
    .spacing(tokens::spacing::STATUS_LIGHT_DOT_TO_LABEL)
    .height(if compact {
        Length::Shrink
    } else {
        Length::Fixed(tokens::dimension::COMPONENT_HEIGHT_75)
    })
    .into()
}

fn status_dot<'a, Message>(severity: ProblemSeverity) -> Element<'a, Message>
where
    Message: 'a,
{
    container(Space::new())
        .width(Length::Fixed(tokens::dimension::STATUS_LIGHT_DOT_SMALL))
        .height(Length::Fixed(tokens::dimension::STATUS_LIGHT_DOT_SMALL))
        .style(move |theme| status_dot_style(theme, severity))
        .into()
}

fn divider<'a, Message>() -> Element<'a, Message>
where
    Message: 'a,
{
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(tokens::dimension::PROBLEMS_DIVIDER_HEIGHT))
        .style(divider_style)
        .into()
}

fn status_dot_style(theme: &Theme, severity: ProblemSeverity) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = match severity {
        ProblemSeverity::Error => colors.negative_background.default,
        ProblemSeverity::Warning => colors.notice,
        ProblemSeverity::Information => colors.focus_indicator,
        ProblemSeverity::Hint => colors.gray.gray_500,
    };

    container::Style::default()
        .background(color)
        .border(Border::default().rounded(tokens::dimension::STATUS_LIGHT_DOT_SMALL / 2.0))
}

fn severity_icon_style(
    theme: &Theme,
    status: svg::Status,
    severity: ProblemSeverity,
) -> svg::Style {
    let colors = SpectrumColors::from_theme(theme);
    let color = match severity {
        ProblemSeverity::Error => match status {
            svg::Status::Idle => colors.negative_background.default,
            svg::Status::Hovered => colors.negative_background.hover,
        },
        ProblemSeverity::Warning => colors.notice,
        ProblemSeverity::Information => colors.focus_indicator,
        ProblemSeverity::Hint => colors.gray.gray_500,
    };

    svg::Style { color: Some(color) }
}

fn problem_count_indicator_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Hovered => Some(colors.gray.gray_100),
        button::Status::Pressed => Some(colors.gray.gray_200),
        button::Status::Active | button::Status::Disabled => None,
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: if status == button::Status::Disabled {
            colors.disabled_content
        } else {
            colors.neutral_content.default
        },
        border: Border::default().rounded(tokens::dimension::CORNER_RADIUS_100),
        ..button::Style::default()
    }
}

fn header_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_50)
}

fn divider_style(theme: &Theme) -> container::Style {
    container::Style::default().background(SpectrumColors::from_theme(theme).gray.gray_300)
}

fn metadata_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(SpectrumColors::from_theme(theme).gray.gray_600),
    }
}

fn problem_row_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = SpectrumColors::from_theme(theme);
    let background = match status {
        button::Status::Hovered => Some(colors.gray.gray_100),
        button::Status::Pressed => Some(colors.gray.gray_200),
        button::Status::Active | button::Status::Disabled => None,
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: if status == button::Status::Disabled {
            colors.disabled_content
        } else {
            colors.neutral_content.default
        },
        border: Border::default(),
        ..button::Style::default()
    }
}

fn tooltip_style(theme: &Theme) -> container::Style {
    let colors = SpectrumColors::from_theme(theme);

    container::Style::default()
        .background(colors.gray.gray_900)
        .color(colors.gray.gray_25)
        .border(Border::default().rounded(tokens::dimension::CORNER_RADIUS_300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_sorted_by_severity_without_reordering_equal_levels() {
        let problems = Problems::new(vec![
            ProblemItem::new(ProblemSeverity::Warning, "aviso 1", "a.typ", Some(1)),
            ProblemItem::new(ProblemSeverity::Error, "erro", "b.typ", Some(2)),
            ProblemItem::new(ProblemSeverity::Warning, "aviso 2", "c.typ", Some(3)),
        ]);

        assert_eq!(problems.items[0].severity, ProblemSeverity::Error);
        assert_eq!(problems.items[1].message, "aviso 1");
        assert_eq!(problems.items[2].message, "aviso 2");
    }

    #[test]
    fn counts_include_every_supported_semantic_level() {
        let items = vec![
            ProblemItem::<()>::new(ProblemSeverity::Error, "e", "a.typ", None),
            ProblemItem::new(ProblemSeverity::Warning, "w", "a.typ", None),
            ProblemItem::new(ProblemSeverity::Information, "i", "a.typ", None),
            ProblemItem::new(ProblemSeverity::Hint, "h", "a.typ", None),
        ];
        let counts = ProblemCounts::from_items(&items);

        assert_eq!(counts.errors, 1);
        assert_eq!(counts.warnings, 1);
        assert_eq!(counts.information, 1);
        assert_eq!(counts.hints, 1);
    }

    #[test]
    fn panel_reports_its_item_count() {
        let problems = Problems::new(vec![ProblemItem::<()>::new(
            ProblemSeverity::Error,
            "erro",
            "main.typ",
            None,
        )]);

        assert_eq!(problems.len(), 1);
        assert!(!problems.is_empty());
    }

    #[test]
    fn severity_uses_distinct_error_and_warning_icons() {
        assert_eq!(
            ProblemSeverity::Error.indicator_icon(),
            WorkflowIcon::AlertCircleFilled
        );
        assert_eq!(
            ProblemSeverity::Warning.indicator_icon(),
            WorkflowIcon::Alert
        );
    }

    #[test]
    fn panel_header_can_be_hidden_when_the_parent_already_has_a_title() {
        let problems = Problems::<()>::new(Vec::new()).show_header(false);

        assert!(!problems.show_header);
    }
}
