//! Drawing for the renderer.

use super::resources::BrushRole;
use super::target::Renderer;
use crate::ui::theme::Color;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;

pub(crate) fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}

impl Renderer {
    pub(crate) fn fill_rect(&self, rect: D2D_RECT_F, role: BrushRole) {
        unsafe { self.target.FillRectangle(&rect, self.brush(role)) }
    }

    pub(crate) fn fill_rounded(&self, rect: D2D_RECT_F, radius: f32, role: BrushRole) {
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe { self.target.FillRoundedRectangle(&rr, self.brush(role)) }
    }

    pub(crate) fn fill_rounded_with_alpha(
        &self,
        rect: D2D_RECT_F,
        radius: f32,
        role: BrushRole,
        alpha: f32,
    ) {
        let mut color: Color = self.brushes.color(role);
        color.a = (color.a as f32 * alpha.clamp(0.0, 1.0)).round() as u8;
        let Ok(brush) = (unsafe { self.target.CreateSolidColorBrush(&color.d2d(), None) }) else {
            return;
        };
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe { self.target.FillRoundedRectangle(&rr, &brush) }
    }

    pub(crate) fn stroke_rounded(
        &self,
        rect: D2D_RECT_F,
        radius: f32,
        role: BrushRole,
        width: f32,
    ) {
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.target
                .DrawRoundedRectangle(&rr, self.brush(role), width, None)
        }
    }

    pub(crate) fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, role: BrushRole, width: f32) {
        unsafe {
            self.target.DrawLine(
                windows_numerics::Vector2 { X: x1, Y: y1 },
                windows_numerics::Vector2 { X: x2, Y: y2 },
                self.brush(role),
                width,
                None,
            )
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors D2D1_ELLIPSE shape
    pub(crate) fn ellipse(
        &self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        role: BrushRole,
        fill: bool,
        width: f32,
    ) {
        let ellipse = windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
            point: windows_numerics::Vector2 { X: cx, Y: cy },
            radiusX: rx,
            radiusY: ry,
        };
        unsafe {
            if fill {
                self.target.FillEllipse(&ellipse, self.brush(role));
            } else {
                self.target
                    .DrawEllipse(&ellipse, self.brush(role), width, None);
            }
        }
    }
}
