use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub tab_width: usize,
    pub auto_pairs: bool,
    pub auto_indent: bool,
    pub wrap_lines: bool,
    pub show_gutter: bool,
    pub editor_font_size: u16,
    pub preview_zoom: u16,
    pub pdf_tagged: bool,
    pub pdf_pretty: bool,
    pub svg_render_bleed: bool,
    pub svg_pretty: bool,
    pub svg_page_gap: u16,
    pub html_pretty: bool,
    pub theme: ThemeMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tab_width: 4,
            auto_pairs: true,
            auto_indent: true,
            wrap_lines: false,
            show_gutter: true,
            editor_font_size: 16,
            preview_zoom: 100,
            pdf_tagged: true,
            pdf_pretty: false,
            svg_render_bleed: false,
            svg_pretty: false,
            svg_page_gap: 12,
            html_pretty: true,
            theme: ThemeMode::Dark,
        }
    }
}

impl Settings {
    pub fn validate(mut self) -> Self {
        self.tab_width = self.tab_width.clamp(1, 8);
        self.editor_font_size = self.editor_font_size.clamp(10, 30);
        self.preview_zoom = self.preview_zoom.clamp(25, 300);
        self.svg_page_gap = self.svg_page_gap.min(72);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_loaded_from_disk_are_bounded() {
        let settings = Settings {
            tab_width: 0,
            editor_font_size: 100,
            preview_zoom: 5,
            svg_page_gap: 100,
            ..Settings::default()
        }
        .validate();

        assert_eq!(settings.tab_width, 1);
        assert_eq!(settings.editor_font_size, 30);
        assert_eq!(settings.preview_zoom, 25);
        assert_eq!(settings.svg_page_gap, 72);
    }

    #[test]
    fn legacy_settings_gain_export_defaults() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "tab_width": 2,
                "theme": "dark"
            }"#,
        )
        .expect("legacy settings should remain compatible");

        assert!(settings.pdf_tagged);
        assert!(!settings.svg_render_bleed);
        assert_eq!(settings.svg_page_gap, 12);
        assert!(settings.html_pretty);
    }
}
