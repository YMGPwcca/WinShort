//! Native text prompt with explicit model, message, and window ownership boundaries.

mod messages;
mod model;
mod window;

pub(crate) use model::PromptAction;
pub(crate) use window::TextPrompt;
