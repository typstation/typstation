use iced::{Theme, theme};

use super::tokens::{ColorScheme, SpectrumColors};

pub fn spectrum_theme(scheme: ColorScheme) -> Theme {
    let colors = SpectrumColors::for_scheme(scheme);
    let name = match scheme {
        ColorScheme::Light => "Spectrum 2 Light",
        ColorScheme::Dark => "Spectrum 2 Dark",
    };

    Theme::custom(
        name,
        theme::Palette {
            background: colors.gray.gray_25,
            text: colors.gray.gray_900,
            primary: colors.accent_background.default,
            success: colors.positive,
            warning: colors.notice,
            danger: colors.negative_background.default,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_theme_keeps_the_requested_scheme() {
        let light = spectrum_theme(ColorScheme::Light);
        let dark = spectrum_theme(ColorScheme::Dark);

        assert_eq!(ColorScheme::from_theme(&light), ColorScheme::Light);
        assert_eq!(ColorScheme::from_theme(&dark), ColorScheme::Dark);
    }
}
