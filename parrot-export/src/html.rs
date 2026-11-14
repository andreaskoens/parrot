//! HTML backend (T-5-04): direct AST tree walk, not a Markdown-text pipeline (`pulldown-cmark`)
//! since the input is already a parsed AST. Output is always self-closed/XHTML-style for void
//! elements (`<hr/>`, `<img .../>`) so it stays well-formed XML, not just well-formed HTML5 —
//! that lets the test suite validate structure with `quick-xml` (already a workspace
//! dependency) instead of pulling in a dedicated HTML parser.
//!
//! ASSUMPTION(D-export-2): footnotes render inline as `<sup class="footnote">(note)</sup>`
//! rather than a separate numbered footnote list with back-reference anchors — a reasonable
//! simplification for a direct tree-walk backend, not a confirmed Parrot HTML-export behavior
//! (no real `.html` export sample was available to check against). `InlineNative`/`NativeBlock`
//! content is always escaped as opaque text, the same as in every other backend (see
//! DECISIONS.md) — its exact target-format-specific semantics aren't confirmed, so treating it
//! as raw unescaped markup risked structural injection for no confirmed benefit.

use crate::css::ExportTheme;
use parrot_core::{Inline, Paragraph, SheetContent, TagKind};

pub fn export_html(sheet: &SheetContent) -> String {
    export_html_with(sheet, &ExportTheme::builtin())
}

/// [`export_html`], with a stylesheet applied: the theme's raw CSS is embedded verbatim
/// in a `<style>` element (wrapped so the output stays well-formed XML even if the CSS
/// contains `<` or `&`, which HTML parses as raw text). A theme without CSS emits no
/// `<style>` tag, byte-identical to [`export_html`].
pub fn export_html_with(sheet: &SheetContent, theme: &ExportTheme) -> String {
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"/><title>Export</title>{}</head><body>{}</body></html>",
        style_block(&theme.raw_css),
        render_paragraphs(&sheet.paragraphs)
    )
}

/// A `<style>` element for `css`, or nothing when it is blank.
fn style_block(css: &str) -> String {
    if css.trim().is_empty() {
        String::new()
    } else {
        format!("<style>/*<![CDATA[*/\n{css}\n/*]]>*/</style>")
    }
}

fn render_paragraphs(paragraphs: &[Paragraph]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < paragraphs.len() {
        let kind = paragraphs[i].tag.as_ref().map(|t| &t.kind);
        match kind {
            Some(TagKind::UnorderedList) => {
                let consumed = render_list(&mut out, &paragraphs[i..], "ul", |k| {
                    matches!(k, TagKind::UnorderedList)
                });
                i += consumed;
            }
            Some(TagKind::OrderedList) => {
                let consumed = render_list(&mut out, &paragraphs[i..], "ol", |k| {
                    matches!(k, TagKind::OrderedList)
                });
                i += consumed;
            }
            _ => {
                out.push_str(&render_single_paragraph(&paragraphs[i]));
                i += 1;
            }
        }
    }
    out
}

fn render_list(
    out: &mut String,
    paragraphs: &[Paragraph],
    tag: &str,
    matches_kind: impl Fn(&TagKind) -> bool,
) -> usize {
    let mut consumed = 0;
    let mut items = String::new();
    for p in paragraphs {
        let is_match = p.tag.as_ref().is_some_and(|t| matches_kind(&t.kind));
        if !is_match {
            break;
        }
        items.push_str("<li>");
        items.push_str(&render_inlines(&p.inlines));
        items.push_str("</li>");
        consumed += 1;
    }
    out.push_str(&format!("<{tag}>{items}</{tag}>"));
    consumed
}

fn render_single_paragraph(p: &Paragraph) -> String {
    match p.tag.as_ref().map(|t| &t.kind) {
        None => format!("<p>{}</p>", render_inlines(&p.inlines)),
        Some(TagKind::Heading(n)) => {
            format!("<h{n}>{}</h{n}>", render_inlines(&p.inlines))
        }
        Some(TagKind::BlockQuote) => {
            format!(
                "<blockquote><p>{}</p></blockquote>",
                render_inlines(&p.inlines)
            )
        }
        Some(TagKind::CodeBlock(lang)) => {
            let class = lang
                .as_ref()
                .map(|l| format!(" class=\"language-{}\"", escape_attr(l)))
                .unwrap_or_default();
            format!(
                "<pre><code{class}>{}</code></pre>",
                escape_text(&flatten_text(&p.inlines))
            )
        }
        Some(TagKind::Comment) => {
            format!("<!-- {} -->", flatten_text(&p.inlines).replace("--", "- -"))
        }
        Some(TagKind::Divider) => "<hr/>".to_string(),
        Some(TagKind::NativeBlock) => {
            format!(
                "<div class=\"native-block\">{}</div>",
                escape_text(&flatten_text(&p.inlines))
            )
        }
        Some(TagKind::UnorderedList) | Some(TagKind::OrderedList) => {
            unreachable!("list paragraphs are grouped by render_list, not reached here")
        }
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    inlines.iter().map(render_inline).collect()
}

fn render_inline(inline: &Inline) -> String {
    match inline {
        Inline::Text(s) => escape_text(s),
        Inline::Strong(c) => format!("<strong>{}</strong>", render_inlines(c)),
        Inline::Emph(c) => format!("<em>{}</em>", render_inlines(c)),
        Inline::Code(s) => format!("<code>{}</code>", escape_text(s)),
        Inline::Link { url, content } => {
            format!(
                "<a href=\"{}\">{}</a>",
                escape_attr(url),
                render_inlines(content)
            )
        }
        Inline::Footnote { content } => {
            format!(
                "<sup class=\"footnote\">({})</sup>",
                render_inlines(content)
            )
        }
        Inline::Delete(c) => format!("<del>{}</del>", render_inlines(c)),
        Inline::InlineComment(c) => format!("<span class=\"comment\">{}</span>", render_inlines(c)),
        Inline::InlineNative(s) => escape_text(s),
        Inline::Mark(c) => format!("<mark>{}</mark>", render_inlines(c)),
        Inline::Annotation { note, content } => format!(
            "<span class=\"annotation\" data-note=\"{}\">{}</span>",
            escape_attr(note),
            render_inlines(content)
        ),
        Inline::Image { url, alt } => format!(
            "<img src=\"{}\" alt=\"{}\"/>",
            escape_attr(url),
            escape_attr(alt.as_deref().unwrap_or(""))
        ),
        Inline::Video { url } => format!("<video src=\"{}\"/>", escape_attr(url)),
    }
}

/// Recursively extracts raw text content (ignoring inline markup), for the places formatting
/// doesn't apply — code-block/comment/native-block bodies.
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

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::full_tag_document;

    #[test]
    fn well_formed_for_full_tag_document() {
        let html = export_html(&full_tag_document());

        use quick_xml::events::Event;
        use quick_xml::reader::Reader;
        let mut reader = Reader::from_str(&html);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("HTML export is not well-formed XML: {e} in:\n{html}"),
            }
        }
    }

    #[test]
    fn escapes_special_characters_in_text_and_attributes() {
        let sheet = SheetContent::new(vec![Paragraph {
            tag: None,
            inlines: vec![
                Inline::Text("<script>&\"</script>".to_string()),
                Inline::Link {
                    url: "http://example.com/?a=1&b=\"2\"".to_string(),
                    content: vec![Inline::Text("link".to_string())],
                },
            ],
        }]);
        let html = export_html(&sheet);
        // Text content only needs &,<,> escaped (a literal '"' is well-formed there); attribute
        // values additionally need '"' escaped since they're themselves quoted with it.
        assert!(html.contains("&lt;script&gt;&amp;\"&lt;/script&gt;"));
        assert!(html.contains("href=\"http://example.com/?a=1&amp;b=&quot;2&quot;\""));
    }

    #[test]
    fn unstyled_output_has_no_style_element() {
        let html = export_html(&full_tag_document());
        assert!(
            !html.contains("<style>"),
            "default export must not gain a style tag"
        );
    }

    #[test]
    fn styled_output_embeds_the_css_verbatim() {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;
        let css = "body { color: #112233; }\nh1 { font-size: 28pt; }";
        let html = export_html_with(&full_tag_document(), &ExportTheme::from_css(css));
        assert!(
            html.contains("<style>"),
            "styled export embeds a style element"
        );
        assert!(html.contains(css), "the stylesheet is embedded verbatim");
        // Still well-formed XML: the CDATA wrapper keeps the CSS from breaking parsing.
        let mut reader = Reader::from_str(&html);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("styled HTML is not well-formed XML: {e}"),
            }
        }
    }

    #[test]
    fn blank_css_is_the_same_as_no_css() {
        let plain = export_html(&full_tag_document());
        let builtin = ExportTheme::builtin();
        assert_eq!(
            export_html_with(&full_tag_document(), &ExportTheme::from_css("  \n ")),
            plain,
            "blank stylesheets embed nothing"
        );
        assert_eq!(
            export_html_with(&full_tag_document(), &builtin),
            plain,
            "builtin theme is byte-identical to no theme"
        );
    }
}
