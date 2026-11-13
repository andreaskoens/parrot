//! Per-project proofing language. Stored in the library's preferences, keyed by project title
//! (the container format has no language field, so this stays out of the on-disk project).
//!
//! Only English ships a (small) word list, so spelling checks run for English variants only;
//! every language still gets the language-neutral grammar rules (casing, punctuation spacing)
//! and its own quotation-mark convention.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    EnglishUs,
    EnglishUk,
    German,
    French,
    Spanish,
    Portuguese,
    PortugueseBr,
    Italian,
    Dutch,
}

impl Language {
    pub const ALL: [Language; 9] = [
        Language::EnglishUs,
        Language::EnglishUk,
        Language::German,
        Language::French,
        Language::Spanish,
        Language::Portuguese,
        Language::PortugueseBr,
        Language::Italian,
        Language::Dutch,
    ];

    /// Code stored in a project's meta file.
    pub fn code(self) -> &'static str {
        match self {
            Language::EnglishUs => "en-US",
            Language::EnglishUk => "en-GB",
            Language::German => "de",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::Portuguese => "pt-PT",
            Language::PortugueseBr => "pt-BR",
            Language::Italian => "it",
            Language::Dutch => "nl",
        }
    }

    /// Inverse of [`Language::code`]; unknown codes give `None` (callers fall back to default).
    pub fn from_code(code: &str) -> Option<Language> {
        Language::ALL.into_iter().find(|l| l.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Language::EnglishUs => "English (US)",
            Language::EnglishUk => "English (UK)",
            Language::German => "German",
            Language::French => "French",
            Language::Spanish => "Spanish",
            Language::Portuguese => "Portuguese (Portugal)",
            Language::PortugueseBr => "Portuguese (Brazil)",
            Language::Italian => "Italian",
            Language::Dutch => "Dutch",
        }
    }

    pub fn is_english(self) -> bool {
        matches!(self, Language::EnglishUs | Language::EnglishUk)
    }

    /// Whether a word list exists for spelling checks.
    pub fn has_dictionary(self) -> bool {
        self.is_english()
    }

    /// Whether punctuation like `!`/`?`/`:` is conventionally preceded by a (non-breaking)
    /// space — French typography — so "Remove the space before" must not fire.
    pub fn space_before_high_punctuation(self) -> bool {
        self == Language::French
    }

    /// The opening and closing double quotation marks to suggest.
    pub fn double_quotes(self) -> (char, char) {
        match self {
            Language::EnglishUs | Language::EnglishUk | Language::Dutch => ('\u{201c}', '\u{201d}'),
            Language::German => ('\u{201e}', '\u{201c}'),
            Language::French
            | Language::Spanish
            | Language::Portuguese
            | Language::PortugueseBr
            | Language::Italian => ('\u{ab}', '\u{bb}'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for l in Language::ALL {
            assert_eq!(Language::from_code(l.code()), Some(l));
        }
        assert_eq!(Language::from_code("xx"), None);
    }

    #[test]
    fn only_english_has_a_dictionary() {
        let with: Vec<_> = Language::ALL
            .iter()
            .filter(|l| l.has_dictionary())
            .collect();
        assert_eq!(with, [&Language::EnglishUs, &Language::EnglishUk]);
    }
}
