//! Style picker (T-4-09): lets the user choose among this project's own built-in default
//! styles, loading the selection as the active ULSS stylesheet (T-4-07's wiring).
//!
//! These are **original stylesheets written for this project**, not copies of Parrot' own
//! bundled `.ulstyle` files — the same licensing discipline T-3-01 applied to syntax grammars
//! applies here too: the real `.ulss` files extracted for test fixtures (T-4-01) have no
//! accompanying license, so none of their *content* is reused in what ships. They're "inspired
//! by" only in the loose sense of setting the same kinds of properties (heading sizes, list/
//! quote indents) that any manuscript-style stylesheet would.

use parrot_style::resolve::{self, Stylesheet};

const DEFAULT_ULSS: &str = r#"
heading-1 { font-size: 28pt }
heading-2 { font-size: 22pt }
heading-3 { font-size: 18pt }
paragraph { font-size: 15pt }
block-quote { font-size: 15pt; margin-left: 24pt }
list-unordered { margin-left: 24pt }
list-ordered { margin-left: 24pt }
"#;

const COMPACT_ULSS: &str = r#"
heading-1 { font-size: 22pt }
heading-2 { font-size: 18pt }
heading-3 { font-size: 16pt }
paragraph { font-size: 13pt }
block-quote { font-size: 13pt; margin-left: 16pt }
list-unordered { margin-left: 16pt }
list-ordered { margin-left: 16pt }
"#;

/// This project's built-in styles, by name.
pub const BUILTIN_STYLES: &[(&str, &str)] = &[("Default", DEFAULT_ULSS), ("Compact", COMPACT_ULSS)];

pub(crate) fn load_builtin_stylesheet(name: &str) -> Option<Stylesheet> {
    BUILTIN_STYLES
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, src)| resolve::resolve(src).ok())
}

/// UI-facing picker state: which built-in styles exist, which one is selected, and the
/// resulting active stylesheet (or `None` before any selection is made).
#[derive(Default)]
pub struct StylePicker {
    pub selected_name: Option<String>,
    pub active_stylesheet: Option<Stylesheet>,
}

impl StylePicker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn available_names(&self) -> impl Iterator<Item = &'static str> {
        BUILTIN_STYLES.iter().map(|(name, _)| *name)
    }

    /// Selects a built-in style by name, loading it as the active stylesheet. Returns `false`
    /// (and leaves the current selection untouched) if `name` isn't a known built-in style.
    pub fn select(&mut self, name: &str) -> bool {
        match load_builtin_stylesheet(name) {
            Some(sheet) => {
                self.selected_name = Some(name.to_string());
                self.active_stylesheet = Some(sheet);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_style::resolve::Value;
    use parrot_style::select::PathNode;

    fn heading_1_font_size(sheet: &Stylesheet) -> Option<Value> {
        resolve::cascade(sheet, &[PathNode::new("heading-1")], None)
            .get("font-size")
            .cloned()
    }

    #[test]
    fn selecting_style_updates_active_stylesheet() {
        let mut picker = StylePicker::new();
        assert!(picker.active_stylesheet.is_none(), "no style selected yet");

        assert!(picker.select("Compact"));
        assert_eq!(picker.selected_name, Some("Compact".to_string()));
        assert_eq!(
            heading_1_font_size(
                picker
                    .active_stylesheet
                    .as_ref()
                    .expect("Compact must be loaded")
            ),
            Some(Value::Number {
                value: 22.0,
                unit: Some("pt".to_string())
            })
        );

        // Selecting a different style replaces the active stylesheet.
        assert!(picker.select("Default"));
        assert_eq!(picker.selected_name, Some("Default".to_string()));
        assert_eq!(
            heading_1_font_size(
                picker
                    .active_stylesheet
                    .as_ref()
                    .expect("Default must be loaded")
            ),
            Some(Value::Number {
                value: 28.0,
                unit: Some("pt".to_string())
            })
        );

        // An unknown style name is rejected, leaving the current selection in place.
        assert!(!picker.select("Nonexistent Style"));
        assert_eq!(picker.selected_name, Some("Default".to_string()));
    }
}
