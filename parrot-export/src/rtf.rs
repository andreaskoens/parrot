//! RTF backend (T-5-05): hand-rolled RTF 1.9 writer. Decision D5: **not** attempting RTFKit
//! fidelity — its internals aren't confirmed — so the target is "valid, opens correctly in
//! Word/LibreOffice/TextEdit," not a byte-for-byte match with Parrot' own RTF output.
//!
//! RTF has no native list-table or comment-field support cheap enough to hand-roll correctly
//! here, so (same opaque-content stance as the HTML backend, see DECISIONS.md): lists render as
//! plain bullet-prefixed/numbered paragraphs, not `\pard\tx\ls` list tables; block comments
//! render as an italicized bracketed paragraph; dividers render as a thin rule-like paragraph of
//! em-dashes. None of these are confirmed real Parrot RTF-export behavior.

use crate::css::{Align, ElStyle, ExportTheme};
use parrot_core::{Inline, Paragraph, SheetContent, TagKind};

pub fn export_rtf(sheet: &SheetContent) -> String {
    export_rtf_inner(sheet, None)
}

/// [`export_rtf`], with a stylesheet applied: the theme's fonts land in the font table
/// (`\f2` and up), its colors in a color table, and each paragraph carries alignment,
/// indentation, size and color. `None` is not a valid theme here — pass
/// [`ExportTheme::builtin`] for the default look through this path. Note the default
/// look differs from the legacy path on purpose: an active stylesheet governs sizing
/// (e.g. headings follow the theme's 20/16/14pt rather than the legacy 24/21/18pt).
pub fn export_rtf_with(sheet: &SheetContent, theme: &ExportTheme) -> String {
    export_rtf_inner(sheet, Some(theme))
}

fn export_rtf_inner(sheet: &SheetContent, theme: Option<&ExportTheme>) -> String {
    let mut out = String::new();
    out.push_str(r"{\rtf1\ansi\deff0");
    out.push_str(&font_table(theme));
    if let Some(table) = color_table(theme) {
        out.push_str(&table);
    }
    // Page geometry only when `@page` sets it; otherwise the reader default
    // applies and unstyled output is unchanged. (Twips: 1/20 pt.)
    if let Some(t) = theme {
        if let Some(page) = t.page.as_ref() {
            if let Some((w_mm, h_mm)) = page.size {
                out.push_str(&format!(
                    "\\paperw{} \\paperh{}",
                    (w_mm * 72.0 / 25.4 * 20.0).round() as u32,
                    (h_mm * 72.0 / 25.4 * 20.0).round() as u32
                ));
            }
            let sides = [
                ("\\margt", 0),
                ("\\margr", 1),
                ("\\margb", 2),
                ("\\margl", 3),
            ];
            for (word, side) in sides {
                if let Some(Some(mm)) = page.margin.get(side) {
                    out.push_str(&format!(
                        "{}{} ",
                        word,
                        (mm * 72.0 / 25.4 * 20.0).round() as u32
                    ));
                }
            }
        }
    }
    out.push('\n');

    let mut ordered_counters: Vec<u32> = Vec::new();
    let mut pending_break = false;
    let mut first_para = true;
    // Consumes one block's break state: `break-before` (or a pending `break-after`
    // from the previous block) breaks unless this is the first paragraph.
    let mut take_break = |el: Option<&ElStyle>| -> bool {
        match el {
            Some(el) => {
                let bb = (el.break_before || pending_break) && !first_para;
                first_para = false;
                pending_break = el.break_after;
                bb
            }
            None => false,
        }
    };
    let mut i = 0;
    while i < sheet.paragraphs.len() {
        let p = &sheet.paragraphs[i];
        match p.tag.as_ref().map(|t| &t.kind) {
            Some(TagKind::OrderedList) => {
                let n = ordered_counters.last().copied().unwrap_or(0) + 1;
                ordered_counters.push(n);
                let bb = take_break(theme.map(|t| t.element_for(Some(&TagKind::OrderedList))));
                out.push_str(&ordered_item(p, n, theme, bb));
                out.push('\n');
                i += 1;
                continue;
            }
            _ => ordered_counters.clear(),
        }
        let bb = take_break(theme.map(|t| t.element_for(p.tag.as_ref().map(|t| &t.kind))));
        out.push_str(&render_paragraph(p, theme, bb));
        out.push('\n');
        i += 1;
    }

    out.push('}');
    out
}

/// The font table. Unthemed output keeps the historical two entries; a theme appends
/// the body's custom family (unless it *is* Calibri or Consolas) and the code family's
/// if it differs from both.
fn font_table(theme: Option<&ExportTheme>) -> String {
    let mut table = String::from(r"{\fonttbl{\f0 Calibri;}{\f1 Consolas;}}");
    let custom = theme_custom_fonts(theme);
    if custom.is_empty() {
        return table;
    }
    // Insert custom faces before the table's closing brace.
    assert_eq!(table.pop(), Some('}'));
    for (slot, name) in custom.iter().enumerate() {
        table.push_str(&format!("{{\\f{} {};}}", slot + 2, name));
    }
    table.push('}');
    table
}

/// Distinct custom font families the theme uses, body first: `\f2`, `\f3`, …
fn theme_custom_fonts(theme: Option<&ExportTheme>) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |name: &Option<String>| {
        if let Some(name) = name {
            if !["calibri", "consolas"].contains(&name.to_lowercase().as_str())
                && !out.iter().any(|n: &String| n.eq_ignore_ascii_case(name))
            {
                out.push(name.clone());
            }
        }
    };
    if let Some(t) = theme {
        push(&t.body.family);
        push(&t.code.family);
    }
    out
}

/// The color table, or `None` when the theme uses no colors (leaving the header
/// byte-identical to the unthemed one).
fn color_table(theme: Option<&ExportTheme>) -> Option<String> {
    let t = theme?;
    let mut colors: Vec<(u8, u8, u8)> = Vec::new();
    for el in [&t.body, &t.h1, &t.h2, &t.h3, &t.quote, &t.code] {
        if let Some(c) = el.color {
            if !colors.contains(&c) {
                colors.push(c);
            }
        }
    }
    if colors.is_empty() {
        return None;
    }
    let mut table = String::from(r"{\colortbl;");
    for (r, g, b) in colors {
        table.push_str(&format!(r"\red{r}\green{g}\blue{b};"));
    }
    table.push('}');
    Some(table)
}

/// A 1-based color-table index for `color`, or `None` when theming is off or the color
/// isn't in the table (it always is — both are built from the same element walk).
fn color_index(theme: Option<&ExportTheme>, color: Option<(u8, u8, u8)>) -> Option<usize> {
    let color = color?;
    let t = theme?;
    let mut colors: Vec<(u8, u8, u8)> = Vec::new();
    for el in [&t.body, &t.h1, &t.h2, &t.h3, &t.quote, &t.code] {
        if let Some(c) = el.color {
            if !colors.contains(&c) {
                colors.push(c);
            }
        }
    }
    colors.iter().position(|&c| c == color).map(|i| i + 1)
}

/// `\pard` plus the theme's page break, alignment, indentation and font state for
/// `el`. `font` is the paragraph's own font slot; `base_indent` is the legacy indent
/// for constructs that have one (lists 360, quotes 720 twips), kept unless the
/// stylesheet sets its own left margin.
fn para_prefix(
    t: &ExportTheme,
    el: &ElStyle,
    font: u32,
    base_indent: Option<i32>,
    page_break: bool,
) -> String {
    let mut s = String::from(r"\pard");
    if page_break {
        s.push_str(r"\pagebb");
    }
    s.push_str(match el.align {
        Align::Left => "",
        Align::Center => r"\qc",
        Align::Right => r"\qr",
        Align::Justify => r"\qj",
    });
    let indent = el
        .margin_left
        .map(|pt| (pt * 20.0).round() as i32)
        .or(base_indent);
    if let Some(tw) = indent {
        s.push_str(&format!(r"\li{tw}"));
    }
    if let Some(pt) = el.text_indent {
        s.push_str(&format!(r"\fi{}", (pt * 20.0).round() as i32));
    }
    s.push_str(&format!(r"\f{font}"));
    s.push_str(&format!(
        r"\fs{}",
        (el.size_pt * 2.0).round().max(1.0) as i32
    ));
    if let Some(i) = color_index(Some(t), el.color) {
        s.push_str(&format!(r"\cf{i}"));
    }
    if el.bold {
        s.push_str(r"\b");
    }
    if el.italic {
        s.push_str(r"\i");
    }
    s.push(' ');
    s
}

/// Closes the weight/style toggles `para_prefix` opened (font, size and color are
/// re-stated by every paragraph, so they need no reset).
fn para_suffix(el: &ElStyle) -> &'static str {
    match (el.bold, el.italic) {
        (true, true) => r"\b0\i0 ",
        (true, false) => r"\b0 ",
        (false, true) => r"\i0 ",
        (false, false) => "",
    }
}

fn render_paragraph_themed(p: &Paragraph, t: &ExportTheme, page_break: bool) -> String {
    let kind = p.tag.as_ref().map(|t| &t.kind);
    let el = t.element_for(kind);
    match kind {
        // Dividers keep their fixed centered rule rendering in every theme.
        Some(TagKind::Divider) => {
            let bb = if page_break { r"\pagebb" } else { "" };
            format!(r"\pard{bb}\qc \emdash\emdash\emdash\emdash\emdash\par")
        }
        Some(TagKind::OrderedList) => {
            unreachable!("ordered-list numbering handled by export_rtf_inner")
        }
        Some(TagKind::UnorderedList) => format!(
            "{}\u{2022}  {}\\par",
            para_prefix(
                t,
                el,
                font_slot(Some(t), el.family.as_deref(), false),
                Some(360),
                page_break
            ),
            render_inlines_themed(&p.inlines, el, t),
        ),
        Some(TagKind::CodeBlock(_)) => format!(
            "{}{}\\par",
            para_prefix(
                t,
                el,
                font_slot(Some(t), el.family.as_deref(), true),
                None,
                page_break
            ),
            escape_rtf(&flatten_text(&p.inlines)),
        ),
        Some(TagKind::Comment) => {
            // Bracketed like the legacy form; the toggles are skipped when the
            // element is already that style.
            let (open, close) = if el.italic {
                ("[", "]")
            } else {
                (r"\i [", r"]\i0 ")
            };
            let mut out = para_prefix(
                t,
                el,
                font_slot(Some(t), el.family.as_deref(), false),
                None,
                page_break,
            );
            out.push_str(open);
            out.push_str(&escape_rtf(&flatten_text(&p.inlines)));
            out.push_str(close);
            out.push_str(para_suffix(el));
            out.push_str(r"\par");
            out
        }
        _ => {
            // Block quotes keep their legacy hanging indent unless the theme overrides it.
            let base_indent = match kind {
                Some(TagKind::BlockQuote) => Some(720),
                _ => None,
            };
            format!(
                "{}{}{}\\par",
                para_prefix(
                    t,
                    el,
                    font_slot(Some(t), el.family.as_deref(), false),
                    base_indent,
                    page_break
                ),
                render_inlines_themed(&p.inlines, el, t),
                para_suffix(el),
            )
        }
    }
}
/// The `\fN` slot a themed run renders in: the body's custom slot when the element
/// uses the body family, the code slot for code, else the Calibri default (`\f0`).
fn font_slot(theme: Option<&ExportTheme>, family: Option<&str>, code: bool) -> u32 {
    let custom = theme_custom_fonts(theme);
    let slot = |name: Option<&str>| {
        custom
            .iter()
            .position(|n| Some(n.as_str()) == name)
            .map(|i| (i + 2) as u32)
    };
    if code {
        slot(family).unwrap_or(1)
    } else {
        slot(family).unwrap_or(0)
    }
}

fn ordered_item(p: &Paragraph, n: u32, theme: Option<&ExportTheme>, page_break: bool) -> String {
    match theme {
        None => format!(r"\pard\li360 {n}. {}\par", render_inlines(&p.inlines)),
        Some(t) => {
            let el = t.element_for(p.tag.as_ref().map(|t| &t.kind));
            format!(
                "{}{n}. {}\\par",
                para_prefix(
                    t,
                    el,
                    font_slot(Some(t), el.family.as_deref(), false),
                    Some(360),
                    page_break
                ),
                render_inlines_themed(&p.inlines, el, t)
            )
        }
    }
}

fn render_paragraph(p: &Paragraph, theme: Option<&ExportTheme>, page_break: bool) -> String {
    match theme {
        None => render_paragraph_legacy(p),
        Some(t) => render_paragraph_themed(p, t, page_break),
    }
}

/// Carry-through state for themed inline rendering: the element being rendered,
/// the bold/italic already active from it or enclosing markup (re-toggling would
/// cancel the style), and the font slots for running text vs code spans.
#[derive(Clone, Copy)]
struct TCtx {
    bold: bool,
    italic: bool,
    f_body: u32,
    f_code: u32,
}

fn render_inlines_themed(inlines: &[Inline], el: &ElStyle, t: &ExportTheme) -> String {
    // A code span keeps the monospace default unless the stylesheet names its own
    // family with a `pre`/`code` rule.
    let ctx = TCtx {
        bold: el.bold,
        italic: el.italic,
        f_body: font_slot(Some(t), el.family.as_deref(), false),
        f_code: font_slot(Some(t), t.code.family.as_deref(), true),
    };
    let mut out = String::new();
    render_inlines_inner(inlines, ctx, &mut out);
    out
}

fn render_inlines_inner(inlines: &[Inline], ctx: TCtx, out: &mut String) {
    for inline in inlines {
        render_inline_themed(inline, ctx, out);
    }
}

fn render_inline_themed(inline: &Inline, ctx: TCtx, out: &mut String) {
    match inline {
        Inline::Text(s) => out.push_str(&escape_rtf(s)),
        Inline::Strong(c) => {
            if ctx.bold {
                render_inlines_inner(c, ctx, out);
            } else {
                out.push_str(r"\b ");
                let inner = TCtx { bold: true, ..ctx };
                render_inlines_inner(c, inner, out);
                out.push_str(r"\b0 ");
            }
        }
        Inline::Emph(c) => {
            if ctx.italic {
                render_inlines_inner(c, ctx, out);
            } else {
                out.push_str(r"\i ");
                let inner = TCtx {
                    italic: true,
                    ..ctx
                };
                render_inlines_inner(c, inner, out);
                out.push_str(r"\i0 ");
            }
        }
        Inline::Code(s) => {
            out.push_str(&format!(
                "\\f{} {} \\f{} ",
                ctx.f_code,
                escape_rtf(s),
                ctx.f_body
            ));
        }
        Inline::Link { url, content } => {
            let mut inner = String::new();
            render_inlines_inner(content, ctx, &mut inner);
            out.push_str(&format!(
                r#"{{\field{{\*\fldinst{{HYPERLINK "{}"}}}}{{\fldrslt {}}}}}"#,
                escape_rtf(url),
                inner
            ));
        }
        Inline::Footnote { content } => {
            out.push_str("\\super (");
            render_inlines_inner(content, ctx, out);
            out.push_str(")\\nosupersub ");
        }
        Inline::Delete(c) => {
            out.push_str("\\strike ");
            render_inlines_inner(c, ctx, out);
            out.push_str("\\strike0 ");
        }
        Inline::InlineComment(c) => {
            // Bracketed like the legacy form; the toggles are skipped when the
            // element is already italic.
            if !ctx.italic {
                out.push_str("\\i ");
            }
            out.push_str(&format!("[{}]", {
                let mut inner = String::new();
                render_inlines_inner(c, ctx, &mut inner);
                inner
            }));
            if !ctx.italic {
                out.push_str("\\i0 ");
            }
        }
        Inline::InlineNative(s) => out.push_str(&escape_rtf(s)),
        Inline::Mark(c) => {
            out.push_str("\\highlight3 ");
            render_inlines_inner(c, ctx, out);
            out.push_str("\\highlight0 ");
        }
        Inline::Annotation { note, content } => {
            render_inlines_inner(content, ctx, out);
            out.push_str(&format!(" [{}]", escape_rtf(note)));
        }
        Inline::Image { alt, .. } => {
            out.push_str(&escape_rtf(&format!(
                "[image: {}]",
                alt.as_deref().unwrap_or("")
            )));
        }
        Inline::Video { url } => out.push_str(&escape_rtf(&format!("[video: {url}]"))),
    }
}

fn render_paragraph_legacy(p: &Paragraph) -> String {
    match p.tag.as_ref().map(|t| &t.kind) {
        None => format!(r"\pard {}\par", render_inlines(&p.inlines)),
        Some(TagKind::Heading(n)) => {
            let size = 48 - (*n as i32 - 1) * 6;
            format!(
                r"\pard\b\fs{size} {}\b0\fs24\par",
                render_inlines(&p.inlines)
            )
        }
        Some(TagKind::BlockQuote) => {
            format!(r"\pard\li720\i {}\i0\par", render_inlines(&p.inlines))
        }
        Some(TagKind::CodeBlock(_)) => {
            format!(r"\pard\f1 {}\f0\par", escape_rtf(&flatten_text(&p.inlines)))
        }
        Some(TagKind::Comment) => {
            format!(
                r"\pard\i [{}]\i0\par",
                escape_rtf(&flatten_text(&p.inlines))
            )
        }
        Some(TagKind::Divider) => r"\pard\qc \emdash\emdash\emdash\emdash\emdash\par".to_string(),
        Some(TagKind::NativeBlock) => {
            format!(r"\pard {}\par", escape_rtf(&flatten_text(&p.inlines)))
        }
        Some(TagKind::UnorderedList) => {
            format!(r"\pard\li360 \bullet  {}\par", render_inlines(&p.inlines))
        }
        Some(TagKind::OrderedList) => unreachable!("ordered-list numbering handled by export_rtf"),
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    inlines.iter().map(render_inline).collect()
}

fn render_inline(inline: &Inline) -> String {
    match inline {
        Inline::Text(s) => escape_rtf(s),
        Inline::Strong(c) => format!(r"\b {}\b0 ", render_inlines(c)),
        Inline::Emph(c) => format!(r"\i {}\i0 ", render_inlines(c)),
        Inline::Code(s) => format!(r"\f1 {}\f0 ", escape_rtf(s)),
        Inline::Link { url, content } => {
            format!(
                r#"{{\field{{\*\fldinst{{HYPERLINK "{}"}}}}{{\fldrslt {}}}}}"#,
                escape_rtf(url),
                render_inlines(content)
            )
        }
        Inline::Footnote { content } => {
            format!(r"\super ({})\nosupersub ", render_inlines(content))
        }
        Inline::Delete(c) => format!(r"\strike {}\strike0 ", render_inlines(c)),
        Inline::InlineComment(c) => format!(r"\i [{}]\i0 ", render_inlines(c)),
        Inline::InlineNative(s) => escape_rtf(s),
        Inline::Mark(c) => format!(r"\highlight3 {}\highlight0 ", render_inlines(c)),
        Inline::Annotation { note, content } => {
            format!(r"{} [{}]", render_inlines(content), escape_rtf(note))
        }
        Inline::Image { alt, .. } => {
            escape_rtf(&format!("[image: {}]", alt.as_deref().unwrap_or("")))
        }
        Inline::Video { url } => escape_rtf(&format!("[video: {url}]")),
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

/// Escapes `\`, `{`, `}` (RTF control syntax) and encodes any non-ASCII character as a
/// `\uN?` Unicode escape (the standard RTF fallback form, `?` as the placeholder for readers
/// that ignore the escape).
fn escape_rtf(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '{' => out.push_str(r"\{"),
            '}' => out.push_str(r"\}"),
            c if c.is_ascii() => out.push(c),
            c => out.push_str(&format!(r"\u{}?", c as u32 as i32)),
        }
    }
    out
}

/// Structural validity: balanced `{`/`}` braces (ignoring escaped `\{`/`\}`) and a well-formed
/// `\rtf1` header — not a full RTF grammar check, matching Decision D5's "valid, not
/// byte-match" scope.
pub fn is_structurally_valid(rtf: &str) -> bool {
    if !rtf.starts_with(r"{\rtf1") || !rtf.ends_with('}') {
        return false;
    }
    let mut depth: i32 = 0;
    let mut chars = rtf.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::full_tag_document;
    use parrot_core::TagMark;

    #[test]
    fn escapes_rtf_control_characters() {
        assert_eq!(escape_rtf(r"a\b{c}"), r"a\\b\{c\}");
    }

    #[test]
    fn non_ascii_becomes_a_unicode_escape() {
        assert_eq!(escape_rtf("café"), r"caf\u233?");
    }

    fn themed(css: &str) -> ExportTheme {
        ExportTheme::from_css(css)
    }

    fn heading_doc(text: &str) -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text(text.to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![Inline::Text("Body text.".to_string())],
            },
        ])
    }

    #[test]
    fn themed_output_stays_structurally_valid() {
        let rtf = export_rtf_with(
            &full_tag_document(),
            &themed("body { font-family: Georgia, serif; color: #112233; } h1 { font-size: 28pt; text-align: center; } @page { margin: 1in; }"),
        );
        assert!(
            is_structurally_valid(&rtf),
            "themed RTF output is structurally invalid:\n{rtf}"
        );
    }

    #[test]
    fn themed_output_declares_fonts_and_colors() {
        let rtf = export_rtf_with(
            &heading_doc("Big"),
            &themed(
                "body { font-family: Georgia, serif; color: #112233; } h1 { font-size: 28pt; }",
            ),
        );
        // Custom body font lands in the font table; h1 gets its size and color.
        assert!(rtf.contains(r"{\f2 Georgia;}"), "font table:\n{rtf}");
        assert!(rtf.contains(r"\colortbl;"), "color table:\n{rtf}");
        assert!(rtf.contains(r"\fs56"), "28pt heading:\n{rtf}");
        assert!(rtf.contains(r"\cf1"), "themed color:\n{rtf}");
        // The default export carries neither table entries nor overrides.
        let plain = export_rtf(&heading_doc("Big"));
        assert!(!plain.contains(r"\f2"), "no custom fonts unthemed");
        assert!(!plain.contains("colortbl"), "no colors unthemed");
    }

    #[test]
    fn themed_center_and_indent_show_up() {
        let rtf = export_rtf_with(
            &heading_doc("Big"),
            &themed("h1 { text-align: center; } body { margin-left: 36pt; }"),
        );
        assert!(rtf.contains(r"\qc"), "centered heading:\n{rtf}");
        assert!(rtf.contains(r"\li720"), "36pt = 720 twips:\n{rtf}");
    }

    #[test]
    fn page_rule_sets_paper_size_margins_and_breaks() {
        let rtf = export_rtf_with(
            &heading_doc("Big"),
            &themed("h1 { break-before: always; } @page { size: letter; margin: 1in; }"),
        );
        // Letter = 12240 x 15840 twips; 1in margins = 1440 twips.
        assert!(rtf.contains(r"\paperw12240"), "letter width:\n{rtf}");
        assert!(rtf.contains(r"\paperh15840"), "letter height:\n{rtf}");
        assert!(rtf.contains(r"\margt1440"), "top margin:\n{rtf}");
        // The h1 is the first paragraph, so no break yet; add a second heading.
        let two = SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("One".to_string())],
            },
            Paragraph {
                tag: Some(parrot_core::TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("Two".to_string())],
            },
        ]);
        let rtf = export_rtf_with(
            &two,
            &themed("h1 { break-before: always; } @page { size: letter; margin: 1in; }"),
        );
        assert_eq!(
            rtf.matches(r"\pagebb").count(),
            1,
            "second h1 breaks, first does not:\n{rtf}"
        );
        assert!(
            is_structurally_valid(&rtf),
            "paged RTF output is structurally invalid:\n{rtf}"
        );
        // Unthemed output carries no paper sizing or breaks.
        let plain = export_rtf(&two);
        assert!(!plain.contains("paperw"), "no paper size unthemed");
        assert!(!plain.contains("pagebb"), "no breaks unthemed");
    }
}
