//! File > Import (wires the `parrot-import` crate into the app): picks the importer from the
//! file's extension and adds one new sheet per imported document under `parent`, titled after
//! the file. Evernote `.enex` exports hold several notes and become several sheets.

use crate::os::FileFilter;
use crate::state::{AppState, StateError};
use parrot_core::SheetContent;
use std::path::Path;
use thiserror::Error;

/// The dialog filters for File > Import…, every supported extension first.
pub const IMPORT_FILTERS: &[FileFilter] = &[
    (
        "All supported",
        &[
            "md", "markdown", "txt", "text", "html", "htm", "rtf", "docx", "enex",
        ],
    ),
    ("Markdown", &["md", "markdown"]),
    ("Plain text", &["txt", "text"]),
    ("HTML", &["html", "htm"]),
    ("Rich Text", &["rtf"]),
    ("Word", &["docx"]),
    ("Evernote", &["enex"]),
];

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("could not read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported file type: .{0}")]
    Unsupported(String),
    #[error("could not import: {0}")]
    Format(String),
    #[error(transparent)]
    State(#[from] StateError),
}

/// Reads `bytes` as text: UTF-8 (lossy, so one bad byte doesn't refuse the file), BOM dropped.
fn text(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.strip_prefix('\u{feff}').unwrap_or(&s).to_string()
}

/// Parses `path` into one or more documents, by extension (or as a TextBundle directory).
pub fn read_documents(path: &Path) -> Result<Vec<SheetContent>, ImportError> {
    if path.is_dir() {
        return parrot_import::textbundle::import_textbundle(path)
            .map(|c| vec![c])
            .map_err(|e| ImportError::Format(e.to_string()));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let bytes = std::fs::read(path)?;
    Ok(match ext.as_str() {
        "md" | "markdown" => vec![parrot_import::markdown::import_markdown(&text(&bytes))],
        "txt" | "text" => vec![parrot_import::plaintext::import_plaintext(&text(&bytes))],
        "html" | "htm" => vec![parrot_import::html::import_html(&text(&bytes))],
        "rtf" => vec![parrot_import::rtf::import_rtf(&text(&bytes))],
        "docx" => vec![parrot_import::docx::import_docx(&bytes)
            .map_err(|e| ImportError::Format(e.to_string()))?],
        "enex" => parrot_import::enex::import_enex(&text(&bytes))
            .map_err(|e| ImportError::Format(e.to_string()))?,
        _ => return Err(ImportError::Unsupported(ext)),
    })
}

/// Imports `path` as new sheets at the end of `parent`; returns how many were added.
pub fn import_file(
    state: &mut AppState,
    parent: &[usize],
    path: &Path,
) -> Result<usize, ImportError> {
    let documents = read_documents(path)?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Imported".to_string());
    let count = documents.len();
    for (i, content) in documents.into_iter().enumerate() {
        let title = if count > 1 {
            format!("{stem} {}", i + 1)
        } else {
            stem.clone()
        };
        state.create_sheet(parent, &title)?;
        let mut sheet_path = parent.to_vec();
        sheet_path.push(state.child_count(parent)? - 1);
        state.set_sheet_content(&sheet_path, content)?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::Node;

    fn sheet<'a>(state: &'a AppState, path: &[usize]) -> &'a parrot_core::Sheet {
        match state.node(path) {
            Ok(Node::Sheet(s)) => s,
            other => panic!("expected a sheet, got {other:?}"),
        }
    }

    #[test]
    fn markdown_and_text_become_titled_sheets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let md = dir.path().join("Chapter One.md");
        std::fs::write(&md, "\u{feff}# Heading\n\nBody **bold**.").expect("write");
        let txt = dir.path().join("notes.TXT");
        std::fs::write(&txt, b"plain \xff text").expect("write");

        let mut state = AppState::new();
        state.create_project("G");
        assert_eq!(import_file(&mut state, &[0], &md).expect("md"), 1);
        assert_eq!(import_file(&mut state, &[0], &txt).expect("txt"), 1);

        let first = sheet(&state, &[0, 0]);
        assert_eq!(first.title, "Chapter One");
        assert!(first.content.plain_text().contains("Heading"));
        let second = sheet(&state, &[0, 1]);
        assert_eq!(second.title, "notes");
        assert!(second.content.plain_text().contains("plain"));
    }

    #[test]
    fn enex_with_several_notes_becomes_numbered_sheets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let enex = dir.path().join("Export.enex");
        let note = |t: &str| {
            format!(
                "<note><title>{t}</title><content><![CDATA[<en-note><div>{t} body</div></en-note>]]></content></note>"
            )
        };
        std::fs::write(
            &enex,
            format!(
                "<?xml version=\"1.0\"?><en-export>{}{}</en-export>",
                note("A"),
                note("B")
            ),
        )
        .expect("write");
        let mut state = AppState::new();
        assert!(matches!(
            import_file(&mut state, &[], &enex),
            Err(ImportError::State(StateError::OutsideProject))
        ));
        let project = state.create_project("P");
        assert_eq!(import_file(&mut state, &project, &enex).expect("enex"), 2);
        assert_eq!(sheet(&state, &[0, 0]).title, "Export 1");
        assert_eq!(sheet(&state, &[0, 1]).title, "Export 2");
    }

    #[test]
    fn unsupported_and_missing_files_are_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let odd = dir.path().join("x.bin");
        std::fs::write(&odd, b"x").expect("write");
        let mut state = AppState::new();
        assert!(matches!(
            import_file(&mut state, &[], &odd),
            Err(ImportError::Unsupported(e)) if e == "bin"
        ));
        assert!(matches!(
            import_file(&mut state, &[], &dir.path().join("missing.md")),
            Err(ImportError::Io(_))
        ));
        assert!(state.library.children.is_empty());
    }
}
