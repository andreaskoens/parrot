//! Keyword Manager (T-10-05): list/add/rename/delete Keywords, and assign/unassign on a
//! specific sheet, per inventory §15's confirmed `editKeywords:`/`showKeywordManager:` surface.
//!
//! `add`/`remove` (one sheet) go through `AppState::assign_keyword`/`unassign_keyword`.
//! `rename`/`delete` are library-wide operations with no single-node path to target, so they
//! walk `AppState::library` directly (a public field) the same way `parrot_core::search`
//! walks a `Library`'s tree.

use crate::state::{AppState, StateError};
use parrot_core::Node;

/// Adds `keyword` to the Sheet at `path`.
pub fn add(state: &mut AppState, path: &[usize], keyword: &str) -> Result<(), StateError> {
    state.assign_keyword(path, keyword)
}

/// Removes `keyword` from the Sheet at `path`.
pub fn remove(state: &mut AppState, path: &[usize], keyword: &str) -> Result<(), StateError> {
    state.unassign_keyword(path, keyword)
}

/// Renames `old` to `new` on every Sheet in the library currently tagged with it. A Sheet
/// already tagged with both ends up tagged with `new` only, not duplicated (via
/// `parrot_core::keyword::assign`'s own no-duplicate guarantee).
pub fn rename(state: &mut AppState, old: &str, new: &str) {
    rename_in(&mut state.library.children, old, new);
}

fn rename_in(nodes: &mut [Node], old: &str, new: &str) {
    for node in nodes {
        match node {
            Node::Sheet(s) => {
                if parrot_core::keyword::has(&s.keywords, old) {
                    parrot_core::keyword::unassign(&mut s.keywords, old);
                    parrot_core::keyword::assign(&mut s.keywords, new);
                }
            }
            Node::Group(g) => rename_in(&mut g.children, old, new),
            Node::Project(p) => rename_in(&mut p.children, old, new),
            Node::Filter(_) => {}
        }
    }
}

/// Deletes `keyword` outright: unassigns it from every Sheet in the library that carries it.
pub fn delete(state: &mut AppState, keyword: &str) {
    delete_in(&mut state.library.children, keyword);
}

fn delete_in(nodes: &mut [Node], keyword: &str) {
    for node in nodes {
        match node {
            Node::Sheet(s) => parrot_core::keyword::unassign(&mut s.keywords, keyword),
            Node::Group(g) => delete_in(&mut g.children, keyword),
            Node::Project(p) => delete_in(&mut p.children, keyword),
            Node::Filter(_) => {}
        }
    }
}

/// Every distinct Keyword currently assigned anywhere in the library, sorted — the Keyword
/// Manager's list view.
pub fn list_all(state: &AppState) -> Vec<String> {
    let mut found = Vec::new();
    list_in(&state.library.children, &mut found);
    found.sort();
    found.dedup();
    found
}

fn list_in(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Sheet(s) => out.extend(s.keywords.iter().cloned()),
            Node::Group(g) => list_in(&g.children, out),
            Node::Project(p) => list_in(&p.children, out),
            Node::Filter(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet_tagged(state: &mut AppState, title: &str, tags: &[&str]) -> usize {
        if state.projects().is_empty() {
            state.create_project("P");
        }
        state
            .create_sheet(&[0], title)
            .expect("create_sheet must succeed in a project");
        let idx = state.child_count(&[0]).expect("project") - 1;
        for tag in tags {
            super::add(state, &[0, idx], tag).expect("add must succeed on a fresh Sheet");
        }
        idx
    }

    #[test]
    fn add() {
        let mut state = AppState::new();
        let idx = sheet_tagged(&mut state, "Untitled", &[]);

        super::add(&mut state, &[0, idx], "todo").expect("add must succeed");

        match state.node(&[0, idx]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.keywords, vec!["todo".to_string()]),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn rename_propagates() {
        let mut state = AppState::new();
        let tagged = sheet_tagged(&mut state, "Tagged", &["draft"]);
        let untagged = sheet_tagged(&mut state, "Untagged", &["other"]);

        rename(&mut state, "draft", "in-progress");

        match state.node(&[0, tagged]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.keywords, vec!["in-progress".to_string()]),
            other => panic!("expected a Sheet, got {other:?}"),
        }
        // A sheet never tagged "draft" is untouched by the rename.
        match state.node(&[0, untagged]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.keywords, vec!["other".to_string()]),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn rename_onto_an_existing_tag_does_not_duplicate() {
        let mut state = AppState::new();
        let idx = sheet_tagged(&mut state, "Both", &["draft", "in-progress"]);

        rename(&mut state, "draft", "in-progress");

        match state.node(&[0, idx]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.keywords, vec!["in-progress".to_string()]),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn delete_unassigns() {
        let mut state = AppState::new();
        let a = sheet_tagged(&mut state, "A", &["todo", "draft"]);
        let b = sheet_tagged(&mut state, "B", &["todo"]);

        delete(&mut state, "todo");

        match state.node(&[0, a]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.keywords, vec!["draft".to_string()]),
            other => panic!("expected a Sheet, got {other:?}"),
        }
        match state.node(&[0, b]).expect("node must exist") {
            Node::Sheet(s) => assert!(s.keywords.is_empty()),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn remove_unassigns_from_one_sheet_only() {
        let mut state = AppState::new();
        let a = sheet_tagged(&mut state, "A", &["todo"]);
        let b = sheet_tagged(&mut state, "B", &["todo"]);

        remove(&mut state, &[0, a], "todo").expect("remove must succeed");

        match state.node(&[0, a]).expect("node must exist") {
            Node::Sheet(s) => assert!(s.keywords.is_empty()),
            other => panic!("expected a Sheet, got {other:?}"),
        }
        match state.node(&[0, b]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.keywords, vec!["todo".to_string()]),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn list_all_returns_sorted_distinct_keywords() {
        let mut state = AppState::new();
        sheet_tagged(&mut state, "A", &["zebra", "todo"]);
        sheet_tagged(&mut state, "B", &["todo", "apple"]);

        assert_eq!(
            list_all(&state),
            vec!["apple".to_string(), "todo".to_string(), "zebra".to_string()]
        );
    }
}
