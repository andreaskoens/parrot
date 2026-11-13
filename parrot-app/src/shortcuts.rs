//! Keyboard shortcuts (T-2-06): bold/italic toggle, save, new sheet, navigate tree. This is a
//! new, deliberately chosen set for this project — Parrot' own `ShortcutRecorder`
//! customization system is explicitly out of scope (feature inventory Tier 4); these bindings
//! are fixed, not user-remappable, at least for now.
//!
//! Tested at the `AppState`/`EditorState` level — what action each binding invokes and its
//! effect — not via simulated key events, so the suite stays headless.

use crate::editor_state::EditorState;
use crate::state::{AppState, NodePath, StateError};
use parrot_core::Node;

/// This project's own shortcut bindings. Documented here as the single source of truth; `main`
/// wires each to its egui `Key`/modifier combination (not tested here — that's a UI-framework
/// binding, not application logic).
pub const BINDINGS: &[(&str, &str)] = &[
    ("Cmd/Ctrl+B", "toggle bold on the current selection"),
    ("Cmd/Ctrl+I", "toggle italic on the current selection"),
    ("Cmd/Ctrl+S", "save the current library"),
    ("Cmd/Ctrl+N", "create a new sheet at the library root"),
    ("Cmd/Ctrl+Down", "navigate to the next sheet in tree order"),
    (
        "Cmd/Ctrl+Up",
        "navigate to the previous sheet in tree order",
    ),
];

/// Wraps (or, if already exactly wrapped, unwraps) the current selection in `marker` — the
/// shared logic behind bold (`**`) and italic (`*`) toggling. A no-op on an empty selection.
fn toggle_wrap(editor: &mut EditorState, marker: &str) {
    let (start, end) = editor.selection().range();
    if start == end {
        return;
    }
    let chars: Vec<char> = editor.text().chars().collect();
    let marker_chars: Vec<char> = marker.chars().collect();
    let m = marker_chars.len();

    let already_wrapped = start >= m
        && end + m <= chars.len()
        && chars[start - m..start] == marker_chars[..]
        && chars[end..end + m] == marker_chars[..];

    let selected: String = chars[start..end].iter().collect();
    if already_wrapped {
        editor.set_selection(start - m, end + m);
        editor.replace_selection(&selected);
        editor.set_selection(start - m, end - m);
    } else {
        editor.replace_selection(&format!("{marker}{selected}{marker}"));
        editor.set_selection(start + m, end + m);
    }
}

pub fn toggle_bold(editor: &mut EditorState) {
    toggle_wrap(editor, "**");
}

pub fn toggle_italic(editor: &mut EditorState) {
    toggle_wrap(editor, "*");
}

pub fn save(state: &AppState) -> Result<(), StateError> {
    state.save()
}

pub fn new_sheet(state: &mut AppState, parent: &[usize]) -> Result<(), StateError> {
    state.create_sheet(parent, "Untitled")
}

/// Every Sheet's path in the tree, in display order (depth-first, matching the library panel).
fn flatten_sheet_paths(nodes: &[Node], prefix: &[usize], out: &mut Vec<NodePath>) {
    for (i, node) in nodes.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push(i);
        match node {
            Node::Sheet(_) => out.push(path),
            Node::Group(g) => flatten_sheet_paths(&g.children, &path, out),
            Node::Project(p) => flatten_sheet_paths(&p.children, &path, out),
            Node::Filter(_) => {}
        }
    }
}

/// The Sheet immediately after `current` in tree order, if any.
pub fn navigate_next_sheet(state: &AppState, current: &NodePath) -> Option<NodePath> {
    let paths = all_sheet_paths(state);
    let i = paths.iter().position(|p| p == current)?;
    paths.get(i + 1).cloned()
}

/// The Sheet immediately before `current` in tree order, if any.
pub fn navigate_previous_sheet(state: &AppState, current: &NodePath) -> Option<NodePath> {
    let paths = all_sheet_paths(state);
    let i = paths.iter().position(|p| p == current)?;
    i.checked_sub(1).and_then(|j| paths.get(j).cloned())
}

fn all_sheet_paths(state: &AppState) -> Vec<NodePath> {
    let mut paths = Vec::new();
    flatten_sheet_paths(&state.library.children, &[], &mut paths);
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_binding_invokes_expected_action() {
        // Cmd/Ctrl+B: toggle bold wraps, then unwraps, the selection.
        let mut editor = EditorState::new("Hello world");
        editor.set_selection(6, 11);
        toggle_bold(&mut editor);
        assert_eq!(editor.text(), "Hello **world**");
        toggle_bold(&mut editor);
        assert_eq!(editor.text(), "Hello world");

        // Cmd/Ctrl+I: toggle italic, same shape with a single marker.
        let mut editor = EditorState::new("Hello world");
        editor.set_selection(6, 11);
        toggle_italic(&mut editor);
        assert_eq!(editor.text(), "Hello *world*");
        toggle_italic(&mut editor);
        assert_eq!(editor.text(), "Hello world");

        // Cmd/Ctrl+S: save, after a library exists.
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let mut state = AppState::new();
        state.create_library(dir.path());
        assert_eq!(save(&state), Ok(()));
        // Saving with no library at all is the documented NoLibraryPath error, not a panic.
        assert_eq!(save(&AppState::new()), Err(StateError::NoLibraryPath));

        // Cmd/Ctrl+N: new sheet in the current project; never at the library root.
        let mut state = AppState::new();
        assert_eq!(new_sheet(&mut state, &[]), Err(StateError::OutsideProject));
        let project = state.create_project("P");
        new_sheet(&mut state, &project).expect("new_sheet must succeed");
        assert_eq!(state.child_count(&project), Ok(1));

        // Cmd/Ctrl+Down / Cmd/Ctrl+Up: navigate next/previous sheet in tree order.
        let mut state = AppState::new();
        state.create_project("P");
        state
            .create_sheet(&[0], "A")
            .expect("create_sheet must succeed");
        state
            .create_sheet(&[0], "B")
            .expect("create_sheet must succeed");
        state
            .create_sheet(&[0], "C")
            .expect("create_sheet must succeed");

        assert_eq!(navigate_next_sheet(&state, &vec![0, 0]), Some(vec![0, 1]));
        assert_eq!(navigate_next_sheet(&state, &vec![0, 1]), Some(vec![0, 2]));
        assert_eq!(navigate_next_sheet(&state, &vec![0, 2]), None);
        assert_eq!(
            navigate_previous_sheet(&state, &vec![0, 1]),
            Some(vec![0, 0])
        );
        assert_eq!(navigate_previous_sheet(&state, &vec![0, 0]), None);
    }
}
