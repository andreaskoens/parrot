//! AST -> `.md`/`.txt` string exporters. Originally ad-hoc (T-0-10); now thin wrappers over the
//! rule-table + DSL conversion engine (T-5-01, `conversion.rs`) — kept as the stable public
//! path (`mvp::export_markdown`/`mvp::export_plain_text`) so existing call sites
//! (`parrot-app`'s e2e test, `textbundle.rs`) didn't need to change when the implementation
//! moved.

use crate::conversion;
use parrot_core::SheetContent;

pub fn export_markdown(content: &SheetContent) -> String {
    conversion::export_markdown(content)
}

pub fn export_plain_text(content: &SheetContent) -> String {
    conversion::export_plain_text(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, TagKind, TagMark};

    /// The literal confirmed example from feature inventory §3.3 — same fixture as T-0-04/T-0-05.
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
    fn markdown_golden() {
        let golden = include_str!("../../tests/fixtures/mvp/confirmed_example.txt");
        assert_eq!(export_markdown(&confirmed_example()), golden);
    }

    #[test]
    fn plain_text_golden() {
        let golden = include_str!("../../tests/fixtures/mvp/confirmed_example.plain.txt");
        assert_eq!(export_plain_text(&confirmed_example()), golden);
    }
}
