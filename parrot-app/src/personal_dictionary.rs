//! Personal dictionary: user-added words the spellchecker accepts (RV-06 follow-up).
//!
//! The bundled Hunspell dictionaries cover real English, but every fiction project has
//! coined names (`Affini`, `Xenoarcheobeureacrat`) no published dictionary will ever
//! contain. Words added here — via the Inspector's Spelling & Grammar cards — are checked
//! before the language dictionaries, so they pass in every project language.
//!
//! Storage is a visible `Dictionary.txt` at the library root (one word per line,
//! case-insensitive): it travels with the writing, survives save/open round-trips
//! (`parrot-container` ignores foreign files), and stays hand-editable as an escape hatch.
//! Matching is case-insensitive; the file holds the lowercase-normalized form.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

/// File name of the personal word list inside the library folder.
pub const FILE_NAME: &str = "Dictionary.txt";

/// User-added words, stored lowercase-normalized.
#[derive(Debug, Clone, Default)]
pub struct PersonalDictionary {
    words: BTreeSet<String>,
}

impl PersonalDictionary {
    /// Normalizes `word` the way lookups do: lowercase, letters and apostrophes only.
    /// Empty when there is nothing to store (numbers, bare punctuation).
    pub fn normalized(word: &str) -> String {
        word.chars()
            .filter(|c| c.is_alphabetic() || *c == '\'')
            .flat_map(char::to_lowercase)
            .collect()
    }

    /// Adds `word` (case-insensitively). Returns whether it is newly added.
    pub fn add(&mut self, word: &str) -> bool {
        let normalized = Self::normalized(word);
        if normalized.is_empty() {
            return false;
        }
        self.words.insert(normalized)
    }

    /// Removes `word` (case-insensitively). Returns whether one was removed.
    pub fn remove(&mut self, word: &str) -> bool {
        self.words.remove(&Self::normalized(word))
    }

    /// Whether `word` (any case) is in the dictionary.
    pub fn contains(&self, word: &str) -> bool {
        let normalized = Self::normalized(word);
        !normalized.is_empty() && self.words.contains(&normalized)
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Words in sorted order (the file order [`PersonalDictionary::save`] writes).
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.words.iter().map(String::as_str)
    }

    /// Reads `path` (`Dictionary.txt`); a missing file means an empty dictionary, never an
    /// error — libraries written before this feature simply have no personal words.
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let mut dictionary = Self::default();
        for line in text.lines() {
            dictionary.add(line.trim());
        }
        Ok(dictionary)
    }

    /// Writes one word per line. Callers skip the write when empty (see
    /// [`crate::state::AppState::save_personal_dictionary`]).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut text: String = self.words.iter().flat_map(|w| [w.as_str(), "\n"]).collect();
        if text.is_empty() {
            text.push('\n');
        }
        fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_is_case_insensitive_and_dedupes() {
        let mut dictionary = PersonalDictionary::default();
        assert!(dictionary.add("Affini"));
        assert!(!dictionary.add("AFFINI"), "same word in other case");
        assert!(!dictionary.add("affini"), "exact duplicate");
        assert!(dictionary.contains("aFfInI"));
        assert_eq!(dictionary.len(), 1);
    }

    #[test]
    fn blank_and_punctuation_add_nothing() {
        let mut dictionary = PersonalDictionary::default();
        assert!(!dictionary.add(""));
        assert!(!dictionary.add("   "));
        assert!(!dictionary.add("123"));
        assert!(dictionary.is_empty());
    }

    #[test]
    fn remove_drops_words_case_insensitively() {
        let mut dictionary = PersonalDictionary::default();
        dictionary.add("Dionaea");
        assert!(dictionary.remove("DIONAEA"));
        assert!(!dictionary.contains("dionaea"));
        assert!(!dictionary.remove("dionaea"), "already gone");
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        let mut dictionary = PersonalDictionary::default();
        dictionary.add("Affini");
        dictionary.add("Xenoarcheobeureacrat");
        dictionary.save(&path).expect("save");
        let reloaded = PersonalDictionary::load(&path).expect("load");
        assert!(reloaded.contains("affini"));
        assert!(reloaded.contains("XENOARCHEOBEUREACRAT"));
        assert_eq!(reloaded.len(), 2);
    }

    #[test]
    fn missing_file_loads_empty_and_blank_lines_are_skipped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join(FILE_NAME);
        assert!(PersonalDictionary::load(&missing).expect("load").is_empty());
        let path = dir.path().join("other.txt");
        fs::write(&path, "Affini\n\n   \nDionaea\n").expect("write");
        let loaded = PersonalDictionary::load(&path).expect("load");
        assert_eq!(loaded.len(), 2);
    }
}
