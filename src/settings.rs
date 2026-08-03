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
            theme: ThemeMode::Dark,
        }
    }
}

impl Settings {
    pub fn validate(mut self) -> Self {
        self.tab_width = self.tab_width.clamp(1, 8);
        self.editor_font_size = self.editor_font_size.clamp(10, 30);
        self.preview_zoom = self.preview_zoom.clamp(25, 300);
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
            ..Settings::default()
        }
        .validate();

        assert_eq!(settings.tab_width, 1);
        assert_eq!(settings.editor_font_size, 30);
        assert_eq!(settings.preview_zoom, 25);
    }
}
