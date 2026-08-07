//! Subconjunto dos tokens oficiais do Adobe Spectrum 2 usado pelo Typstation.
//!
//! Fonte: `@adobe/spectrum-tokens` 14.15.0, commit `89cab5d` do
//! repositório `adobe/spectrum-design-data`. Os valores são do sistema
//! Spectrum, escala desktop, nos temas claro e escuro.

use iced::{Color, Theme};

pub const SOURCE_VERSION: &str = "14.15.0";
pub const SOURCE_COMMIT: &str = "89cab5d";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrayScale {
    pub gray_25: Color,
    pub gray_50: Color,
    pub gray_75: Color,
    pub gray_100: Color,
    pub gray_200: Color,
    pub gray_300: Color,
    pub gray_400: Color,
    pub gray_500: Color,
    pub gray_600: Color,
    pub gray_700: Color,
    pub gray_800: Color,
    pub gray_900: Color,
    pub gray_1000: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateColors {
    pub default: Color,
    pub hover: Color,
    pub down: Color,
    pub key_focus: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumColors {
    pub gray: GrayScale,
    pub accent_background: StateColors,
    pub negative_background: StateColors,
    pub neutral_background: StateColors,
    pub neutral_content: StateColors,
    pub focus_indicator: Color,
    pub disabled_background: Color,
    pub disabled_border: Color,
    pub disabled_content: Color,
    pub positive: Color,
    pub notice: Color,
}

pub mod dimension {
    pub const COMPONENT_HEIGHT_50: f32 = 20.0;
    pub const COMPONENT_HEIGHT_75: f32 = 24.0;
    pub const COMPONENT_HEIGHT_100: f32 = 32.0;
    pub const COMPONENT_HEIGHT_200: f32 = 40.0;
    pub const COMPONENT_HEIGHT_300: f32 = 48.0;

    pub const BORDER_WIDTH_100: f32 = 1.0;
    pub const BORDER_WIDTH_200: f32 = 2.0;
    pub const FOCUS_RING_THICKNESS: f32 = 2.0;
    pub const FOCUS_RING_GAP: f32 = 2.0;

    pub const CORNER_RADIUS_100: f32 = 4.0;
    pub const CORNER_RADIUS_300: f32 = 6.0;
    pub const CORNER_RADIUS_400: f32 = 7.0;
    pub const CORNER_RADIUS_500: f32 = 8.0;
    pub const CORNER_RADIUS_600: f32 = 9.0;
    pub const CORNER_RADIUS_700: f32 = 10.0;
    pub const CORNER_RADIUS_FULL_MULTIPLIER: f32 = 0.5;
    pub const ACTION_GROUP_COMPACT_RADIUS: f32 = CORNER_RADIUS_500;

    pub const BUTTON_MINIMUM_WIDTH_MULTIPLIER: f32 = 2.25;

    pub const TREE_VIEW_MINIMUM_HEIGHT: f32 = 40.0;
    pub const TREE_VIEW_MINIMUM_WIDTH: f32 = 160.0;
    pub const TREE_VIEW_DISCLOSURE_HEIGHT: f32 = 32.0;
    pub const TREE_VIEW_DISCLOSURE_WIDTH: f32 = 34.0;
    pub const TREE_VIEW_STATUS_ICON_SLOT_WIDTH: f32 = COMPONENT_HEIGHT_100;

    pub const PROBLEMS_HEADER_HEIGHT: f32 = COMPONENT_HEIGHT_100;
    pub const PROBLEMS_DIVIDER_HEIGHT: f32 = 1.0;
    pub const STATUS_LIGHT_DOT_SMALL: f32 = 8.0;
    pub const STATUS_BAR_HEIGHT: f32 = COMPONENT_HEIGHT_100;
    pub const STATUS_BAR_INDICATOR_HEIGHT: f32 = COMPONENT_HEIGHT_75;

    pub const FIELD_HEIGHT_MEDIUM: f32 = COMPONENT_HEIGHT_100;
    pub const PICKER_MINIMUM_WIDTH_MEDIUM: f32 = FIELD_HEIGHT_MEDIUM * 2.0;
    pub const PICKER_HANDLE_SIZE_MEDIUM: f32 = 10.0;
    pub const CHECKBOX_SIZE_MEDIUM: f32 = 16.0;
    pub const CHECKBOX_CORNER_RADIUS: f32 = 2.0;
    pub const SWITCH_CONTROL_HEIGHT_MEDIUM: f32 = 16.0;
    pub const SWITCH_HANDLE_SIZE_MEDIUM: f32 = 10.0;
    pub const SLIDER_HANDLE_SIZE_MEDIUM: f32 = 16.0;
    pub const SLIDER_TRACK_THICKNESS: f32 = 2.0;
    pub const SLIDER_CONTROL_HEIGHT_MEDIUM: f32 = COMPONENT_HEIGHT_75;
    pub const SPLIT_VIEW_DIVIDER_THICKNESS: f32 = 1.0;
    pub const SPLIT_VIEW_DIVIDER_INTERACTION_THICKNESS: f32 = 2.0;

    pub const ALERT_DIALOG_MINIMUM_WIDTH: f32 = 288.0;
    pub const ALERT_DIALOG_MAXIMUM_WIDTH: f32 = 480.0;
    pub const ALERT_DIALOG_DIVIDER_HEIGHT: f32 = 1.0;

    pub const TAB_ITEM_COMPACT_HEIGHT_MEDIUM: f32 = COMPONENT_HEIGHT_100;
    pub const TAB_SELECTION_INDICATOR_HEIGHT: f32 = BORDER_WIDTH_200;
    pub const TAB_CLOSE_BUTTON_SIZE: f32 = COMPONENT_HEIGHT_75;

    pub const MENU_ITEM_HEIGHT_MEDIUM: f32 = COMPONENT_HEIGHT_100;
    pub const MENU_ITEM_CORNER_RADIUS: f32 = CORNER_RADIUS_100;
    pub const MENU_POPOVER_CORNER_RADIUS: f32 = CORNER_RADIUS_700;
    pub const MENU_SECTION_DIVIDER_HEIGHT: f32 = 12.0;

    pub const PANEL_TABS_RAIL_WIDTH: f32 = COMPONENT_HEIGHT_300;
    pub const PANEL_TAB_ITEM_SIZE: f32 = COMPONENT_HEIGHT_200;
    pub const PANEL_TAB_SELECTION_INDICATOR_WIDTH: f32 = BORDER_WIDTH_200;
    pub const PANEL_TAB_NOTIFICATION_SIZE: f32 = 10.0;

    pub const QUICK_SWITCHER_WIDTH: f32 = 640.0;
    pub const QUICK_SWITCHER_RESULTS_HEIGHT: f32 = 320.0;
    pub const QUICK_SWITCHER_ITEM_HEIGHT: f32 = COMPONENT_HEIGHT_300;

    pub const APP_HEADER_HEIGHT: f32 = COMPONENT_HEIGHT_200;
    pub const PANEL_HEADER_HEIGHT: f32 = COMPONENT_HEIGHT_200;
    pub const SETTINGS_HEADER_HEIGHT: f32 = 56.0;
    pub const SETTINGS_FOOTER_HEIGHT: f32 = 64.0;
}

pub mod spacing {
    pub const SPACING_50: f32 = 2.0;
    pub const SPACING_75: f32 = 4.0;
    pub const SPACING_100: f32 = 8.0;
    pub const SPACING_200: f32 = 12.0;
    pub const SPACING_300: f32 = 16.0;
    pub const SPACING_400: f32 = 24.0;
    pub const SPACING_500: f32 = 32.0;

    pub const BASE_GAP_SMALL: f32 = 4.0;
    pub const BASE_GAP_MEDIUM: f32 = 6.0;
    pub const BASE_GAP_LARGE: f32 = 6.0;
    pub const BASE_GAP_EXTRA_LARGE: f32 = 6.0;

    pub const COMPONENT_EDGE_TO_VISUAL_ONLY_50: f32 = 3.0;
    pub const COMPONENT_EDGE_TO_VISUAL_ONLY_75: f32 = 4.0;
    pub const COMPONENT_EDGE_TO_VISUAL_ONLY_100: f32 = 6.0;
    pub const COMPONENT_EDGE_TO_VISUAL_ONLY_200: f32 = 9.0;
    pub const COMPONENT_EDGE_TO_VISUAL_ONLY_300: f32 = 11.0;

    pub const COMPONENT_TOP_TO_WORKFLOW_ICON_50: f32 = 3.0;
    pub const COMPONENT_TOP_TO_WORKFLOW_ICON_75: f32 = 4.0;
    pub const COMPONENT_TOP_TO_WORKFLOW_ICON_100: f32 = 6.0;
    pub const COMPONENT_TOP_TO_WORKFLOW_ICON_200: f32 = 9.0;
    pub const COMPONENT_TOP_TO_WORKFLOW_ICON_300: f32 = 11.0;

    pub const ACTION_GROUP_COMPACT_SPACING: f32 = -1.0;

    pub const BASE_PADDING_HORIZONTAL_SMALL: f32 = 10.0;
    pub const BASE_PADDING_HORIZONTAL_MEDIUM: f32 = 12.0;
    pub const BASE_PADDING_HORIZONTAL_LARGE: f32 = 14.0;
    pub const BASE_PADDING_HORIZONTAL_EXTRA_LARGE: f32 = 16.0;

    pub const BASE_PADDING_VERTICAL_SMALL: f32 = 4.0;
    pub const BASE_PADDING_VERTICAL_MEDIUM: f32 = 7.0;
    pub const BASE_PADDING_VERTICAL_LARGE: f32 = 10.0;
    pub const BASE_PADDING_VERTICAL_EXTRA_LARGE: f32 = 13.0;

    // O Button mantém a regra pública de padding horizontal igual à metade
    // da altura do componente.
    pub const BUTTON_HORIZONTAL_SMALL: f32 = 12.0;
    pub const BUTTON_HORIZONTAL_MEDIUM: f32 = 16.0;
    pub const BUTTON_HORIZONTAL_LARGE: f32 = 20.0;
    pub const BUTTON_HORIZONTAL_EXTRA_LARGE: f32 = 24.0;
    pub const BUTTON_GROUP_GAP: f32 = 8.0;

    pub const TREE_VIEW_EDGE_TO_CONTENT: f32 = 6.0;
    pub const TREE_VIEW_ITEM_GAP: f32 = 8.0;
    pub const TREE_VIEW_ACTION_GAP: f32 = BASE_GAP_SMALL;
    // Inclui a área ocupada pelo scrollbar sobreposto e preserva 12 px visíveis.
    pub const TREE_VIEW_ACTION_END_PADDING: f32 = 24.0;
    pub const TREE_VIEW_LEVEL_INCREMENT: f32 = 16.0;
    pub const TREE_VIEW_TOP_TO_DISCLOSURE: f32 = 4.0;

    pub const PROBLEMS_EDGE_TO_CONTENT: f32 = BASE_PADDING_HORIZONTAL_EXTRA_LARGE;
    pub const PROBLEMS_SUMMARY_GAP: f32 = BASE_PADDING_HORIZONTAL_MEDIUM;
    pub const PROBLEMS_MESSAGE_TO_METADATA: f32 = 2.0;
    pub const STATUS_LIGHT_DOT_TO_LABEL: f32 = BASE_GAP_MEDIUM;
    pub const STATUS_BAR_EDGE_TO_CONTENT: f32 = 8.0;
    pub const STATUS_BAR_INDICATOR_GAP: f32 = BASE_GAP_SMALL;
    pub const STATUS_BAR_ICON_TO_COUNT: f32 = BASE_GAP_SMALL;

    pub const FIELD_EDGE_TO_TEXT_MEDIUM: f32 = 12.0;
    pub const FIELD_TOP_TO_TEXT_MEDIUM: f32 = 6.0;
    pub const PICKER_EDGE_TO_TEXT_MEDIUM: f32 = FIELD_EDGE_TO_TEXT_MEDIUM;
    pub const PICKER_TEXT_TO_HANDLE_MEDIUM: f32 = SPACING_100;
    pub const PICKER_TO_MENU_MEDIUM: f32 = SPACING_100;
    pub const CHECKBOX_TO_LABEL: f32 = 8.0;
    pub const SWITCH_TO_LABEL: f32 = SPACING_100;

    pub const ALERT_DIALOG_PADDING: f32 = 32.0;
    pub const ALERT_DIALOG_ICON_TO_TITLE: f32 = 12.0;
    pub const ALERT_DIALOG_TITLE_TO_DIVIDER: f32 = 16.0;
    pub const ALERT_DIALOG_DIVIDER_TO_DESCRIPTION: f32 = 16.0;
    pub const ALERT_DIALOG_DESCRIPTION_TO_BUTTONS: f32 = 24.0;
    pub const ALERT_DIALOG_BUTTON_GAP: f32 = 8.0;

    pub const SEARCH_FIELD_ICON_SLOT: f32 = 36.0;
    pub const SEARCH_PANEL_EDGE_TO_CONTENT: f32 = 8.0;
    pub const SEARCH_PANEL_CONTROL_GAP: f32 = 6.0;
    pub const SEARCH_PANEL_ROW_GAP: f32 = 6.0;
    pub const TABLE_ROW_TOP_TO_TEXT_MEDIUM_COMPACT: f32 = 6.0;
    pub const TABLE_ROW_BOTTOM_TO_TEXT_MEDIUM_COMPACT: f32 = 9.0;

    pub const TAB_GAP_HORIZONTAL_MEDIUM: f32 = 24.0;
    pub const TAB_START_TO_EDGE_MEDIUM: f32 = BASE_PADDING_HORIZONTAL_MEDIUM;
    pub const TAB_ITEM_CONTENT_GAP: f32 = BASE_GAP_MEDIUM;

    pub const MENU_EDGE_TO_CONTENT_MEDIUM: f32 = 12.0;
    pub const MENU_CHECKMARK_TO_TEXT: f32 = 8.0;
    pub const MENU_TEXT_TO_VALUE: f32 = 24.0;
    pub const MENU_POPOVER_PADDING: f32 = BASE_GAP_SMALL;

    pub const PANEL_TABS_PADDING_HORIZONTAL: f32 = SPACING_75;
    pub const PANEL_TABS_PADDING_VERTICAL: f32 = SPACING_100;
    pub const PANEL_TABS_ITEM_GAP: f32 = SPACING_50;
    pub const PANEL_TAB_NOTIFICATION_OFFSET: f32 = SPACING_50;

    pub const QUICK_SWITCHER_TOP_MARGIN: f32 = 64.0;
    pub const QUICK_SWITCHER_EDGE_TO_CONTENT: f32 = SPACING_300;
    pub const QUICK_SWITCHER_CONTENT_GAP: f32 = SPACING_200;
    pub const QUICK_SWITCHER_ITEM_GAP: f32 = SPACING_200;
    pub const QUICK_SWITCHER_ITEM_EDGE_TO_CONTENT: f32 = SPACING_200;

    pub const APP_BAR_PADDING_VERTICAL: f32 = BASE_GAP_SMALL;
    pub const APP_BAR_PADDING_HORIZONTAL: f32 = SPACING_100;
    pub const APP_BAR_ACTION_GAP: f32 = SPACING_200;
    pub const PANEL_EDGE_TO_CONTENT: f32 = 8.0;
    pub const SETTINGS_EDGE_TO_CONTENT: f32 = 24.0;
    pub const SETTINGS_PANEL_HORIZONTAL_PADDING: f32 = 32.0;
    pub const SETTINGS_CONTROL_GAP: f32 = 16.0;
    pub const FIELD_LABEL_TO_CONTROL: f32 = 8.0;
    pub const TOOLTIP_EDGE_TO_CONTENT: f32 = SPACING_100;
}

pub mod typography {
    pub const FONT_SIZE_50: f32 = 11.0;
    pub const FONT_SIZE_75: f32 = 12.0;
    pub const FONT_SIZE_100: f32 = 14.0;
    pub const FONT_SIZE_200: f32 = 16.0;
    pub const FONT_SIZE_300: f32 = 18.0;
    pub const LINE_HEIGHT_100: f32 = 1.3;
    pub const ALERT_DIALOG_TITLE: f32 = 22.0;
    pub const ALERT_DIALOG_DESCRIPTION: f32 = 16.0;
}

pub mod icon {
    pub const WORKFLOW_SIZE_50: f32 = 14.0;
    pub const WORKFLOW_SIZE_75: f32 = 16.0;
    pub const WORKFLOW_SIZE_100: f32 = 20.0;
    pub const WORKFLOW_SIZE_200: f32 = 22.0;
    pub const WORKFLOW_SIZE_300: f32 = 26.0;

    pub const TREE_VIEW_DISCLOSURE_SIZE: f32 = 10.0;
    pub const TREE_VIEW_WORKFLOW_SIZE: f32 = 18.0;
    pub const STATUS_BAR_SEVERITY_SIZE: f32 = WORKFLOW_SIZE_50;
    pub const SEARCH_FIELD_ICON_SIZE: f32 = WORKFLOW_SIZE_75;
    pub const ALERT_DIALOG_ICON_SIZE: f32 = WORKFLOW_SIZE_200;
    pub const UI_CROSS_100_SIZE: f32 = 8.0;
    pub const UI_CHECKMARK_100_SIZE: f32 = 10.0;
}

impl ColorScheme {
    pub fn from_theme(theme: &Theme) -> Self {
        let background = theme.palette().background;
        let luminance = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;

        if luminance < 0.5 {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

impl SpectrumColors {
    pub const fn for_scheme(scheme: ColorScheme) -> Self {
        match scheme {
            ColorScheme::Light => LIGHT,
            ColorScheme::Dark => DARK,
        }
    }

    pub fn from_theme(theme: &Theme) -> Self {
        Self::for_scheme(ColorScheme::from_theme(theme))
    }
}

pub const LIGHT: SpectrumColors = SpectrumColors {
    gray: GrayScale {
        gray_25: Color::from_rgb8(255, 255, 255),
        gray_50: Color::from_rgb8(248, 248, 248),
        gray_75: Color::from_rgb8(243, 243, 243),
        gray_100: Color::from_rgb8(233, 233, 233),
        gray_200: Color::from_rgb8(225, 225, 225),
        gray_300: Color::from_rgb8(218, 218, 218),
        gray_400: Color::from_rgb8(198, 198, 198),
        gray_500: Color::from_rgb8(143, 143, 143),
        gray_600: Color::from_rgb8(113, 113, 113),
        gray_700: Color::from_rgb8(80, 80, 80),
        gray_800: Color::from_rgb8(41, 41, 41),
        gray_900: Color::from_rgb8(19, 19, 19),
        gray_1000: Color::from_rgb8(0, 0, 0),
    },
    accent_background: StateColors {
        default: Color::from_rgb8(59, 99, 251),
        hover: Color::from_rgb8(39, 77, 234),
        down: Color::from_rgb8(39, 77, 234),
        key_focus: Color::from_rgb8(39, 77, 234),
    },
    negative_background: StateColors {
        default: Color::from_rgb8(215, 50, 32),
        hover: Color::from_rgb8(183, 40, 24),
        down: Color::from_rgb8(183, 40, 24),
        key_focus: Color::from_rgb8(183, 40, 24),
    },
    neutral_background: StateColors {
        default: Color::from_rgb8(41, 41, 41),
        hover: Color::from_rgb8(19, 19, 19),
        down: Color::from_rgb8(19, 19, 19),
        key_focus: Color::from_rgb8(19, 19, 19),
    },
    neutral_content: StateColors {
        default: Color::from_rgb8(41, 41, 41),
        hover: Color::from_rgb8(19, 19, 19),
        down: Color::from_rgb8(19, 19, 19),
        key_focus: Color::from_rgb8(19, 19, 19),
    },
    focus_indicator: Color::from_rgb8(75, 117, 255),
    disabled_background: Color::from_rgb8(233, 233, 233),
    disabled_border: Color::from_rgb8(218, 218, 218),
    disabled_content: Color::from_rgb8(198, 198, 198),
    positive: Color::from_rgb8(5, 131, 78),
    notice: Color::from_rgb8(194, 78, 0),
};

pub const DARK: SpectrumColors = SpectrumColors {
    gray: GrayScale {
        gray_25: Color::from_rgb8(17, 17, 17),
        gray_50: Color::from_rgb8(27, 27, 27),
        gray_75: Color::from_rgb8(34, 34, 34),
        gray_100: Color::from_rgb8(44, 44, 44),
        gray_200: Color::from_rgb8(50, 50, 50),
        gray_300: Color::from_rgb8(57, 57, 57),
        gray_400: Color::from_rgb8(68, 68, 68),
        gray_500: Color::from_rgb8(109, 109, 109),
        gray_600: Color::from_rgb8(138, 138, 138),
        gray_700: Color::from_rgb8(175, 175, 175),
        gray_800: Color::from_rgb8(219, 219, 219),
        gray_900: Color::from_rgb8(242, 242, 242),
        gray_1000: Color::from_rgb8(255, 255, 255),
    },
    accent_background: StateColors {
        default: Color::from_rgb8(64, 105, 253),
        hover: Color::from_rgb8(52, 91, 248),
        down: Color::from_rgb8(52, 91, 248),
        key_focus: Color::from_rgb8(52, 91, 248),
    },
    negative_background: StateColors {
        default: Color::from_rgb8(223, 52, 34),
        hover: Color::from_rgb8(205, 46, 29),
        down: Color::from_rgb8(205, 46, 29),
        key_focus: Color::from_rgb8(205, 46, 29),
    },
    neutral_background: StateColors {
        default: Color::from_rgb8(219, 219, 219),
        hover: Color::from_rgb8(242, 242, 242),
        down: Color::from_rgb8(242, 242, 242),
        key_focus: Color::from_rgb8(242, 242, 242),
    },
    neutral_content: StateColors {
        default: Color::from_rgb8(219, 219, 219),
        hover: Color::from_rgb8(242, 242, 242),
        down: Color::from_rgb8(242, 242, 242),
        key_focus: Color::from_rgb8(242, 242, 242),
    },
    focus_indicator: Color::from_rgb8(64, 105, 253),
    disabled_background: Color::from_rgb8(44, 44, 44),
    disabled_border: Color::from_rgb8(57, 57, 57),
    disabled_content: Color::from_rgb8(68, 68, 68),
    positive: Color::from_rgb8(9, 157, 89),
    notice: Color::from_rgb8(224, 100, 0),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_medium_component_tokens_are_preserved() {
        assert_eq!(dimension::COMPONENT_HEIGHT_100, 32.0);
        assert_eq!(typography::FONT_SIZE_100, 14.0);
        assert_eq!(dimension::CORNER_RADIUS_100, 4.0);
        assert_eq!(dimension::CORNER_RADIUS_500, 8.0);
        assert_eq!(dimension::BUTTON_MINIMUM_WIDTH_MULTIPLIER, 2.25);
        assert_eq!(spacing::COMPONENT_EDGE_TO_VISUAL_ONLY_75, 4.0);
        assert_eq!(spacing::COMPONENT_TOP_TO_WORKFLOW_ICON_75, 4.0);
        assert_eq!(spacing::ACTION_GROUP_COMPACT_SPACING, -1.0);
        assert_eq!(spacing::BUTTON_GROUP_GAP, 8.0);
        assert_eq!(dimension::ACTION_GROUP_COMPACT_RADIUS, 8.0);
        assert_eq!(dimension::TREE_VIEW_MINIMUM_HEIGHT, 40.0);
        assert_eq!(dimension::TREE_VIEW_DISCLOSURE_WIDTH, 34.0);
        assert_eq!(dimension::TREE_VIEW_STATUS_ICON_SLOT_WIDTH, 32.0);
        assert_eq!(spacing::TREE_VIEW_LEVEL_INCREMENT, 16.0);
        assert_eq!(spacing::TREE_VIEW_ACTION_END_PADDING, 24.0);
        assert_eq!(dimension::PROBLEMS_HEADER_HEIGHT, 32.0);
        assert_eq!(dimension::STATUS_LIGHT_DOT_SMALL, 8.0);
        assert_eq!(spacing::PROBLEMS_EDGE_TO_CONTENT, 16.0);
        assert_eq!(spacing::STATUS_LIGHT_DOT_TO_LABEL, 6.0);
        assert_eq!(spacing::TABLE_ROW_TOP_TO_TEXT_MEDIUM_COMPACT, 6.0);
        assert_eq!(spacing::TABLE_ROW_BOTTOM_TO_TEXT_MEDIUM_COMPACT, 9.0);
        assert_eq!(icon::TREE_VIEW_WORKFLOW_SIZE, 18.0);
        assert_eq!(icon::TREE_VIEW_DISCLOSURE_SIZE, 10.0);
        assert_eq!(dimension::TAB_ITEM_COMPACT_HEIGHT_MEDIUM, 32.0);
        assert_eq!(dimension::TAB_SELECTION_INDICATOR_HEIGHT, 2.0);
        assert_eq!(dimension::TAB_CLOSE_BUTTON_SIZE, 24.0);
        assert_eq!(spacing::TAB_GAP_HORIZONTAL_MEDIUM, 24.0);
        assert_eq!(icon::UI_CROSS_100_SIZE, 8.0);
        assert_eq!(dimension::PANEL_TABS_RAIL_WIDTH, 48.0);
        assert_eq!(dimension::PANEL_TAB_ITEM_SIZE, 40.0);
        assert_eq!(dimension::PANEL_TAB_SELECTION_INDICATOR_WIDTH, 2.0);
        assert_eq!(dimension::PANEL_TAB_NOTIFICATION_SIZE, 10.0);
        assert_eq!(spacing::PANEL_TABS_PADDING_HORIZONTAL, 4.0);
        assert_eq!(spacing::PANEL_TABS_ITEM_GAP, 2.0);
        assert_eq!(spacing::APP_BAR_PADDING_VERTICAL, 4.0);
        assert_eq!(spacing::APP_BAR_PADDING_HORIZONTAL, 8.0);
    }

    #[test]
    fn color_schemes_use_opposite_neutral_scales() {
        assert_eq!(LIGHT.gray.gray_25, Color::WHITE);
        assert_eq!(DARK.gray.gray_1000, Color::WHITE);
        assert_ne!(
            LIGHT.accent_background.default,
            DARK.accent_background.default
        );
    }
}
