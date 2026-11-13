//! Testable editor-state core (T-2-02): cursor, selection, insert/delete, undo/redo. Pure Rust,
//! no `egui`/`eframe` dependency, so it's unit-testable headlessly — the UI layer (T-2-04) only
//! ever paints from this, never carries its own copy of the text.
//!
//! Operates on `char` positions, not byte offsets or grapheme clusters — simplified, like
//! `parrot_markup::parse`'s own char-based scanner; fine for the Latin-script editing this
//! project targets today, not Unicode-grapheme-correct for combining marks/emoji.
//!
//! T-2-03 wires this to the `SheetContent` AST two ways: `to_ast` parses the live buffer (the
//! same blank-line-separated-paragraphs convention `parrot_markup::render` produces) for
//! typing -> AST; `from_ast` renders an AST back into a fresh buffer for AST -> typing (e.g. an
//! import, M6, replacing the open sheet's content).

use parrot_core::SheetContent;

/// A text selection: `anchor` is where the selection started, `head` is the live cursor end. A
/// cursor with no selection has `anchor == head`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn cursor(pos: usize) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    /// The selection as an ordered `(start, end)` pair, regardless of which end is the anchor.
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Debug, Clone)]
struct UndoEntry {
    text: String,
    selection: Selection,
}

#[derive(Debug, Clone)]
pub struct EditorState {
    text: String,
    selection: Selection,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
}

impl EditorState {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.chars().count();
        Self {
            text,
            selection: Selection::cursor(len),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// Sets the selection directly (e.g. the UI reporting a mouse drag or shift-click).
    /// Clamped to the text's current length.
    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        let len = self.text.chars().count();
        self.selection = Selection {
            anchor: anchor.min(len),
            head: head.min(len),
        };
    }

    /// Inserts `s` at the cursor, replacing the selection if one is active.
    pub fn insert(&mut self, s: &str) {
        self.push_undo();
        let (start, end) = self.selection.range();
        self.replace_range(start, end, s);
        self.selection = Selection::cursor(start + s.chars().count());
    }

    /// Replaces the current selection with `s` — same operation as `insert`, named for call
    /// sites where "replace the selection" is the intent being expressed (e.g. find/replace).
    pub fn replace_selection(&mut self, s: &str) {
        self.insert(s);
    }

    /// Deletes the selection if non-empty, otherwise the character before the cursor.
    pub fn delete_backward(&mut self) {
        let (start, end) = self.selection.range();
        if start == end {
            if start == 0 {
                return;
            }
            self.push_undo();
            self.replace_range(start - 1, start, "");
            self.selection = Selection::cursor(start - 1);
        } else {
            self.push_undo();
            self.replace_range(start, end, "");
            self.selection = Selection::cursor(start);
        }
    }

    pub fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(UndoEntry {
                text: self.text.clone(),
                selection: self.selection,
            });
            self.text = entry.text;
            self.selection = entry.selection;
        }
    }

    pub fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                text: self.text.clone(),
                selection: self.selection,
            });
            self.text = entry.text;
            self.selection = entry.selection;
        }
    }

    /// Snapshots the pre-edit state onto the undo stack and clears redo — standard
    /// "new edit invalidates the redo branch" behavior.
    fn push_undo(&mut self) {
        self.undo_stack.push(UndoEntry {
            text: self.text.clone(),
            selection: self.selection,
        });
        self.redo_stack.clear();
    }

    fn replace_range(&mut self, start: usize, end: usize, s: &str) {
        let mut chars: Vec<char> = self.text.chars().collect();
        chars.splice(start..end, s.chars());
        self.text = chars.into_iter().collect();
    }

    /// Parses the live buffer into a `SheetContent` AST.
    pub fn to_ast(&self) -> SheetContent {
        let paragraphs = self
            .text
            .split("\n\n")
            .filter(|s| !s.is_empty())
            .map(parrot_markup::parse::parse_paragraph)
            .collect();
        SheetContent::new(paragraphs)
    }

    /// Builds an `EditorState` from an AST by rendering it back to plain Markdown-XL text —
    /// the inverse of `to_ast`. The cursor starts at the end of the rendered text.
    pub fn from_ast(content: &SheetContent) -> Self {
        let mut text = parrot_markup::render::render(content);
        if text.ends_with('\n') {
            text.pop();
        }
        Self::new(text)
    }

    /// Finds every occurrence of `query` in the buffer (naive substring match over char
    /// positions, case-sensitive), as `(start, end)` char ranges in order. This operates on the
    /// plain-text buffer — the same typed Markdown-XL projection `to_ast` parses — so a match
    /// can land inside a markup span (e.g. the "bold" in `**bold**`); `replace_all` relies on
    /// that to let search/replace affect text inside inline spans without disturbing them.
    pub fn find_all(&self, query: &str) -> Vec<(usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }
        let chars: Vec<char> = self.text.chars().collect();
        let query_chars: Vec<char> = query.chars().collect();
        let mut matches = Vec::new();
        let mut i = 0;
        while i + query_chars.len() <= chars.len() {
            if chars[i..i + query_chars.len()] == query_chars[..] {
                matches.push((i, i + query_chars.len()));
                i += query_chars.len();
            } else {
                i += 1;
            }
        }
        matches
    }

    /// Replaces every occurrence of `query` with `replacement`, pushing a single undo entry for
    /// the whole operation. Returns the number of occurrences replaced.
    pub fn replace_all(&mut self, query: &str, replacement: &str) -> usize {
        let matches = self.find_all(query);
        if matches.is_empty() {
            return 0;
        }
        self.push_undo();
        // Replace from the end backward so earlier matches' char offsets stay valid.
        for (start, end) in matches.iter().rev() {
            self.replace_range(*start, *end, replacement);
        }
        self.selection = Selection::cursor(self.text.chars().count());
        matches.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, TagKind, TagMark};

    #[test]
    fn insert() {
        let mut state = EditorState::new("");
        state.insert("Hello");
        assert_eq!(state.text(), "Hello");
        assert_eq!(state.selection(), Selection::cursor(5));

        state.set_selection(5, 5);
        state.insert(", world");
        assert_eq!(state.text(), "Hello, world");
        assert_eq!(state.selection(), Selection::cursor(12));
    }

    #[test]
    fn delete() {
        let mut state = EditorState::new("Hello");
        state.delete_backward();
        assert_eq!(state.text(), "Hell");
        assert_eq!(state.selection(), Selection::cursor(4));

        // Deleting at position 0 is a no-op, not a panic.
        state.set_selection(0, 0);
        state.delete_backward();
        assert_eq!(state.text(), "Hell");

        // Deleting a non-empty selection removes the whole range.
        let mut state = EditorState::new("Hello World");
        state.set_selection(5, 11);
        state.delete_backward();
        assert_eq!(state.text(), "Hello");
        assert_eq!(state.selection(), Selection::cursor(5));
    }

    #[test]
    fn undo_redo_stack() {
        let mut state = EditorState::new("");
        state.insert("a");
        state.insert("b");
        state.insert("c");
        assert_eq!(state.text(), "abc");

        state.undo();
        assert_eq!(state.text(), "ab");
        state.undo();
        assert_eq!(state.text(), "a");
        state.undo();
        assert_eq!(state.text(), "");
        state.undo(); // nothing left to undo — no-op, not a panic.
        assert_eq!(state.text(), "");

        state.redo();
        assert_eq!(state.text(), "a");
        state.redo();
        assert_eq!(state.text(), "ab");
        state.redo();
        assert_eq!(state.text(), "abc");
        state.redo(); // nothing left to redo — no-op.
        assert_eq!(state.text(), "abc");

        // A new edit after undoing must clear the redo branch.
        state.undo();
        assert_eq!(state.text(), "ab");
        state.insert("X");
        assert_eq!(state.text(), "abX");
        state.redo(); // nothing to redo now — the "c" branch is gone.
        assert_eq!(state.text(), "abX");
    }

    #[test]
    fn selection_replace() {
        let mut state = EditorState::new("Hello World");
        state.set_selection(6, 11);
        state.replace_selection("there");
        assert_eq!(state.text(), "Hello there");
        assert_eq!(state.selection(), Selection::cursor(11));

        // Replacing a reversed selection (head before anchor) works the same way.
        let mut state = EditorState::new("Hello World");
        state.set_selection(11, 6);
        state.replace_selection("there");
        assert_eq!(state.text(), "Hello there");
    }

    #[test]
    fn typed_sequence_matches_direct_ast_build() {
        let mut state = EditorState::new("");
        for ch in "# Heading\n\nSome **bold** text.".chars() {
            state.insert(&ch.to_string());
        }

        let expected = SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("Heading".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Some ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    Inline::Text(" text.".to_string()),
                ],
            },
        ]);

        assert_eq!(state.to_ast(), expected);

        // Round-trips the other direction too: from_ast(x).to_ast() == x.
        let rebuilt = EditorState::from_ast(&expected);
        assert_eq!(rebuilt.to_ast(), expected);
    }

    #[test]
    fn find_all_occurrences() {
        let state = EditorState::new("the cat sat on the mat");
        let matches = state.find_all("at");
        assert_eq!(matches, vec![(5, 7), (9, 11), (20, 22)]);

        assert_eq!(state.find_all("xyz"), Vec::new());
        assert_eq!(state.find_all(""), Vec::new());
    }

    #[test]
    fn replace_preserves_surrounding_spans() {
        let mut state = EditorState::new("Some **bold** text here.");
        let count = state.replace_all("bold", "big");
        assert_eq!(count, 1);
        assert_eq!(state.text(), "Some **big** text here.");

        let ast = state.to_ast();
        assert_eq!(
            ast.paragraphs[0].inlines,
            vec![
                Inline::Text("Some ".to_string()),
                Inline::Strong(vec![Inline::Text("big".to_string())]),
                Inline::Text(" text here.".to_string()),
            ]
        );

        // Replacing text that spans outside any markup works too, and a repeated occurrence
        // replaces every match.
        let mut state = EditorState::new("cat cat cat");
        assert_eq!(state.replace_all("cat", "dog"), 3);
        assert_eq!(state.text(), "dog dog dog");
    }
}
