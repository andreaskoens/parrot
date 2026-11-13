//! RTF backend (T-5-05): hand-rolled RTF 1.9 writer. Decision D5: **not** attempting RTFKit
//! fidelity — its internals aren't confirmed — so the target is "valid, opens correctly in
//! Word/LibreOffice/TextEdit," not a byte-for-byte match with Parrot' own RTF output.
//!
//! RTF has no native list-table or comment-field support cheap enough to hand-roll correctly
//! here, so (same opaque-content stance as the HTML backend, see DECISIONS.md): lists render as
//! plain bullet-prefixed/numbered paragraphs, not `\pard\tx\ls` list tables; block comments
//! render as an italicized bracketed paragraph; dividers render as a thin rule-like paragraph of
//! em-dashes. None of these are confirmed real Parrot RTF-export behavior.

use parrot_core::{Inline, Paragraph, SheetContent, TagKind};

pub fn export_rtf(sheet: &SheetContent) -> String {
    let mut out = String::new();
    out.push_str(r"{\rtf1\ansi\deff0");
    out.push_str(r"{\fonttbl{\f0 Calibri;}{\f1 Consolas;}}");
    out.push('\n');

    let mut ordered_counters: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < sheet.paragraphs.len() {
        let p = &sheet.paragraphs[i];
        match p.tag.as_ref().map(|t| &t.kind) {
            Some(TagKind::OrderedList) => {
                let n = ordered_counters.last().copied().unwrap_or(0) + 1;
                ordered_counters.push(n);
                out.push_str(&format!(
                    r"\pard\li360 {n}. {}\par",
                    render_inlines(&p.inlines)
                ));
                out.push('\n');
                i += 1;
                continue;
            }
            _ => ordered_counters.clear(),
        }
        out.push_str(&render_paragraph(p));
        out.push('\n');
        i += 1;
    }

    out.push('}');
    out
}

fn render_paragraph(p: &Paragraph) -> String {
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

    #[test]
    fn structurally_valid_for_full_tag_document() {
        let rtf = export_rtf(&full_tag_document());
        assert!(
            is_structurally_valid(&rtf),
            "RTF output is structurally invalid:\n{rtf}"
        );
    }

    #[test]
    fn escapes_rtf_control_characters() {
        assert_eq!(escape_rtf(r"a\b{c}"), r"a\\b\{c\}");
    }

    #[test]
    fn non_ascii_becomes_a_unicode_escape() {
        assert_eq!(escape_rtf("café"), r"caf\u233?");
    }
}
