//! TextBundle importer (T-6-05): reads the T-5-09 `.textbundle` structure back into AST.
//!
//! `text.markdown` is plain Markdown, so this is a thin wrapper over `markdown::import_markdown`
//! — no separate parser. `info.json` isn't read: nothing in this project's own AST model has a
//! slot for TextBundle's metadata (creator identifier, source URL), so there'd be nothing to do
//! with it.

use crate::markdown::import_markdown;
use parrot_core::SheetContent;
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TextBundleImportError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub fn import_textbundle(path: &Path) -> Result<SheetContent, TextBundleImportError> {
    let markdown = fs::read_to_string(path.join("text.markdown"))?;
    Ok(import_markdown(&markdown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, TagKind, TagMark};
    use parrot_export::textbundle::export_textbundle;

    fn representative() -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(2),
                    literal: "## ".to_string(),
                }),
                inlines: vec![Inline::Text("Title".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Body with ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    Inline::Text(" and a ".to_string()),
                    Inline::Link {
                        url: "https://example.com".to_string(),
                        content: vec![Inline::Text("link".to_string())],
                    },
                    Inline::Text(".".to_string()),
                ],
            },
        ])
    }

    /// "Lossless" here means export->import->export is a stable fixed point: re-exporting the
    /// imported AST reproduces the exact `text.markdown` bytes the bundle was written with —
    /// not that the re-imported `SheetContent` is `PartialEq` to the original (Markdown is a
    /// lossy AST *representation*, e.g. `TagMark.literal` round-trips through its own text, not
    /// its original struct value).
    #[test]
    fn lossless_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let bundle_path = dir.path().join("Note.textbundle");
        export_textbundle(&bundle_path, &representative()).expect("export must succeed");

        let imported = import_textbundle(&bundle_path).expect("import must succeed");
        let reexported = parrot_export::mvp::export_markdown(&imported);

        let original_markdown = fs::read_to_string(bundle_path.join("text.markdown")).unwrap();
        assert_eq!(reexported, original_markdown);
    }
}
