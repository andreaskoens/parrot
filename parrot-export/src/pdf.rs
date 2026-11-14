//! PDF backend (T-5-08): `printpdf` with the standard-14 built-in fonts
//! (`BuiltinFont::Helvetica`/`HelveticaBold`), which every PDF reader ships — nothing to
//! embed, no font licensing question. Layout is a top-to-bottom flow: each paragraph is
//! greedily word-wrapped to the text width and broken across pages; headings are larger
//! and bold. Wrapping estimates glyph widths per character (exact for monospace,
//! Helvetica advance buckets for proportional) with a small safety margin, so lines
//! fill the measure without crossing the margin.

use crate::css::{pdf_family_for, Align, ExportTheme, PdfFamily};
use parrot_core::{Inline, Paragraph, SheetContent, TagKind};
use printpdf::{
    BuiltinFont, Color, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Point, Pt, Rgb, TextItem,
};

/// A4, the page size [`export_pdf`] uses.
pub const A4_MM: (f32, f32) = (210.0, 297.0);
pub const MARGIN_MM: f32 = 20.0;
pub const BODY_PT: f32 = 11.0;
/// Line height as a multiple of the font size.
const LEADING: f32 = 1.4;
/// Extra space after each paragraph, as a fraction of the body line height.
const PARAGRAPH_GAP: f32 = 0.5;
const PT_PER_MM: f32 = 72.0 / 25.4;

/// One laid-out line: its text, font size and weight, horizontal start and vertical
/// baseline from the page top in millimetres. `color`/`italic`/`font` come from the
/// active stylesheet; the unstyled default is black, upright Helvetica.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfLine {
    pub text: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: Option<(u8, u8, u8)>,
    pub left_mm: f32,
    pub top_mm: f32,
    pub font: BuiltinFont,
}

/// Exports at A4. See [`export_pdf_sized`].
pub fn export_pdf(sheet: &SheetContent) -> Vec<u8> {
    export_pdf_sized(sheet, A4_MM)
}

/// `sheet` laid out into pages of `(width_mm, height_mm)`, exactly as [`export_pdf_sized`]
/// draws them. Public so the export preview shows the same wrapping and page breaks.
pub fn page_lines(sheet: &SheetContent, (width_mm, height_mm): (f32, f32)) -> Vec<Vec<PdfLine>> {
    page_lines_with(sheet, (width_mm, height_mm), &ExportTheme::builtin())
}

/// [`page_lines`], with `theme` applied: per-element sizes, colors and fonts, alignment,
/// margins, line height and spacing, and `@page` breaks and page size (resolved by the
/// caller via [`ExportTheme::page_dims_mm`]).
pub fn page_lines_with(
    sheet: &SheetContent,
    (width_mm, height_mm): (f32, f32),
    theme: &ExportTheme,
) -> Vec<Vec<PdfLine>> {
    let left = theme.page_margin_mm(3, MARGIN_MM);
    let right = theme.page_margin_mm(1, MARGIN_MM);
    let top = theme.page_margin_mm(0, MARGIN_MM);
    let bottom = height_mm - theme.page_margin_mm(2, MARGIN_MM);
    let measure_pt = (width_mm - left - right).max(1.0) * PT_PER_MM;
    let mut pages = vec![Vec::new()];
    let mut y = top;
    for p in &sheet.paragraphs {
        let kind = p.tag.as_ref().map(|t| &t.kind);
        let el = theme.element_for(kind);
        let text = sanitize_pdf_text(&paragraph_text(p));
        let size_pt = el.size_pt;
        let leading = el.line_height.unwrap_or(LEADING);
        let line_mm = size_pt * leading / PT_PER_MM;
        let family = el
            .family
            .as_deref()
            .map(pdf_family_for)
            .unwrap_or(PdfFamily::Sans);
        // The first line shares its width with the first-line indent, if any.
        let first_avail_pt = (measure_pt - el.text_indent.unwrap_or(0.0)).max(1.0);
        if el.break_before && pages.last().is_some_and(|page| !page.is_empty()) {
            pages.push(Vec::new());
            y = top;
        }
        let mut first_line = true;
        for line in wrap_width(&text, first_avail_pt, measure_pt, size_pt, family, el.bold) {
            if y + line_mm > bottom && pages.last().is_some_and(|page| !page.is_empty()) {
                pages.push(Vec::new());
                y = top;
            }
            y += line_mm;
            // Measured line width drives centered/right alignment; justify keeps the
            // left edge (this backend does not spread words).
            let est_w_mm = text_width_pt(&line, size_pt, family, el.bold) / PT_PER_MM;
            let mut x_mm = match el.align {
                Align::Left | Align::Justify => left,
                Align::Center => left + ((measure_pt / PT_PER_MM - est_w_mm) / 2.0).max(0.0),
                Align::Right => (width_mm - right - est_w_mm).max(left),
            };
            if first_line {
                if let Some(indent) = el.text_indent {
                    x_mm += indent / PT_PER_MM;
                }
                first_line = false;
            }
            if let Some(page) = pages.last_mut() {
                page.push(PdfLine {
                    text: line,
                    size_pt,
                    bold: el.bold,
                    italic: el.italic,
                    color: el.color,
                    left_mm: x_mm,
                    top_mm: y,
                    font: builtin_font(el.family.as_deref(), el.bold, el.italic),
                });
            }
        }
        let gap_mm = el
            .space_after_pt
            .map(|pt| pt / PT_PER_MM)
            .unwrap_or(BODY_PT * LEADING * PARAGRAPH_GAP / PT_PER_MM);
        y += gap_mm;
        if el.break_after && pages.last().is_some_and(|page| !page.is_empty()) {
            pages.push(Vec::new());
            y = top;
        }
    }
    pages
}

/// Exports `sheet` on pages of `(width_mm, height_mm)`.
pub fn export_pdf_sized(sheet: &SheetContent, (width_mm, height_mm): (f32, f32)) -> Vec<u8> {
    export_pdf_sized_with(sheet, (width_mm, height_mm), &ExportTheme::builtin())
}

/// [`export_pdf_sized`], with `theme` applied: per-element sizes, colors and fonts,
/// alignment, indentation, margins and page breaks.
pub fn export_pdf_sized_with(
    sheet: &SheetContent,
    (width_mm, height_mm): (f32, f32),
    theme: &ExportTheme,
) -> Vec<u8> {
    let mut doc = PdfDocument::new("Export");
    let pages = page_lines_with(sheet, (width_mm, height_mm), theme)
        .into_iter()
        .map(|lines| {
            let mut ops = Vec::new();
            for line in lines {
                // One text section per line: `SetTextCursor` (`Td`) is relative to the
                // section's start, so a fresh section makes each position absolute.
                ops.push(Op::StartTextSection);
                ops.push(Op::SetFontSizeBuiltinFont {
                    size: Pt(line.size_pt),
                    font: line.font,
                });
                if let Some((r, g, b)) = line.color {
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(
                            f32::from(r) / 255.0,
                            f32::from(g) / 255.0,
                            f32::from(b) / 255.0,
                            None,
                        )),
                    });
                }
                ops.push(Op::SetTextCursor {
                    pos: Point {
                        x: Mm(line.left_mm).into(),
                        y: Mm(height_mm - line.top_mm).into(),
                    },
                });
                ops.push(Op::WriteTextBuiltinFont {
                    items: vec![TextItem::Text(line.text)],
                    font: line.font,
                });
                ops.push(Op::EndTextSection);
            }
            PdfPage::new(Mm(width_mm), Mm(height_mm), ops)
        })
        .collect();
    doc.with_pages(pages);
    doc.save(&PdfSaveOptions::default(), &mut Vec::new())
}

/// Advance width of one character in ems. Monospace is exact — Courier gives every
/// glyph 0.6em. Proportional uses Helvetica advance buckets (space, narrow, digits,
/// caps, and `m`/`w` corrected, 0.52em otherwise), which also cover Times on the
/// safe side since Times runs narrower.
fn char_advance_em(c: char, family: PdfFamily) -> f32 {
    if family == PdfFamily::Mono {
        return 0.6;
    }
    match c {
        'i' | 'j' | 'l' => 0.24,
        ' ' | '!' | '\'' | ',' | '.' | ':' | ';' | '|' | 'f' | 't' | 'I' => 0.28,
        '"' | '(' | ')' | '[' | ']' | '{' | '}' | 'r' | '-' => 0.34,
        '0'..='9' => 0.556,
        'A'..='Z' => 0.70,
        'm' | 'w' => 0.78,
        _ => 0.52,
    }
}

/// Estimated width of a string in points. Bold runs a little wider.
fn text_width_pt(s: &str, size_pt: f32, family: PdfFamily, bold: bool) -> f32 {
    let em: f32 = s.chars().map(|c| char_advance_em(c, family)).sum();
    em * size_pt * if bold { 1.06 } else { 1.0 }
}

/// Greedy word wrap to `avail_pt` points; the first line gets `first_avail_pt`
/// (reduced by a first-line indent when one applies). A word longer than a line is
/// hard-split. An empty paragraph keeps one empty line, so blank lines still take
/// space. A 2% safety margin absorbs estimator error so lines never cross the margin.
fn wrap_width(
    text: &str,
    first_avail_pt: f32,
    avail_pt: f32,
    size_pt: f32,
    family: PdfFamily,
    bold: bool,
) -> Vec<String> {
    let width = |s: &str| text_width_pt(s, size_pt, family, bold);
    let space_w = width(" ");
    let first_budget = (first_avail_pt * 0.98).max(1.0);
    let budget = (avail_pt * 0.98).max(1.0);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut cur_w = 0.0;
    let mut line_budget = first_budget;
    for word in text.split_whitespace() {
        // Break the word into chunks that each fit a full line (usually just one).
        let mut chunks: Vec<(String, f32)> = Vec::new();
        let mut chunk = String::new();
        let mut chunk_w = 0.0;
        for c in word.chars() {
            let cw = char_advance_em(c, family) * size_pt * if bold { 1.06 } else { 1.0 };
            if !chunk.is_empty() && chunk_w + cw > budget {
                chunks.push((std::mem::take(&mut chunk), chunk_w));
                chunk_w = 0.0;
            }
            chunk.push(c);
            chunk_w += cw;
        }
        chunks.push((chunk, chunk_w));
        for (piece, piece_w) in chunks {
            if piece.is_empty() {
                continue;
            }
            let need = if current.is_empty() {
                piece_w
            } else {
                space_w + piece_w
            };
            if !current.is_empty() && cur_w + need > line_budget {
                lines.push(std::mem::take(&mut current));
                cur_w = 0.0;
                line_budget = budget;
            }
            if !current.is_empty() {
                current.push(' ');
                cur_w += space_w;
            }
            current.push_str(&piece);
            cur_w += piece_w;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Maps a CSS font family name (if any) plus weight and slant onto one of the
/// standard-14 built-in fonts — the only fonts this backend embeds.
fn builtin_font(family: Option<&str>, bold: bool, italic: bool) -> BuiltinFont {
    use printpdf::BuiltinFont as B;
    match (
        family.map(pdf_family_for).unwrap_or(PdfFamily::Sans),
        bold,
        italic,
    ) {
        (PdfFamily::Sans, false, false) => B::Helvetica,
        (PdfFamily::Sans, true, false) => B::HelveticaBold,
        (PdfFamily::Sans, false, true) => B::HelveticaOblique,
        (PdfFamily::Sans, true, true) => B::HelveticaBoldOblique,
        (PdfFamily::Serif, false, false) => B::TimesRoman,
        (PdfFamily::Serif, true, false) => B::TimesBold,
        (PdfFamily::Serif, false, true) => B::TimesItalic,
        (PdfFamily::Serif, true, true) => B::TimesBoldItalic,
        (PdfFamily::Mono, false, false) => B::Courier,
        (PdfFamily::Mono, true, false) => B::CourierBold,
        (PdfFamily::Mono, false, true) => B::CourierOblique,
        (PdfFamily::Mono, true, true) => B::CourierBoldOblique,
    }
}

/// The paragraph's printed text: flattened inline content plus the list/divider
/// prefixes the layout counts as text. The divider is plain ASCII hyphens on purpose:
/// [`sanitize_pdf_text`] must map every line to ASCII (see below), so synthetic
/// prefixes are already ASCII and never depend on the fallback table.
fn paragraph_text(p: &Paragraph) -> String {
    let text = flatten_text(&p.inlines);
    match p.tag.as_ref().map(|t| &t.kind) {
        Some(TagKind::UnorderedList) => format!("\u{2022} {text}"),
        Some(TagKind::OrderedList) => format!("1. {text}"),
        Some(TagKind::Divider) => "-----".to_string(),
        _ => text,
    }
}

/// Maps one line to the characters this backend can actually print. `printpdf`'s
/// builtin-font path writes the string's raw UTF-8 bytes into the content stream, and
/// base-14 fonts have no `ToUnicode` map, so the reader decodes each byte as WinAnsi:
/// every multi-byte character comes out as 2–3 garbage glyphs (`"` shows as `â€œ`,
/// `—` as `â€"`). Only ASCII passes through byte-identical, so everything else is
/// transliterated here — smart quotes to straight ones, dashes to hyphens, accented
/// Latin to its base letter — with `?` as the last resort for scripts base-14 cannot
/// cover (CJK and the like). Applied before wrapping, so layout counts final widths.
fn sanitize_pdf_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            c if c.is_ascii() => out.push(c),
            // Single/double quotes and guillemets.
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2039}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{00AB}' | '\u{00BB}' => {
                out.push('"');
            }
            '\u{2032}' => out.push('\''),
            '\u{2033}' => out.push('"'),
            // Dashes and hyphens: em dash doubles (manuscript convention).
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2212}' => out.push('-'),
            '\u{2014}' | '\u{2015}' => out.push_str("--"),
            // Ellipsis, bullets, middots.
            '\u{2026}' => out.push_str("..."),
            '\u{2022}' | '\u{00B7}' | '\u{2219}' => out.push('-'),
            // Spaces.
            '\u{00A0}' | '\u{2000}' | '\u{2001}' | '\u{2002}' | '\u{2003}' | '\u{2004}'
            | '\u{2005}' | '\u{2006}' | '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}'
            | '\u{202F}' | '\u{205F}' | '\u{3000}' => out.push(' '),
            '\u{200B}' | '\u{FEFF}' => {}
            // Arrows and common symbols.
            '\u{2190}' => out.push_str("<-"),
            '\u{2192}' => out.push_str("->"),
            '\u{2194}' => out.push_str("<->"),
            '\u{00A9}' => out.push_str("(c)"),
            '\u{00AE}' => out.push_str("(R)"),
            '\u{2122}' => out.push_str("(TM)"),
            '\u{00B0}' => out.push_str("deg"),
            '\u{00D7}' => out.push('x'),
            '\u{00F7}' => out.push('/'),
            '\u{00BF}' => out.push('?'),
            '\u{00A1}' => out.push('!'),
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            // Accented Latin: strip to the base letter.
            '\u{00C0}' | '\u{00C1}' | '\u{00C2}' | '\u{00C3}' | '\u{00C4}' | '\u{00C5}'
            | '\u{0100}' | '\u{0102}' | '\u{0104}' => out.push('A'),
            '\u{00C6}' | '\u{01FC}' => out.push_str("AE"),
            '\u{00C7}' | '\u{0106}' | '\u{0108}' | '\u{010A}' | '\u{010C}' => out.push('C'),
            '\u{00C8}' | '\u{00C9}' | '\u{00CA}' | '\u{00CB}' | '\u{0112}' | '\u{0114}'
            | '\u{0116}' | '\u{0118}' | '\u{011A}' => out.push('E'),
            '\u{00CC}' | '\u{00CD}' | '\u{00CE}' | '\u{00CF}' | '\u{0128}' | '\u{012A}'
            | '\u{012C}' | '\u{012E}' | '\u{0130}' => out.push('I'),
            '\u{00D1}' | '\u{0143}' | '\u{0145}' | '\u{0147}' => out.push('N'),
            '\u{00D2}' | '\u{00D3}' | '\u{00D4}' | '\u{00D5}' | '\u{00D6}' | '\u{00D8}'
            | '\u{014C}' | '\u{014E}' | '\u{0150}' => out.push('O'),
            '\u{0152}' => out.push_str("OE"),
            '\u{0153}' => out.push_str("oe"),
            '\u{00D9}' | '\u{00DA}' | '\u{00DB}' | '\u{00DC}' | '\u{0168}' | '\u{016A}'
            | '\u{016C}' | '\u{016E}' | '\u{0170}' | '\u{0172}' => out.push('U'),
            '\u{00DD}' | '\u{0178}' | '\u{0176}' => out.push('Y'),
            '\u{00DE}' => out.push_str("TH"),
            '\u{00DF}' => out.push_str("ss"),
            '\u{00E0}' | '\u{00E1}' | '\u{00E2}' | '\u{00E3}' | '\u{00E4}' | '\u{00E5}'
            | '\u{0101}' | '\u{0103}' | '\u{0105}' => out.push('a'),
            '\u{00E6}' | '\u{01FD}' => out.push_str("ae"),
            '\u{00E7}' | '\u{0107}' | '\u{0109}' | '\u{010B}' | '\u{010D}' => out.push('c'),
            '\u{00E8}' | '\u{00E9}' | '\u{00EA}' | '\u{00EB}' | '\u{0113}' | '\u{0115}'
            | '\u{0117}' | '\u{0119}' | '\u{011B}' => out.push('e'),
            '\u{00EC}' | '\u{00ED}' | '\u{00EE}' | '\u{00EF}' | '\u{0129}' | '\u{012B}'
            | '\u{012D}' | '\u{012F}' | '\u{0131}' => out.push('i'),
            '\u{00F1}' | '\u{0144}' | '\u{0146}' | '\u{0148}' => out.push('n'),
            '\u{00F2}' | '\u{00F3}' | '\u{00F4}' | '\u{00F5}' | '\u{00F6}' | '\u{00F8}'
            | '\u{014D}' | '\u{014F}' | '\u{0151}' => out.push('o'),
            '\u{00F9}' | '\u{00FA}' | '\u{00FB}' | '\u{00FC}' | '\u{0169}' | '\u{016B}'
            | '\u{016D}' | '\u{016F}' | '\u{0171}' | '\u{0173}' => out.push('u'),
            '\u{00FD}' | '\u{00FF}' | '\u{0177}' => out.push('y'),
            '\u{00FE}' => out.push_str("th"),
            '\u{0141}' | '\u{0142}' => out.push('l'),
            '\u{015A}' | '\u{015B}' | '\u{0160}' | '\u{0161}' => out.push('s'),
            '\u{015E}' | '\u{015F}' | '\u{0162}' | '\u{0163}' | '\u{0164}' | '\u{0165}' => {
                out.push('t');
            }
            '\u{0179}' | '\u{017A}' | '\u{017B}' | '\u{017C}' | '\u{017D}' | '\u{017E}' => {
                out.push('z');
            }
            _ => out.push('?'),
        }
    }
    out
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
        // Every line's measured width must fit the measure (unlike the old
        // average-advance wrapper, which let Courier run ~15% past the margin).
        let avail_pt = (A4_MM.0 - 2.0 * MARGIN_MM) * PT_PER_MM;
        assert!(lines.iter().all(|l| {
            text_width_pt(&l.text, BODY_PT, PdfFamily::Sans, false) <= avail_pt + 0.01
        }));
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
        // Monospace 10pt: 6pt per char. A 19pt measure fits 3 chars after the
        // 2% safety margin (3 × 6 = 18 ≤ 18.62).
        let mono =
            |text: &str, avail: f32| wrap_width(text, avail, avail, 10.0, PdfFamily::Mono, false);
        assert_eq!(mono("abcdefgh", 19.0), vec!["abc", "def", "gh"]);
        assert_eq!(mono("", 19.0), vec![""]);
        assert_eq!(mono("a b c", 19.0), vec!["a b", "c"]);
        assert_eq!(mono("a b c", 100.0), vec!["a b c"]);
    }

    #[test]
    fn courier_lines_fit_the_measure() {
        // The reported bug: Manuscript (11pt Courier, Letter, 1in margins) ran
        // ~15% past the right margin because wrapping assumed a 0.52em average
        // against Courier's real 0.6em advance.
        let words: Vec<String> = (0..400).map(|i| format!("word{i}")).collect();
        let sheet = SheetContent::new(vec![body(&words.join(" "))]);
        let theme = themed(
            "body { font-family: Courier, monospace; font-size: 11pt; } @page { size: letter; margin: 1in; }",
        );
        let (w, h) = theme.page_dims_mm((210.0, 297.0));
        assert_eq!((w, h), (215.9, 279.4));
        let layout = page_lines_with(&sheet, (w, h), &theme);
        let avail_pt = (w - 2.0 * 25.4) * PT_PER_MM;
        assert!(
            layout.iter().flatten().all(|l| {
                text_width_pt(&l.text, 11.0, PdfFamily::Mono, false) <= avail_pt + 0.01
            }),
            "no Courier line may cross the right margin"
        );
        // And the measure holds 69 full-width characters, not the old 81.
        let lines = wrap_width(
            &"m".repeat(69),
            avail_pt,
            avail_pt,
            11.0,
            PdfFamily::Mono,
            false,
        );
        assert_eq!(lines.len(), 1, "69 Courier chars fit Letter/1in");
        let lines = wrap_width(
            &"m".repeat(70),
            avail_pt,
            avail_pt,
            11.0,
            PdfFamily::Mono,
            false,
        );
        assert_eq!(lines.len(), 2, "70 Courier chars wrap on Letter/1in");
    }

    fn themed(css: &str) -> ExportTheme {
        ExportTheme::from_css(css)
    }

    #[test]
    fn styled_sizes_come_from_the_theme_not_the_defaults() {
        let sheet = SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("Big".to_string())],
            },
            body("plain"),
        ]);
        let layout = page_lines_with(&sheet, A4_MM, &themed("h1 { font-size: 28pt; }"));
        let lines: Vec<&PdfLine> = layout.iter().flatten().collect();
        assert_eq!(lines[0].size_pt, 28.0);
        assert!(lines[0].bold);
        assert_eq!(lines[1].size_pt, BODY_PT);
        // The unstyled path keeps the historical sizes.
        let plain_layout = page_lines(&sheet, A4_MM);
        let plain: Vec<&PdfLine> = plain_layout.iter().flatten().collect();
        assert_eq!(plain[0].size_pt, 20.0);
    }

    #[test]
    fn styled_colors_fonts_and_alignment_land_on_the_line() {
        let sheet = SheetContent::new(vec![body("centered red")]);
        let layout = page_lines_with(
            &sheet,
            A4_MM,
            &themed("body { color: #ff0000; text-align: center; font-family: Georgia, serif; }"),
        );
        let lines: Vec<&PdfLine> = layout.iter().flatten().collect();
        let line = lines[0];
        assert_eq!(line.color, Some((255, 0, 0)));
        assert!(
            line.left_mm > MARGIN_MM,
            "centered text starts right of the margin"
        );
        assert_eq!(line.font, BuiltinFont::TimesRoman);
        // ...and the default rendering stays black Helvetica at the margin.
        let plain_layout = page_lines(&sheet, A4_MM);
        let plain: Vec<&PdfLine> = plain_layout.iter().flatten().collect();
        assert_eq!(plain[0].color, None);
        assert_eq!(plain[0].left_mm, MARGIN_MM);
        assert_eq!(plain[0].font, BuiltinFont::Helvetica);
    }

    #[test]
    fn page_rule_sizes_margins_and_breaks() {
        let sheet = SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("One".to_string())],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("Two".to_string())],
            },
        ]);
        let theme = themed("h1 { break-before: always; } @page { size: letter; margin: 1in; }");
        let (w, h) = theme.page_dims_mm(A4_MM);
        assert_eq!((w, h), (215.9, 279.4));
        let pages = page_lines_with(&sheet, (w, h), &theme);
        assert_eq!(pages.len(), 2, "each h1 starts its own page");
        // top_mm is the first baseline: the 1in top margin plus one h1 line height.
        let expected_top = 25.4 + 20.0 * 1.4 / (72.0 / 25.4);
        assert!(
            (pages[0][0].top_mm - expected_top).abs() < 1.0,
            "first baseline below the 1in top margin, got {}",
            pages[0][0].top_mm
        );
        let bytes = export_pdf_sized_with(&sheet, (w, h), &theme);
        let doc = reparse(&bytes);
        assert_eq!(doc.pages.len(), 2);
        let width_pt = doc.pages[0].media_box.width.0;
        assert!(
            (width_pt - 215.9 * 72.0 / 25.4).abs() < 1.0,
            "letter width {width_pt}"
        );
        assert!(
            !doc.pages[0]
                .ops
                .iter()
                .any(|op| matches!(op, Op::SetFillColor { .. })),
            "no color ops without colors"
        );
    }

    #[test]
    fn styled_export_writes_fill_colors() {
        let sheet = SheetContent::new(vec![body("red")]);
        let bytes = export_pdf_sized_with(&sheet, A4_MM, &themed("body { color: #ff0000; }"));
        let doc = reparse(&bytes);
        assert!(
            doc.pages[0]
                .ops
                .iter()
                .any(|op| matches!(op, Op::SetFillColor { col: Color::Rgb(_) })),
            "themed color must reach the page"
        );
    }

    #[test]
    fn asymmetric_margins_use_the_correct_sides() {
        // top/right/bottom/left each distinct: left (10mm) must govern the text
        // edge, top (40mm) the first baseline — not each other.
        let sheet = SheetContent::new(vec![body("edged")]);
        let theme = themed("@page { margin-top: 40mm; margin-right: 20mm; margin-left: 10mm; }");
        let pages = page_lines_with(&sheet, A4_MM, &theme);
        let line = &pages[0][0];
        assert!(
            (line.left_mm - 10.0).abs() < 0.01,
            "text starts at the left margin, got {}",
            line.left_mm
        );
        let expected_top = 40.0 + BODY_PT * 1.4 / (72.0 / 25.4);
        assert!(
            (line.top_mm - expected_top).abs() < 1.0,
            "first baseline below the top margin, got {}",
            line.top_mm
        );
    }

    #[test]
    fn smart_punctuation_transliterates_to_ascii() {
        // The exact mojibake from the bug report: curly quotes and friends must
        // come out as straight ASCII, never raw multi-byte text.
        assert_eq!(
            sanitize_pdf_text("\u{201C}Oh absolutely not,\u{201D} said \u{2018}Bo\u{2019}."),
            "\"Oh absolutely not,\" said 'Bo'."
        );
        assert_eq!(sanitize_pdf_text("Haustorium.\u{201D}"), "Haustorium.\"");
        assert_eq!(sanitize_pdf_text("a\u{2014}b"), "a--b");
        assert_eq!(sanitize_pdf_text("a\u{2013}b"), "a-b");
        assert_eq!(sanitize_pdf_text("wait\u{2026}"), "wait...");
        assert_eq!(sanitize_pdf_text("\u{2022} item"), "- item");
        assert_eq!(sanitize_pdf_text("caf\u{00E9} na\u{00EF}ve"), "cafe naive");
        assert_eq!(sanitize_pdf_text("\u{00DF}tra\u{00DF}e"), "sstrasse");
        assert_eq!(sanitize_pdf_text("a\u{00A0}b"), "a b");
        assert_eq!(sanitize_pdf_text("A\u{2192}B"), "A->B");
        // Scripts base-14 cannot cover degrade to `?` rather than mojibake.
        assert_eq!(sanitize_pdf_text("\u{4E2D}\u{6587}"), "??");
        assert!(sanitize_pdf_text("plain").is_ascii());
    }

    #[test]
    fn layout_output_is_pure_ascii() {
        // Every laid-out line must survive printpdf's raw-byte content stream:
        // any non-ASCII char would render as WinAnsi mojibake.
        let prose = SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text(
                    "\u{201C}An Interrupted Trigger\u{201D}".to_string(),
                )],
            },
            Paragraph {
                tag: None,
                inlines: vec![Inline::Text(
                    "\u{2018}Oh absolutely not,\u{2019} little one \u{2014} caf\u{00E9}, na\u{00EF}ve\u{2026}".to_string(),
                )],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::UnorderedList,
                    literal: "- ".to_string(),
                }),
                inlines: vec![Inline::Text("list \u{2014} item".to_string())],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Divider,
                    literal: "----".to_string(),
                }),
                inlines: vec![],
            },
        ]);
        for theme in [
            &ExportTheme::builtin(),
            &themed("body { font-size: 12pt; }"),
        ] {
            let pages = page_lines_with(&prose, A4_MM, theme);
            assert!(!pages.is_empty());
            for line in pages.iter().flatten() {
                assert!(
                    line.text.is_ascii(),
                    "non-ASCII would mojibake in the PDF: {:?}",
                    line.text
                );
            }
        }
        // The divider renders as plain hyphens, the bullet as a hyphen prefix.
        let pages = page_lines_with(&prose, A4_MM, &ExportTheme::builtin());
        let flat: Vec<&str> = pages.iter().flatten().map(|l| l.text.as_str()).collect();
        assert!(flat.iter().any(|l| l.contains("-----")));
        assert!(flat.iter().any(|l| l.starts_with("- list")));
        assert!(flat
            .iter()
            .any(|l| l.contains("\"An Interrupted Trigger\"")));
    }
}
