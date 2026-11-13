//! DOCX backend (T-5-06): `docx-rs` + `zip`, per the roadmap. Inline formatting (bold/italic
//! /strike/highlight/monospace) maps onto `docx-rs`'s `Run` properties directly; constructs with
//! no DOCX-native analogue in this crate's scope — true hyperlink fields, list numbering
//! definitions — render as plain annotated text instead (" (url)", a literal "• "/"1. " prefix),
//! the same "valid, not a feature-complete port" stance as the RTF backend (Decision D5).

use docx_rs::{read_docx, AlignmentType, Docx, Paragraph as DocxParagraph, Run, RunFonts};
use parrot_core::{Inline, Paragraph, SheetContent, TagKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocxExportError {
    #[error("failed to pack docx: {0}")]
    Pack(#[from] zip::result::ZipError),
}

pub fn export_docx(sheet: &SheetContent) -> Result<Vec<u8>, DocxExportError> {
    let mut docx = Docx::new();
    for p in &sheet.paragraphs {
        docx = docx.add_paragraph(build_paragraph(p));
    }
    let mut buf = Vec::new();
    docx.build().pack(std::io::Cursor::new(&mut buf))?;
    Ok(buf)
}

fn build_paragraph(p: &Paragraph) -> DocxParagraph {
    let mut para = DocxParagraph::new();
    let kind = p.tag.as_ref().map(|t| &t.kind);

    let prefix: Option<Run> = match kind {
        Some(TagKind::Heading(n)) => {
            para = para.style(&format!("Heading{n}"));
            None
        }
        Some(TagKind::BlockQuote) => {
            para = para.indent(Some(720), None, None, None);
            None
        }
        Some(TagKind::UnorderedList) => Some(Run::new().add_text("\u{2022} ")),
        Some(TagKind::OrderedList) => Some(Run::new().add_text("1. ")),
        Some(TagKind::Comment) => Some(Run::new().italic().add_text("[")),
        Some(TagKind::Divider) => {
            para = para.align(AlignmentType::Center);
            Some(Run::new().add_text("\u{2014}\u{2014}\u{2014}\u{2014}\u{2014}"))
        }
        _ => None,
    };
    if let Some(run) = prefix {
        para = para.add_run(run);
    }

    if let Some(TagKind::CodeBlock(_)) = kind {
        let run = Run::new()
            .add_text(flatten_text(&p.inlines))
            .fonts(RunFonts::new().ascii("Consolas"));
        para = para.add_run(run);
    } else {
        let mut runs = Vec::new();
        render_inline_runs(&p.inlines, RunStyle::default(), &mut runs);
        for run in runs {
            para = para.add_run(run);
        }
    }

    if matches!(kind, Some(TagKind::Comment)) {
        para = para.add_run(Run::new().italic().add_text("]"));
    }

    para
}

#[derive(Clone, Copy, Default)]
struct RunStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    highlight: bool,
}

fn render_inline_runs(inlines: &[Inline], style: RunStyle, out: &mut Vec<Run>) {
    for inline in inlines {
        match inline {
            Inline::Text(s) => out.push(styled_run(s, style)),
            Inline::Strong(c) => render_inline_runs(
                c,
                RunStyle {
                    bold: true,
                    ..style
                },
                out,
            ),
            Inline::Emph(c) => render_inline_runs(
                c,
                RunStyle {
                    italic: true,
                    ..style
                },
                out,
            ),
            Inline::Code(s) => out.push(styled_run(
                s,
                RunStyle {
                    code: true,
                    ..style
                },
            )),
            Inline::Link { url, content } => {
                render_inline_runs(content, style, out);
                out.push(styled_run(&format!(" ({url})"), style));
            }
            Inline::Footnote { content } => {
                out.push(styled_run(" (", style));
                render_inline_runs(content, style, out);
                out.push(styled_run(")", style));
            }
            Inline::Delete(c) => render_inline_runs(
                c,
                RunStyle {
                    strike: true,
                    ..style
                },
                out,
            ),
            Inline::InlineComment(c) => {
                out.push(styled_run(" [", style));
                render_inline_runs(c, style, out);
                out.push(styled_run("]", style));
            }
            Inline::InlineNative(s) => out.push(styled_run(s, style)),
            Inline::Mark(c) => render_inline_runs(
                c,
                RunStyle {
                    highlight: true,
                    ..style
                },
                out,
            ),
            Inline::Annotation { note, content } => {
                render_inline_runs(content, style, out);
                out.push(styled_run(&format!(" [{note}]"), style));
            }
            Inline::Image { alt, .. } => out.push(styled_run(
                &format!("[image: {}]", alt.as_deref().unwrap_or("")),
                style,
            )),
            Inline::Video { url } => out.push(styled_run(&format!("[video: {url}]"), style)),
        }
    }
}

fn styled_run(text: &str, style: RunStyle) -> Run {
    let mut run = Run::new().add_text(text);
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if style.strike {
        run = run.strike();
    }
    if style.highlight {
        run = run.highlight("yellow");
    }
    if style.code {
        run = run.fonts(RunFonts::new().ascii("Consolas"));
    }
    run
}

fn flatten_text(inlines: &[Inline]) -> String {
    inlines.iter().map(flatten_one).collect()
}

fn flatten_one(inline: &Inline) -> String {
    match inline {
        Inline::Text(s) | Inline::Code(s) | Inline::InlineNative(s) => s.clone(),
        Inline::Strong(c)
        | Inline::Emph(c)
        | Inline::Delete(c)
        | Inline::InlineComment(c)
        | Inline::Mark(c) => flatten_text(c),
        Inline::Link { content, .. }
        | Inline::Footnote { content }
        | Inline::Annotation { content, .. } => flatten_text(content),
        Inline::Image { alt, .. } => alt.clone().unwrap_or_default(),
        Inline::Video { .. } => String::new(),
    }
}

/// Re-opens `bytes` via `docx-rs`'s own read path — the round-trip the task's Done-when
/// criterion asks for.
pub fn reopen(bytes: &[u8]) -> Result<docx_rs::Docx, docx_rs::ReaderError> {
    read_docx(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::full_tag_document;

    #[test]
    fn reopreadable_by_docx_rs() {
        let bytes = export_docx(&full_tag_document()).expect("export must succeed");
        reopen(&bytes).expect("docx-rs must be able to re-open its own output");
    }
}
