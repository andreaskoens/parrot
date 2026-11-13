//! PDF backend (T-5-08): `printpdf` with the standard-14 built-in fonts
//! (`BuiltinFont::Helvetica`/`HelveticaBold`), which every PDF reader ships — nothing to
//! embed, no font licensing question. Layout is a top-to-bottom flow: each paragraph is
//! greedily word-wrapped to the text width and broken across pages; headings are larger
//! and bold. Wrapping estimates glyph width from an average Helvetica advance rather than
//! real metrics, so lines run slightly short rather than overflowing the margin.

use parrot_core::{Inline, Paragraph, SheetContent, TagKind};
use printpdf::{BuiltinFont, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Point, Pt, TextItem};

/// A4, the page size [`export_pdf`] uses.
pub const A4_MM: (f32, f32) = (210.0, 297.0);
pub const MARGIN_MM: f32 = 20.0;
pub const BODY_PT: f32 = 11.0;
/// Line height as a multiple of the font size.
const LEADING: f32 = 1.4;
/// Extra space after each paragraph, as a fraction of the body line height.
const PARAGRAPH_GAP: f32 = 0.5;
/// Average Helvetica advance per character, in ems (conservative, so lines never overflow).
const AVG_ADVANCE_EM: f32 = 0.52;
const PT_PER_MM: f32 = 72.0 / 25.4;

/// One laid-out line: its text, font size in points, weight, and baseline from the page top
/// in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfLine {
    pub text: String,
    pub size_pt: f32,
    pub bold: bool,
    pub top_mm: f32,
}

/// Exports at A4. See [`export_pdf_sized`].
pub fn export_pdf(sheet: &SheetContent) -> Vec<u8> {
    export_pdf_sized(sheet, A4_MM)
}

/// `sheet` laid out into pages of `(width_mm, height_mm)`, exactly as [`export_pdf_sized`]
/// draws them. Public so the export preview shows the same wrapping and page breaks.
pub fn page_lines(sheet: &SheetContent, (width_mm, height_mm): (f32, f32)) -> Vec<Vec<PdfLine>> {
    let text_width_pt = (width_mm - 2.0 * MARGIN_MM) * PT_PER_MM;
    let bottom = height_mm - MARGIN_MM;
    let mut pages = vec![Vec::new()];
    let mut y = MARGIN_MM;
    for p in &sheet.paragraphs {
        let (text, size_pt, bold) = paragraph_style(p);
        let line_mm = size_pt * LEADING / PT_PER_MM;
        let max_chars = (text_width_pt / (size_pt * AVG_ADVANCE_EM))
            .floor()
            .max(1.0) as usize;
        for line in wrap(&text, max_chars) {
            if y + line_mm > bottom && pages.last().is_some_and(|page| !page.is_empty()) {
                pages.push(Vec::new());
                y = MARGIN_MM;
            }
            y += line_mm;
            if let Some(page) = pages.last_mut() {
                page.push(PdfLine {
                    text: line,
                    size_pt,
                    bold,
                    top_mm: y,
                });
            }
        }
        y += BODY_PT * LEADING * PARAGRAPH_GAP / PT_PER_MM;
    }
    pages
}

/// Exports `sheet` on pages of `(width_mm, height_mm)`.
pub fn export_pdf_sized(sheet: &SheetContent, (width_mm, height_mm): (f32, f32)) -> Vec<u8> {
    let mut doc = PdfDocument::new("Export");
    let pages = page_lines(sheet, (width_mm, height_mm))
        .into_iter()
        .map(|lines| {
            let mut ops = Vec::new();
            for line in lines {
                let font = if line.bold {
                    BuiltinFont::HelveticaBold
                } else {
                    BuiltinFont::Helvetica
                };
                // One text section per line: `SetTextCursor` (`Td`) is relative to the
                // section's start, so a fresh section makes each position absolute.
                ops.push(Op::StartTextSection);
                ops.push(Op::SetFontSizeBuiltinFont {
                    size: Pt(line.size_pt),
                    font,
                });
                ops.push(Op::SetTextCursor {
                    pos: Point {
                        x: Mm(MARGIN_MM).into(),
                        y: Mm(height_mm - line.top_mm).into(),
                    },
                });
                ops.push(Op::WriteTextBuiltinFont {
                    items: vec![TextItem::Text(line.text)],
                    font,
                });
                ops.push(Op::EndTextSection);
            }
            PdfPage::new(Mm(width_mm), Mm(height_mm), ops)
        })
        .collect();
    doc.with_pages(pages);
    doc.save(&PdfSaveOptions::default(), &mut Vec::new())
}

/// Greedy word wrap to at most `max_chars` characters per line; a word longer than a line is
/// split. An empty paragraph keeps one empty line, so blank lines still take space.
fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        while word.len() > max_chars {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(word.drain(..max_chars).collect());
        }
        let word: String = word.into_iter().collect();
        let len = current.chars().count();
        if len > 0 && len + 1 + word.chars().count() > max_chars {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&word);
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// The paragraph's printed text, font size and weight.
fn paragraph_style(p: &Paragraph) -> (String, f32, bool) {
    let text = flatten_text(&p.inlines);
    match p.tag.as_ref().map(|t| &t.kind) {
        Some(TagKind::Heading(level)) => {
            let size = match level {
                1 => 20.0,
                2 => 16.0,
                3 => 14.0,
                _ => 12.0,
            };
            (text, size, true)
        }
        Some(TagKind::UnorderedList) => (format!("\u{2022} {text}"), BODY_PT, false),
        Some(TagKind::OrderedList) => (format!("1. {text}"), BODY_PT, false),
        Some(TagKind::Divider) => (
            "\u{2014}\u{2014}\u{2014}\u{2014}\u{2014}".to_string(),
            BODY_PT,
            false,
        ),
        _ => (text, BODY_PT, false),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};
    use printpdf::{PdfDocument as ReadPdfDocument, PdfParseOptions};

    /// A document with enough paragraphs that `export_pdf`'s manual page-break logic must
    /// produce more than one page — the task's own "correct page count" criterion needs a
    /// document long enough to actually exercise it, which the shared 10-paragraph
    /// `fixtures::full_tag_document` isn't guaranteed to be at this line height.
    fn many_paragraph_document() -> SheetContent {
        let paragraphs = (0..60)
            .map(|i| Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text(format!("Paragraph {i}"))],
            })
            .collect();
        SheetContent::new(paragraphs)
    }

    fn reparse(bytes: &[u8]) -> ReadPdfDocument {
        assert!(bytes.starts_with(b"%PDF-"), "not a PDF header");
        let mut warnings = Vec::new();
        ReadPdfDocument::parse(bytes, &PdfParseOptions::default(), &mut warnings)
            .expect("printpdf must be able to re-parse its own output")
    }

    /// Every text run drawn on `page`, in drawing order.
    fn page_text(page: &printpdf::PdfPage) -> Vec<String> {
        page.ops
            .iter()
            .filter_map(|op| match op {
                Op::WriteText { items, .. } | Op::WriteTextBuiltinFont { items, .. } => Some(
                    items
                        .iter()
                        .filter_map(|i| match i {
                            TextItem::Text(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect()
    }

    fn body(text: &str) -> Paragraph {
        Paragraph {
            tag: None,
            inlines: vec![Inline::Text(text.to_string())],
        }
    }

    #[test]
    fn page_count_matches_the_layout() {
        let doc = reparse(&export_pdf(&many_paragraph_document()));
        let layout = page_lines(&many_paragraph_document(), A4_MM);
        assert!(layout.len() > 1, "60 headings must span several pages");
        assert_eq!(doc.pages.len(), layout.len());
    }

    /// Regression: text drawn outside a text section is dropped by readers, so the exported
    /// PDF rendered blank. Every paragraph's text must come back out of the file.
    #[test]
    fn exported_pdf_contains_every_paragraph_text() {
        let sheet = SheetContent::new(vec![
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
                    Inline::Text("Plain and ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                ],
            },
        ]);
        let doc = reparse(&export_pdf(&sheet));
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(page_text(&doc.pages[0]), vec!["Title", "Plain and bold"]);
        assert!(doc.pages[0]
            .ops
            .iter()
            .any(|op| matches!(op, Op::StartTextSection)));
    }

    #[test]
    fn long_paragraphs_wrap_inside_the_margins_and_keep_every_word() {
        let words: Vec<String> = (0..400).map(|i| format!("word{i}")).collect();
        let sheet = SheetContent::new(vec![body(&words.join(" "))]);
        let layout = page_lines(&sheet, A4_MM);
        let lines: Vec<&PdfLine> = layout.iter().flatten().collect();
        assert!(lines.len() > 10, "400 words must wrap onto many lines");
        let max_chars =
            ((A4_MM.0 - 2.0 * MARGIN_MM) * PT_PER_MM / (BODY_PT * AVG_ADVANCE_EM)) as usize;
        assert!(lines.iter().all(|l| l.text.chars().count() <= max_chars));
        for page in &layout {
            assert!(page
                .iter()
                .all(|l| l.top_mm > MARGIN_MM && l.top_mm <= A4_MM.1 - MARGIN_MM));
        }
        let doc = reparse(&export_pdf(&sheet));
        let printed: Vec<String> = doc.pages.iter().flat_map(page_text).collect();
        assert_eq!(printed.join(" "), words.join(" "));
    }

    #[test]
    fn page_size_sets_the_media_box() {
        let doc = reparse(&export_pdf_sized(
            &SheetContent::new(vec![body("x")]),
            (148.0, 210.0),
        ));
        let page = &doc.pages[0];
        let (w, h) = (page.media_box.width.0, page.media_box.height.0);
        assert!((w - 148.0 * PT_PER_MM).abs() < 1.0, "width {w}");
        assert!((h - 210.0 * PT_PER_MM).abs() < 1.0, "height {h}");
    }

    #[test]
    fn wrap_splits_overlong_words_and_keeps_empty_paragraphs() {
        assert_eq!(wrap("abcdefgh", 3), vec!["abc", "def", "gh"]);
        assert_eq!(wrap("", 10), vec![""]);
        assert_eq!(wrap("a b c", 3), vec!["a b", "c"]);
    }
}
