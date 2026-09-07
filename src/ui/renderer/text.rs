//! Text for the renderer.

use super::resources::{BrushRole, TextStyle};
use super::target::Renderer;
use crate::error::{Error, Result};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_DRAW_TEXT_OPTIONS_NONE,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteTextFormat, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
};

pub(super) fn trimming_for(style: TextStyle) -> Option<DWRITE_TRIMMING> {
    matches!(
        style,
        TextStyle::Section
            | TextStyle::Body
            | TextStyle::BodyStrong
            | TextStyle::Caption
            | TextStyle::CaptionRight
            | TextStyle::Button
            | TextStyle::ButtonSmall
            | TextStyle::Value
    )
    .then_some(DWRITE_TRIMMING {
        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
        delimiter: 0,
        delimiterCount: 0,
    })
}

impl Renderer {
    pub(crate) fn text(&self, text: &str, rect: D2D_RECT_F, style: TextStyle, role: BrushRole) {
        self.draw_text(text, rect, style, role, D2D1_DRAW_TEXT_OPTIONS_NONE);
    }

    pub(crate) fn text_clipped(
        &self,
        text: &str,
        rect: D2D_RECT_F,
        style: TextStyle,
        role: BrushRole,
    ) {
        self.draw_text(text, rect, style, role, D2D1_DRAW_TEXT_OPTIONS_CLIP);
    }

    /// Measure wrapped text using the same DirectWrite format used for
    /// painting. Layout callers reserve a safe block; this keeps the actual
    /// line geometry driven by the installed font rather than character counts.
    pub(crate) fn text_height(
        &self,
        text: &str,
        style: TextStyle,
        width: f32,
        max_height: f32,
    ) -> Option<f32> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = unsafe {
            self.dwrite.CreateTextLayout(
                &wide,
                self.format(style),
                width.max(1.0),
                max_height.max(1.0),
            )
        }
        .ok()?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics) }.ok()?;
        Some(metrics.height)
    }

    pub(crate) fn text_width(&self, text: &str, style: TextStyle, max_width: f32) -> Option<f32> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = unsafe {
            self.dwrite
                .CreateTextLayout(&wide, self.format(style), max_width.max(1.0), 64.0)
        }
        .ok()?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics) }.ok()?;
        Some(metrics.width)
    }

    pub(super) fn draw_text(
        &self,
        text: &str,
        rect: D2D_RECT_F,
        style: TextStyle,
        role: BrushRole,
        options: windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS,
    ) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            self.target.DrawText(
                &wide,
                self.format(style),
                std::ptr::from_ref(&rect),
                self.brush(role),
                options,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        }
    }

    pub(crate) fn format(&self, style: TextStyle) -> &IDWriteTextFormat {
        self.formats.get(style)
    }

    pub(super) fn rebuild_formats(&mut self) -> Result<()> {
        let formats = TextFormats::create(&self.dwrite)?;
        self.formats = formats;
        Ok(())
    }
}

/// Missing formats are unrepresentable, including during a failed DPI refresh.
pub(super) struct TextFormats {
    title: IDWriteTextFormat,
    subtitle: IDWriteTextFormat,
    section: IDWriteTextFormat,
    section_description: IDWriteTextFormat,
    body: IDWriteTextFormat,
    body_strong: IDWriteTextFormat,
    caption: IDWriteTextFormat,
    caption_right: IDWriteTextFormat,
    button: IDWriteTextFormat,
    button_small: IDWriteTextFormat,
    value: IDWriteTextFormat,
}

impl TextFormats {
    pub(super) fn create(
        dwrite: &windows::Win32::Graphics::DirectWrite::IDWriteFactory,
    ) -> Result<Self> {
        let build = |style, size, weight, alignment| {
            let format = create_format(dwrite, size, weight, alignment)?;
            if matches!(
                style,
                TextStyle::Title | TextStyle::Subtitle | TextStyle::SectionDescription
            ) {
                unsafe { format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP) }
                    .map_err(|e| Error::win("IDWriteTextFormat::SetWordWrapping", &e))?;
            }
            if let Some(trimming) = trimming_for(style) {
                let sign = unsafe { dwrite.CreateEllipsisTrimmingSign(&format) }
                    .map_err(|e| Error::win("CreateEllipsisTrimmingSign", &e))?;
                unsafe { format.SetTrimming(&trimming, &sign) }
                    .map_err(|e| Error::win("IDWriteTextFormat::SetTrimming", &e))?;
            }
            Ok::<_, Error>(format)
        };
        Ok(Self {
            title: build(
                TextStyle::Title,
                28.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            subtitle: build(
                TextStyle::Subtitle,
                14.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            section: build(
                TextStyle::Section,
                17.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            section_description: build(
                TextStyle::SectionDescription,
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            body: build(
                TextStyle::Body,
                14.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            body_strong: build(
                TextStyle::BodyStrong,
                14.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            caption: build(
                TextStyle::Caption,
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
            caption_right: build(
                TextStyle::CaptionRight,
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            )?,
            button: build(
                TextStyle::Button,
                13.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
            )?,
            button_small: build(
                TextStyle::ButtonSmall,
                12.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
            )?,
            value: build(
                TextStyle::Value,
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )?,
        })
    }

    fn get(&self, style: TextStyle) -> &IDWriteTextFormat {
        match style {
            TextStyle::Title => &self.title,
            TextStyle::Subtitle => &self.subtitle,
            TextStyle::Section => &self.section,
            TextStyle::SectionDescription => &self.section_description,
            TextStyle::Body => &self.body,
            TextStyle::BodyStrong => &self.body_strong,
            TextStyle::Caption => &self.caption,
            TextStyle::CaptionRight => &self.caption_right,
            TextStyle::Button => &self.button,
            TextStyle::ButtonSmall => &self.button_small,
            TextStyle::Value => &self.value,
        }
    }
}

fn create_format(
    dwrite: &windows::Win32::Graphics::DirectWrite::IDWriteFactory,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    alignment: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
) -> Result<IDWriteTextFormat> {
    let family = HSTRING::from("Segoe UI Variable Text");
    let locale = HSTRING::from("en-US");
    let format = unsafe {
        dwrite
            .CreateTextFormat(
                PCWSTR(family.as_ptr()),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(locale.as_ptr()),
            )
            .or_else(|_| {
                let fallback = HSTRING::from("Segoe UI");
                dwrite.CreateTextFormat(
                    PCWSTR(fallback.as_ptr()),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    PCWSTR(locale.as_ptr()),
                )
            })
            .map_err(|e| Error::win("CreateTextFormat", &e))?
    };
    unsafe {
        format
            .SetTextAlignment(alignment)
            .map_err(|e| Error::win("SetTextAlignment", &e))?;
        format
            .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
            .map_err(|e| Error::win("SetParagraphAlignment", &e))?;
        format
            .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
            .map_err(|e| Error::win("SetWordWrapping", &e))?;
    }
    Ok(format)
}
