//! ENEX (Evernote export) importer (T-6-04): XML parse via `quick-xml` -> AST.
//!
//! An `.enex` file is `<en-export><note>...</note>...</en-export>`; each note's `<content>` is
//! a CDATA-wrapped ENML document (`<en-note>...</en-note>`) — ENML is a restricted XHTML
//! dialect, so once the CDATA text is extracted, it's handed straight to `html::import_html`
//! rather than writing a second markup walker. The note's `<title>` becomes a synthesized
//! `Heading(1)` paragraph prepended to that note's content (not itself part of ENML — this
//! importer's own choice of where a title naturally belongs in the resulting AST).

use crate::html::import_html;
use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnexError {
    #[error("xml error: {0}")]
    Xml(#[from] quick_xml::Error),
}

/// One `SheetContent` per `<note>` in the export, in document order.
pub fn import_enex(enex: &str) -> Result<Vec<SheetContent>, EnexError> {
    let mut reader = Reader::from_str(enex);
    reader.trim_text(false);

    let mut notes = Vec::new();
    let mut title: Option<String> = None;
    let mut content: Option<String> = None;
    let mut in_title = false;
    let mut in_content = false;

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == b"note" => {
                title = None;
                content = None;
            }
            Event::Start(e) if e.local_name().as_ref() == b"title" => in_title = true,
            Event::End(e) if e.local_name().as_ref() == b"title" => in_title = false,
            Event::Start(e) if e.local_name().as_ref() == b"content" => in_content = true,
            Event::End(e) if e.local_name().as_ref() == b"content" => in_content = false,
            Event::Text(t) if in_title => {
                title = Some(t.unescape()?.into_owned());
            }
            Event::CData(c) if in_content => {
                content = Some(String::from_utf8_lossy(&c.into_inner()).into_owned());
            }
            Event::End(e) if e.local_name().as_ref() == b"note" => {
                notes.push(build_note(title.take(), content.take()));
            }
            _ => {}
        }
    }

    Ok(notes)
}

fn build_note(title: Option<String>, content: Option<String>) -> SheetContent {
    let mut paragraphs = Vec::new();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        paragraphs.push(Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Heading(1),
                literal: "# ".to_string(),
            }),
            inlines: vec![Inline::Text(title)],
        });
    }
    if let Some(enml) = content {
        paragraphs.extend(import_html(&enml).paragraphs);
    }
    SheetContent::new(paragraphs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_then_export_matches_golden() {
        let enex = include_str!("../../tests/fixtures/import/representative.enex");
        let notes = import_enex(enex).expect("representative fixture must parse");
        assert_eq!(notes.len(), 1, "fixture has exactly one note");

        let exported = parrot_export::mvp::export_markdown(&notes[0]);
        let golden = include_str!("../../tests/fixtures/import/representative.enex.exported.md");
        assert_eq!(exported, golden);
    }
}
