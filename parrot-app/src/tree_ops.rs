//! Move/duplicate/delete tree operations (T-7-03). Move and duplicate pass straight through to
//! `AppState`'s own primitives; delete is gated behind an explicit `confirmed` flag — `ui.rs` is
//! expected to show a confirmation dialog and only pass `confirmed: true` once the user accepts
//! it, so a stray click can never destroy a node.

use crate::state::{AppState, StateError};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TreeOpError {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("deletion requires confirmation")]
    NotConfirmed,
}

pub fn move_node(
    state: &mut AppState,
    from: &[usize],
    to_parent: &[usize],
) -> Result<(), TreeOpError> {
    Ok(state.move_node(from, to_parent)?)
}

pub fn duplicate_node(state: &mut AppState, path: &[usize]) -> Result<(), TreeOpError> {
    Ok(state.duplicate_node(path)?)
}

/// Deletes the node at `path` only if `confirmed` is `true`; otherwise returns
/// `TreeOpError::NotConfirmed` and leaves the tree untouched.
pub fn delete_node(
    state: &mut AppState,
    path: &[usize],
    confirmed: bool,
) -> Result<(), TreeOpError> {
    if !confirmed {
        return Err(TreeOpError::NotConfirmed);
    }
    Ok(state.delete_node(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::Node;

    fn two_sheets_in_a_group() -> AppState {
        let mut state = AppState::new();
        state.create_project("Drafts");
        state.create_sheet(&[0], "A").expect("create_sheet A");
        state.create_sheet(&[0], "B").expect("create_sheet B");
        state
    }

    #[test]
    fn move_node() {
        let mut state = two_sheets_in_a_group();
        state.create_project("Archive"); // path [1]

        super::move_node(&mut state, &[0, 0], &[1]).expect("move must succeed");

        match state.node(&[0]).unwrap() {
            Node::Project(g) => assert_eq!(g.children.len(), 1, "only B should remain in Drafts"),
            other => panic!("expected Project, got {other:?}"),
        }
        match state.node(&[1]).unwrap() {
            Node::Project(g) => {
                assert_eq!(g.children.len(), 1);
                match &g.children[0] {
                    Node::Sheet(s) => assert_eq!(s.title, "A"),
                    other => panic!("expected Sheet A in Archive, got {other:?}"),
                }
            }
            other => panic!("expected Project, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_node() {
        let mut state = two_sheets_in_a_group();

        super::duplicate_node(&mut state, &[0, 0]).expect("duplicate must succeed");

        match state.node(&[0]).unwrap() {
            Node::Project(g) => {
                assert_eq!(g.children.len(), 3);
                match (&g.children[0], &g.children[1]) {
                    (Node::Sheet(a), Node::Sheet(b)) => {
                        assert_eq!(
                            a.title, b.title,
                            "the duplicate must sit right after the original"
                        );
                        assert_eq!(a.title, "A");
                    }
                    other => panic!("expected two Sheets, got {other:?}"),
                }
                match &g.children[2] {
                    Node::Sheet(s) => {
                        assert_eq!(s.title, "B", "B must be unaffected, now at index 2")
                    }
                    other => panic!("expected Sheet B, got {other:?}"),
                }
            }
            other => panic!("expected Project, got {other:?}"),
        }
    }

    #[test]
    fn delete_requires_confirmation() {
        let mut state = two_sheets_in_a_group();

        let err = super::delete_node(&mut state, &[0, 0], false).unwrap_err();
        assert_eq!(err, TreeOpError::NotConfirmed);
        match state.node(&[0]).unwrap() {
            Node::Project(g) => assert_eq!(
                g.children.len(),
                2,
                "no deletion must occur without confirmation"
            ),
            other => panic!("expected Project, got {other:?}"),
        }

        super::delete_node(&mut state, &[0, 0], true).expect("confirmed delete must succeed");
        match state.node(&[0]).unwrap() {
            Node::Project(g) => assert_eq!(
                g.children.len(),
                1,
                "confirmed delete must actually remove the node"
            ),
            other => panic!("expected Project, got {other:?}"),
        }
    }
}
