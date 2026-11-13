//! DOCX importer (T-6-06, best-effort): `docx-rs`'s own read path -> AST.
//!
//! Scoped to the task's own representative set — headings, bold, italic, lists — not a general
//! DOCX reader. Known limitations (documented here, not silently wrong):
//! - Only `DocumentChild::Paragraph` is read; tables, bookmarks, and other document children are
//!   skipped (no confirmed Markdown-XL counterpart).
//! - Heading detection relies on the paragraph style id being literally `"HeadingN"` — exactly
//!   what this project's own DOCX exporter (`parrot_export::docx`) writes, not a general
//!   Word-style-name mapping (a document styled by a different tool, e.g. "Title" or a
//!   localized style name, won't be recognized as a heading).
//! - List detection is a heuristic over the flattened run text (`"• "` / `"<n>. "` prefixes) —
//!   again matching this project's own exporter's convention (plain text prefixes, not real
//!   Word numbering definitions), not a general list reader.
//! - Blockquote/divider/comment/codeblock paragraphs (the exporter's indent/font-based
//!   conventions) are not detected — they round-trip as plain paragraphs. Not in the task's own
//!   representative set.
//! - Bold+italic on the same run becomes `Strong(vec![Emph(...)])`; this project's own exporter
//!   never nests structurally-wrapped `Strong`/`Emph` in a way DOCX runs distinguish, so the
//!   wrapping order here is an arbitrary (but stable) choice.

use docx_rs::{read_docx, DocumentChild, ParagraphChild, ReaderError, RunChild};
use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocxImportError {
    #[error("docx read error: {0:?}")]
    Read(ReaderError),
}

pub fn import_docx(bytes: &[u8]) -> Result<SheetContent, DocxImportError> {
    let docx = read_docx(bytes).map_err(DocxImportError::Read)?;
    let paragraphs = docx
        .document
        .children
        .into_iter()
        .filter_map(|child| match child {
            DocumentChild::Paragraph(p) => Some(convert_paragraph(*p)),
            _ => None,
        })
        .collect();
    Ok(SheetContent::new(paragraphs))
}

fn convert_paragraph(p: docx_rs::Paragraph) -> Paragraph {
    let style_id = p.property.style.as_ref().map(|s| s.val.clone());
    let inlines: Vec<Inline> = p
        .children
        .into_iter()
        .filter_map(|child| match child {
            ParagraphChild::Run(run) => Some(convert_run(*run)),
            _ => None,
        })
        .collect();

    if let Some(n) = style_id
        .as_deref()
        .and_then(|s| s.strip_prefix("Heading"))
        .and_then(|n| n.parse::<u8>().ok())
    {
        return Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Heading(n),
                literal: format!("{} ", "#".repeat(n as usize)),
            }),
            inlines,
        };
    }

    let flat = flatten_text(&inlines);
    if let Some(rest) = flat.strip_prefix('\u{2022}') {
        return Paragraph {
            tag: Some(TagMark {
                kind: TagKind::UnorderedList,
                literal: "- ".to_string(),
            }),
            inlines: vec![Inline::Text(rest.trim_start().to_string())],
        };
    }
    if let Some(rest) = strip_ordered_prefix(&flat) {
        return Paragraph {
            tag: Some(TagMark {
                kind: TagKind::OrderedList,
                literal: "1. ".to_string(),
            }),
            inlines: vec![Inline::Text(rest)],
        };
    }

    Paragraph { tag: None, inlines }
}

/// Strips a `"<digits>. "` prefix (this project's own list-numbering convention), returning the
/// remainder if the whole paragraph is exactly one such prefix followed by plain text.
fn strip_ordered_prefix(s: &str) -> Option<String> {
    let dot = s.find(". ")?;
    let (digits, rest) = (&s[..dot], &s[dot + 2..]);
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        Some(rest.to_string())
    } else {
        None
    }
}

fn convert_run(run: docx_rs::Run) -> Inline {
    let bold = run.run_property.bold.is_some();
    let italic = run.run_property.italic.is_some();
    let text: String = run
        .children
        .into_iter()
        .filter_map(|c| match c {
            RunChild::Text(t) => Some(t.text),
            _ => None,
        })
        .collect();

    match (bold, italic) {
        (true, true) => Inline::Strong(vec![Inline::Emph(vec![Inline::Text(text)])]),
        (true, false) => Inline::Strong(vec![Inline::Text(text)]),
        (false, true) => Inline::Emph(vec![Inline::Text(text)]),
        (false, false) => Inline::Text(text),
    }
}

fn flatten_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(s) => s.clone(),
            Inline::Strong(c) | Inline::Emph(c) => flatten_text(c),
            _ => String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_export::docx::export_docx;

    fn representative() -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("Title".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Body with ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    Inline::Text(" and ".to_string()),
                    Inline::Emph(vec![Inline::Text("italic".to_string())]),
                    Inline::Text(".".to_string()),
                ],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::UnorderedList,
                    literal: "- ".to_string(),
                }),
                inlines: vec![Inline::Text("first item".to_string())],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::OrderedList,
                    literal: "1. ".to_string(),
                }),
                inlines: vec![Inline::Text("one".to_string())],
            },
        ])
    }

    #[test]
    fn import_then_export_matches_golden() {
        let bytes = export_docx(&representative()).expect("export must succeed");
        let imported = import_docx(&bytes).expect("import must succeed");

        let exported = parrot_export::mvp::export_markdown(&imported);
        let golden = include_str!("../../tests/fixtures/import/representative.docx.exported.md");
        assert_eq!(exported, golden);
    }
}
