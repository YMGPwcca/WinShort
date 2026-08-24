//! Per-Monitor V2 DPI awareness and scale helpers.

use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};

/// Must run before any window is created. Idempotent; failure (already set by
/// manifest or a prior call) is non-fatal.
pub fn set_process_awareness() {
    // SAFETY: documented process-wide setting, no lifetime constraints.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Render DPI policy for the overlay (#49): the effective DPI of the TARGET
/// monitor is the sole source of truth — never the pre-move window DPI and
/// never a monotonic max. Falls back to 96 when no monitor resolves.
pub fn effective_render_dpi(target_monitor_dpi: Option<u32>) -> u32 {
    target_monitor_dpi.map_or(96, |dpi| dpi.max(96))
}

pub fn monitor_dpi(hmon: HMONITOR) -> u32 {
    // SAFETY: hmon from MonitorFromWindow; MDT_EFFECTIVE_DPI matches PMv2 scaling.
    unsafe {
        let mut dx = 0u32;
        let mut dy = 0u32;
        if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 {
            dx
        } else {
            96
        }
    }
}

#[cfg(test)]
mod render_dpi_tests {
    use super::*;

    #[test]
    fn high_to_low_transition_scales_down() {
        // #49: overlay moving 150% -> 100% must render at the TARGET dpi,
        // never retain the stale higher window DPI.
        assert_eq!(effective_render_dpi(Some(96)), 96);
    }

    #[test]
    fn low_to_high_transition_scales_up() {
        assert_eq!(effective_render_dpi(Some(144)), 144);
    }

    #[test]
    fn same_dpi_is_unchanged() {
        assert_eq!(effective_render_dpi(Some(120)), 120);
    }

    #[test]
    fn missing_target_monitor_falls_back_to_96() {
        assert_eq!(effective_render_dpi(None), 96);
    }

    #[test]
    fn sub_96_target_clamps_to_96() {
        assert_eq!(effective_render_dpi(Some(72)), 96);
    }
}
