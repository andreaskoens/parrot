//! AST -> plain Markdown-XL-tagged text, the confirmed `Text.txt` sibling-file convention
//! (feature inventory §3.3): paragraphs separated by a blank line, with the paragraph's literal
//! tag prefix and inline markers reconstructed inline so the file reads as plain Markdown-XL
//! source, not just extracted text.
//!
//! Markers for strong/emph/code mirror the canonical choice made in `parrot-container::xml`
//! (`**`/`*`/backtick); link/footnote/delete/inlineComment/inlineNative/mark/annotation/
//! image/video plain-text shapes are this project's own decisions (inventory §3.2 confirms the
//! marker *tokens* exist — `[...]`, `(fn)`, `||...||`, `++...++`, `~...~`, `::...::`, `{...}`,
//! `(img)`, `(vid)` — but not their exact inline rendering). Picked plain, readable renderings
//! consistent with the rest of the dialect reading as Markdown-like text; see DECISIONS.md.

use parrot_core::{Inline, Paragraph, SheetContent};

/// Renders a full sheet's AST to its `Text.txt` plain-text form.
pub fn render(content: &SheetContent) -> String {
    let mut out = content
        .paragraphs
        .iter()
        .map(render_paragraph)
        .collect::<Vec<_>>()
        .join("\n\n");
    out.push('\n');
    out
}

fn render_paragraph(p: &Paragraph) -> String {
    let mut s = String::new();
    if let Some(tag) = &p.tag {
        s.push_str(&tag.literal);
    }
    for inline in &p.inlines {
        render_inline(inline, &mut s);
    }
    s
}

fn render_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(t) => out.push_str(t),
        Inline::Strong(children) => {
            out.push_str("**");
            for c in children {
                render_inline(c, out);
            }
            out.push_str("**");
        }
        Inline::Emph(children) => {
            out.push('*');
            for c in children {
                render_inline(c, out);
            }
            out.push('*');
        }
        Inline::Code(s) => {
            out.push('`');
            out.push_str(s);
            out.push('`');
        }
        Inline::Link { url, content } => {
            out.push('[');
            for c in content {
                render_inline(c, out);
            }
            out.push_str("](");
            out.push_str(url);
            out.push(')');
        }
        Inline::Footnote { content } => {
            out.push_str("(fn: ");
            for c in content {
                render_inline(c, out);
            }
            out.push(')');
        }
        Inline::Delete(children) => {
            out.push_str("||");
            for c in children {
                render_inline(c, out);
            }
            out.push_str("||");
        }
        Inline::InlineComment(children) => {
            out.push_str("++");
            for c in children {
                render_inline(c, out);
            }
            out.push_str("++");
        }
        Inline::InlineNative(s) => {
            out.push('~');
            out.push_str(s);
            out.push('~');
        }
        Inline::Mark(children) => {
            out.push_str("::");
            for c in children {
                render_inline(c, out);
            }
            out.push_str("::");
        }
        Inline::Annotation { note, content } => {
            out.push('{');
            for c in content {
                render_inline(c, out);
            }
            if !note.is_empty() {
                out.push_str(": ");
                out.push_str(note);
            }
            out.push('}');
        }
        Inline::Image { url, alt } => {
            out.push_str("![");
            out.push_str(alt.as_deref().unwrap_or(""));
            out.push_str("](");
            out.push_str(url);
            out.push(')');
        }
        Inline::Video { url } => {
            out.push_str("(vid: ");
            out.push_str(url);
            out.push(')');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{TagKind, TagMark};

    /// The literal confirmed example from feature inventory §3.3 (same fixture as
    /// `parrot_container::xml::tests::roundtrip_confirmed_sample`).
    fn confirmed_example() -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(3),
                    literal: "### ".to_string(),
                }),
                inlines: vec![Inline::Text("Heading text".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Body with ".to_string()),
                    Inline::Emph(vec![Inline::Text("italic".to_string())]),
                    Inline::Text(" and ".to_string()),
                    Inline::Link {
                        url: "example.com".to_string(),
                        content: vec![Inline::Text("label".to_string())],
                    },
                    Inline::Text(".".to_string()),
                ],
            },
        ])
    }

    #[test]
    fn matches_golden_text_txt() {
        let golden = include_str!("../../tests/fixtures/mvp/confirmed_example.txt");
        assert_eq!(render(&confirmed_example()), golden);
    }
}
