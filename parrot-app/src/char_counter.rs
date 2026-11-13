//! Character Counter tool (an Inspector tab): a from-scratch reimplementation of the
//! metric set a typical online character counter offers — counts, syllable and word-length
//! figures, a Dale-Chall-style readability score, reading/speaking time, per-platform
//! character limits and a character / 1–3-word frequency table. `egui`-free; `ui.rs` paints it.
//!
//! Text rules, kept deliberately simple and documented so the numbers are explainable:
//! - A *word* is a whitespace-separated token with its leading/trailing punctuation trimmed
//!   (inner `'` and `-` stay: "don't", "well-known"), kept only if it has a letter or digit.
//! - A *sentence* ends at a run of `.`, `!`, `?` or `…` followed by whitespace or the end of
//!   the text, or at a paragraph break; trailing text without a terminator still counts.
//! - A *paragraph* is a block of non-blank lines (paragraphs are blank-line separated, as in
//!   the editor buffer).
//! - *Syllables* use an English vowel-group heuristic (silent final `e`, `-es`/`-ed` endings).
//! - *Difficult words* approximate the Dale-Chall notion: the official 3,000-familiar-word
//!   list isn't bundled, so a word counts as difficult when it has three or more syllables and
//!   isn't a grammar word. The score uses the real Dale-Chall formula on that count, so it
//!   reads lower than a list-based score would — the UI labels both as approximate.

use std::collections::{BTreeMap, HashSet};

pub const READING_WPM: u32 = 200;
pub const SPEAKING_WPM: u32 = 125;

/// Every figure the stats panel shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStats {
    pub characters: usize,
    pub characters_no_spaces: usize,
    pub words: usize,
    pub alphanumeric: usize,
    pub alpha: usize,
    pub numeric: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    pub unique_words: usize,
    pub syllables: usize,
    pub monosyllabic_words: usize,
    pub polysyllabic_words: usize,
    pub short_words: usize,
    pub long_words: usize,
    pub difficult_words: usize,
    pub syllables_per_word: f32,
    pub average_word_length: f32,
    pub average_sentence_length: f32,
    pub dale_chall_score: f32,
    pub reading_seconds: u32,
    pub speaking_seconds: u32,
}

/// The words of `text`, per the module rules, in order and with original case.
pub fn words(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect()
}

pub fn analyze(text: &str) -> TextStats {
    let chars: Vec<char> = text.chars().filter(|&c| c != '\r').collect();
    let words = words(text);
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let sentences = count_sentences(text);
    let syllables: Vec<usize> = words.iter().map(|w| syllable_count(w)).collect();
    let letters: Vec<usize> = words
        .iter()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).count())
        .collect();
    let difficult = lower
        .iter()
        .zip(&syllables)
        .filter(|(w, &s)| s >= 3 && !is_grammar_word(w) && w.chars().any(char::is_alphabetic))
        .count();

    let n = words.len();
    let ratio = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f32 / b as f32 };
    let total_syllables = syllables.iter().sum();
    TextStats {
        characters: chars.len(),
        characters_no_spaces: chars.iter().filter(|c| !c.is_whitespace()).count(),
        words: n,
        alphanumeric: chars.iter().filter(|c| c.is_alphanumeric()).count(),
        alpha: chars.iter().filter(|c| c.is_alphabetic()).count(),
        numeric: chars.iter().filter(|c| c.is_numeric()).count(),
        sentences,
        paragraphs: count_paragraphs(text),
        unique_words: lower.iter().collect::<HashSet<_>>().len(),
        syllables: total_syllables,
        monosyllabic_words: syllables.iter().filter(|&&s| s == 1).count(),
        polysyllabic_words: syllables.iter().filter(|&&s| s >= 3).count(),
        short_words: letters.iter().filter(|&&l| l <= 3).count(),
        long_words: letters.iter().filter(|&&l| l >= 7).count(),
        difficult_words: difficult,
        syllables_per_word: ratio(total_syllables, n),
        average_word_length: ratio(letters.iter().sum(), n),
        average_sentence_length: ratio(n, sentences),
        dale_chall_score: dale_chall(n, sentences, difficult),
        reading_seconds: seconds_at(n, READING_WPM),
        speaking_seconds: seconds_at(n, SPEAKING_WPM),
    }
}

fn seconds_at(words: usize, wpm: u32) -> u32 {
    let words = u32::try_from(words).unwrap_or(u32::MAX);
    (words.saturating_mul(60) + wpm / 2) / wpm
}

/// "0 sec", "45 sec", "3 min 20 sec", "1 hr 2 min".
pub fn format_duration(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match (h, m) {
        (0, 0) => format!("{s} sec"),
        (0, _) if s == 0 => format!("{m} min"),
        (0, _) => format!("{m} min {s} sec"),
        _ => format!("{h} hr {m} min"),
    }
}

fn count_sentences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    let mut pending = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphanumeric() {
            pending = true;
        } else if matches!(c, '.' | '!' | '?' | '…') {
            let mut j = i;
            while j < chars.len() && matches!(chars[j], '.' | '!' | '?' | '…') {
                j += 1;
            }
            // Closing quotes/brackets may follow the terminator: `"Done."`.
            while j < chars.len() && matches!(chars[j], '"' | '\'' | ')' | ']' | '”' | '’') {
                j += 1;
            }
            if pending && (j == chars.len() || chars[j].is_whitespace()) {
                count += 1;
                pending = false;
            }
            i = j;
            continue;
        } else if c == '\n' && pending && blank_line_follows(&chars, i) {
            count += 1;
            pending = false;
        }
        i += 1;
    }
    count + usize::from(pending)
}

/// Whether the newline at `i` starts a blank line (a paragraph break).
fn blank_line_follows(chars: &[char], i: usize) -> bool {
    chars[i + 1..]
        .iter()
        .take_while(|&&c| c != '\n')
        .all(|c| c.is_whitespace())
        && chars[i + 1..].contains(&'\n')
}

fn count_paragraphs(text: &str) -> usize {
    let mut count = 0;
    let mut in_block = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if !blank && !in_block {
            count += 1;
        }
        in_block = !blank;
    }
    count
}

/// English syllable estimate for one word (always at least 1 for a word with letters).
pub fn syllable_count(word: &str) -> usize {
    let w: Vec<char> = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    if w.is_empty() {
        return 0;
    }
    if w.len() <= 3 {
        return 1;
    }
    let vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let n = w.len();
    // Silent endings: "makes" → mak, "jumped" → jump, "cake" → cak — but not the voiced
    // "boxes", "wanted", or a consonant + "le" ("table").
    let silent = if w.ends_with(&['e', 's']) {
        let k = w[n - 3];
        !vowel(k) && !matches!(k, 's' | 'x' | 'z' | 'c' | 'g' | 'h' | 'l')
    } else if w.ends_with(&['e', 'd']) {
        let k = w[n - 3];
        !vowel(k) && !matches!(k, 't' | 'd')
    } else {
        false
    };
    let end = if silent {
        n - 2
    } else if w[n - 1] == 'e' && !(w[n - 2] == 'l' && !vowel(w[n - 3])) {
        n - 1
    } else {
        n
    };
    let mut count = 0;
    let mut prev_vowel = false;
    for (i, &c) in w[..end].iter().enumerate() {
        // A leading y is a consonant ("yellow").
        let v = vowel(c) && !(i == 0 && c == 'y');
        if v && !prev_vowel {
            count += 1;
        }
        prev_vowel = v;
    }
    count.max(1)
}

/// The Dale-Chall formula: `0.1579 × difficult% + 0.0496 × words/sentence`, plus 3.6365 when
/// more than 5% of words are difficult.
fn dale_chall(words: usize, sentences: usize, difficult: usize) -> f32 {
    if words == 0 || sentences == 0 {
        return 0.0;
    }
    let pct = 100.0 * difficult as f32 / words as f32;
    let raw = 0.1579 * pct + 0.0496 * (words as f32 / sentences as f32);
    if pct > 5.0 {
        raw + 3.6365
    } else {
        raw
    }
}

/// The grade band for a Dale-Chall score (standard interpretation table).
pub fn dale_chall_level(score: f32, words: usize) -> &'static str {
    if words == 0 {
        return "—";
    }
    match score {
        s if s < 5.0 => "Grade 4 and below",
        s if s < 6.0 => "Grades 5–6",
        s if s < 7.0 => "Grades 7–8",
        s if s < 8.0 => "Grades 9–10",
        s if s < 9.0 => "Grades 11–12",
        s if s < 10.0 => "College",
        _ => "College graduate",
    }
}

/// English function words — articles, pronouns, prepositions, conjunctions, auxiliaries —
/// left out of the frequency table by "Exclude grammar words".
const GRAMMAR_WORDS: &[&str] = &[
    "a",
    "about",
    "above",
    "after",
    "again",
    "against",
    "all",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "between",
    "both",
    "but",
    "by",
    "can",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "down",
    "during",
    "each",
    "either",
    "few",
    "for",
    "from",
    "further",
    "had",
    "has",
    "have",
    "having",
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
    "may",
    "me",
    "might",
    "more",
    "most",
    "must",
    "my",
    "myself",
    "neither",
    "no",
    "nor",
    "not",
    "of",
    "off",
    "on",
    "once",
    "only",
    "or",
    "other",
    "our",
    "ours",
    "ourselves",
    "out",
    "over",
    "own",
    "same",
    "shall",
    "she",
    "should",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "themselves",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "to",
    "too",
    "under",
    "until",
    "up",
    "upon",
    "us",
    "very",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "whom",
    "whose",
    "why",
    "will",
    "with",
    "within",
    "without",
    "would",
    "you",
    "your",
    "yours",
    "yourself",
    "yourselves",
    "i'm",
    "it's",
    "don't",
    "doesn't",
    "didn't",
    "isn't",
    "aren't",
    "wasn't",
    "weren't",
    "can't",
    "won't",
    "i've",
    "you're",
    "we're",
    "they're",
];

pub fn is_grammar_word(lowercase_word: &str) -> bool {
    GRAMMAR_WORDS.contains(&lowercase_word)
}

/// One of the frequency table's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencyKind {
    Character,
    /// Phrases of 1–3 words.
    Words(usize),
}

impl FrequencyKind {
    pub const ALL: [FrequencyKind; 4] = [
        FrequencyKind::Character,
        FrequencyKind::Words(1),
        FrequencyKind::Words(2),
        FrequencyKind::Words(3),
    ];

    pub fn label(self) -> &'static str {
        match self {
            FrequencyKind::Character => "Character",
            FrequencyKind::Words(1) => "1 Word",
            FrequencyKind::Words(2) => "2 Words",
            FrequencyKind::Words(_) => "3 Words",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrequencyEntry {
    pub term: String,
    pub count: usize,
    /// Share of all items of this kind in the text (characters, or n-word phrases), so the
    /// percentages stay comparable whether or not grammar words are hidden.
    pub percent: f32,
}

/// The `top` most frequent characters (case-folded, whitespace ignored) or n-word phrases,
/// most frequent first, ties alphabetical. With `exclude_grammar`, a single grammar word, or a
/// phrase starting or ending with one, is left out.
pub fn frequency(
    text: &str,
    kind: FrequencyKind,
    exclude_grammar: bool,
    top: usize,
) -> Vec<FrequencyEntry> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let total = match kind {
        FrequencyKind::Character => {
            let mut total = 0;
            for c in text.chars().filter(|c| !c.is_whitespace()) {
                total += 1;
                *counts.entry(c.to_lowercase().collect()).or_default() += 1;
            }
            total
        }
        FrequencyKind::Words(n) => {
            let n = n.clamp(1, 3);
            let lower: Vec<String> = words(text).iter().map(|w| w.to_lowercase()).collect();
            let mut total = 0;
            for window in lower.windows(n) {
                total += 1;
                let skip = exclude_grammar
                    && (is_grammar_word(&window[0]) || is_grammar_word(&window[n - 1]));
                if !skip {
                    *counts.entry(window.join(" ")).or_default() += 1;
                }
            }
            total
        }
    };
    let mut entries: Vec<FrequencyEntry> = counts
        .into_iter()
        .map(|(term, count)| FrequencyEntry {
            term,
            count,
            percent: 100.0 * count as f32 / total.max(1) as f32,
        })
        .collect();
    entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.term.cmp(&b.term)));
    entries.truncate(top);
    entries
}

/// Character limits for common destinations; `note` explains display-only limits.
pub struct PlatformLimit {
    pub name: &'static str,
    pub limit: usize,
    pub note: Option<&'static str>,
}

pub const PLATFORM_LIMITS: &[PlatformLimit] = &[
    PlatformLimit {
        name: "Twitter",
        limit: 280,
        note: None,
    },
    PlatformLimit {
        name: "SMS",
        limit: 160,
        note: None,
    },
    PlatformLimit {
        name: "Reddit Title",
        limit: 300,
        note: None,
    },
    PlatformLimit {
        name: "Ebay Title",
        limit: 80,
        note: None,
    },
    PlatformLimit {
        name: "Yelp Post",
        limit: 5000,
        note: None,
    },
    PlatformLimit {
        name: "LinkedIn Summary",
        limit: 2000,
        note: None,
    },
    PlatformLimit {
        name: "Pinterest Description",
        limit: 500,
        note: None,
    },
    PlatformLimit {
        name: "Blogspot Description",
        limit: 500,
        note: None,
    },
    PlatformLimit {
        name: "Facebook Status",
        limit: 63_206,
        note: None,
    },
    PlatformLimit {
        name: "HTML Title Tag",
        limit: 70,
        note: Some("Only the first 70 characters display"),
    },
    PlatformLimit {
        name: "HTML Meta Description",
        limit: 155,
        note: Some("No hard limit; search results show about 155"),
    },
];

pub const TOP_CHOICES: [usize; 4] = [10, 20, 50, 100];

/// What the counter is counting: the editor's selection, or the open sheets in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountScope {
    Selection,
    Sheets(usize),
}

impl CountScope {
    pub fn label(self) -> String {
        match self {
            CountScope::Selection => "Counting the selected text.".to_string(),
            CountScope::Sheets(0) => "No sheet open.".to_string(),
            CountScope::Sheets(1) => "Counting the open sheet.".to_string(),
            CountScope::Sheets(n) => format!("Counting {n} open sheets."),
        }
    }
}

/// A sheet's text as the counter sees it: one paragraph per block, blank-line separated
/// (so paragraph counts match the editor), empty paragraphs skipped.
pub fn sheet_text(content: &parrot_core::SheetContent) -> String {
    content
        .paragraphs
        .iter()
        .map(|p| p.plain_text())
        .filter(|t| !t.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The text to count: `selection` when it's non-empty, else every open sheet's text, one
/// paragraph-separated block per sheet.
pub fn source_text(selection: Option<&str>, sheets: &[String]) -> (String, CountScope) {
    match selection.filter(|s| !s.is_empty()) {
        Some(sel) => (sel.to_string(), CountScope::Selection),
        None => (sheets.join("\n\n"), CountScope::Sheets(sheets.len())),
    }
}

/// The tool's UI state: table options, plus the last counted text and its results, so a
/// frame only re-analyzes when the text or options change. Lives in `ParrotApp`.
#[derive(Debug, Clone)]
pub struct CounterUi {
    pub kind: FrequencyKind,
    pub top: usize,
    pub exclude_grammar: bool,
    text: String,
    stats: TextStats,
    table: Option<((FrequencyKind, bool, usize), Vec<FrequencyEntry>)>,
}

impl Default for CounterUi {
    fn default() -> Self {
        Self {
            kind: FrequencyKind::Words(1),
            top: TOP_CHOICES[0],
            exclude_grammar: false,
            text: String::new(),
            stats: TextStats::default(),
            table: None,
        }
    }
}

impl CounterUi {
    /// Points the counter at `text`, re-analyzing only if it changed.
    pub fn set_text(&mut self, text: String) {
        if text != self.text {
            self.stats = analyze(&text);
            self.text = text;
            self.table = None;
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn stats(&self) -> &TextStats {
        &self.stats
    }

    /// The frequency table for the current text and options.
    pub fn frequency(&mut self) -> &[FrequencyEntry] {
        let key = (self.kind, self.exclude_grammar, self.top);
        if self.table.as_ref().map(|(k, _)| *k) != Some(key) {
            let entries = frequency(&self.text, self.kind, self.exclude_grammar, self.top);
            self.table = Some((key, entries));
        }
        self.table
            .as_ref()
            .map(|(_, e)| e.as_slice())
            .unwrap_or(&[])
    }
}

/// The live status line above the input, e.g. "12 characters, 3 words."
pub fn status_line(stats: &TextStats) -> String {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    format!(
        "{}, {}.",
        plural(stats.characters, "character", "characters"),
        plural(stats.words, "word", "words")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_is_the_selection_else_the_open_sheets() {
        let sheets = vec!["One two.".to_string(), "Three.".to_string()];
        let (text, scope) = source_text(Some("two"), &sheets);
        assert_eq!((text.as_str(), scope), ("two", CountScope::Selection));
        let (text, scope) = source_text(Some(""), &sheets);
        assert_eq!(scope, CountScope::Sheets(2));
        assert_eq!(analyze(&text).words, 3);
        assert_eq!(analyze(&text).paragraphs, 2);
        assert_eq!(source_text(None, &[]).1.label(), "No sheet open.");
        assert_eq!(CountScope::Sheets(2).label(), "Counting 2 open sheets.");
    }

    #[test]
    fn counter_ui_caches_until_text_or_options_change() {
        let mut ui = CounterUi::default();
        ui.set_text("the cat the".to_string());
        assert_eq!(ui.stats().words, 3);
        assert_eq!(ui.frequency()[0].term, "the");
        ui.exclude_grammar = true;
        assert_eq!(ui.frequency()[0].term, "cat");
        ui.set_text("dog".to_string());
        assert_eq!(ui.stats().words, 1);
        assert_eq!(ui.frequency()[0].term, "dog");
    }

    #[test]
    fn empty_text_is_all_zero() {
        let s = analyze("");
        assert_eq!(s, TextStats::default());
        assert_eq!(dale_chall_level(s.dale_chall_score, s.words), "—");
        assert_eq!(status_line(&s), "0 characters, 0 words.");
    }

    #[test]
    fn character_classes() {
        let s = analyze("Hi 42, ok?\r\n");
        assert_eq!(s.characters, 11, "\\r is not counted");
        assert_eq!(s.characters_no_spaces, 8);
        assert_eq!(s.alphanumeric, 6);
        assert_eq!(s.alpha, 4);
        assert_eq!(s.numeric, 2);
        assert_eq!(s.words, 3);
    }

    #[test]
    fn words_trim_punctuation_but_keep_inner_marks() {
        assert_eq!(
            words("\"Don't\" stop—well-known (yes)! -- …"),
            ["Don't", "stop—well-known", "yes"]
        );
    }

    #[test]
    fn sentences_and_paragraphs() {
        let text = "A title\n\nIt costs 3.50 today. Really?! \"Yes.\"\nSame paragraph\n\n\nLast";
        let s = analyze(text);
        assert_eq!(
            s.sentences, 6,
            "title, 3 terminated, 'Same paragraph', 'Last'"
        );
        assert_eq!(s.paragraphs, 3);
        assert_eq!(analyze("no terminator").sentences, 1);
        assert_eq!(analyze("...").sentences, 0);
    }

    #[test]
    fn syllable_heuristic() {
        for (word, n) in [
            ("cat", 1),
            ("the", 1),
            ("hello", 2),
            ("table", 2),
            ("cake", 1),
            ("makes", 1),
            ("boxes", 2),
            ("jumped", 1),
            ("wanted", 2),
            ("played", 1),
            ("yellow", 2),
            ("syllable", 3),
            ("beautiful", 3),
            ("readability", 5),
            ("42", 0),
        ] {
            assert_eq!(syllable_count(word), n, "{word}");
        }
    }

    #[test]
    fn word_length_and_syllable_figures() {
        let s = analyze("The cat sat. Beautiful readability everywhere.");
        assert_eq!(s.words, 6);
        assert_eq!(s.unique_words, 6);
        assert_eq!(s.short_words, 3);
        assert_eq!(s.long_words, 3);
        assert_eq!(s.monosyllabic_words, 3);
        assert_eq!(s.polysyllabic_words, 3);
        assert_eq!(s.difficult_words, 3);
        assert_eq!(s.average_sentence_length, 3.0);
        assert!((s.average_word_length - 39.0 / 6.0).abs() < 1e-4);
    }

    #[test]
    fn dale_chall_formula_and_bands() {
        // 10 words, 1 sentence, 0 difficult: 0.0496 × 10.
        assert!((dale_chall(10, 1, 0) - 0.496).abs() < 1e-4);
        // 20% difficult adds the 3.6365 adjustment.
        assert!((dale_chall(10, 2, 2) - (0.1579 * 20.0 + 0.0496 * 5.0 + 3.6365)).abs() < 1e-4);
        assert_eq!(dale_chall_level(4.9, 1), "Grade 4 and below");
        assert_eq!(dale_chall_level(7.5, 1), "Grades 9–10");
        assert_eq!(dale_chall_level(10.0, 1), "College graduate");
    }

    #[test]
    fn reading_and_speaking_time() {
        let text = "word ".repeat(500);
        let s = analyze(&text);
        assert_eq!(s.reading_seconds, 150);
        assert_eq!(s.speaking_seconds, 240);
        assert_eq!(format_duration(150), "2 min 30 sec");
        assert_eq!(format_duration(240), "4 min");
        assert_eq!(format_duration(9), "9 sec");
        assert_eq!(format_duration(3720), "1 hr 2 min");
    }

    #[test]
    fn frequency_tables() {
        let text = "The cat and the hat. The cat!";
        let one = frequency(text, FrequencyKind::Words(1), false, 2);
        assert_eq!(one[0].term, "the");
        assert_eq!(one[0].count, 3);
        assert!((one[0].percent - 3.0 / 7.0 * 100.0).abs() < 1e-3);
        assert_eq!(one[1].term, "cat");

        let excluded = frequency(text, FrequencyKind::Words(1), true, 10);
        assert!(excluded.iter().all(|e| e.term != "the" && e.term != "and"));

        let two = frequency(text, FrequencyKind::Words(2), false, 1);
        assert_eq!((two[0].term.as_str(), two[0].count), ("the cat", 2));
        let two_ex = frequency(text, FrequencyKind::Words(2), true, 10);
        assert!(
            two_ex.is_empty(),
            "every pair starts or ends with a grammar word"
        );

        let three = frequency("a b c a b c", FrequencyKind::Words(3), false, 10);
        assert_eq!(three[0].term, "a b c");
        assert_eq!(three[0].count, 2);

        let chars = frequency("Aa b", FrequencyKind::Character, false, 10);
        assert_eq!((chars[0].term.as_str(), chars[0].count), ("a", 2));
        assert!((chars[0].percent - 200.0 / 3.0).abs() < 1e-3);
    }

    #[test]
    fn platform_limits_match_the_reference_list() {
        assert_eq!(PLATFORM_LIMITS.len(), 11);
        let twitter = PLATFORM_LIMITS.iter().find(|p| p.name == "Twitter");
        assert_eq!(twitter.map(|p| p.limit), Some(280));
    }
}
