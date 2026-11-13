//! DOCX backend (T-5-06): `docx-rs` + `zip`, per the roadmap. Inline formatting (bold/italic
//! /strike/highlight/monospace) maps onto `docx-rs`'s `Run` properties directly; constructs with
//! no DOCX-native analogue in this crate's scope — true hyperlink fields, list numbering
//! definitions — render as plain annotated text instead (" (url)", a literal "• "/"1. " prefix),
//! the same "valid, not a feature-complete port" stance as the RTF backend (Decision D5).

use crate::css::{Align, ExportTheme};
use docx_rs::{
    read_docx, AlignmentType, Docx, PageMargin, Paragraph as DocxParagraph, Run, RunFonts,
};
use parrot_core::{Inline, Paragraph, SheetContent, TagKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocxExportError {
    #[error("failed to pack docx: {0}")]
    Pack(#[from] zip::result::ZipError),
}

pub fn export_docx(sheet: &SheetContent) -> Result<Vec<u8>, DocxExportError> {
    export_docx_inner(sheet, None)
}

/// [`export_docx`], with a stylesheet applied: per-element sizes, colors and fonts,
/// alignment and quote indentation follow the theme. `None` is not a valid theme
/// here — pass [`ExportTheme::builtin`] for the default look through this path.
pub fn export_docx_with(
    sheet: &SheetContent,
    theme: &ExportTheme,
) -> Result<Vec<u8>, DocxExportError> {
    export_docx_inner(sheet, Some(theme))
}

fn export_docx_inner(
    sheet: &SheetContent,
    theme: Option<&ExportTheme>,
) -> Result<Vec<u8>, DocxExportError> {
    let mut docx = Docx::new();
    // Page geometry only when `@page` sets it; otherwise the library default
    // applies and unstyled output is unchanged.
    if let Some(t) = theme {
        if let Some(page) = t.page.as_ref() {
            if let Some((w_mm, h_mm)) = page.size {
                docx = docx.page_size(
                    (w_mm * 72.0 / 25.4 * 20.0).round() as u32,
                    (h_mm * 72.0 / 25.4 * 20.0).round() as u32,
                );
            }
            if page.margin.iter().any(|m| m.is_some()) {
                let dxa = |side: usize, fallback_dxa: i32| {
                    page.margin
                        .get(side)
                        .copied()
                        .flatten()
                        .map(|mm| (mm * 72.0 / 25.4 * 20.0).round() as i32)
                        .unwrap_or(fallback_dxa)
                };
                // docx-rs defaults: 1in top/bottom/left/right, ~0.5in header/footer.
                docx = docx.page_margin(PageMargin {
                    top: dxa(0, 1440),
                    left: dxa(3, 1440),
                    bottom: dxa(2, 1440),
                    right: dxa(1, 1440),
                    header: 720,
                    footer: 720,
                    gutter: 0,
                });
            }
        }
    }
    let mut pending_break = false;
    let mut first_para = true;
    for p in &sheet.paragraphs {
        let el = theme.map(|t| t.element_for(p.tag.as_ref().map(|t| &t.kind)));
        let break_before = match el {
            Some(el) => take_page_break(el, &mut pending_break, &mut first_para),
            None => false,
        };
        let mut para = build_paragraph(p, theme);
        if break_before {
            para = para.page_break_before(true);
        }
        docx = docx.add_paragraph(para);
    }
    let mut buf = Vec::new();
    docx.build().pack(std::io::Cursor::new(&mut buf))?;
    Ok(buf)
}

/// Ariadne's break sequencing: `break-before` (or a pending `break-after` from the
/// previous block) starts a new page unless this is the first paragraph; `break-after`
/// arms the next block. Shared with the RTF backend's semantics.
fn take_page_break(style: &crate::css::ElStyle, pending: &mut bool, first: &mut bool) -> bool {
    let bb = (style.break_before || *pending) && !*first;
    *first = false;
    *pending = style.break_after;
    bb
}

fn build_paragraph(p: &Paragraph, theme: Option<&ExportTheme>) -> DocxParagraph {
    let mut para = DocxParagraph::new();
    let kind = p.tag.as_ref().map(|t| &t.kind);
    // The themed element for this block, if a stylesheet is active. Headings keep
    // their HeadingN style (Word handles numbering and outline); the theme only adds
    // explicit sizing and color on top of it.
    let el = theme.map(|t| t.element_for(kind));

    let prefix: Option<Run> = match kind {
        Some(TagKind::Heading(n)) => {
            para = para.style(&format!("Heading{n}"));
            None
        }
        Some(TagKind::BlockQuote) => {
            // 720 dxa = half an inch, unless the stylesheet sets its own indent.
            let left = el
                .and_then(|el| el.margin_left)
                .map(|pt| (pt * 20.0).round() as i32);
            para = para.indent(left.or(Some(720)), None, None, None);
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
        para = para.add_run(themed_run(run, el));
    }

    if let Some(TagKind::CodeBlock(_)) = kind {
        let code_family = el
            .and_then(|el| el.family.clone())
            .unwrap_or_else(|| "Consolas".to_string());
        let run = Run::new()
            .add_text(flatten_text(&p.inlines))
            .fonts(RunFonts::new().ascii(&code_family).hi_ansi(&code_family));
        para = para.add_run(themed_run(run, el));
    } else {
        let mut runs = Vec::new();
        let code_font = theme.and_then(|t| t.code.family.as_deref());
        render_inline_runs(&p.inlines, RunStyle::default(), code_font, &mut runs);
        for run in runs {
            para = para.add_run(themed_run(run, el));
        }
    }

    if matches!(kind, Some(TagKind::Comment)) {
        para = para.add_run(themed_run(Run::new().italic().add_text("]"), el));
    }

    match el.map(|el| el.align) {
        // Left is Word's default; writing it explicitly would only bloat the XML.
        Some(Align::Center) => para = para.align(AlignmentType::Center),
        Some(Align::Right) => para = para.align(AlignmentType::Right),
        Some(Align::Justify) => para = para.align(AlignmentType::Justified),
        Some(Align::Left) | None => {}
    }

    para
}

/// Applies a themed element's sizing, color and font to an already-built run.
/// Markup-driven bold/italic stay as the renderer left them; the theme only adds
/// weight or slant on top when the element itself calls for it.
fn themed_run(mut run: Run, el: Option<&crate::css::ElStyle>) -> Run {
    let Some(el) = el else {
        return run;
    };
    // docx-rs sizes are half-points.
    run = run.size((el.size_pt * 2.0).round().max(1.0) as usize);
    if let Some((r, g, b)) = el.color {
        run = run.color(format!("{r:02X}{g:02X}{b:02X}"));
    }
    if let Some(family) = &el.family {
        run = run.fonts(RunFonts::new().ascii(family).hi_ansi(family));
    }
    if el.bold {
        run = run.bold();
    }
    if el.italic {
        run = run.italic();
    }
    run
}

#[derive(Clone, Copy, Default)]
struct RunStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    highlight: bool,
}

fn render_inline_runs(
    inlines: &[Inline],
    style: RunStyle,
    code_font: Option<&str>,
    out: &mut Vec<Run>,
) {
    for inline in inlines {
        match inline {
            Inline::Text(s) => out.push(styled_run(s, style, code_font)),
            Inline::Strong(c) => render_inline_runs(
                c,
                RunStyle {
                    bold: true,
                    ..style
                },
                code_font,
                out,
            ),
            Inline::Emph(c) => render_inline_runs(
                c,
                RunStyle {
                    italic: true,
                    ..style
                },
                code_font,
                out,
            ),
            Inline::Code(s) => out.push(styled_run(
                s,
                RunStyle {
                    code: true,
                    ..style
                },
                code_font,
            )),
            Inline::Link { url, content } => {
                render_inline_runs(content, style, code_font, out);
                out.push(styled_run(&format!(" ({url})"), style, code_font));
            }
            Inline::Footnote { content } => {
                out.push(styled_run(" (", style, code_font));
                render_inline_runs(content, style, code_font, out);
                out.push(styled_run(")", style, code_font));
            }
            Inline::Delete(c) => render_inline_runs(
                c,
                RunStyle {
                    strike: true,
                    ..style
                },
                code_font,
                out,
            ),
            Inline::InlineComment(c) => {
                out.push(styled_run(" [", style, code_font));
                render_inline_runs(c, style, code_font, out);
                out.push(styled_run("]", style, code_font));
            }
            Inline::InlineNative(s) => out.push(styled_run(s, style, code_font)),
            Inline::Mark(c) => render_inline_runs(
                c,
                RunStyle {
                    highlight: true,
                    ..style
                },
                code_font,
                out,
            ),
            Inline::Annotation { note, content } => {
                render_inline_runs(content, style, code_font, out);
                out.push(styled_run(&format!(" [{note}]"), style, code_font));
            }
            Inline::Image { alt, .. } => out.push(styled_run(
                &format!("[image: {}]", alt.as_deref().unwrap_or("")),
                style,
                code_font,
            )),
            Inline::Video { url } => {
                out.push(styled_run(&format!("[video: {url}]"), style, code_font))
            }
        }
    }
}

fn styled_run(text: &str, style: RunStyle, code_font: Option<&str>) -> Run {
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
        // Monospace stays the default for code; a `pre`/`code` rule overrides it.
        run = run.fonts(RunFonts::new().ascii(code_font.unwrap_or("Consolas")));
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

    fn document_xml(bytes: &[u8]) -> String {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
        let mut xml = String::new();
        use std::io::Read as _;
        archive
            .by_name("word/document.xml")
            .expect("document.xml")
            .read_to_string(&mut xml)
            .expect("readable");
        xml
    }

    #[test]
    fn themed_export_applies_sizes_colors_fonts_and_alignment() {
        let sheet = SheetContent::new(vec![
            parrot_core::Paragraph {
                tag: Some(parrot_core::TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![parrot_core::Inline::Text("Big".to_string())],
            },
            parrot_core::Paragraph {
                tag: None,
                inlines: vec![parrot_core::Inline::Text("body".to_string())],
            },
        ]);
        let theme = ExportTheme::from_css(
            "h1 { font-size: 28pt; color: #112233; text-align: center; font-family: Georgia, serif; }",
        );
        let xml =
            document_xml(&export_docx_with(&sheet, &theme).expect("themed export must succeed"));
        // 28pt = 56 half-points on the heading run; the theme color lands as hex.
        assert!(xml.contains(r#"w:sz w:val="56""#), "themed size:\n{xml}");
        assert!(
            xml.contains(r#"w:color w:val="112233""#),
            "themed color:\n{xml}"
        );
        assert!(xml.contains("Georgia"), "themed font:\n{xml}");
        assert!(
            xml.contains(r#"w:jc w:val="center""#),
            "themed alignment:\n{xml}"
        );
        // ...and the themed file still round-trips.
        reopen(&export_docx_with(&sheet, &theme).expect("themed export must succeed"))
            .expect("reopen");
    }

    #[test]
    fn unthemed_export_has_no_explicit_theming() {
        // The legacy path stays free of style overrides: no explicit sizes, colors
        // or fonts beyond the ones the backend has always written.
        let xml = document_xml(&export_docx(&full_tag_document()).expect("export"));
        assert!(!xml.contains("w:sz "), "no explicit sizes unthemed");
        assert!(!xml.contains("w:color "), "no explicit colors unthemed");
    }

    #[test]
    fn page_rule_sets_size_margins_and_breaks() {
        let sheet = SheetContent::new(vec![
            parrot_core::Paragraph {
                tag: Some(parrot_core::TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![parrot_core::Inline::Text("One".to_string())],
            },
            parrot_core::Paragraph {
                tag: Some(parrot_core::TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![parrot_core::Inline::Text("Two".to_string())],
            },
        ]);
        // Letter = 12240 x 15840 dxa; 1in margins = 1440 dxa.
        let theme = ExportTheme::from_css(
            "h1 { break-before: always; } @page { size: letter; margin: 1in; }",
        );
        let xml =
            document_xml(&export_docx_with(&sheet, &theme).expect("themed export must succeed"));
        assert!(xml.contains(r#"w:w="12240""#), "letter width:\n{xml}");
        assert!(xml.contains(r#"w:h="15840""#), "letter height:\n{xml}");
        assert!(xml.contains(r#"w:top="1440""#), "top margin:\n{xml}");
        // The second h1 breaks; the first (opening paragraph) does not.
        assert_eq!(
            xml.matches("w:pageBreakBefore").count(),
            1,
            "exactly one break:\n{xml}"
        );
        // Unthemed output keeps the library default and carries no breaks.
        let plain = document_xml(&export_docx(&sheet).expect("export"));
        assert!(
            !plain.contains(r#"w:w="12240""#),
            "no letter width unthemed"
        );
        assert!(!plain.contains("pageBreakBefore"), "no breaks unthemed");
    }
}
