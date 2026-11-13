//! Grammar-adjacent checks (T-10-08, Decision D7/ADR-8): the four confirmed categories
//! (inventory §13 — Casing, Compounding, Punctuation, Typography) that don't need an external
//! engine, implemented here as small rule-based checks.
//!
//! **The fifth confirmed category, Grammar itself, is explicitly deferred — not implemented.**
//! The roadmap's own Done-when for this task allows either outcome ("the LanguageTool subprocess
//! integration passes... **or**... it is explicitly logged as a scoped deferral"); this is the
//! second one, and a genuine one: this sandbox has no Java runtime at all (`java -version` finds
//! none), and while this session confirmed `languagetool.org`'s download endpoint is actually
//! reachable, acquiring and bundling a ~200MB third-party Java application (plus the JRE it
//! needs, plus the open question of how an end user's Windows machine would have a JRE present
//! at all) is a disproportionate, open-ended cost for one rule category — the same class of
//! judgment call already made for `cargo-xwin` at T-7-06. Logged in `DECISIONS.md`.

use crate::language::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarCategory {
    Casing,
    Compounding,
    Punctuation,
    Typography,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarIssue {
    pub category: GrammarCategory,
    /// Short card heading, e.g. "Remove a space".
    pub title: String,
    pub message: String,
    pub byte_range: std::ops::Range<usize>,
    /// Text that replaces `byte_range` when the issue is accepted; `None` when there is no
    /// single unambiguous fix.
    pub fix: Option<String>,
}

/// Runs every implemented category over `text`, in category order (Casing, Compounding,
/// Punctuation, Typography), each internally in source order.
pub fn check(text: &str) -> Vec<GrammarIssue> {
    check_in(text, Language::default())
}

/// [`check`] for a project's `language`: compounding fixes are English-only, French keeps its
/// space before `!?:;`, and quote suggestions follow the language's convention. Findings that
/// would apply the identical edit to the same span are reported once.
pub fn check_in(text: &str, language: Language) -> Vec<GrammarIssue> {
    let mut issues = Vec::new();
    check_casing(text, &mut issues);
    if language.is_english() {
        check_compounding(text, &mut issues);
    }
    check_punctuation(text, language, &mut issues);
    check_typography(text, language, &mut issues);
    let mut seen = std::collections::HashSet::new();
    issues.retain(|i| seen.insert((i.byte_range.clone(), i.fix.clone())));
    issues
}

/// Every sentence (text start, or right after `.`/`!`/`?`) must begin with an uppercase letter.
fn check_casing(text: &str, out: &mut Vec<GrammarIssue>) {
    for start in sentence_starts(text) {
        let rest = &text[start..];
        let Some((rel_offset, first_char)) = rest.char_indices().find(|(_, c)| !c.is_whitespace())
        else {
            continue;
        };
        if first_char.is_alphabetic() && first_char.is_lowercase() {
            let abs = start + rel_offset;
            out.push(GrammarIssue {
                category: GrammarCategory::Casing,
                title: "Capitalize the first letter".to_string(),
                message: "Sentence should start with a capital letter.".to_string(),
                byte_range: abs..abs + first_char.len_utf8(),
                fix: Some(first_char.to_uppercase().collect()),
            });
        }
    }
}

/// A sentence boundary is the end of a *run* of `.`/`!`/`?` (so "...", "?!", etc. count as one
/// boundary, not several) — otherwise an ellipsis mid-sentence would be misread as several
/// sentence starts, each flagging the next lowercase word as a bogus Casing issue.
fn sentence_starts(text: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut starts = vec![0];
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i].1, '.' | '!' | '?') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j].1, '.' | '!' | '?') {
                j += 1;
            }
            if j < chars.len() {
                starts.push(chars[j].0);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    starts
}

/// A small, originally-curated list of commonly-misjoined/missplit word pairs — not an attempt
/// at exhaustive coverage, the same honestly-scoped stance `spellcheck.rs`'s word list takes.
const COMPOUND_FIXES: &[(&str, &str)] = &[
    ("alot", "a lot"),
    ("atleast", "at least"),
    ("infact", "in fact"),
    ("alltogether", "altogether"),
    ("eachother", "each other"),
];

fn check_compounding(text: &str, out: &mut Vec<GrammarIssue>) {
    for (start, word) in crate::spellcheck::word_tokens(text) {
        let lower = word.to_lowercase();
        if let Some((_, suggestion)) = COMPOUND_FIXES.iter().find(|(bad, _)| *bad == lower) {
            out.push(GrammarIssue {
                category: GrammarCategory::Compounding,
                title: format!("Replace with \"{suggestion}\""),
                message: format!("Did you mean \"{suggestion}\"?"),
                byte_range: start..start + word.len(),
                fix: Some(match_case(word, suggestion)),
            });
        }
    }
}

/// A space immediately before `,.!?;:`, or a run of 2+ spaces anywhere.
fn check_punctuation(text: &str, language: Language, out: &mut Vec<GrammarIssue>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for i in 0..chars.len() {
        let (idx, c) = chars[i];
        let french_ok =
            language.space_before_high_punctuation() && matches!(c, '!' | '?' | ':' | ';');
        if matches!(c, ',' | '.' | '!' | '?' | ';' | ':')
            && i > 0
            && chars[i - 1].1 == ' '
            && !french_ok
        {
            out.push(GrammarIssue {
                category: GrammarCategory::Punctuation,
                title: "Remove a space".to_string(),
                message: format!("Remove the space before \"{c}\"."),
                byte_range: chars[i - 1].0..idx + c.len_utf8(),
                fix: Some(c.to_string()),
            });
        }
        let starts_a_run = c == ' ' && (i == 0 || chars[i - 1].1 != ' ');
        if starts_a_run && i + 1 < chars.len() && chars[i + 1].1 == ' ' {
            let end = chars[i..]
                .iter()
                .find(|(_, ch)| *ch != ' ')
                .map_or(text.len(), |(b, _)| *b);
            out.push(GrammarIssue {
                category: GrammarCategory::Punctuation,
                title: "Remove extra spaces".to_string(),
                message: "Use a single space.".to_string(),
                byte_range: idx..end,
                fix: Some(" ".to_string()),
            });
        }
    }
}

/// Straight quotes and three-period ellipses where this project's own typography convention —
/// no pixel-level source was ever reverse-engineered for this, so it's an original choice, same
/// disposition as `parrot-app/README.md`'s spacing scale — prefers curly quotes/a real ellipsis.
fn check_typography(text: &str, language: Language, out: &mut Vec<GrammarIssue>) {
    for (idx, _) in text.match_indices("...") {
        out.push(GrammarIssue {
            category: GrammarCategory::Typography,
            title: "Use an ellipsis character".to_string(),
            message: "Consider using the ellipsis character \"…\" instead of three periods."
                .to_string(),
            byte_range: idx..idx + 3,
            fix: Some("\u{2026}".to_string()),
        });
    }
    let (open, close) = language.double_quotes();
    let mut prev: Option<char> = None;
    for (idx, c) in text.char_indices() {
        if c == '"' {
            let opening =
                prev.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | '\u{2014}'));
            out.push(GrammarIssue {
                category: GrammarCategory::Typography,
                title: "Use a typographic quote".to_string(),
                message: format!(
                    "Consider using {open}{close} quotes instead of a straight quote."
                ),
                byte_range: idx..idx + 1,
                fix: Some(if opening { open } else { close }.to_string()),
            });
        }
        prev = Some(c);
    }
}

/// Re-applies `word`'s capitalization (all caps / initial cap) to `suggestion`.
pub(crate) fn match_case(word: &str, suggestion: &str) -> String {
    let letters = || word.chars().filter(|c| c.is_alphabetic());
    if letters().count() > 1 && letters().all(char::is_uppercase) {
        return suggestion.to_uppercase();
    }
    if word.chars().next().is_some_and(char::is_uppercase) {
        let mut cs = suggestion.chars();
        return cs
            .next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(cs).collect());
    }
    suggestion.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn categories(text: &str) -> Vec<GrammarCategory> {
        check(text).into_iter().map(|i| i.category).collect()
    }

    /// Exercises the rule-based categories this task actually implements (Casing here) —
    /// `Grammar` itself is the deferred category (see module doc), so this is not a
    /// LanguageTool-backed check.
    #[test]
    fn flags_representative_grammar_error() {
        let text = "hello there. this should be capitalized.";
        let issues = check(text);
        assert!(
            issues.iter().any(|i| i.category == GrammarCategory::Casing),
            "a lowercase sentence start must be flagged: {issues:?}"
        );
    }

    #[test]
    fn casing_ignores_correctly_capitalized_sentences() {
        assert!(categories("Hello there. This is fine.").is_empty());
    }

    #[test]
    fn compounding_flags_known_bad_compound() {
        let issues = check("I have alot of work.");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].category, GrammarCategory::Compounding);
        assert_eq!(
            &"I have alot of work."[issues[0].byte_range.clone()],
            "alot"
        );
    }

    #[test]
    fn punctuation_flags_space_before_comma_and_double_space() {
        let cats = categories("Hello , world  there.");
        assert_eq!(
            cats,
            vec![GrammarCategory::Punctuation, GrammarCategory::Punctuation]
        );
    }

    #[test]
    fn fixes_apply_cleanly() {
        let apply = |text: &str| {
            let mut t = text.to_string();
            // Apply back to front so earlier byte ranges stay valid.
            let mut issues = check(text);
            issues.sort_by_key(|i| std::cmp::Reverse(i.byte_range.start));
            for i in issues {
                if let Some(f) = i.fix {
                    t.replace_range(i.byte_range, &f);
                }
            }
            t
        };
        assert_eq!(apply("Go, run !"), "Go, run!");
        assert_eq!(apply("Two  spaces."), "Two spaces.");
        assert_eq!(apply("Alot of it."), "A lot of it.");
        assert_eq!(apply("She said \"no\"."), "She said \u{201c}no\u{201d}.");
    }

    #[test]
    fn french_keeps_space_before_high_punctuation_and_german_quotes() {
        assert!(check_in("Bonjour !", Language::French).is_empty());
        let q = check_in("Er sagte \"nein\".", Language::German);
        assert_eq!(q[0].fix.as_deref(), Some("\u{201e}"));
    }

    #[test]
    fn typography_flags_ellipsis_and_straight_quotes() {
        // Capitalized after the ellipsis so this case doesn't also trip a (separate, correct)
        // Casing flag — this test is only about the Typography category.
        let cats = categories("Wait... She said \"no\".");
        assert_eq!(
            cats,
            vec![
                GrammarCategory::Typography,
                GrammarCategory::Typography,
                GrammarCategory::Typography,
            ]
        );
    }
}
