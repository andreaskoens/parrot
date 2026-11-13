//! Sheet-level Attachments (T-10-11): per inventory §15's confirmed `addFileAttachment:`/
//! `addNoteAttachment:`, a note or file attached to a *sheet as a whole* — distinct from the
//! inline `annotation` tag (T-1-01, a span inside the document's own AST content) and from
//! Keywords (T-10-04, a tag set, not content of its own).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attachment {
    Note {
        text: String,
    },
    /// Records which file was attached by name only — this rewrite doesn't copy arbitrary file
    /// bytes into the library (no confirmed on-disk schema for that, and it's real scope on its
    /// own); same "name reference, not bytes" stance `Preferences`'s font substitutions take.
    File {
        file_name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_and_file_variants_are_distinct() {
        let note = Attachment::Note {
            text: "remember this".to_string(),
        };
        let file = Attachment::File {
            file_name: "reference.pdf".to_string(),
        };
        assert_ne!(note, file);
    }
}
