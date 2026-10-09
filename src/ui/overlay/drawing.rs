//! Drawing for the overlay.

use super::icons::draw_icon;
use super::layout::{presentation_geometry, CARD_CORNER_RADIUS_DIP, PAD, ROW_HEIGHT};
use super::model::{OverlayIcon, OverlayModel, OverlayTone};
use super::palette::OverlayPalette;
use crate::error::{Error, Result};

use crate::ui::theme::Color;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    ID2D1RenderTarget, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_WORD_WRAPPING_NO_WRAP,
};

#[derive(Clone, Copy)]
pub(super) struct OverlayDrawOptions {
    pub(super) compact: f32,
    pub(super) fill_card: bool,
    pub(super) draw_card_border: bool,
}

pub(super) fn draw_overlay(
    target: &ID2D1RenderTarget,
    dwrite: &IDWriteFactory,
    model: &OverlayModel,
    scale: f32,
    palette: OverlayPalette,
    content_alpha: f32,
    options: OverlayDrawOptions,
) -> Result<()> {
    unsafe {
        let scale = scale.clamp(0.7, 1.6);
        let compact = options.compact.clamp(0.0, 1.0);
        let geometry = presentation_geometry(scale, model.rows.len(), compact);
        let body = D2D_RECT_F {
            left: geometry.body_left,
            top: geometry.body_top,
            right: geometry.body_right,
            bottom: geometry.body_bottom,
        };
        let left = geometry.body_left;
        let top = geometry.body_top;
        if options.fill_card {
            let surface = color(with_alpha(palette.surface, content_alpha));
            let surface_brush = target.CreateSolidColorBrush(&surface, None)?;
            target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: body,
                    radiusX: CARD_CORNER_RADIUS_DIP,
                    radiusY: CARD_CORNER_RADIUS_DIP,
                },
                &surface_brush,
            );
        }

        let border = color(with_alpha(palette.border, content_alpha));
        let border_brush = target.CreateSolidColorBrush(&border, None)?;
        if options.draw_card_border {
            let inset = 0.5;
            target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: body.left + inset,
                        top: body.top + inset,
                        right: body.right - inset,
                        bottom: body.bottom - inset,
                    },
                    radiusX: (CARD_CORNER_RADIUS_DIP - inset).max(0.0),
                    radiusY: (CARD_CORNER_RADIUS_DIP - inset).max(0.0),
                },
                &border_brush,
                1.0,
                None,
            );
        }

        let title_format = make_format(dwrite, 14.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
        let detail_format = make_format(dwrite, 12.0 * scale, DWRITE_FONT_WEIGHT_NORMAL)?;
        let text_alpha = content_alpha * (1.0 - compact * 3.0).max(0.0);
        let text = color(with_alpha(palette.text, text_alpha));
        let secondary = color(with_alpha(palette.secondary, text_alpha));
        let text_brush = target.CreateSolidColorBrush(&text, None)?;
        let secondary_brush = target.CreateSolidColorBrush(&secondary, None)?;
        let unavailable_text = color(with_alpha(palette.unavailable_text, text_alpha));
        let unavailable_brush = target.CreateSolidColorBrush(&unavailable_text, None)?;

        for (index, row) in model.rows.iter().enumerate() {
            let y = top + PAD * scale + index as f32 * ROW_HEIGHT * scale;
            if index > 0 {
                target.DrawLine(
                    windows_numerics::Vector2 {
                        X: left + 58.0 * scale,
                        Y: y,
                    },
                    windows_numerics::Vector2 {
                        X: body.right - 16.0 * scale,
                        Y: y,
                    },
                    &border_brush,
                    1.0,
                    None,
                );
            }
            draw_badge(
                target,
                row,
                windows_numerics::Vector2 {
                    X: left + 34.0 * scale,
                    Y: y + ROW_HEIGHT * scale * 0.5 - 13.0 * scale * compact,
                },
                scale,
                palette,
                content_alpha,
                compact,
            )?;
            if text_alpha <= 0.0 {
                continue;
            }

            draw_text(
                target,
                &row.title,
                &title_format,
                D2D_RECT_F {
                    left: left + 62.0 * scale,
                    top: y + 8.0 * scale,
                    right: body.right - 18.0 * scale,
                    bottom: y + 32.0 * scale,
                },
                &text_brush,
            );
            draw_text(
                target,
                &row.detail,
                &detail_format,
                D2D_RECT_F {
                    left: left + 62.0 * scale,
                    top: y + 30.0 * scale,
                    right: body.right - 18.0 * scale,
                    bottom: y + 54.0 * scale,
                },
                if row.tone == OverlayTone::Unavailable {
                    &unavailable_brush
                } else {
                    &secondary_brush
                },
            );
        }
        Ok(())
    }
}

unsafe fn draw_badge(
    target: &ID2D1RenderTarget,
    row: &super::model::OverlayRow,
    center: windows_numerics::Vector2,
    scale: f32,
    palette: OverlayPalette,
    content_alpha: f32,
    compact: f32,
) -> Result<()> {
    unsafe {
        let tone_color = match row.tone {
            OverlayTone::Muted => palette.tone_muted,
            OverlayTone::Active => palette.tone_active,
            OverlayTone::Changed => palette.tone_changed,
            OverlayTone::Unavailable => palette.tone_unavailable,
        };
        let tone = color(with_alpha(tone_color, content_alpha));
        let tone_brush = target.CreateSolidColorBrush(&tone, None)?;
        let icon_color = if row.tone == OverlayTone::Changed {
            palette.changed_icon
        } else {
            palette.icon
        };
        let icon_brush_color = color(with_alpha(icon_color, content_alpha));
        let icon_brush = target.CreateSolidColorBrush(&icon_brush_color, None)?;
        target.FillEllipse(
            &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                point: windows_numerics::Vector2 {
                    X: center.X,
                    Y: center.Y,
                },
                radiusX: 17.0 * scale,
                radiusY: 17.0 * scale,
            },
            &tone_brush,
        );
        draw_icon(target, row.icon, center.X, center.Y, scale, &icon_brush);

        if compact > 0.0 && row.icon == OverlayIcon::Microphone && row.tone == OverlayTone::Muted {
            target.DrawLine(
                windows_numerics::Vector2 {
                    X: center.X - 10.0 * scale,
                    Y: center.Y - 11.0 * scale,
                },
                windows_numerics::Vector2 {
                    X: center.X + 10.0 * scale,
                    Y: center.Y + 11.0 * scale,
                },
                &icon_brush,
                1.8 * scale,
                None,
            );
        }

        Ok(())
    }
}

unsafe fn make_format(
    dwrite: &IDWriteFactory,
    size: f32,
    weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
) -> Result<windows::Win32::Graphics::DirectWrite::IDWriteTextFormat> {
    unsafe {
        let family = HSTRING::from("Segoe UI Variable Text");
        let locale = HSTRING::from("en-US");
        let format = dwrite
            .CreateTextFormat(
                PCWSTR(family.as_ptr()),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(locale.as_ptr()),
            )
            .map_err(|e| Error::win("overlay CreateTextFormat", &e))?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        Ok(format)
    }
}

unsafe fn draw_text(
    target: &ID2D1RenderTarget,
    value: &str,
    format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
    rect: D2D_RECT_F,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    unsafe {
        let wide: Vec<u16> = value.encode_utf16().collect();
        target.DrawText(
            &wide,
            format,
            &rect,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

fn with_alpha(value: Color, multiplier: f32) -> Color {
    Color::rgba(
        value.r,
        value.g,
        value.b,
        (value.a as f32 * multiplier.clamp(0.0, 1.0)).round() as u8,
    )
}

fn color(color: Color) -> D2D1_COLOR_F {
    color.d2d()
}
