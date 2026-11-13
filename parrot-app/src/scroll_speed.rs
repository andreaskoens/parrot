//! Mouse-wheel scrolling speed that follows the system setting. winit reports one wheel notch as
//! one "line" and egui scrolls a fixed [`DEFAULT_POINTS_PER_NOTCH`] points per line, ignoring how
//! many lines the user asked Windows to scroll per notch (Settings > Mouse). This reads that
//! setting and turns it into the points one notch scrolls, so the editor, sheet list and
//! library tree all scroll at the user's speed. Elsewhere the OS already scales the wheel
//! events (macOS, trackpads), so there's nothing to adjust.

/// What egui scrolls per notch by default — the right amount for Windows' default of 3 lines.
pub const DEFAULT_POINTS_PER_NOTCH: f32 = 40.0;
/// Lines per notch Windows uses unless the user changed it.
const DEFAULT_LINES: u32 = 3;
/// Windows reports "one page per notch" as this value.
const PAGE_AT_A_TIME: u32 = u32::MAX;
/// A page per notch, in points, when the window height isn't known here.
const PAGE_POINTS: f32 = 480.0;

/// Points to scroll for one wheel notch when the system scrolls `lines` lines per notch.
pub fn points_per_notch(lines: u32) -> f32 {
    match lines {
        PAGE_AT_A_TIME => PAGE_POINTS,
        // No scrolling at all stays slow rather than dead.
        0 => DEFAULT_POINTS_PER_NOTCH / DEFAULT_LINES as f32,
        n => DEFAULT_POINTS_PER_NOTCH / DEFAULT_LINES as f32 * n as f32,
    }
}

/// The system's wheel setting in lines per notch, where the platform has one.
#[cfg(windows)]
pub fn system_wheel_lines() -> Option<u32> {
    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            param: u32,
            value: *mut core::ffi::c_void,
            win_ini: u32,
        ) -> i32;
    }
    const SPI_GETWHEELSCROLLLINES: u32 = 0x0068;
    let mut lines: u32 = 0;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one UINT through `value`, which points at a live
    // u32 for the whole call; the other arguments are plain integers.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            std::ptr::addr_of_mut!(lines).cast(),
            0,
        )
    };
    (ok != 0).then_some(lines)
}

#[cfg(not(windows))]
pub fn system_wheel_lines() -> Option<u32> {
    None
}

/// The points per notch to use right now, or `None` to leave egui's default.
pub fn system_points_per_notch() -> Option<f32> {
    system_wheel_lines().map(points_per_notch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_default_keeps_eguis_speed_and_other_settings_scale_from_it() {
        assert_eq!(points_per_notch(3), DEFAULT_POINTS_PER_NOTCH);
        assert!(points_per_notch(1) < points_per_notch(3));
        assert_eq!(points_per_notch(6), 2.0 * DEFAULT_POINTS_PER_NOTCH);
        assert_eq!(points_per_notch(PAGE_AT_A_TIME), PAGE_POINTS);
        assert!(points_per_notch(0) > 0.0);
    }

    #[cfg(windows)]
    #[test]
    fn windows_reports_its_wheel_setting() {
        assert!(system_wheel_lines().is_some());
        assert!(system_points_per_notch().is_some_and(|p| p > 0.0));
    }
}
