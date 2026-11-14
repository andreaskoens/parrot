//! Core document/library data model for the Parrot rewrite.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

pub mod ast;
pub mod attachment;
pub mod filter;
pub mod goal;
pub mod keyword;
pub mod node;
pub mod search;
pub mod statistics;

pub use ast::{Inline, MarkupInfo, Paragraph, SheetContent, TagKind, TagMark};
pub use attachment::Attachment;
pub use filter::{DateRange, Filter, NodeKindMatch};
pub use goal::{DailySession, Goal, GoalCounter, GoalKind, GoalProgress};
pub use node::{Group, Library, Node, Project, Section, Sheet};
pub use statistics::{ActivityPoint, ActivityTracker};
