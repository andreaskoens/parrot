//! Conversion engine integration (T-5-01): the MVP's ad-hoc Markdown/PlainText exporters
//! (T-0-10, `mvp.rs`), refactored onto the rule-table + DSL engine built in T-5-02/T-5-03.
//!
//! Markdown-XL is already this project's own native notation — `parrot_markup::render` is the
//! authoritative round-trip serializer (also used for `Text.txt`) — so most tags/inlines here
//! reproduce that exact native syntax directly: there's no real "conversion rule" to express
//! for them, since the native rendering already *is* the Markdown export for those cases. The
//! one case with a genuine confirmed rule is `Inline::Code`: `parrot_markup::render`'s own Code
//! case is an unconditional single-backtick wrap — correct for round-tripping the app's own
//! notation (never re-parsed by a third-party CommonMark parser, so it never needed backtick-
//! escaping) — but Markdown *export*, read by third-party tools, does need the confirmed real
//! backtick-escaping rule from `Markdown/Conversion/Default.plist`. That's the one case this
//! module actually routes through `rules::RuleTable` + `dsl`, using `markdown.toml`/
//! `plaintext.toml` (embedded from the crate's `rules/` directory).
//!
//! Done when (T-5-01): output byte-identical to the pre-refactor golden files. The existing
//! confirmed-example fixture has no `Inline::Code` node, so this refactor is a pure regression
//! check for every other tag/inline, while also fixing a real gap (missing backtick-escaping)
//! that the ad-hoc renderer never had reason to handle.

use crate::dsl::{eval_bool, render as dsl_render, EvalContext};
use crate::rules::RuleTable;
use parrot_core::{Inline, Paragraph, SheetContent};
use std::sync::OnceLock;

const MARKDOWN_RULES_TOML: &str = include_str!("../rules/markdown.toml");
const PLAINTEXT_RULES_TOML: &str = include_str!("../rules/plaintext.toml");

fn markdown_rules() -> &'static RuleTable {
    static TABLE: OnceLock<RuleTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        RuleTable::load(MARKDOWN_RULES_TOML).unwrap_or_else(|e| {
            unreachable!("parrot-export's own embedded markdown.toml must be valid: {e}")
        })
    })
}

fn plaintext_rules() -> &'static RuleTable {
    static TABLE: OnceLock<RuleTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        RuleTable::load(PLAINTEXT_RULES_TOML).unwrap_or_else(|e| {
            unreachable!("parrot-export's own embedded plaintext.toml must be valid: {e}")
        })
    })
}

/// Dispatches `code_text` through `table`'s "code" rules (first matching precondition wins),
/// falling back to a plain single-backtick wrap if the table has no match or a rule fails to
/// evaluate — export never panics on a rule-table miss, it just degrades to the pre-engine
/// behavior.
fn convert_code(table: &RuleTable, code_text: &str) -> String {
    let ctx = EvalContext {
        content_plain_text: code_text.to_string(),
    };
    let rule = table.dispatch("code", |precondition| {
        eval_bool(precondition, &ctx).unwrap_or(false)
    });
    rule.and_then(|r| dsl_render(&r.template, &ctx).ok())
        .unwrap_or_else(|| format!("`{code_text}`"))
}

pub fn export_markdown(content: &SheetContent) -> String {
    let table = markdown_rules();
    let mut out = content
        .paragraphs
        .iter()
        .map(|p| render_paragraph(p, table))
        .collect::<Vec<_>>()
        .join("\n\n");
    out.push('\n');
    out
}

fn render_paragraph(p: &Paragraph, table: &RuleTable) -> String {
    let mut s = String::new();
    if let Some(tag) = &p.tag {
        s.push_str(&tag.literal);
    }
    for inline in &p.inlines {
        render_inline(inline, table, &mut s);
    }
    s
}

fn render_inline(inline: &Inline, table: &RuleTable, out: &mut String) {
    match inline {
        Inline::Text(t) => out.push_str(t),
        Inline::Strong(children) => {
            out.push_str("**");
            for c in children {
                render_inline(c, table, out);
            }
            out.push_str("**");
        }
        Inline::Emph(children) => {
            out.push('*');
            for c in children {
                render_inline(c, table, out);
            }
            out.push('*');
        }
        Inline::Code(s) => out.push_str(&convert_code(table, s)),
        Inline::Link { url, content } => {
            out.push('[');
            for c in content {
                render_inline(c, table, out);
            }
            out.push_str("](");
            out.push_str(url);
            out.push(')');
        }
        Inline::Footnote { content } => {
            out.push_str("(fn: ");
            for c in content {
                render_inline(c, table, out);
            }
            out.push(')');
        }
        Inline::Delete(children) => {
            out.push_str("||");
            for c in children {
                render_inline(c, table, out);
            }
            out.push_str("||");
        }
        Inline::InlineComment(children) => {
            out.push_str("++");
            for c in children {
                render_inline(c, table, out);
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
                render_inline(c, table, out);
            }
            out.push_str("::");
        }
        Inline::Annotation { note, content } => {
            out.push('{');
            for c in content {
                render_inline(c, table, out);
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

pub fn export_plain_text(content: &SheetContent) -> String {
    let table = plaintext_rules();
    let mut out = content
        .paragraphs
        .iter()
        .map(|p| render_plain_paragraph(p, table))
        .collect::<Vec<_>>()
        .join("\n\n");
    out.push('\n');
    out
}

fn render_plain_paragraph(p: &Paragraph, table: &RuleTable) -> String {
    let mut s = String::new();
    for inline in &p.inlines {
        render_plain_inline(inline, table, &mut s);
    }
    s
}

fn render_plain_inline(inline: &Inline, table: &RuleTable, out: &mut String) {
    match inline {
        Inline::Text(t) => out.push_str(t),
        Inline::Code(s) => out.push_str(&convert_code(table, s)),
        Inline::Strong(children)
        | Inline::Emph(children)
        | Inline::Footnote { content: children }
        | Inline::Delete(children)
        | Inline::InlineComment(children)
        | Inline::Mark(children) => {
            for c in children {
                render_plain_inline(c, table, out);
            }
        }
        Inline::Link { content, .. } | Inline::Annotation { content, .. } => {
            for c in content {
                render_plain_inline(c, table, out);
            }
        }
        Inline::InlineNative(s) => out.push_str(s),
        Inline::Image { alt, .. } => {
            if let Some(a) = alt {
                out.push_str(a);
            }
        }
        Inline::Video { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{TagKind, TagMark};

    /// Same confirmed example as `mvp.rs`'s own tests — the pre-refactor golden files this
    /// task's own Done-when criterion is about.
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
    fn markdown_matches_pre_refactor_golden() {
        let golden = include_str!("../../tests/fixtures/mvp/confirmed_example.txt");
        assert_eq!(export_markdown(&confirmed_example()), golden);
    }

    #[test]
    fn plain_text_matches_pre_refactor_golden() {
        let golden = include_str!("../../tests/fixtures/mvp/confirmed_example.plain.txt");
        assert_eq!(export_plain_text(&confirmed_example()), golden);
    }

    #[test]
    fn code_span_without_backtick_gets_single_backtick_wrap() {
        let sheet = SheetContent::new(vec![Paragraph {
            tag: None,
            inlines: vec![Inline::Code("fn main()".to_string())],
        }]);
        assert_eq!(export_markdown(&sheet), "`fn main()`\n");
    }

    #[test]
    fn code_span_containing_a_backtick_gets_double_backtick_wrap() {
        let sheet = SheetContent::new(vec![Paragraph {
            tag: None,
            inlines: vec![Inline::Code("a`b".to_string())],
        }]);
        assert_eq!(export_markdown(&sheet), "``a`b``\n");
    }

    #[test]
    fn plain_text_code_span_has_no_wrapping() {
        let sheet = SheetContent::new(vec![Paragraph {
            tag: None,
            inlines: vec![Inline::Code("fn main()".to_string())],
        }]);
        assert_eq!(export_plain_text(&sheet), "fn main()\n");
    }
}
