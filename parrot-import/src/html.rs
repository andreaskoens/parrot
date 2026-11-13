//! HTML importer (T-6-03): `scraper` (html5ever-backed) -> AST, best-effort tag mapping.
//!
//! Unknown/unhandled block elements (div, section, span-as-block, etc.) are treated as
//! transparent — their children are walked as if the wrapper weren't there — rather than
//! dropped, since real-world HTML nests constructs the confirmed Markdown-XL tag table has no
//! equivalent for. Nested lists, tables, and `<pre>` without a `<code>` child aren't specially
//! handled (documented, not silently wrong): tables are skipped entirely. Nested lists keep
//! their level through an indent before the item marker.

use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};
use scraper::{ElementRef, Html, Node, Selector};

pub fn import_html(html: &str) -> SheetContent {
    let document = Html::parse_document(html);
    let body_selector = Selector::parse("body")
        .unwrap_or_else(|e| unreachable!("the literal selector \"body\" must always parse: {e:?}"));
    let root = document
        .select(&body_selector)
        .next()
        .unwrap_or_else(|| document.root_element());

    let mut paragraphs = Vec::new();
    walk_block_children(root, &mut paragraphs);
    SheetContent::new(paragraphs)
}

fn walk_block_children(parent: ElementRef, paragraphs: &mut Vec<Paragraph>) {
    for child in parent.children() {
        let Some(el) = ElementRef::wrap(child) else {
            continue; // bare text directly under a block container: not a confirmed case, skip
        };
        walk_block_element(el, paragraphs);
    }
}

/// One list (`<ul>`/`<ol>`) at nesting `depth`: an item per `<li>`, then that item's nested
/// lists right after it, indented two spaces a level. Each `<ol>` numbers from 1 (or its
/// `start`).
fn walk_list(list: ElementRef, ordered: bool, depth: usize, paragraphs: &mut Vec<Paragraph>) {
    let indent = "  ".repeat(depth);
    let mut number = list
        .value()
        .attr("start")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1);
    for li in list.children().filter_map(ElementRef::wrap) {
        if li.value().name() != "li" {
            continue;
        }
        let is_list = |c: &ElementRef| matches!(c.value().name(), "ul" | "ol");
        let inlines = li
            .children()
            .filter(|c| !ElementRef::wrap(*c).is_some_and(|e| is_list(&e)))
            .filter_map(walk_inline_node)
            .collect();
        let (kind, literal) = if ordered {
            (TagKind::OrderedList, format!("{indent}{number}. "))
        } else {
            (TagKind::UnorderedList, format!("{indent}- "))
        };
        number += 1;
        paragraphs.push(Paragraph {
            tag: Some(TagMark { kind, literal }),
            inlines,
        });
        for nested in li.children().filter_map(ElementRef::wrap).filter(is_list) {
            walk_list(nested, nested.value().name() == "ol", depth + 1, paragraphs);
        }
    }
}

fn walk_block_element(el: ElementRef, paragraphs: &mut Vec<Paragraph>) {
    let name = el.value().name();
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let n = name[1..].parse::<u8>().unwrap_or(1);
            paragraphs.push(Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(n),
                    literal: format!("{} ", "#".repeat(n as usize)),
                }),
                inlines: walk_inline_children(el),
            });
        }
        // `div` is treated the same as `p`: ENML (the HTML dialect T-6-04's ENEX importer feeds
        // through this module) represents each paragraph as `<div>line of text</div>`, and a
        // transparent-wrapper recursion would drop that div's own direct text children (only
        // nested *elements* get walked, not bare text) since nothing further down would ever
        // re-wrap them into a paragraph.
        "p" | "div" => paragraphs.push(Paragraph {
            tag: None,
            inlines: walk_inline_children(el),
        }),
        "blockquote" => paragraphs.push(Paragraph {
            tag: Some(TagMark {
                kind: TagKind::BlockQuote,
                literal: "> ".to_string(),
            }),
            inlines: walk_inline_children(el),
        }),
        "ul" | "ol" => walk_list(el, name == "ol", 0, paragraphs),
        "pre" => {
            let code_el = el
                .children()
                .filter_map(ElementRef::wrap)
                .find(|c| c.value().name() == "code");
            let lang = code_el
                .and_then(|c| c.value().attr("class"))
                .and_then(|class| {
                    class
                        .split_whitespace()
                        .find_map(|c| c.strip_prefix("language-"))
                        .map(str::to_string)
                });
            let literal = match &lang {
                Some(l) => format!("```{l} "),
                None => "``` ".to_string(),
            };
            let text = el
                .text()
                .collect::<String>()
                .trim_end_matches('\n')
                .to_string();
            paragraphs.push(Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::CodeBlock(lang),
                    literal,
                }),
                inlines: vec![Inline::Text(text)],
            });
        }
        "hr" => paragraphs.push(Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Divider,
                literal: "----".to_string(),
            }),
            inlines: vec![],
        }),
        "script" | "style" | "head" | "table" => {} // no confirmed Markdown-XL counterpart — skip
        _ => walk_block_children(el, paragraphs),   // transparent wrapper
    }
}

fn walk_inline_children(el: ElementRef) -> Vec<Inline> {
    el.children().filter_map(walk_inline_node).collect()
}

fn walk_inline_node(node: ego_tree::NodeRef<Node>) -> Option<Inline> {
    match node.value() {
        Node::Text(text) => Some(Inline::Text(text.to_string())),
        Node::Element(_) => {
            let el = ElementRef::wrap(node)?;
            Some(walk_inline_element(el))
        }
        _ => None,
    }
}

fn walk_inline_element(el: ElementRef) -> Inline {
    let name = el.value().name();
    match name {
        "strong" | "b" => Inline::Strong(walk_inline_children(el)),
        "em" | "i" => Inline::Emph(walk_inline_children(el)),
        "code" => Inline::Code(el.text().collect::<String>()),
        "del" | "s" | "strike" => Inline::Delete(walk_inline_children(el)),
        "mark" => Inline::Mark(walk_inline_children(el)),
        "a" => Inline::Link {
            url: el.value().attr("href").unwrap_or_default().to_string(),
            content: walk_inline_children(el),
        },
        "img" => Inline::Image {
            url: el.value().attr("src").unwrap_or_default().to_string(),
            alt: el.value().attr("alt").map(str::to_string),
        },
        "br" => Inline::Text(" ".to_string()),
        // Unknown inline elements (span, etc.): transparent — flatten children into one run of
        // text-ish inlines via Strong-free passthrough isn't representable as a single `Inline`,
        // so best-effort fold their text content.
        _ => Inline::Text(el.text().collect::<String>()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_then_export_matches_golden() {
        let html = include_str!("../../tests/fixtures/import/representative.html");
        let sheet = import_html(html);
        let exported = parrot_export::mvp::export_markdown(&sheet);
        // Same expected output as T-6-01's Markdown importer test — this HTML fixture is a
        // deliberate HTML-tagged equivalent of that same representative document.
        let golden = include_str!("../../tests/fixtures/import/representative.exported.md");
        assert_eq!(exported, golden);
    }

    #[test]
    fn nested_lists_keep_items_and_levels_and_each_ol_restarts() {
        let sheet = import_html(
            "<ul><li>a</li><li>b<ul><li>c<ul><li>d</li></ul></li></ul></li><li>e</li></ul>\
             <ol><li>1</li><li>2</li></ol><ol><li>again</li></ol>",
        );
        let rendered = parrot_markup::render::render(&sheet);
        assert_eq!(
            rendered,
            "- a\n\n- b\n\n  - c\n\n    - d\n\n- e\n\n1. 1\n\n2. 2\n\n1. again\n"
        );
    }

    #[test]
    fn code_blocks_and_dividers_use_the_editors_markers() {
        let sheet =
            import_html("<pre><code class=\"language-rs\">let a;\n</code></pre><hr><pre>x</pre>");
        assert_eq!(
            parrot_markup::render::render(&sheet),
            "```rs let a;\n\n----\n\n``` x\n"
        );
    }
}
