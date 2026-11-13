//! PlainText / generic-string importer (T-6-02): trivial wrapping, covering both the confirmed
//! `ULPlainTextImporter` and `ULStringImporter` — the feature inventory (§4) notes these are
//! equivalent in scope (no markup to parse, just raw text split into paragraphs).

use parrot_core::{Inline, Paragraph, SheetContent};

/// Splits `text` on blank lines into paragraphs, mirroring `export_plain_text`'s own
/// paragraph-join convention (`"\n\n"` between paragraphs, one trailing `"\n"`) so a plain-text
/// round-trip is the identity for text already in that shape.
pub fn import_plaintext(text: &str) -> SheetContent {
    // Runs of blank lines are one separator: a chunk never starts or ends with a stray newline
    // (it would show as an empty line in the editor), and no paragraph is empty.
    let paragraphs = text
        .split("\n\n")
        .map(|chunk| chunk.trim_matches('\n'))
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| Paragraph {
            tag: None,
            inlines: vec![Inline::Text(chunk.to_string())],
        })
        .collect();
    SheetContent::new(paragraphs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_blank_lines_do_not_make_empty_paragraphs_or_leading_newlines() {
        let sheet = import_plaintext("a\nb\n\n\n\nc\n\n");
        let texts: Vec<&Inline> = sheet.paragraphs.iter().flat_map(|p| &p.inlines).collect();
        assert_eq!(
            texts,
            [&Inline::Text("a\nb".into()), &Inline::Text("c".into())]
        );
    }

    #[test]
    fn import_then_export_matches_golden() {
        let text = include_str!("../../tests/fixtures/import/representative.plain.txt");
        let sheet = import_plaintext(text);
        let exported = parrot_export::mvp::export_plain_text(&sheet);
        assert_eq!(exported, text);
    }
}
