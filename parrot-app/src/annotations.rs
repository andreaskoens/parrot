//! Text-checker annotations (T-12-11, ui-description §9): one merged, source-ordered list of
//! spelling (T-10-07) and grammar (T-10-08) findings, with the spec's two colors. Shared by the
//! Inspector's Spelling & Grammar tab, the editor's inline underlines and the sheet list's error
//! dot.

use crate::language::Language;
use crate::tokens;
use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    Spelling,
    Grammar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub kind: AnnotationKind,
    /// Short card heading ("Remove a space").
    pub title: String,
    pub message: String,
    pub byte_range: std::ops::Range<usize>,
    /// Replacement for `byte_range` applied by Accept; `None` = nothing to auto-apply.
    pub fix: Option<String>,
}

/// §9.1/§10.1: spelling `#f5a623` orange, grammar `#5ab0f6` blue.
pub fn annotation_color(kind: AnnotationKind) -> Color32 {
    match kind {
        AnnotationKind::Spelling => tokens::SPELLING,
        AnnotationKind::Grammar => tokens::GRAMMAR,
    }
}

/// Every finding in `text` (English), ordered by position (spelling first on ties).
pub fn check(text: &str) -> Vec<Annotation> {
    check_in(text, Language::default())
}

/// [`check`] for a project's language. Spelling needs a word list, so it only runs for
/// languages that have one.
pub fn check_in(text: &str, language: Language) -> Vec<Annotation> {
    let spelling = if language.has_dictionary() {
        crate::spellcheck::check(text)
    } else {
        Vec::new()
    };
    let mut all: Vec<Annotation> = spelling
        .into_iter()
        .map(|f| Annotation {
            kind: AnnotationKind::Spelling,
            title: "Possibly misspelled".to_string(),
            message: format!("Possibly misspelled: {}", f.word),
            byte_range: f.byte_range,
            fix: None,
        })
        .chain(
            crate::grammar::check_in(text, language)
                .into_iter()
                .map(|i| Annotation {
                    kind: AnnotationKind::Grammar,
                    title: i.title,
                    message: i.message,
                    byte_range: i.byte_range,
                    fix: i.fix,
                }),
        )
        .collect();
    all.sort_by_key(|a| (a.byte_range.start, a.kind == AnnotationKind::Grammar));
    all
}

/// A card's context line: the finding's span with up to `radius` chars either side, flattened
/// to one line. Returns `(before, span, after)`; `before`/`after` carry a leading/trailing
/// "…" when clipped.
pub fn context(
    text: &str,
    range: &std::ops::Range<usize>,
    radius: usize,
) -> (String, String, String) {
    let flat = |s: &str| s.replace(['\n', '\r'], " ");
    let before: Vec<char> = text[..range.start].chars().rev().take(radius).collect();
    let clipped_before = text[..range.start].chars().count() > before.len();
    let before: String = before.into_iter().rev().collect();
    let after: String = text[range.end..].chars().take(radius).collect();
    let clipped_after = text[range.end..].chars().count() > after.chars().count();
    (
        format!("{}{}", if clipped_before { "…" } else { "" }, flat(&before)),
        flat(&text[range.clone()]),
        format!("{}{}", flat(&after), if clipped_after { "…" } else { "" }),
    )
}

/// The index of the paragraph (as [`crate::editor::paragraph_spans`] counts them) holding
/// `byte` in `text` — what a finding's click-to-jump scrolls to.
pub fn paragraph_of(text: &str, byte: usize) -> usize {
    crate::editor::paragraph_spans(text)
        .iter()
        .filter(|s| s.start <= byte)
        .count()
        .saturating_sub(1)
}

/// `egui` rich text for one finding's line in the Spelling & Grammar tab (§9.3).
pub fn label(annotation: &Annotation) -> egui::RichText {
    egui::RichText::new(&annotation.message)
        .font(tokens::UI_CAPTION.font_id())
        .color(annotation_color(annotation.kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_of(kind: AnnotationKind, text: &str) -> Annotation {
        check(text)
            .into_iter()
            .find(|a| a.kind == kind)
            .unwrap_or_else(|| panic!("expected a {kind:?} finding in {text:?}"))
    }

    #[test]
    fn non_english_skips_spelling() {
        let found = check_in("Der qwzx Hund.", Language::German);
        assert!(found.iter().all(|a| a.kind == AnnotationKind::Grammar));
    }

    #[test]
    fn context_clips_and_flattens() {
        let text = "aaaa bbbb\ncccc dddd";
        let (b, s, a) = context(text, &(10..14), 3);
        assert_eq!(
            (b.as_str(), s.as_str(), a.as_str()),
            ("…bb ", "cccc", " dd…")
        );
    }

    #[test]
    fn spelling_annotation_color_matches_spec() {
        let a = first_of(AnnotationKind::Spelling, "The asdasdasdddddd is here.");
        assert_eq!(a.message, "Possibly misspelled: asdasdasdddddd");
        assert_eq!(annotation_color(a.kind).to_hex(), "#f5a623ff");
    }

    #[test]
    fn grammar_annotation_color_matches_spec() {
        let a = first_of(AnnotationKind::Grammar, "the start is lowercase.");
        assert_eq!(annotation_color(a.kind).to_hex(), "#5ab0f6ff");
    }

    #[test]
    fn findings_map_to_their_paragraph() {
        let text = "First one.\n\nSecond qwzx.\n\nthird.";
        let found = |needle: &str| {
            let a = check(text)
                .into_iter()
                .find(|a| &text[a.byte_range.clone()] == needle)
                .unwrap_or_else(|| panic!("{needle:?} flagged"));
            paragraph_of(text, a.byte_range.start)
        };
        assert_eq!(found("qwzx"), 1);
        assert_eq!(paragraph_of(text, text.len() - 1), 2);
        assert_eq!(paragraph_of("", 0), 0);
    }

    #[test]
    fn findings_are_source_ordered() {
        let all = check("the qwzx start.");
        let starts: Vec<usize> = all.iter().map(|a| a.byte_range.start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }
}
