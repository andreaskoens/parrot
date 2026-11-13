//! Spelling checker (T-10-07, Decision D7/ADR-8): an offline, dictionary-based checker
//! over a sheet's plain-text projection. No network, no sockets, no third-party checker:
//! the engine is this module, and the word lists are vendored flat files, so the crate
//! tree stays MIT-only.
//!
//! The lists are SCOWL 2020.12.07 size-60 derivations (see `scripts/generate_wordlists.py`
//! and `THIRD_PARTY_LICENSES.md`): one spelling per line, fully expanded inflections,
//! each tagged with its SCOWL size band — lower bands are more common words, which is
//! what ranks correction suggestions. Checking is a hash lookup; suggestions are
//! Norvig-style single-edit candidates (plus capped double edits) ordered by band.
//!
//! Dictionaries are vendored under `assets/dictionaries/` and embedded with `include_str!`,
//! so checking works fully offline at runtime:
//! - `en_US.txt`: shared English + American spellings, contractions, top-1000 names.
//! - `en_GB.txt`: shared English + British (-ise) spellings, contractions, names.
//!
//! `is_known_word_in` picks the list per [`Language`](crate::language::Language);
//! personal words (see [`PersonalDictionary`](crate::personal_dictionary)) always pass.

use std::collections::HashMap;
use std::sync::OnceLock;

use parrot_core::SheetContent;

use crate::language::Language;
use crate::personal_dictionary::PersonalDictionary;

/// A vendored word list: every accepted spelling, each with the SCOWL size band it first
/// appears in (lower = more common). Membership answers `check`; bands rank `suggestions`.
struct WordList {
    bands: HashMap<String, u8>,
}

fn parse_word_list(text: &str) -> WordList {
    let mut bands = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((band, word)) = line.split_once(' ') else {
            continue;
        };
        let Ok(band) = band.parse::<u8>() else {
            continue;
        };
        bands.entry(word.to_string()).or_insert(band);
    }
    WordList { bands }
}

static EN_US_TXT: &str = include_str!("../assets/dictionaries/en_US.txt");
static EN_GB_TXT: &str = include_str!("../assets/dictionaries/en_GB.txt");

static EN_US_LIST: OnceLock<WordList> = OnceLock::new();
static EN_GB_LIST: OnceLock<WordList> = OnceLock::new();

fn us_word_list() -> &'static WordList {
    EN_US_LIST.get_or_init(|| parse_word_list(EN_US_TXT))
}

fn gb_word_list() -> &'static WordList {
    EN_GB_LIST.get_or_init(|| parse_word_list(EN_GB_TXT))
}

fn word_list_for(language: Language) -> &'static WordList {
    match language {
        Language::EnglishUk => gb_word_list(),
        // Only English variants reach the spellchecker (`has_dictionary` gates the rest),
        // so anything else falls back to US English.
        _ => us_word_list(),
    }
}

/// One misspelled-word hit: the word as written and its byte range in the checked text (so a
/// future rendering pass can locate it without re-tokenizing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlaggedWord {
    pub word: String,
    pub byte_range: std::ops::Range<usize>,
}

/// Whether `word` (any case) is in `personal` or the dictionary for `language`.
/// Non-alphabetic input (numbers, bare punctuation) is trivially "known" — there's nothing
/// to spell-check there.
///
/// Lookup is case-insensitive (the vendored lists are lowercase). Trailing possessives
/// (`girl's`, `girls'`) check via their stem as a fallback.
pub fn is_known_word_in(word: &str, language: Language, personal: &PersonalDictionary) -> bool {
    if personal.contains(word) {
        return true;
    }
    let lower = PersonalDictionary::normalized(word);
    if lower.is_empty() {
        return true;
    }
    let list = word_list_for(language);
    if list.bands.contains_key(&lower) {
        return true;
    }
    if is_ordinal_suffix(&lower) {
        // `11th`, `21st`: the tokenizer splits off digits, leaving the suffix as its
        // own token. Also legit abbreviations (`St.`, `Rd.`). A lone `th` typo slips
        // through with them — accepted as the rarer mistake.
        return true;
    }
    if let Some(stem) = possessive_stem(&lower) {
        if list.bands.contains_key(stem) {
            return true;
        }
    }
    false
}

/// Ordinal suffixes (`1st`, `2nd`, `3rd`, `11th`), which reach the checker as lone
/// tokens because the tokenizer splits digits off (`11th` → `th`).
fn is_ordinal_suffix(lower: &str) -> bool {
    matches!(lower, "st" | "nd" | "rd" | "th")
}

/// Whether `word` (any case) is in the default (US English) dictionary.
pub fn is_known_word(word: &str) -> bool {
    is_known_word_in(word, Language::default(), &PersonalDictionary::default())
}

/// Strips a trailing possessive (`'s`, `'`, or `s'`) so `girl's` checks via `girl`.
/// Returns `None` when there is no possessive ending or nothing would remain.
fn possessive_stem(lower: &str) -> Option<&str> {
    if let Some(stem) = lower.strip_suffix("'s") {
        return (!stem.is_empty()).then_some(stem);
    }
    if let Some(stem) = lower.strip_suffix('\'') {
        // `girls'` -> `girls`; keeping the `s` lets the plural form still match.
        return (!stem.is_empty()).then_some(stem);
    }
    None
}

/// Up to `limit` correction suggestions for `word`, best first. Call only for flagged
/// words (known words yield nothing useful) and only on demand — e.g. when the editor's
/// context menu opens — never per frame.
///
/// Candidates are Norvig-style single edits (deletes, transposes, replaces, inserts),
/// ranked by the word's frequency band (lower = more common), then alphabetically; a
/// capped double-edit pass fills in when nothing is one edit away. The word's own
/// capitalization is re-applied (`Teh` → `The`).
pub fn suggestions(
    word: &str,
    language: Language,
    personal: &PersonalDictionary,
    limit: usize,
) -> Vec<String> {
    if limit == 0 || is_known_word_in(word, language, personal) {
        return Vec::new();
    }
    let stripped: String = word
        .chars()
        .filter(|c| c.is_alphabetic() || *c == '\'')
        .collect();
    let lower = stripped.to_lowercase();
    if lower.is_empty() {
        return Vec::new();
    }
    let list = word_list_for(language);
    let mut ranked: Vec<(u8, u8, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (kind, candidate) in edits1(&lower) {
        push_candidate(&mut ranked, &mut seen, list, kind, candidate);
    }
    // Too far for one edit (coined words usually land here): try two, but only for
    // short words — the expansion is quadratic in word length.
    if ranked.is_empty() && lower.len() <= 12 {
        for (_, first) in edits1(&lower) {
            for (kind, candidate) in edits1(&first) {
                push_candidate(&mut ranked, &mut seen, list, kind + 4, candidate);
            }
        }
    }
    ranked.sort();
    ranked
        .into_iter()
        .map(|(_, _, s)| crate::grammar::match_case(&stripped, &s))
        .take(limit)
        .collect()
}

/// Records `candidate` (found via a `kind`-ranked edit) with its frequency band when it
/// is a real word not seen before.
fn push_candidate(
    ranked: &mut Vec<(u8, u8, String)>,
    seen: &mut std::collections::HashSet<String>,
    list: &WordList,
    kind: u8,
    candidate: String,
) {
    if seen.insert(candidate.clone()) {
        if let Some(&band) = list.bands.get(&candidate) {
            ranked.push((band, kind, candidate));
        }
    }
}

/// Every distinct string one edit from `word`, tagged by edit kind for ranking:
/// transpositions (the classic adjacent-letter typo) outrank replacements, inserts,
/// then deletes. Input is already lowercase ASCII (`[a-z']`), so byte indexing is safe.
fn edits1(word: &str) -> Vec<(u8, String)> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz'";
    const TRANSPOSE: u8 = 0;
    const REPLACE: u8 = 1;
    const INSERT: u8 = 2;
    const DELETE: u8 = 3;
    let bytes = word.as_bytes();
    let mut out = Vec::with_capacity(100 + 54 * bytes.len());
    let mut push = |kind: u8, variant: Vec<u8>| {
        out.push((kind, String::from_utf8(variant).expect("ASCII input")));
    };
    for i in 0..bytes.len().saturating_sub(1) {
        let mut variant = bytes.to_vec();
        variant.swap(i, i + 1);
        push(TRANSPOSE, variant);
    }
    for i in 0..bytes.len() {
        for &c in ALPHABET {
            let mut variant = bytes.to_vec();
            variant[i] = c;
            push(REPLACE, variant);
        }
    }
    for i in 0..=bytes.len() {
        for &c in ALPHABET {
            let mut variant = bytes.to_vec();
            variant.insert(i, c);
            push(INSERT, variant);
        }
    }
    for i in 0..bytes.len() {
        let mut variant = bytes.to_vec();
        variant.remove(i);
        push(DELETE, variant);
    }
    out
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

/// Checks `text` against `personal` plus the dictionary for `language`, returning every
/// unknown word in source order.
pub fn check_in(text: &str, language: Language, personal: &PersonalDictionary) -> Vec<FlaggedWord> {
    word_tokens(text)
        .into_iter()
        .filter(|(_, word)| !is_known_word_in(word, language, personal))
        .map(|(start, word)| FlaggedWord {
            word: word.to_string(),
            byte_range: start..start + word.len(),
        })
        .collect()
}

/// Checks `text`, returning every word not in the default dictionary, in source order.
pub fn check(text: &str) -> Vec<FlaggedWord> {
    check_in(text, Language::default(), &PersonalDictionary::default())
}

/// Convenience wrapper over [`check`] for a Sheet's plain-text projection — what `ui.rs` and the
/// task's own wording ("the plain-text projection of the active sheet(s)") actually call.
pub fn check_sheet(content: &SheetContent) -> Vec<FlaggedWord> {
    check(&content.plain_text())
}

/// [`check_sheet`] for a project's `language` and personal dictionary.
pub fn check_sheet_in(
    content: &SheetContent,
    language: Language,
    personal: &PersonalDictionary,
) -> Vec<FlaggedWord> {
    check_in(&content.plain_text(), language, personal)
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
    fn ordinal_suffixes_from_split_numbers_are_known() {
        assert!(check("the 11th day.").is_empty());
        assert!(check("on the 21st and 3rd.").is_empty());
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

    /// Regression test for the ~90-false-positive report: ordinary prose from a real
    /// passage must pass, with only coined/proper nouns flagged.
    #[test]
    fn passage_common_words_are_known() {
        let words = [
            "absolutely",
            "sensitive",
            "broken",
            "attempts",
            "safely",
            "functioning",
            "course",
            "lucky",
            "happened",
            "irreparably",
            "petal",
            "complaint",
            "domestication",
            "learning",
            "process",
            "decided",
            "reins",
            "prior",
            "owner",
            "clearly",
            "sleep",
            "dear",
            "mess",
            "pinnate",
            "fortunately",
            "nervous",
            "systems",
            "fairly",
            "resilient",
            "hello",
            "world",
            "computer",
            "brown",
            "sentence",
        ];
        let unknown: Vec<&&str> = words.iter().filter(|w| !is_known_word(w)).collect();
        assert!(unknown.is_empty(), "common words flagged: {unknown:?}");
    }

    #[test]
    fn affix_forms_derive_from_stems() {
        for word in [
            "systems", "running", "decided", "clearly", "learning", "jumps",
        ] {
            assert!(is_known_word(word), "{word} should derive via affix rules");
        }
    }

    #[test]
    fn contractions_and_possessives_are_known() {
        for word in [
            "don't", "I've", "wasn't", "you're", "haven't", "can't", "o'clock",
        ] {
            assert!(is_known_word(word), "{word} should be known");
        }
        for word in ["girl's", "girls'", "owner's"] {
            assert!(is_known_word(word), "{word} should check via its stem");
        }
    }

    #[test]
    fn coined_words_are_still_flagged() {
        for word in ["Xenoarcheobeureacrat", "Dionaea", "Affini"] {
            assert!(!is_known_word(word), "{word} should be flagged");
        }
        let flagged = check("Xenoarcheobeureacrat of the Affini Compact.");
        let words: Vec<&str> = flagged.iter().map(|f| f.word.as_str()).collect();
        assert_eq!(words, ["Xenoarcheobeureacrat", "Affini"]);
    }

    #[test]
    fn suggestions_offer_corrections_best_first() {
        let personal = PersonalDictionary::default();
        let out = suggestions("graet", Language::default(), &personal, 5);
        assert_eq!(out.first().map(String::as_str), Some("great"));
        assert!(out.len() <= 5 && out.len() > 1);
    }

    #[test]
    fn suggestions_are_empty_for_known_words_and_respect_limit() {
        let personal = PersonalDictionary::default();
        assert!(suggestions("great", Language::default(), &personal, 5).is_empty());
        assert!(suggestions("", Language::default(), &personal, 5).is_empty());
        let out = suggestions("teh", Language::default(), &personal, 2);
        assert_eq!(out, vec!["the".to_string(), "tea".to_string()]);
    }

    #[test]
    fn suggestions_match_the_words_case_and_skip_personal_words() {
        let personal = PersonalDictionary::default();
        let out = suggestions("Teh", Language::default(), &personal, 3);
        assert_eq!(out.first().map(String::as_str), Some("The"));
        let mut personal = PersonalDictionary::default();
        personal.add("affini");
        assert!(suggestions("affini", Language::default(), &personal, 5).is_empty());
    }

    #[test]
    fn uk_dictionary_covers_british_words() {
        let personal = PersonalDictionary::default();
        assert!(is_known_word_in("colour", Language::EnglishUk, &personal));
        assert!(is_known_word_in(
            "trajectory",
            Language::EnglishUk,
            &personal
        ));
    }

    #[test]
    fn personal_words_are_accepted_but_nothing_else_changes() {
        let mut personal = PersonalDictionary::default();
        personal.add("Affini");
        assert!(check_in("The Affini are here.", Language::default(), &personal).is_empty());
        assert!(
            check_in("The Dionaea are here.", Language::default(), &personal)
                .iter()
                .any(|f| f.word == "Dionaea")
        );
        // Personal words apply to the other English dictionary too.
        assert!(check_in("The Affini are here.", Language::EnglishUk, &personal).is_empty());
    }
}
