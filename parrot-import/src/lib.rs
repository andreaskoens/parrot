//! Format importers -> parrot-core AST.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

#[cfg(test)]
mod consolidated_tests;
pub mod docx;
pub mod enex;
pub mod html;
pub mod markdown;
pub mod plaintext;
pub mod rtf;
pub mod textbundle;
