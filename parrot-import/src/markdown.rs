//! Markdown importer (T-6-01): `pulldown-cmark` -> `parrot-core` AST.
//!
//! Best-effort, not a byte-perfect inverse of M5's Markdown exporter — standard Markdown's
//! grammar covers constructs the confirmed Markdown-XL tag table doesn't (tables, footnote
//! definitions, definition lists, HTML blocks) and is missing others the dialect has (comment,
//! native block, mark, annotation, video). Unsupported block constructs are skipped rather than
//! guessed at (see DECISIONS.md); `ENABLE_STRIKETHROUGH` is the only CommonMark extension turned
//! on, since `Inline::Delete` is a confirmed AST variant worth exercising.

use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

pub fn import_markdown(markdown: &str) -> SheetContent {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(markdown, options);

    let mut builder = Builder::default();
    for event in parser {
        builder.handle(event);
    }
    SheetContent::new(builder.paragraphs)
}

enum SpanKind {
    Root,
    Link(String),
    Image(String),
}

struct ListState {
    ordered: bool,
    next_number: u64,
}

#[derive(Default)]
struct Builder {
    paragraphs: Vec<Paragraph>,
    current_tag: Option<TagMark>,
    /// Set by a block container (`BlockQuote`) that pulldown-cmark *always* wraps a nested
    /// `Paragraph` event around — consumed by that nested `Paragraph`'s `Start` event instead of
    /// opening a paragraph immediately, so the two don't stomp on each other's
    /// `current_tag`/`inline_stack` reset.
    pending_tag: Option<TagMark>,
    /// Set when a list `Item` just opened its own paragraph frame eagerly (to handle a *tight*
    /// list, which never nests a `Paragraph` event) but a *loose* list's nested `Paragraph`
    /// might still follow immediately — consumed by that `Paragraph`'s `Start`/`End` pair (see
    /// `paragraph_owned_by_item`) so it reuses the Item's frame instead of resetting it.
    awaiting_nested_paragraph: bool,
    /// A list item's paragraph frame is open (its text is still being collected).
    item_open: bool,
    /// Set while inside a loose list item's nested `Paragraph` — its own `End` must NOT close
    /// the paragraph (the enclosing `Item`'s `End` does that), since they share one frame.
    paragraph_owned_by_item: bool,
    inline_stack: Vec<(SpanKind, Vec<Inline>)>,
    list_stack: Vec<ListState>,
}

impl Builder {
    fn open_paragraph(&mut self, tag: Option<TagMark>) {
        self.current_tag = tag;
        self.inline_stack = vec![(SpanKind::Root, Vec::new())];
    }

    fn close_paragraph(&mut self) {
        let inlines = self.inline_stack.pop().map(|(_, c)| c).unwrap_or_default();
        self.paragraphs.push(Paragraph {
            tag: self.current_tag.take(),
            inlines,
        });
    }

    fn push_inline(&mut self, inline: Inline) {
        if let Some((_, children)) = self.inline_stack.last_mut() {
            children.push(inline);
        }
    }

    /// Pops the current span frame and wraps its accumulated children via `build`, pushing the
    /// result into the new top frame. Used for spans whose `Inline` shape needs no metadata
    /// beyond the children (emphasis/strong/strikethrough) — Link/Image carry a URL from their
    /// `Start` event and are built directly in `end_tag` instead.
    fn end_span(&mut self, build: impl FnOnce(Vec<Inline>) -> Inline) {
        if let Some((_, children)) = self.inline_stack.pop() {
            let inline = build(children);
            self.push_inline(inline);
        }
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag_end) => self.end_tag(tag_end),
            Event::Text(text) => self.push_inline(Inline::Text(text.to_string())),
            Event::Code(text) => self.push_inline(Inline::Code(text.to_string())),
            Event::Html(html) | Event::InlineHtml(html) => {
                self.push_inline(Inline::InlineNative(html.to_string()))
            }
            Event::FootnoteReference(label) => self.push_inline(Inline::Footnote {
                content: vec![Inline::Text(label.to_string())],
            }),
            Event::SoftBreak | Event::HardBreak => self.push_inline(Inline::Text(" ".to_string())),
            Event::Rule => self.paragraphs.push(Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Divider,
                    literal: "----".to_string(),
                }),
                inlines: vec![],
            }),
            Event::TaskListMarker(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                if self.awaiting_nested_paragraph {
                    // A loose list item's own paragraph: reuse the frame Item already opened.
                    self.awaiting_nested_paragraph = false;
                    self.paragraph_owned_by_item = true;
                } else {
                    let tag = self.pending_tag.take();
                    self.open_paragraph(tag);
                }
            }
            Tag::Heading { level, .. } => {
                let n = level as u8;
                let literal = format!("{} ", "#".repeat(n as usize));
                self.open_paragraph(Some(TagMark {
                    kind: TagKind::Heading(n),
                    literal,
                }));
            }
            Tag::BlockQuote(_) => {
                self.pending_tag = Some(TagMark {
                    kind: TagKind::BlockQuote,
                    literal: "> ".to_string(),
                })
            }
            Tag::CodeBlock(kind) => {
                let (lang, literal) = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        (Some(lang.to_string()), format!("```{lang} "))
                    }
                    _ => (None, "``` ".to_string()),
                };
                self.open_paragraph(Some(TagMark {
                    kind: TagKind::CodeBlock(lang),
                    literal,
                }));
            }
            Tag::List(start) => self.list_stack.push(ListState {
                ordered: start.is_some(),
                next_number: start.unwrap_or(1),
            }),
            Tag::Item => {
                // A nested item starts while its parent's text is still open: finish the
                // parent first (an item holding only a nested list leaves no empty item).
                if self.item_open {
                    if self.inline_stack.last().is_some_and(|(_, c)| !c.is_empty()) {
                        self.close_paragraph();
                    }
                    self.item_open = false;
                }
                let ordered = self.list_stack.last().is_some_and(|l| l.ordered);
                // Two spaces of indent per nesting level, as the editor's lists nest.
                let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
                let (kind, literal) = if ordered {
                    let n = self.list_stack.last().map(|l| l.next_number).unwrap_or(1);
                    (TagKind::OrderedList, format!("{indent}{n}. "))
                } else {
                    (TagKind::UnorderedList, format!("{indent}- "))
                };
                self.open_paragraph(Some(TagMark { kind, literal }));
                self.awaiting_nested_paragraph = true;
                self.item_open = true;
            }
            Tag::Emphasis => self.inline_stack.push((SpanKind::Root, Vec::new())),
            Tag::Strong => self.inline_stack.push((SpanKind::Root, Vec::new())),
            Tag::Strikethrough => self.inline_stack.push((SpanKind::Root, Vec::new())),
            Tag::Link { dest_url, .. } => self
                .inline_stack
                .push((SpanKind::Link(dest_url.to_string()), Vec::new())),
            Tag::Image { dest_url, .. } => self
                .inline_stack
                .push((SpanKind::Image(dest_url.to_string()), Vec::new())),
            // Tables, footnote definitions, definition lists, raw HTML blocks, metadata blocks:
            // no confirmed Markdown-XL counterpart — best-effort skip rather than guessed output.
            _ => {}
        }
    }

    fn end_tag(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Paragraph => {
                if self.paragraph_owned_by_item {
                    // The enclosing `Item`'s `End` closes it instead — they share one frame.
                    self.paragraph_owned_by_item = false;
                } else {
                    self.close_paragraph();
                }
            }
            TagEnd::Heading(_) => self.close_paragraph(),
            TagEnd::CodeBlock => {
                // The fence's closing newline isn't part of the code (and would break the
                // blank-line paragraph separator).
                if let Some((_, children)) = self.inline_stack.last_mut() {
                    if let Some(Inline::Text(text)) = children.last_mut() {
                        text.truncate(text.trim_end_matches('\n').len());
                    }
                }
                self.close_paragraph();
            }
            // No-op: the nested `Paragraph`'s own `End` already closed it (see `pending_tag`).
            TagEnd::BlockQuote(_) => {}
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            TagEnd::Item => {
                if let Some(state) = self.list_stack.last_mut() {
                    state.next_number += 1;
                }
                self.awaiting_nested_paragraph = false;
                if self.item_open {
                    self.item_open = false;
                    self.close_paragraph();
                }
            }
            TagEnd::Emphasis => self.end_span(Inline::Emph),
            TagEnd::Strong => self.end_span(Inline::Strong),
            TagEnd::Strikethrough => self.end_span(Inline::Delete),
            TagEnd::Link => {
                if let Some((kind, children)) = self.inline_stack.pop() {
                    let url = match kind {
                        SpanKind::Link(u) => u,
                        _ => String::new(),
                    };
                    self.push_inline(Inline::Link {
                        url,
                        content: children,
                    });
                }
            }
            TagEnd::Image => {
                if let Some((kind, children)) = self.inline_stack.pop() {
                    let url = match kind {
                        SpanKind::Image(u) => u,
                        _ => String::new(),
                    };
                    let alt = flatten_text(&children);
                    self.push_inline(Inline::Image {
                        url,
                        alt: if alt.is_empty() { None } else { Some(alt) },
                    });
                }
            }
            _ => {}
        }
    }
}

fn flatten_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(s) => s.clone(),
            _ => String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_then_export_matches_golden() {
        let markdown = include_str!("../../tests/fixtures/import/representative.md");
        let sheet = import_markdown(markdown);
        let exported = parrot_export::mvp::export_markdown(&sheet);
        let golden = include_str!("../../tests/fixtures/import/representative.exported.md");
        assert_eq!(exported, golden);
    }

    /// A blank line between list items makes pulldown-cmark treat the list as "loose" and nest
    /// a `Paragraph` event inside each `Item` — the same nesting pulldown-cmark always does for
    /// `BlockQuote`. This is exactly what M5's Markdown exporter always produces (it joins every
    /// paragraph, list items included, with a blank line), so a tight-list-only importer would
    /// break on our own exporter's real output.
    #[test]
    fn nested_lists_keep_every_item_and_indent_by_level() {
        let sheet = import_markdown("- a\n- b\n  - nested\n    - deeper\n- c\n");
        let items: Vec<(String, String)> = sheet
            .paragraphs
            .iter()
            .map(|p| {
                (
                    p.tag.as_ref().unwrap().literal.clone(),
                    parrot_markup::render::render(&SheetContent::new(vec![p.clone()]))
                        .trim_end()
                        .to_string(),
                )
            })
            .collect();
        let literals: Vec<&str> = items.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(literals, ["- ", "- ", "  - ", "    - ", "- "]);
        assert_eq!(items[2].1, "  - nested");
        let ordered = import_markdown("1. one\n   1. inner\n2. two\n");
        let literals: Vec<&str> = ordered
            .paragraphs
            .iter()
            .map(|p| p.tag.as_ref().unwrap().literal.as_str())
            .collect();
        assert_eq!(literals, ["1. ", "  1. ", "2. "]);
    }

    #[test]
    fn code_blocks_and_dividers_use_the_editors_markers() {
        let sheet =
            import_markdown("```rust\nfn main() {}\nlet x = 1;\n```\n\n---\n\n```\nplain\n```\n");
        let tags: Vec<&str> = sheet
            .paragraphs
            .iter()
            .map(|p| p.tag.as_ref().unwrap().literal.as_str())
            .collect();
        assert_eq!(tags, ["```rust ", "----", "``` "]);
        assert_eq!(
            sheet.paragraphs[0].inlines,
            vec![Inline::Text("fn main() {}\nlet x = 1;".to_string())]
        );
    }

    #[test]
    fn loose_list_items_keep_their_list_tag() {
        let sheet = import_markdown("- first item\n\n- second item\n");
        assert_eq!(sheet.paragraphs.len(), 2);
        assert_eq!(
            sheet.paragraphs[0].tag.as_ref().unwrap().kind,
            TagKind::UnorderedList
        );
        assert_eq!(
            sheet.paragraphs[0].inlines,
            vec![Inline::Text("first item".to_string())]
        );
        assert_eq!(
            sheet.paragraphs[1].tag.as_ref().unwrap().kind,
            TagKind::UnorderedList
        );
        assert_eq!(
            sheet.paragraphs[1].inlines,
            vec![Inline::Text("second item".to_string())]
        );
    }
}
