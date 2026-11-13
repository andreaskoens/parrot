//! Spelling checker (T-10-07, Decision D7/ADR-8): an offline, dictionary-based checker over a
//! sheet's plain-text projection. No network, no sockets.
//!
//! **On the "dictionary":** a real Hunspell-format dictionary (the kind a crate like
//! `spellbook` expects you to supply) is a third-party licensed asset — bundling one raises
//! exactly the question T-3-01 gated for syntax-highlighting grammars, and this task's own
//! Done-when only needs a representative flagged/not-flagged pair, not full English coverage.
//! `COMMON_WORDS` below is a small, originally-written common-word list, not copied from any
//! dictionary file — good enough for this scope, and a real dictionary crate is a drop-in swap
//! later: `is_known_word` is the one function that would need to change.
//!
//! **On "underline":** actually underlining misspelled glyphs in place would mean teaching
//! `render.rs`'s `StyledSpan`/`paint_paragraph` about a new per-span flag — a real rendering
//! change, not this task's own narrow scope. `ui.rs` instead lists flagged words below the
//! editor, a simpler, honestly-scoped UI treatment — the same "build a reasonable new UX, not a
//! byte-for-byte port of something never nib-verified anyway" stance ADR-7 already takes for
//! the sheet-flow divider.

use parrot_core::SheetContent;

/// Small, original common-word list (see module doc) — not a dictionary file, not
/// comprehensive. Lowercase, used for exact (post-normalization) lookup.
const COMMON_WORDS: &[&str] = &[
    "a",
    "about",
    "after",
    "again",
    "all",
    "also",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "back",
    "bad",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "best",
    "better",
    "between",
    "big",
    "book",
    "both",
    "but",
    "by",
    "call",
    "can",
    "could",
    "dark",
    "day",
    "days",
    "did",
    "do",
    "document",
    "documents",
    "does",
    "dog",
    "down",
    "draft",
    "early",
    "edit",
    "editor",
    "else",
    "end",
    "example",
    "fast",
    "few",
    "find",
    "first",
    "for",
    "found",
    "from",
    "get",
    "give",
    "go",
    "goal",
    "going",
    "good",
    "great",
    "group",
    "had",
    "happy",
    "has",
    "have",
    "he",
    "her",
    "here",
    "hers",
    "herself",
    "him",
    "himself",
    "his",
    "how",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "keep",
    "keyword",
    "know",
    "large",
    "last",
    "later",
    "lazy",
    "learn",
    "library",
    "like",
    "line",
    "lines",
    "list",
    "little",
    "long",
    "look",
    "made",
    "make",
    "many",
    "may",
    "me",
    "might",
    "more",
    "most",
    "much",
    "must",
    "my",
    "myself",
    "need",
    "new",
    "next",
    "no",
    "not",
    "note",
    "notes",
    "now",
    "of",
    "off",
    "often",
    "ok",
    "old",
    "on",
    "once",
    "one",
    "only",
    "open",
    "or",
    "other",
    "our",
    "out",
    "over",
    "page",
    "paragraph",
    "paragraphs",
    "part",
    "preferences",
    "preview",
    "project",
    "quick",
    "quickly",
    "read",
    "really",
    "right",
    "said",
    "same",
    "save",
    "saw",
    "say",
    "see",
    "seen",
    "selected",
    "set",
    "settings",
    "sheet",
    "sheets",
    "should",
    "show",
    "simple",
    "since",
    "small",
    "so",
    "some",
    "soon",
    "still",
    "style",
    "stylesheet",
    "such",
    "take",
    "tell",
    "test",
    "testing",
    "text",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "time",
    "to",
    "today",
    "together",
    "tomorrow",
    "too",
    "two",
    "under",
    "until",
    "up",
    "us",
    "use",
    "used",
    "user",
    "very",
    "want",
    "was",
    "way",
    "we",
    "well",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "why",
    "will",
    "with",
    "without",
    "word",
    "words",
    "work",
    "working",
    "would",
    "write",
    "writer",
    "writing",
    "written",
    "wrote",
    "yes",
    "yesterday",
    "yet",
    "you",
    "your",
    "yours",
    "yourself",
];

/// One misspelled-word hit: the word as written and its byte range in the checked text (so a
/// future rendering pass can locate it without re-tokenizing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlaggedWord {
    pub word: String,
    pub byte_range: std::ops::Range<usize>,
}

/// Whether `word` (any case) is in the known-word list. Non-alphabetic input (numbers, bare
/// punctuation) is treated as trivially "known" — there's nothing to spell-check there.
pub fn is_known_word(word: &str) -> bool {
    let normalized = normalize(word);
    normalized.is_empty() || COMMON_WORDS.contains(&normalized.as_str())
}

fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphabetic() || *c == '\'')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every word-like token in `text` with its byte start, in source order. `pub(crate)` so
/// `grammar.rs` (T-10-08) can reuse the same tokenizer rather than duplicating it.
pub(crate) fn word_tokens(text: &str) -> Vec<(usize, &str)> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        let is_word_char = c.is_alphabetic() || c == '\'';
        match (is_word_char, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                tokens.push((s, &text[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        tokens.push((s, &text[s..]));
    }
    tokens
}

/// Checks `text`, returning every word not in [`is_known_word`]'s list, in source order.
pub fn check(text: &str) -> Vec<FlaggedWord> {
    word_tokens(text)
        .into_iter()
        .filter(|(_, word)| !is_known_word(word))
        .map(|(start, word)| FlaggedWord {
            word: word.to_string(),
            byte_range: start..start + word.len(),
        })
        .collect()
}

/// Convenience wrapper over [`check`] for a Sheet's plain-text projection — what `ui.rs` and the
/// task's own wording ("the plain-text projection of the active sheet(s)") actually call.
pub fn check_sheet(content: &SheetContent) -> Vec<FlaggedWord> {
    check(&content.plain_text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_misspelled_word() {
        let flagged = check("This is a graet day.");
        let words: Vec<&str> = flagged.iter().map(|f| f.word.as_str()).collect();
        assert!(
            words.contains(&"graet"),
            "misspelled word must be flagged: {words:?}"
        );
    }

    #[test]
    fn ignores_correct_word() {
        let flagged = check("This is a great day.");
        let words: Vec<&str> = flagged.iter().map(|f| f.word.as_str()).collect();
        assert!(
            !words.contains(&"great"),
            "correctly spelled common word must not be flagged: {words:?}"
        );
    }

    #[test]
    fn is_case_insensitive() {
        assert!(is_known_word("The"));
        assert!(is_known_word("THE"));
        assert!(!is_known_word("Thiis"));
    }

    #[test]
    fn byte_range_locates_the_word_in_the_original_text() {
        let text = "a graet day";
        let flagged = check(text);
        assert_eq!(flagged.len(), 1);
        assert_eq!(&text[flagged[0].byte_range.clone()], "graet");
    }
}
