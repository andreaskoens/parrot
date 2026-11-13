//! Thin wrapper over `syntect` for code-block syntax highlighting (T-3-02). Uses `syntect`'s own
//! bundled default syntax/theme definitions exclusively — no Parrot/SyntaxKit grammar file is
//! reused; see `THIRD_PARTY_LICENSES.md` (T-3-01) for the license review that decided this.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("unknown language: {0}")]
    UnknownLanguage(String),
    #[error("highlighting failed: {0}")]
    Highlight(String),
}

/// One highlighted token: its text and foreground color, as `syntect` computed them against the
/// bundled default theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub color: (u8, u8, u8),
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

/// Highlights `code` as `lang` (a language name or file extension `syntect` recognizes, e.g.
/// `"rust"`/`"rs"`, `"python"`/`"py"`), returning one `StyledSpan` per contiguous same-style run.
pub fn highlight(code: &str, lang: &str) -> Result<Vec<StyledSpan>, SyntaxError> {
    let ps = syntax_set();
    let syntax = ps
        .find_syntax_by_token(lang)
        .ok_or_else(|| SyntaxError::UnknownLanguage(lang.to_string()))?;
    let theme = &theme_set().themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, theme);

    let mut spans = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(SynStyle, &str)> = h
            .highlight_line(line, ps)
            .map_err(|e| SyntaxError::Highlight(e.to_string()))?;
        for (style, text) in ranges {
            spans.push(StyledSpan {
                text: text.to_string(),
                color: (style.foreground.r, style.foreground.g, style.foreground.b),
            });
        }
    }
    Ok(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_trivial_rust_snippet() {
        let spans = highlight("fn main() {}\n", "rust").expect("rust must be a known language");

        assert!(
            !spans.is_empty(),
            "a non-empty snippet must produce at least one span"
        );
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "fn main() {}\n");

        // "fn" is a keyword and should get a different color than plain identifier text like
        // "main" — proves this is real syntax-aware highlighting, not just one uniform span.
        let fn_color = spans
            .iter()
            .find(|s| s.text.contains("fn"))
            .map(|s| s.color);
        let main_color = spans
            .iter()
            .find(|s| s.text.contains("main"))
            .map(|s| s.color);
        assert!(fn_color.is_some() && main_color.is_some());
        assert_ne!(
            fn_color, main_color,
            "keyword and identifier must be colored differently"
        );
    }

    #[test]
    fn unknown_language_is_an_error_not_a_panic() {
        assert!(highlight("x", "not-a-real-language").is_err());
    }

    // T-3-05: token-snapshot tests for the 5 languages this project ships highlighting for
    // (T-3-01's license review). Each snapshot is the full `Vec<StyledSpan>` — text and color —
    // so a change to syntect's bundled grammars/themes or this wrapper's logic shows up as a
    // diff on `cargo insta review`/`cargo test`, not a silent behavior change.

    #[test]
    fn snapshot_rust() {
        let spans = highlight(
            "fn main() {\n    let x: i32 = 42;\n    println!(\"{}\", x);\n}\n",
            "rust",
        )
        .expect("rust must be a known language");
        insta::assert_debug_snapshot!(spans);
    }

    #[test]
    fn snapshot_python() {
        let spans = highlight("def main():\n    x = 42\n    print(x)\n", "python")
            .expect("python must be a known language");
        insta::assert_debug_snapshot!(spans);
    }

    #[test]
    fn snapshot_js() {
        let spans = highlight(
            "function main() {\n  const x = 42;\n  console.log(x);\n}\n",
            "js",
        )
        .expect("js must be a known language");
        insta::assert_debug_snapshot!(spans);
    }

    #[test]
    fn snapshot_json() {
        let spans = highlight("{\n  \"key\": \"value\",\n  \"num\": 42\n}\n", "json")
            .expect("json must be a known language");
        insta::assert_debug_snapshot!(spans);
    }

    #[test]
    fn snapshot_markdown() {
        let spans = highlight(
            "# Heading\n\nSome **bold** and *italic* text.\n",
            "markdown",
        )
        .expect("markdown must be a known language");
        insta::assert_debug_snapshot!(spans);
    }
}
