//! Directory-package I/O for `.parrot`/`.ulgroup`/`.ulproject`/`.ulfilter` (std::fs, not zip).

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

pub mod project;
pub mod sheet;
pub mod tree;
pub mod xml;
