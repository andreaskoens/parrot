//! T-0-11: one headless end-to-end test driving the full MVP vertical slice through the logic
//! layer only (no `eframe::App`/rendering) — create library -> create group -> create sheet ->
//! type content -> save -> close -> reopen -> assert content -> export .txt/.md -> assert
//! golden files.

use parrot_app::state::AppState;
use parrot_core::{Node, SheetContent};

const TYPED_CONTENT: &str = "# My Sheet\n\nThis is **bold** and *italic* text.";

/// Mirrors `ui.rs`'s `apply_editor_buffer`: splits typed text on blank lines into paragraphs
/// and parses each with the Markdown-XL typing-time parser (T-0-08).
fn parse_typed_content(text: &str) -> SheetContent {
    let paragraphs = text
        .split("\n\n")
        .filter(|s| !s.is_empty())
        .map(parrot_markup::parse::parse_paragraph)
        .collect();
    SheetContent::new(paragraphs)
}

#[test]
fn full_round_trip_and_export() {
    let dir = tempfile::tempdir().expect("tempdir must be creatable");

    // create library -> create project -> create section -> create sheet -> type content -> save
    let mut state = AppState::new();
    state.create_library(dir.path());
    state.create_project("Novel");
    state
        .create_group(&[0], "Drafts")
        .expect("create_group must succeed");
    state
        .create_sheet(&[0, 0], "My Sheet")
        .expect("create_sheet must succeed");
    state
        .set_sheet_content(&[0, 0, 0], parse_typed_content(TYPED_CONTENT))
        .expect("set_sheet_content must succeed");
    state.save().expect("save must succeed");

    // close -> reopen
    drop(state);
    let mut reopened = AppState::new();
    reopened
        .open_library(dir.path())
        .expect("open_library must succeed");

    // assert content
    let sheet_content = match reopened
        .node(&[0, 0, 0])
        .expect("sheet must exist after reopen")
    {
        Node::Sheet(s) => s.content.clone(),
        other => panic!("expected a Sheet, got {other:?}"),
    };
    assert_eq!(sheet_content, parse_typed_content(TYPED_CONTENT));

    // export .txt/.md -> assert golden files
    let markdown = parrot_export::mvp::export_markdown(&sheet_content);
    let plain_text = parrot_export::mvp::export_plain_text(&sheet_content);

    assert_eq!(
        markdown,
        include_str!("../../tests/fixtures/mvp/e2e_sheet.md")
    );
    assert_eq!(
        plain_text,
        include_str!("../../tests/fixtures/mvp/e2e_sheet.plain.txt")
    );
}
