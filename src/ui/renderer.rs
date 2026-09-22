//! Direct2D renderer facade. Native target lifetime, text formats and paint resources are explicit.

mod drawing;
mod resources;
mod target;
mod text;

pub(crate) use resources::{BrushRole, TextStyle};
pub(crate) use target::Renderer;

#[cfg(test)]
use self::resources::brush_colors;
#[cfg(test)]
use self::text::trimming_for;

#[cfg(test)]
use windows::Win32::Graphics::DirectWrite::DWRITE_TRIMMING_GRANULARITY_CHARACTER;

#[cfg(test)]
mod tests;
