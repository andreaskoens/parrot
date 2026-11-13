//! Conversion-rule engine (M5) + per-target export backends.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

#[cfg(test)]
mod consolidated_tests;
pub mod conversion;
pub mod css;
pub mod docx;
pub mod dsl;
pub mod epub;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod html;
pub mod mvp;
pub mod pdf;
pub mod rtf;
pub mod rules;
pub mod textbundle;
