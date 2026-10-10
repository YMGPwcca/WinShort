//! Icons for the overlay.

use super::model::OverlayIcon;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1RenderTarget, D2D1_ROUNDED_RECT};

pub(super) unsafe fn draw_icon(
    target: &ID2D1RenderTarget,
    icon: OverlayIcon,
    cx: f32,
    cy: f32,
    scale: f32,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    unsafe {
        let w = 1.8 * scale;
        match icon {
            OverlayIcon::Executable(_) => {}
            OverlayIcon::Microphone => {
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: cx - 4.0 * scale,
                            top: cy - 10.0 * scale,
                            right: cx + 4.0 * scale,
                            bottom: cy + 4.0 * scale,
                        },
                        radiusX: 4.0 * scale,
                        radiusY: 4.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                for (x1, y1, x2, y2) in [
                    (-8.0, 1.0, -8.0, 3.0),
                    (-8.0, 3.0, -6.0, 6.0),
                    (-6.0, 6.0, 0.0, 8.0),
                    (0.0, 8.0, 6.0, 6.0),
                    (6.0, 6.0, 8.0, 3.0),
                    (8.0, 3.0, 8.0, 1.0),
                    (0.0, 8.0, 0.0, 11.0),
                    (-4.0, 11.0, 4.0, 11.0),
                ] {
                    target.DrawLine(
                        windows_numerics::Vector2 {
                            X: cx + x1 * scale,
                            Y: cy + y1 * scale,
                        },
                        windows_numerics::Vector2 {
                            X: cx + x2 * scale,
                            Y: cy + y2 * scale,
                        },
                        brush,
                        w,
                        None,
                    );
                }
            }
            OverlayIcon::Output => {
                let points = [
                    (
                        cx - 9.0 * scale,
                        cy - 4.0 * scale,
                        cx - 4.0 * scale,
                        cy - 4.0 * scale,
                    ),
                    (
                        cx - 4.0 * scale,
                        cy - 4.0 * scale,
                        cx + 2.0 * scale,
                        cy - 9.0 * scale,
                    ),
                    (
                        cx + 2.0 * scale,
                        cy - 9.0 * scale,
                        cx + 2.0 * scale,
                        cy + 9.0 * scale,
                    ),
                    (
                        cx + 2.0 * scale,
                        cy + 9.0 * scale,
                        cx - 4.0 * scale,
                        cy + 4.0 * scale,
                    ),
                    (
                        cx - 4.0 * scale,
                        cy + 4.0 * scale,
                        cx - 9.0 * scale,
                        cy + 4.0 * scale,
                    ),
                ];
                for (x1, y1, x2, y2) in points {
                    target.DrawLine(
                        windows_numerics::Vector2 { X: x1, Y: y1 },
                        windows_numerics::Vector2 { X: x2, Y: y2 },
                        brush,
                        w,
                        None,
                    );
                }
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx + 6.0 * scale,
                        Y: cy - 6.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 10.0 * scale,
                        Y: cy,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx + 10.0 * scale,
                        Y: cy,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 6.0 * scale,
                        Y: cy + 6.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
            }
            OverlayIcon::Application => {
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: cx - 9.0 * scale,
                            top: cy - 7.0 * scale,
                            right: cx + 9.0 * scale,
                            bottom: cy + 7.0 * scale,
                        },
                        radiusX: 2.0 * scale,
                        radiusY: 2.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx - 9.0 * scale,
                        Y: cy - 2.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 9.0 * scale,
                        Y: cy - 2.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
            }
            OverlayIcon::Workspace => {
                target.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: cx - 9.0 * scale,
                            top: cy - 7.0 * scale,
                            right: cx + 9.0 * scale,
                            bottom: cy + 7.0 * scale,
                        },
                        radiusX: 2.0 * scale,
                        radiusY: 2.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy - 6.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy + 6.0 * scale,
                    },
                    brush,
                    w * 0.8,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx - 7.0 * scale,
                        Y: cy,
                    },
                    windows_numerics::Vector2 {
                        X: cx + 7.0 * scale,
                        Y: cy,
                    },
                    brush,
                    w * 0.8,
                    None,
                );
            }
            OverlayIcon::Info => {
                target.DrawEllipse(
                    &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                        point: windows_numerics::Vector2 { X: cx, Y: cy },
                        radiusX: 9.0 * scale,
                        radiusY: 9.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy - 1.0 * scale,
                    },
                    windows_numerics::Vector2 {
                        X: cx,
                        Y: cy + 5.0 * scale,
                    },
                    brush,
                    w,
                    None,
                );
                target.FillEllipse(
                    &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                        point: windows_numerics::Vector2 {
                            X: cx,
                            Y: cy - 5.0 * scale,
                        },
                        radiusX: 1.2 * scale,
                        radiusY: 1.2 * scale,
                    },
                    brush,
                );
            }
        }
    }
}
