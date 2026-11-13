//! Quick Export modal (T-12-10, ui-description §8/§12.7): format, preview theme, style and
//! page-size settings, a live preview, and the two export entry points — the modal's Export
//! button ([`ExportModal::export`]) and the toolbar's Quick Export ([`ExportModal::quick_export`],
//! last-used format and folder, no modal). Both write through `export_ui::export_sheet_with`,
//! the same dispatch T-10-01 built.

use crate::export_ui::{export_sheet_with, ExportError, ExportFormat, ExportOptions, PageSize};
use crate::state::NodePath;
use parrot_core::SheetContent;
use parrot_export::pdf::PdfLine;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// §8 sun/moon toggle: how the preview page is drawn. Doesn't change the exported file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewTheme {
    #[default]
    Light,
    Dark,
}

impl PreviewTheme {
    pub fn toggled(self) -> Self {
        match self {
            PreviewTheme::Light => PreviewTheme::Dark,
            PreviewTheme::Dark => PreviewTheme::Light,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            PreviewTheme::Light => crate::icons::LIGHT,
            PreviewTheme::Dark => crate::icons::DARK,
        }
    }
}

/// Everything the preview is rendered from — when any of these changes, the preview changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewInput {
    pub format: ExportFormat,
    pub page_size: PageSize,
    pub style_name: Option<String>,
    pub theme: PreviewTheme,
}

/// What the preview shows: text formats their exact output, paged formats the PDF backend's
/// own layout (same wrapping, sizes and page breaks as the written file).
#[derive(Debug, Clone, PartialEq)]
pub enum PreviewBody {
    Text(String),
    Pages(Vec<Vec<PdfLine>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    pub input: PreviewInput,
    pub body: PreviewBody,
}

#[derive(Debug, Error)]
pub enum QuickExportError {
    /// Quick Export repeats the last export; before the first one there's nothing to repeat,
    /// so the UI opens the full modal instead.
    #[error("no previous export to repeat — use Export… first")]
    NoPreviousExport,
    #[error(transparent)]
    Export(#[from] ExportError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastExport {
    pub format: ExportFormat,
    pub page_size: PageSize,
    pub folder: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ExportModal {
    pub open: bool,
    /// The sheets being exported, glued together in this order.
    pub targets: Vec<NodePath>,
    /// The export's name: the sheet's title, or the selection's for several sheets.
    pub title: String,
    pub format: ExportFormat,
    pub theme: PreviewTheme,
    pub page_size: PageSize,
    pub style_name: Option<String>,
    pub destination: String,
    pub last_used: Option<LastExport>,
}

impl ExportModal {
    /// Opens the modal for the sheets at `targets`, pre-filling the destination as
    /// `<folder>/<title>.<ext>` — the last export's folder, else `default_folder`.
    pub fn open_for(&mut self, targets: Vec<NodePath>, title: &str, default_folder: Option<&Path>) {
        if let Some(last) = &self.last_used {
            self.format = last.format;
            self.page_size = last.page_size;
        }
        let folder = self
            .last_used
            .as_ref()
            .map(|l| l.folder.clone())
            .or_else(|| default_folder.map(Path::to_path_buf))
            .unwrap_or_default();
        self.destination = destination_in(&folder, title, self.format)
            .display()
            .to_string();
        self.targets = targets;
        self.title = title.to_string();
        self.open = true;
    }

    /// Changes the format, swapping the destination's extension to match.
    pub fn set_format(&mut self, format: ExportFormat) {
        self.format = format;
        if !self.destination.is_empty() {
            self.destination = Path::new(&self.destination)
                .with_extension(format.extension())
                .display()
                .to_string();
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn preview_input(&self) -> PreviewInput {
        PreviewInput {
            format: self.format,
            page_size: self.page_size,
            style_name: self.style_name.clone(),
            theme: self.theme,
        }
    }

    /// Renders the preview for `content` from [`Self::preview_input`]. Paged formats use the
    /// PDF backend's own pagination (`parrot_export::pdf::page_lines`), so the preview breaks
    /// pages where the PDF does; text formats show the exact text that would be written.
    pub fn preview(&self, content: &SheetContent) -> Preview {
        let input = self.preview_input();
        let text = match input.format {
            ExportFormat::Markdown => Some(parrot_export::mvp::export_markdown(content)),
            ExportFormat::PlainText => Some(parrot_export::mvp::export_plain_text(content)),
            ExportFormat::Html => Some(parrot_export::html::export_html(content)),
            ExportFormat::Rtf => Some(parrot_export::rtf::export_rtf(content)),
            ExportFormat::Pdf
            | ExportFormat::Docx
            | ExportFormat::Epub
            | ExportFormat::TextBundle => None,
        };
        let body = match text {
            Some(text) => PreviewBody::Text(text),
            None => PreviewBody::Pages(parrot_export::pdf::page_lines(
                content,
                input.page_size.mm(),
            )),
        };
        Preview { input, body }
    }

    fn options(&self) -> ExportOptions {
        ExportOptions {
            page_size: self.page_size,
        }
    }

    /// The modal's Export button: writes `content` to the destination with the chosen format
    /// and page size, remembers them for Quick Export, and closes the modal.
    pub fn export(&mut self, content: &SheetContent) -> Result<PathBuf, ExportError> {
        let dest = PathBuf::from(&self.destination);
        export_sheet_with(self.format, content, &dest, self.options())?;
        self.last_used = Some(LastExport {
            format: self.format,
            page_size: self.page_size,
            folder: dest.parent().map(Path::to_path_buf).unwrap_or_default(),
        });
        self.open = false;
        Ok(dest)
    }

    /// The toolbar's Quick Export: repeats the last export's format, page size and folder for
    /// the sheet titled `title`, without opening the modal.
    pub fn quick_export(
        &mut self,
        content: &SheetContent,
        title: &str,
    ) -> Result<PathBuf, QuickExportError> {
        let last = self
            .last_used
            .clone()
            .ok_or(QuickExportError::NoPreviousExport)?;
        let dest = destination_in(&last.folder, title, last.format);
        export_sheet_with(
            last.format,
            content,
            &dest,
            ExportOptions {
                page_size: last.page_size,
            },
        )?;
        Ok(dest)
    }
}

/// `<folder>/<title>.<ext>`, with characters Windows forbids in file names replaced.
pub fn destination_in(folder: &Path, title: &str, format: ExportFormat) -> PathBuf {
    let mut stem: String = title
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect();
    if stem.is_empty() {
        stem = "Untitled".to_string();
    }
    folder.join(format!("{stem}.{}", format.extension()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_ui::export_sheet;
    use parrot_core::{Inline, Paragraph, TagKind, TagMark};

    fn sample() -> SheetContent {
        SheetContent::new(
            std::iter::once(Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("Export".to_string())],
            })
            .chain((0..70).map(|i| Paragraph {
                tag: None,
                inlines: vec![Inline::Text(format!("Line {i}"))],
            }))
            .collect(),
        )
    }

    /// Every file under `path` (a file, or a TextBundle directory), as (relative name, bytes).
    fn snapshot(path: &Path) -> Vec<(String, Vec<u8>)> {
        if path.is_file() {
            let bytes = std::fs::read(path).expect("read export");
            // printpdf stamps every save with a random document ID and timestamps, so PDFs are
            // compared by their parsed page content instead of raw bytes.
            if path.extension().is_some_and(|e| e == "pdf") {
                let mut warnings = Vec::new();
                let doc = printpdf::PdfDocument::parse(
                    &bytes,
                    &printpdf::PdfParseOptions::default(),
                    &mut warnings,
                )
                .expect("parse exported PDF");
                let pages = format!("{:?}", doc.pages.iter().map(|p| &p.ops).collect::<Vec<_>>());
                return vec![("pages".to_string(), pages.into_bytes())];
            }
            return vec![(String::new(), bytes)];
        }
        let mut out = Vec::new();
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read_dir") {
                let p = entry.expect("entry").path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(path).expect("prefix").display().to_string();
                    out.push((rel, std::fs::read(&p).expect("read")));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn modal_export_matches_direct_dispatch_per_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let content = sample();
        for format in ExportFormat::ALL {
            let mut modal = ExportModal::default();
            modal.open_for(vec![vec![0]], "Modal", Some(&dir.path().join("modal")));
            std::fs::create_dir_all(dir.path().join("modal")).expect("mkdir");
            modal.set_format(format);
            let written = modal.export(&content).expect("modal export");
            assert!(!modal.open, "Export must close the modal");

            let direct = dir.path().join(format!("direct.{}", format.extension()));
            export_sheet(format, &content, &direct).expect("direct export");
            assert_eq!(
                snapshot(&written),
                snapshot(&direct),
                "{} output differs between modal and direct dispatch",
                format.label()
            );
        }
    }

    #[test]
    fn style_and_page_size_changes_update_preview_input() {
        let content = sample();
        let mut modal = ExportModal::default();
        let pages = |p: &Preview| match &p.body {
            PreviewBody::Pages(pages) => pages.clone(),
            PreviewBody::Text(_) => panic!("PDF previews are paged"),
        };
        let a4 = modal.preview(&content);

        modal.style_name = Some("Compact".to_string());
        let styled = modal.preview(&content);
        assert_ne!(styled.input, a4.input);
        assert_eq!(styled.input.style_name.as_deref(), Some("Compact"));

        modal.page_size = PageSize::A5;
        let a5 = modal.preview(&content);
        assert_eq!(a5.input.page_size, PageSize::A5);
        assert!(
            pages(&a5).len() > pages(&a4).len(),
            "a shorter page must paginate into more pages"
        );
        assert_eq!(
            pages(&a5),
            parrot_export::pdf::page_lines(&content, PageSize::A5.mm())
        );

        modal.theme = modal.theme.toggled();
        assert_eq!(modal.preview(&content).input.theme, PreviewTheme::Dark);

        modal.set_format(ExportFormat::Markdown);
        let md = modal.preview(&content);
        assert_eq!(
            md.body,
            PreviewBody::Text(parrot_export::mvp::export_markdown(&content))
        );
        assert!(matches!(&md.body, PreviewBody::Text(t) if t.starts_with("# Export")));
    }

    #[test]
    fn quick_export_requires_and_reuses_last_export() {
        let dir = tempfile::tempdir().expect("tempdir");
        let content = sample();
        let mut modal = ExportModal::default();
        assert!(matches!(
            modal.quick_export(&content, "A"),
            Err(QuickExportError::NoPreviousExport)
        ));
        modal.open_for(vec![vec![0]], "First", Some(dir.path()));
        modal.set_format(ExportFormat::Html);
        modal.export(&content).expect("export");
        let quick = modal
            .quick_export(&content, "Second: draft")
            .expect("quick");
        assert_eq!(quick, dir.path().join("Second- draft.html"));
        assert!(quick.is_file());
        assert!(!modal.open, "Quick Export never opens the modal");
    }
}
