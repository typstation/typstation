use display_info::DisplayInfo;

const MILLIMETERS_PER_INCH: f64 = 25.4;
const MINIMUM_VALID_PPI: f32 = 40.0;
const MAXIMUM_VALID_PPI: f32 = 1_000.0;

pub const FALLBACK_LOGICAL_PPI: f32 = 96.0;

pub fn logical_pixels_per_inch() -> f32 {
    let Ok(displays) = DisplayInfo::all() else {
        return FALLBACK_LOGICAL_PPI;
    };
    let Some(display) = select_initial_display(&displays) else {
        return FALLBACK_LOGICAL_PPI;
    };

    ppi_from_dimensions(
        display.width,
        display.height,
        display.width_mm,
        display.height_mm,
    )
    .unwrap_or(FALLBACK_LOGICAL_PPI)
}

fn select_initial_display(displays: &[DisplayInfo]) -> Option<&DisplayInfo> {
    displays
        .iter()
        .find(|display| display.is_primary)
        .or_else(|| displays.iter().find(|display| contains_origin(display)))
        .or_else(|| displays.first())
}

fn contains_origin(display: &DisplayInfo) -> bool {
    let left = i64::from(display.x);
    let top = i64::from(display.y);
    let right = left + i64::from(display.width);
    let bottom = top + i64::from(display.height);

    left <= 0 && 0 < right && top <= 0 && 0 < bottom
}

fn ppi_from_dimensions(width: u32, height: u32, width_mm: i32, height_mm: i32) -> Option<f32> {
    if width == 0 || height == 0 || width_mm <= 0 || height_mm <= 0 {
        return None;
    }

    let diagonal_pixels = f64::from(width).hypot(f64::from(height));
    let diagonal_inches = f64::from(width_mm).hypot(f64::from(height_mm)) / MILLIMETERS_PER_INCH;
    let ppi = (diagonal_pixels / diagonal_inches) as f32;

    (ppi.is_finite() && (MINIMUM_VALID_PPI..=MAXIMUM_VALID_PPI).contains(&ppi)).then_some(ppi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppi_uses_logical_pixels_and_physical_millimeters() {
        let ppi =
            ppi_from_dimensions(1920, 1080, 528, 297).expect("the monitor dimensions are valid");

        assert!((ppi - 92.36).abs() < 0.01);
    }

    #[test]
    fn invalid_physical_dimensions_do_not_produce_a_density() {
        assert_eq!(ppi_from_dimensions(1920, 1080, 0, 0), None);
        assert_eq!(ppi_from_dimensions(0, 0, 528, 297), None);
    }
}
