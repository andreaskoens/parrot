//! Shared "one of every confirmed tag" fixture document (T-5-11): every backend's own validity
//! test (T-5-04..T-5-09) is built against this same AST shape, so the consolidated suite can
//! just re-run each backend against it without re-deriving per-backend documents.

use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};

pub fn full_tag_document() -> SheetContent {
    SheetContent::new(vec![
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
                Inline::Text("Body with ".to_string()),
                Inline::Strong(vec![Inline::Text("bold".to_string())]),
                Inline::Text(", ".to_string()),
                Inline::Emph(vec![Inline::Text("italic".to_string())]),
                Inline::Text(", ".to_string()),
                Inline::Code("code()".to_string()),
                Inline::Text(", a ".to_string()),
                Inline::Link {
                    url: "https://example.com".to_string(),
                    content: vec![Inline::Text("link".to_string())],
                },
                Inline::Text(", a ".to_string()),
                Inline::Footnote {
                    content: vec![Inline::Text("note".to_string())],
                },
                Inline::Text(", ".to_string()),
                Inline::Delete(vec![Inline::Text("struck".to_string())]),
                Inline::Text(", an ".to_string()),
                Inline::InlineComment(vec![Inline::Text("inline comment".to_string())]),
                Inline::Text(", ".to_string()),
                Inline::InlineNative("<raw/>".to_string()),
                Inline::Text(", ".to_string()),
                Inline::Mark(vec![Inline::Text("marked".to_string())]),
                Inline::Text(", an ".to_string()),
                Inline::Annotation {
                    note: "an annotation note".to_string(),
                    content: vec![Inline::Text("annotated".to_string())],
                },
                Inline::Text(", an ".to_string()),
                Inline::Image {
                    url: "https://example.com/i.png".to_string(),
                    alt: Some("alt text".to_string()),
                },
                Inline::Text(" and a ".to_string()),
                Inline::Video {
                    url: "https://example.com/v.mp4".to_string(),
                },
                Inline::Text(".".to_string()),
            ],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::UnorderedList,
                literal: "- ".to_string(),
            }),
            inlines: vec![Inline::Text("first item".to_string())],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::UnorderedList,
                literal: "- ".to_string(),
            }),
            inlines: vec![Inline::Text("second item".to_string())],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::OrderedList,
                literal: "1. ".to_string(),
            }),
            inlines: vec![Inline::Text("first ordered".to_string())],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::BlockQuote,
                literal: "> ".to_string(),
            }),
            inlines: vec![Inline::Text("a quotation".to_string())],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::CodeBlock(Some("rust".to_string())),
                literal: "```rust".to_string(),
            }),
            inlines: vec![Inline::Text("fn main() {}".to_string())],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Comment,
                literal: "// ".to_string(),
            }),
            inlines: vec![Inline::Text("a block comment".to_string())],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Divider,
                literal: "---".to_string(),
            }),
            inlines: vec![],
        },
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::NativeBlock,
                literal: "!!! ".to_string(),
            }),
            inlines: vec![Inline::Text("<div>native</div>".to_string())],
        },
    ])
}
