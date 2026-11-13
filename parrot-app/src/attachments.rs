//! Attachments UI wiring (T-10-11): thin convenience wrappers over
//! `AppState::add_attachment`/`has_attachments`, per inventory §15's confirmed
//! `addFileAttachment:`/`addNoteAttachment:`.
//!
//! Unlike T-10-02/T-10-04's Goal/Keywords (deliberately left unpersisted — see their own
//! DECISIONS.md entries), this task's own Done-when explicitly requires a real save/reopen
//! round-trip, so `parrot_container::sheet` grew a real `Attachments.txt` sidecar file for it
//! (see that module's doc). The tests below exercise that real round-trip through
//! `AppState::save`/`open_library`, not an in-memory clone.

use crate::state::{AppState, StateError};
use parrot_core::Attachment;

pub fn attach_note(state: &mut AppState, path: &[usize], text: &str) -> Result<(), StateError> {
    state.add_attachment(
        path,
        Attachment::Note {
            text: text.to_string(),
        },
    )
}

pub fn attach_file(
    state: &mut AppState,
    path: &[usize],
    file_name: &str,
) -> Result<(), StateError> {
    state.add_attachment(
        path,
        Attachment::File {
            file_name: file_name.to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::Node;

    #[test]
    fn roundtrip_note_attachment() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("Library");

        let mut state = AppState::new();
        state.create_library(&path);
        state.create_project("P");
        state
            .create_sheet(&[0], "Untitled")
            .expect("create_sheet must succeed");
        attach_note(&mut state, &[0, 0], "Remember to revise the intro")
            .expect("attach_note must succeed");
        state.save().expect("save must succeed");

        let mut reopened = AppState::new();
        reopened
            .open_library(&path)
            .expect("open_library must succeed");
        match reopened.node(&[0, 0]).expect("sheet must exist") {
            Node::Sheet(s) => assert_eq!(
                s.attachments,
                vec![Attachment::Note {
                    text: "Remember to revise the intro".to_string()
                }]
            ),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_file_attachment() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("Library");

        let mut state = AppState::new();
        state.create_library(&path);
        state.create_project("P");
        state
            .create_sheet(&[0], "Untitled")
            .expect("create_sheet must succeed");
        attach_file(&mut state, &[0, 0], "reference.pdf").expect("attach_file must succeed");
        state.save().expect("save must succeed");

        let mut reopened = AppState::new();
        reopened
            .open_library(&path)
            .expect("open_library must succeed");
        match reopened.node(&[0, 0]).expect("sheet must exist") {
            Node::Sheet(s) => assert_eq!(
                s.attachments,
                vec![Attachment::File {
                    file_name: "reference.pdf".to_string()
                }]
            ),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn badge_reflects_presence() {
        let mut state = AppState::new();
        state.create_project("P");
        state
            .create_sheet(&[0], "Untitled")
            .expect("create_sheet must succeed");
        assert!(
            !state.has_attachments(&[0, 0]),
            "a fresh sheet has no attachments, so no badge"
        );

        attach_note(&mut state, &[0, 0], "a note").expect("attach_note must succeed");
        assert!(
            state.has_attachments(&[0, 0]),
            "the badge must appear once an attachment exists"
        );

        state
            .remove_attachment(&[0, 0], 0)
            .expect("remove_attachment must succeed");
        assert!(
            !state.has_attachments(&[0, 0]),
            "the badge must disappear once the last attachment is removed"
        );
    }
}
