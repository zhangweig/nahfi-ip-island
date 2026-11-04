//! Resolution-aware zoom on top of the OS DPI, without applying DPI twice.
pub fn zoom(physical_height: f32, native_ppp: f32, manual_percent: u32) -> f32 {
    let native = if native_ppp.is_finite() && native_ppp > 0.0 {
        native_ppp
    } else {
        1.0
    };
    if manual_percent != 0 {
        return manual_percent.clamp(75, 250) as f32 / 100.0;
    }
    let height = if physical_height.is_finite() {
        physical_height
    } else {
        1080.0
    };
    let resolution = (height / 1080.0).clamp(1.0, 2.5);
    (resolution / native).max(1.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_4k_scales_once_and_manual_override_is_respected() {
        assert_eq!(zoom(1080.0, 1.0, 0), 1.0);
        assert_eq!(zoom(2160.0, 1.0, 0), 2.0);
        assert_eq!(zoom(2160.0, 1.5, 0) * 1.5, 2.0);
        assert_eq!(zoom(2160.0, 2.0, 0), 1.0);
        assert_eq!(zoom(2160.0, 2.5, 0), 1.0);
        assert_eq!(zoom(2160.0, 2.0, 150), 1.5);
        assert_eq!(zoom(f32::NAN, f32::NAN, 0), 1.0);
    }
}
