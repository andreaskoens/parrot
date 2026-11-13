//! Conversion rule-table format (T-5-02): this project's own schema — TOML, not a copy of
//! Apple's plist — informed by the confirmed `elementStyles`/`{element, precondition}` shape
//! (feature inventory §5.1, decoded directly from the real
//! `Export Formats/Markdown/Conversion/Default.plist`): a dispatch table keyed by AST node
//! kind, each holding an ordered list of `{precondition, template}` rules, first match wins.
//!
//! Schema (see `README.md` for the full write-up):
//! ```toml
//! [[code]]
//! precondition = "true"
//! template = "`{{ content }}`"
//! ```
//! `precondition` is an expression string evaluated by the DSL evaluator (T-5-03); this module
//! only owns loading the table and "first rule whose precondition matches wins" dispatch order
//! — evaluating what a precondition actually *means* is the caller's job (via the `matches`
//! predicate passed to `dispatch`), keeping this module usable before T-5-03 exists.

use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub precondition: String,
    pub template: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleTable {
    #[serde(flatten)]
    node_kinds: HashMap<String, Vec<Rule>>,
}

#[derive(Debug, Error)]
pub enum RuleTableError {
    #[error("invalid rule table: {0}")]
    Toml(#[from] toml::de::Error),
}

impl RuleTable {
    pub fn load(toml_str: &str) -> Result<Self, RuleTableError> {
        Ok(toml::from_str(toml_str)?)
    }

    /// The first rule for `node_kind` whose precondition satisfies `matches` — "first match
    /// wins," per the confirmed `elementStyles` dispatch order.
    pub fn dispatch<'a>(
        &'a self,
        node_kind: &str,
        mut matches: impl FnMut(&str) -> bool,
    ) -> Option<&'a Rule> {
        self.node_kinds
            .get(node_kind)?
            .iter()
            .find(|rule| matches(&rule.precondition))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_dispatches_trivial_table() {
        let toml_str = r#"
            [[code]]
            precondition = "true"
            template = "`{{ content }}`"
        "#;

        let table = RuleTable::load(toml_str).expect("trivial table must load");
        let rule = table
            .dispatch("code", |p| p == "true")
            .expect("must dispatch to the trivial rule");

        assert_eq!(rule.template, "`{{ content }}`");
        assert!(
            table.dispatch("strong", |_| true).is_none(),
            "unknown node kind must dispatch to nothing"
        );
    }

    #[test]
    fn first_match_wins() {
        let toml_str = r#"
            [[code]]
            precondition = "has_backtick"
            template = "``{{ content }}``"

            [[code]]
            precondition = "true"
            template = "`{{ content }}`"
        "#;

        let table = RuleTable::load(toml_str).expect("table must load");
        // Both rules "match" under this permissive predicate, but the first one in document
        // order must win.
        let rule = table.dispatch("code", |_| true).expect("must dispatch");
        assert_eq!(rule.precondition, "has_backtick");
    }
}
