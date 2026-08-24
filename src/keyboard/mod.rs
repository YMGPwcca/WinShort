//! Keyboard subsystem: pure engine + Win32 hook integration.
//!
//! `engine`/`binding`/`keystate` are Windows-free and unit-tested; `hook`/
//! `dispatcher` contain the thin unsafe layer.

pub mod binding;
pub mod dispatcher;
pub mod engine;
pub mod hook;
pub mod keystate;

#[cfg(test)]
#[cfg(test)]
mod engine_props;

#[cfg(test)]
mod engine_tests;
