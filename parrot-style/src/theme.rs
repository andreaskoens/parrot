//! `.ultheme` as a ULSS subset (T-4-08, roadmap Decision D3): the inventory never
//! independently confirmed `.ultheme` as a distinct format — only that `readingmode.ulss`
//! ("a dark style suited for reading") is written in plain ULSS syntax using theme-scoped
//! selectors (`syntax-*`, `inline-*`, `area-*`, plus base `font-color`/`background-color`)
//! rather than print-layout ones (`page-*`, `table-*`). This project infers `.ultheme` is the
//! same ULSS language, just restricted to that selector subset — the simplest hypothesis
//! consistent with the evidence, not a confirmed fact.
//!
//! // ASSUMPTION(D3): `.ultheme` is a ULSS subset, not a second format. Flagged here per the
//! // roadmap's instruction, not silently assumed. Worst case if wrong: themes become their own
//! // small format later, built on this already-working ULSS parser — cheap to revisit.

use crate::resolve::{self, ComputedStyle, ResolveError, Stylesheet};
use crate::select::PathNode;

/// The theme-scoped selector prefixes this project treats `.ultheme` as restricted to
/// (confirmed in `readingmode.ulss`: `syntax-*`, `inline-*`, `area-*`), plus `defaults`/
/// `paragraph` for the base text/background color.
pub const THEME_SELECTOR_PREFIXES: &[&str] = &["syntax-", "inline-", "area-"];

/// A loaded `.ultheme` — just a ULSS stylesheet (Decision D3), queried by theme-scoped
/// selector name rather than walked as a document tree.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Theme {
    stylesheet: Stylesheet,
}

impl Theme {
    /// Parses and resolves a `.ultheme` file's ULSS source.
    pub fn load(input: &str) -> Result<Self, ResolveError> {
        Ok(Self {
            stylesheet: resolve::resolve(input)?,
        })
    }

    /// The cascaded style for a theme-scoped selector (e.g. `"paragraph"` for the base text
    /// color, `"inline-link"`, `"syntax-keyword"`).
    pub fn computed(&self, selector_name: &str) -> ComputedStyle {
        resolve::cascade(&self.stylesheet, &[PathNode::new(selector_name)], None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::Value;

    /// T-4-08: loading the real `readingmode.ulss` as a theme must reproduce its own confirmed
    /// palette (inventory §5.2) — base text color `#f5f5f5`, link color `#d1a804`, gray/comment
    /// color `#a0a0a0` — read back through the theme-scoped selectors that use them.
    #[test]
    fn readingmode_theme_matches_confirmed_colors() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/ulss/readingmode.ulss"
        );
        let content = std::fs::read_to_string(path).expect("readingmode.ulss must be readable");
        let theme = Theme::load(&content).expect("readingmode.ulss must resolve as a theme");

        // Base text color, via `paragraph: @ul-readingmode-default {}`.
        assert_eq!(
            theme.computed("paragraph").get("font-color"),
            Some(&Value::HexColor(0xf5, 0xf5, 0xf5))
        );
        // Link color, via `inline-link: @ul-readingmode-default { font-color: ...; }`.
        assert_eq!(
            theme.computed("inline-link").get("font-color"),
            Some(&Value::HexColor(0xd1, 0xa8, 0x04))
        );
        // Gray/comment color, via `inline-comment: @ul-readingmode-default { font-color: ...; }`.
        assert_eq!(
            theme.computed("inline-comment").get("font-color"),
            Some(&Value::HexColor(0xa0, 0xa0, 0xa0))
        );
    }
}
