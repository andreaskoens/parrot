//! "Markdown XL" tag table: text <-> AST parser/renderer.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

pub mod parse;
pub mod render;
