//! Library-wide search UI (T-7-02): a thin, headlessly-testable wrapper over T-1-07's
//! `parrot_core::search::search` — `ui.rs` binds a text box to `query` and calls `execute()`
//! on each change/click; the actual search logic stays in `parrot-core`, not duplicated here.

use parrot_core::Sheet;

#[derive(Debug, Clone, Default)]
pub struct SearchUiState {
    pub query: String,
}

impl SearchUiState {
    /// Runs the current query against `library`, returning every matching sheet in tree order —
    /// exactly `parrot_core::search::search`'s own result, just reached through the UI's state.
    pub fn execute<'a>(&self, library: &'a parrot_core::Library) -> Vec<&'a Sheet> {
        parrot_core::search::search(library, &self.query)
    }

    /// Same results as [`Self::execute`], but with each match's tree path included (T-9-03) —
    /// so a result row can actually be selected/opened, not just displayed.
    pub fn execute_with_paths<'a>(
        &self,
        library: &'a parrot_core::Library,
    ) -> Vec<(Vec<usize>, &'a Sheet)> {
        parrot_core::search::search_with_paths(library, &self.query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use parrot_core::{Inline, Node, Paragraph, SheetContent};

    fn sheet(title: &str, text: &str) -> Node {
        Node::Sheet(Sheet {
            title: title.to_string(),
            content: SheetContent::new(vec![Paragraph {
                tag: None,
                inlines: vec![Inline::Text(text.to_string())],
            }]),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        })
    }

    #[test]
    fn triggers_core_search() {
        let mut state = AppState::new();
        state
            .library
            .children
            .push(sheet("Alpha", "the quick brown fox"));
        state
            .library
            .children
            .push(sheet("Beta", "jumps over the lazy dog"));

        let ui = SearchUiState {
            query: "quick".to_string(),
        };

        let via_ui = ui.execute(&state.library);
        let direct = parrot_core::search::search(&state.library, "quick");

        assert_eq!(
            via_ui, direct,
            "UI search must produce exactly the core search's own result"
        );
        assert_eq!(
            via_ui.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            vec!["Alpha"]
        );
    }

    #[test]
    fn execute_with_paths_resolves_back_to_the_matching_node() {
        let mut state = AppState::new();
        state
            .library
            .children
            .push(sheet("Alpha", "the quick brown fox"));
        state
            .library
            .children
            .push(sheet("Beta", "jumps over the lazy dog"));

        let ui = SearchUiState {
            query: "lazy".to_string(),
        };

        let results = ui.execute_with_paths(&state.library);
        assert_eq!(results.len(), 1);
        let (path, sheet) = &results[0];
        assert_eq!(sheet.title, "Beta");
        assert_eq!(
            state.node(path).expect("path must resolve"),
            &Node::Sheet((*sheet).clone())
        );
    }
}
