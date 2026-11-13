//! Export UI wiring (T-10-01): `parrot-export` is a real `parrot-app` dependency now, not just
//! a `dev-dependency` for the e2e test — this module is the one place that knows about every
//! backend, so `ui.rs` only needs an `ExportFormat` and a destination path. Per inventory §15's
//! confirmed "Export"/"Quick Export" entry points.
//!
//! `mvp::export_markdown`/`mvp::export_plain_text` are used (not `conversion::` directly) because
//! `mvp` is `parrot-export`'s own documented "stable public path" for those two formats (see its
//! module doc) — this module follows that same convention rather than reaching past it.

use parrot_core::SheetContent;
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    Markdown,
    PlainText,
    Html,
    Rtf,
    Docx,
    Epub,
    /// The modal's initial format, matching §8's "PDF ▾" default.
    #[default]
    Pdf,
    TextBundle,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 8] = [
        ExportFormat::Markdown,
        ExportFormat::PlainText,
        ExportFormat::Html,
        ExportFormat::Rtf,
        ExportFormat::Docx,
        ExportFormat::Epub,
        ExportFormat::Pdf,
        ExportFormat::TextBundle,
    ];

    /// The formats the Export dialog offers. The other backends stay available to code and
    /// Quick Export, but the dialog keeps to the two with a faithful live preview.
    pub const OFFERED: [ExportFormat; 2] = [ExportFormat::Markdown, ExportFormat::Pdf];

    /// Label for the Export dialog's format picker.
    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Markdown => "Markdown",
            ExportFormat::PlainText => "Plain Text",
            ExportFormat::Html => "HTML",
            ExportFormat::Rtf => "RTF",
            ExportFormat::Docx => "Word (.docx)",
            ExportFormat::Epub => "EPUB",
            ExportFormat::Pdf => "PDF",
            ExportFormat::TextBundle => "TextBundle",
        }
    }

    /// Default file extension/suffix, for pre-filling a destination path in the UI.
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Markdown => "md",
            ExportFormat::PlainText => "txt",
            ExportFormat::Html => "html",
            ExportFormat::Rtf => "rtf",
            ExportFormat::Docx => "docx",
            ExportFormat::Epub => "epub",
            ExportFormat::Pdf => "pdf",
            ExportFormat::TextBundle => "textbundle",
        }
    }
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    /// Wraps whichever backend-specific error type the chosen format's exporter returns
    /// (`DocxExportError`, `TextBundleError`, ...) — stringified rather than named one by one, so
    /// this module doesn't need to depend on every backend's error type directly.
    #[error("export error: {0}")]
    Backend(String),
}

/// Page sizes offered by the export modal (T-12-10, ui-description §8). Only the PDF backend
/// is paginated; every other format ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    #[default]
    A4,
    A5,
    Letter,
    Legal,
}

impl PageSize {
    pub const ALL: [PageSize; 4] = [
        PageSize::A4,
        PageSize::A5,
        PageSize::Letter,
        PageSize::Legal,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            PageSize::A4 => "A4",
            PageSize::A5 => "A5",
            PageSize::Letter => "Letter",
            PageSize::Legal => "Legal",
        }
    }

    /// `(width, height)` in millimetres.
    pub fn mm(&self) -> (f32, f32) {
        match self {
            PageSize::A4 => parrot_export::pdf::A4_MM,
            PageSize::A5 => (148.0, 210.0),
            PageSize::Letter => (215.9, 279.4),
            PageSize::Legal => (215.9, 355.6),
        }
    }
}

/// Per-export settings beyond the format itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportOptions {
    pub page_size: PageSize,
}

/// Dispatches `content` to `format`'s backend with default [`ExportOptions`] (A4).
pub fn export_sheet(
    format: ExportFormat,
    content: &SheetContent,
    dest: &Path,
) -> Result<(), ExportError> {
    export_sheet_with(format, content, dest, ExportOptions::default())
}

/// Dispatches `content` to `format`'s backend, writing the result to `dest`.
///
/// TextBundle writes its own directory package straight to `dest` (it isn't a flat byte stream,
/// per `parrot_export::textbundle`'s own atomic directory-rename convention) — every other
/// format writes exactly one file at `dest`.
pub fn export_sheet_with(
    format: ExportFormat,
    content: &SheetContent,
    dest: &Path,
    options: ExportOptions,
) -> Result<(), ExportError> {
    match format {
        ExportFormat::Markdown => {
            fs::write(dest, parrot_export::mvp::export_markdown(content))?;
        }
        ExportFormat::PlainText => {
            fs::write(dest, parrot_export::mvp::export_plain_text(content))?;
        }
        ExportFormat::Html => {
            fs::write(dest, parrot_export::html::export_html(content))?;
        }
        ExportFormat::Rtf => {
            fs::write(dest, parrot_export::rtf::export_rtf(content))?;
        }
        ExportFormat::Docx => {
            let bytes = parrot_export::docx::export_docx(content)
                .map_err(|e| ExportError::Backend(e.to_string()))?;
            fs::write(dest, bytes)?;
        }
        ExportFormat::Epub => {
            let bytes = parrot_export::epub::export_epub(content)
                .map_err(|e| ExportError::Backend(e.to_string()))?;
            fs::write(dest, bytes)?;
        }
        ExportFormat::Pdf => {
            fs::write(
                dest,
                parrot_export::pdf::export_pdf_sized(content, options.page_size.mm()),
            )?;
        }
        ExportFormat::TextBundle => {
            parrot_export::textbundle::export_textbundle(dest, content)
                .map_err(|e| ExportError::Backend(e.to_string()))?;
        }
    }
    Ok(())
}

/// Several sheets as one document for export: their paragraphs in order, so each sheet
/// starts a new paragraph (and headings stay headings).
pub fn glue(contents: &[&SheetContent]) -> SheetContent {
    SheetContent::new(
        contents
            .iter()
            .flat_map(|c| c.paragraphs.iter().cloned())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, TagKind, TagMark};

    /// A small representative sheet — doesn't need to cover every tag (each backend already has
    /// its own fixture coverage in `parrot-export`); this just needs to exercise dispatch.
    fn sample() -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(2),
                    literal: "## ".to_string(),
                }),
                inlines: vec![Inline::Text("Export test".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Some ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    Inline::Text(" text.".to_string()),
                ],
            },
        ])
    }

    #[test]
    fn glued_sheets_export_in_order() {
        let a = sample();
        let b = SheetContent::new(vec![parrot_core::Paragraph {
            tag: None,
            inlines: vec![parrot_core::Inline::Text("Second sheet".to_string())],
        }]);
        let glued = glue(&[&a, &b]);
        assert_eq!(glued.paragraphs.len(), a.paragraphs.len() + 1);
        let text = parrot_export::mvp::export_plain_text(&glued);
        let first = a.paragraphs[0].plain_text();
        assert!(text.find(&first).expect("first") < text.find("Second sheet").expect("second"));
    }

    #[test]
    fn markdown_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.md");
        export_sheet(ExportFormat::Markdown, &sample(), &dest).expect("export must succeed");
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            parrot_export::mvp::export_markdown(&sample())
        );
    }

    #[test]
    fn plain_text_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.txt");
        export_sheet(ExportFormat::PlainText, &sample(), &dest).expect("export must succeed");
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            parrot_export::mvp::export_plain_text(&sample())
        );
    }

    #[test]
    fn html_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.html");
        export_sheet(ExportFormat::Html, &sample(), &dest).expect("export must succeed");
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            parrot_export::html::export_html(&sample())
        );
    }

    #[test]
    fn rtf_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.rtf");
        export_sheet(ExportFormat::Rtf, &sample(), &dest).expect("export must succeed");
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            parrot_export::rtf::export_rtf(&sample())
        );
    }

    #[test]
    fn docx_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.docx");
        export_sheet(ExportFormat::Docx, &sample(), &dest).expect("export must succeed");
        assert_eq!(
            fs::read(&dest).unwrap(),
            parrot_export::docx::export_docx(&sample()).unwrap()
        );
    }

    #[test]
    fn epub_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.epub");
        export_sheet(ExportFormat::Epub, &sample(), &dest).expect("export must succeed");
        assert_eq!(
            fs::read(&dest).unwrap(),
            parrot_export::epub::export_epub(&sample()).unwrap()
        );
    }

    /// The written `.md` file parses back into the very sheet that was exported.
    #[test]
    fn exported_markdown_reads_back_as_the_same_sheet() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.md");
        export_sheet(ExportFormat::Markdown, &sample(), &dest).expect("export must succeed");
        let file = fs::read_to_string(&dest).unwrap();
        // The file ends with a newline, as text files do; the sheet doesn't.
        let back = crate::editor::buffer_to_content(file.strip_suffix('\n').unwrap_or(&file));
        assert_eq!(back.paragraphs, sample().paragraphs);
    }

    /// The written `.pdf` is a valid PDF at the chosen page size whose text is every
    /// paragraph's, in order — not just a file with empty pages.
    #[test]
    fn exported_pdf_holds_every_paragraph_at_the_page_size() {
        use printpdf::{Op, PdfDocument, PdfParseOptions, TextItem};

        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.pdf");
        let options = ExportOptions {
            page_size: PageSize::Letter,
        };
        export_sheet_with(ExportFormat::Pdf, &sample(), &dest, options)
            .expect("export must succeed");
        let bytes = fs::read(&dest).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let doc = PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut Vec::new())
            .expect("output must be a valid PDF");
        assert_eq!(doc.pages.len(), 1);
        let width_pt = doc.pages[0].media_box.width.0;
        assert!(
            (width_pt - 215.9 * 72.0 / 25.4).abs() < 1.0,
            "width {width_pt}"
        );
        let text: Vec<String> = doc.pages[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::WriteText { items, .. } | Op::WriteTextBuiltinFont { items, .. } => Some(
                    items
                        .iter()
                        .filter_map(|i| match i {
                            TextItem::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .collect();
        let expected: Vec<String> = sample().paragraphs.iter().map(|p| p.plain_text()).collect();
        assert_eq!(text, expected);
    }

    #[test]
    fn text_bundle_dispatches_to_corresponding_exporter() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let dest = dir.path().join("out.textbundle");
        export_sheet(ExportFormat::TextBundle, &sample(), &dest).expect("export must succeed");

        let direct = dir.path().join("direct.textbundle");
        parrot_export::textbundle::export_textbundle(&direct, &sample())
            .expect("direct export must succeed");

        assert_eq!(
            fs::read_to_string(dest.join("text.markdown")).unwrap(),
            fs::read_to_string(direct.join("text.markdown")).unwrap()
        );
        assert_eq!(
            fs::read_to_string(dest.join("info.json")).unwrap(),
            fs::read_to_string(direct.join("info.json")).unwrap()
        );
    }
}
