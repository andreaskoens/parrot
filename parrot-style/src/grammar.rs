//! `pest`-generated ULSS parser (T-4-02). See `grammar.pest` for the grammar itself and the
//! sourcing notes (extracted from the real `ULSS.tmLanguage`, not just the inventory's
//! paraphrase).

use pest::iterators::Pairs;
use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct UlssParser;

#[derive(Debug, Error)]
pub enum GrammarError {
    #[error("ULSS parse error: {0}")]
    Parse(String),
}

/// Parses a complete ULSS stylesheet into its `pest` parse tree.
pub fn parse(input: &str) -> Result<Pairs<'_, Rule>, GrammarError> {
    UlssParser::parse(Rule::stylesheet, input).map_err(|e| GrammarError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trivial_stylesheet() {
        let input = "paragraph {\n    font-size: 12pt\n}\n";
        let result = parse(input);
        assert!(result.is_ok(), "trivial stylesheet must parse: {result:?}");
    }

    /// T-4-03: every real `.ulss` fixture (T-4-01; corrected to 10 files, not the roadmap's
    /// original 17 — see DECISIONS.md) must parse without error. Reads the fixtures directory
    /// at runtime, not hardcoded includes, so it automatically covers any fixture added later.
    #[test]
    fn parses_all_real_fixtures() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/ulss");
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("fixtures directory must exist")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "ulss"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        assert_eq!(
            entries.len(),
            10,
            "expected exactly 10 real ULSS fixtures (T-4-01)"
        );

        for entry in entries {
            let path = entry.path();
            let content = std::fs::read_to_string(&path).expect("fixture must be readable");
            let result = parse(&content);
            assert!(
                result.is_ok(),
                "{} failed to parse: {:?}",
                path.display(),
                result.err()
            );
        }
    }
}
