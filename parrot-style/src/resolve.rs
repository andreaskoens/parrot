//! AST -> Stylesheet model (T-4-04): resolves variables and expands mixins (merged into their
//! usage sites with override precedence — the mixin's own declarations first, then the usage
//! site's block overriding by property name).

use crate::grammar::{self, GrammarError, Rule};
use pest::iterators::Pair;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("grammar error: {0}")]
    Grammar(#[from] GrammarError),
    #[error("unknown mixin referenced: {0}")]
    UnknownMixin(String),
}

/// A resolved property value — variables already substituted.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Keyword(String),
    Number { value: f64, unit: Option<String> },
    HexColor(u8, u8, u8),
    RgbColor(u8, u8, u8, Option<f32>),
    Str(String),
    List(Vec<Value>),
}

/// One token of a (possibly compound) selector path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorToken {
    Plain(String),
    Pseudo(String),
    Universal,
    AdjacentSibling,
    /// Only ever appears when a rule's *entire* selector is a lone mixin reference — i.e. a
    /// mixin definition, not a real rule. `resolve` filters these out of `Stylesheet::rules`.
    MixinRef(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRule {
    pub selector: Vec<SelectorToken>,
    pub declarations: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    pub rules: Vec<ResolvedRule>,
}

/// The cascaded result for one node: every property set by any rule that matches it, with
/// later-in-document-order matching rules overriding earlier ones on shared properties — the
/// same last-wins precedence `resolve`'s mixin expansion already uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComputedStyle {
    pub properties: HashMap<String, Value>,
}

impl ComputedStyle {
    pub fn get(&self, property: &str) -> Option<&Value> {
        self.properties.get(property)
    }
}

/// T-4-06: the cascade/resolution function — walks every rule in `stylesheet` in order,
/// merging the declarations of every rule whose selector matches `path` (via
/// `crate::select::matches`) into one `ComputedStyle`.
pub fn cascade(
    stylesheet: &Stylesheet,
    path: &[crate::select::PathNode],
    preceding_sibling: Option<&crate::select::PathNode>,
) -> ComputedStyle {
    let mut properties = HashMap::new();
    for rule in &stylesheet.rules {
        if crate::select::matches(&rule.selector, path, preceding_sibling) {
            for (property, value) in &rule.declarations {
                properties.insert(property.clone(), value.clone());
            }
        }
    }
    ComputedStyle { properties }
}

/// Parses and fully resolves a ULSS stylesheet: variables substituted, mixins expanded.
pub fn resolve(input: &str) -> Result<Stylesheet, ResolveError> {
    let pairs = grammar::parse(input)?;
    let stylesheet_pair = pairs
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("grammar always produces one `stylesheet` pair"));
    let items: Vec<Pair<Rule>> = stylesheet_pair.into_inner().collect();

    // Pass 1: variables. None of the real fixtures (T-4-01) have a variable definition
    // referencing another variable, so a single flat pass is sufficient.
    let mut variables: HashMap<String, Value> = HashMap::new();
    for item in &items {
        if item.as_rule() == Rule::variable_definition {
            let mut inner = item.clone().into_inner();
            let var_pair = inner
                .next()
                .unwrap_or_else(|| unreachable!("variable_definition has a variable"));
            let value_pair = inner
                .next()
                .unwrap_or_else(|| unreachable!("variable_definition has a value"));
            variables.insert(
                strip_first_char(var_pair.as_str()),
                resolve_value(&value_pair, &variables.clone()),
            );
        }
    }

    // Pass 2: every `rule` item, split into real rules vs. mixin definitions (a mixin
    // definition's selector_path is exactly one lone mixin_ref token).
    struct RawRule<'a> {
        selector: Vec<SelectorToken>,
        mixin_ref: Option<String>,
        block: Pair<'a, Rule>,
    }

    let mut raw_rules = Vec::new();
    for item in &items {
        if item.as_rule() != Rule::rule {
            continue;
        }
        let mut inner = item.clone().into_inner();
        let selector_path_pair = inner
            .next()
            .unwrap_or_else(|| unreachable!("rule has a selector_path"));
        let mut mixin_ref = None;
        let mut block_pair = None;
        for p in inner {
            match p.as_rule() {
                Rule::mixin_application => {
                    let mr = p
                        .into_inner()
                        .next()
                        .unwrap_or_else(|| unreachable!("mixin_application has a mixin_ref"));
                    mixin_ref = Some(strip_first_char(mr.as_str()));
                }
                Rule::block => block_pair = Some(p),
                _ => {}
            }
        }
        raw_rules.push(RawRule {
            selector: parse_selector_path(&selector_path_pair),
            mixin_ref,
            block: block_pair.unwrap_or_else(|| unreachable!("rule has a block")),
        });
    }

    let mut mixin_defs: HashMap<String, (Option<String>, Pair<Rule>)> = HashMap::new();
    let mut real_rules = Vec::new();
    for rr in raw_rules {
        if let [SelectorToken::MixinRef(name)] = rr.selector.as_slice() {
            mixin_defs.insert(name.clone(), (rr.mixin_ref, rr.block));
        } else {
            real_rules.push(rr);
        }
    }

    let mut resolved_mixins: HashMap<String, Vec<(String, Value)>> = HashMap::new();
    let mixin_names: Vec<String> = mixin_defs.keys().cloned().collect();
    for name in mixin_names {
        resolve_mixin(&name, &mixin_defs, &variables, &mut resolved_mixins)?;
    }

    let mut rules = Vec::new();
    for rr in real_rules {
        let mut declarations: Vec<(String, Value)> = Vec::new();
        if let Some(mixin_name) = &rr.mixin_ref {
            let base = resolved_mixins
                .get(mixin_name)
                .ok_or_else(|| ResolveError::UnknownMixin(mixin_name.clone()))?;
            declarations.extend(base.iter().cloned());
        }
        merge_override(&mut declarations, parse_declarations(&rr.block, &variables));
        rules.push(ResolvedRule {
            selector: rr.selector,
            declarations,
        });
    }

    Ok(Stylesheet { rules })
}

fn resolve_mixin<'a>(
    name: &str,
    defs: &HashMap<String, (Option<String>, Pair<'a, Rule>)>,
    vars: &HashMap<String, Value>,
    resolved: &mut HashMap<String, Vec<(String, Value)>>,
) -> Result<Vec<(String, Value)>, ResolveError> {
    if let Some(existing) = resolved.get(name) {
        return Ok(existing.clone());
    }
    let (base_name, block) = defs
        .get(name)
        .ok_or_else(|| ResolveError::UnknownMixin(name.to_string()))?;
    let block = block.clone();
    let mut declarations = match base_name {
        Some(base) => resolve_mixin(base, defs, vars, resolved)?,
        None => Vec::new(),
    };
    merge_override(&mut declarations, parse_declarations(&block, vars));
    resolved.insert(name.to_string(), declarations.clone());
    Ok(declarations)
}

/// Overrides `base` in place with `overrides`, by property name — a later declaration for the
/// same property replaces the earlier one; a new property is appended.
fn merge_override(base: &mut Vec<(String, Value)>, overrides: Vec<(String, Value)>) {
    for (key, value) in overrides {
        match base.iter_mut().find(|(k, _)| *k == key) {
            Some(existing) => existing.1 = value,
            None => base.push((key, value)),
        }
    }
}

fn parse_selector_path(pair: &Pair<Rule>) -> Vec<SelectorToken> {
    pair.clone()
        .into_inner()
        .map(|token| match token.as_rule() {
            Rule::mixin_ref => SelectorToken::MixinRef(strip_first_char(token.as_str())),
            Rule::pseudo_class => SelectorToken::Pseudo(strip_first_char(token.as_str())),
            Rule::universal_selector => SelectorToken::Universal,
            Rule::combinator => SelectorToken::AdjacentSibling,
            Rule::plain_selector => SelectorToken::Plain(token.as_str().to_string()),
            other => unreachable!("unexpected selector token: {other:?}"),
        })
        .collect()
}

fn parse_declarations(block: &Pair<Rule>, vars: &HashMap<String, Value>) -> Vec<(String, Value)> {
    block
        .clone()
        .into_inner()
        .filter(|p| p.as_rule() == Rule::declaration)
        .map(|decl| {
            let mut inner = decl.into_inner();
            let property = inner
                .next()
                .unwrap_or_else(|| unreachable!("declaration has a property"))
                .as_str()
                .to_string();
            let value_pair = inner
                .next()
                .unwrap_or_else(|| unreachable!("declaration has a value"));
            (property, resolve_value(&value_pair, vars))
        })
        .collect()
}

fn resolve_value(value_pair: &Pair<Rule>, vars: &HashMap<String, Value>) -> Value {
    let items: Vec<Value> = value_pair
        .clone()
        .into_inner()
        .map(|item| resolve_value_item(&item, vars))
        .collect();
    match items.len() {
        1 => items.into_iter().next().unwrap_or_else(|| unreachable!()),
        _ => Value::List(items),
    }
}

fn resolve_value_item(pair: &Pair<Rule>, vars: &HashMap<String, Value>) -> Value {
    match pair.as_rule() {
        Rule::hex_color => {
            let hex = &pair.as_str()[1..]; // strip '#'
            let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
            Value::HexColor(byte(0), byte(2), byte(4))
        }
        Rule::rgb_color => {
            let nums: Vec<f64> = pair
                .clone()
                .into_inner()
                .filter(|p| p.as_rule() == Rule::number)
                .map(|p| parse_number_str(p.as_str()).0)
                .collect();
            Value::RgbColor(
                nums.first().copied().unwrap_or(0.0) as u8,
                nums.get(1).copied().unwrap_or(0.0) as u8,
                nums.get(2).copied().unwrap_or(0.0) as u8,
                nums.get(3).map(|a| *a as f32),
            )
        }
        Rule::number => {
            let (value, unit) = parse_number_str(pair.as_str());
            Value::Number { value, unit }
        }
        Rule::string => {
            let s = pair.as_str();
            Value::Str(s[1..s.len() - 1].to_string())
        }
        Rule::variable => {
            let name = strip_first_char(pair.as_str());
            vars.get(&name).cloned().unwrap_or(Value::Keyword(name))
        }
        Rule::bracket_list => Value::List(
            pair.clone()
                .into_inner()
                .map(|item| resolve_value_item(&item, vars))
                .collect(),
        ),
        Rule::keyword => Value::Keyword(pair.as_str().to_string()),
        other => unreachable!("unexpected value item: {other:?}"),
    }
}

const UNITS: &[&str] = &["pt", "mm", "in", "ex", "en", "em", "cm", "%"];

fn parse_number_str(s: &str) -> (f64, Option<String>) {
    for unit in UNITS {
        if let Some(stripped) = s.strip_suffix(unit) {
            return (stripped.parse().unwrap_or(0.0), Some(unit.to_string()));
        }
    }
    (s.parse().unwrap_or(0.0), None)
}

fn strip_first_char(s: &str) -> String {
    s.chars().skip(1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-4-04's confirmed worked example — verbatim from `defaults.ulss` lines 130-135
    /// (location corrected per T-4-01's fixture review; the content was always accurate):
    ///
    /// ```text
    /// @inline-footnote-style {
    ///     baseline-shift: superscript
    /// }
    ///
    /// inline-footnote :anchor : @inline-footnote-style {}
    /// inline-annotation :anchor : @inline-footnote-style {}
    /// ```
    #[test]
    fn mixin_and_variable_resolution_matches_manuscript_example() {
        let input = r#"
            $test-color = #d1a804;

            @inline-footnote-style {
                baseline-shift: superscript
            }

            inline-footnote :anchor : @inline-footnote-style {
                font-color: $test-color
            }
        "#;

        let sheet = resolve(input).expect("resolve must succeed");
        assert_eq!(
            sheet.rules.len(),
            1,
            "the mixin definition must not itself become a rule"
        );

        let rule = &sheet.rules[0];
        assert_eq!(
            rule.selector,
            vec![
                SelectorToken::Plain("inline-footnote".to_string()),
                SelectorToken::Pseudo("anchor".to_string()),
            ]
        );

        // Inherited from the mixin.
        assert_eq!(
            rule.declarations
                .iter()
                .find(|(k, _)| k == "baseline-shift")
                .map(|(_, v)| v.clone()),
            Some(Value::Keyword("superscript".to_string()))
        );
        // The usage site's own declaration, with its variable resolved.
        assert_eq!(
            rule.declarations
                .iter()
                .find(|(k, _)| k == "font-color")
                .map(|(_, v)| v.clone()),
            Some(Value::HexColor(0xd1, 0xa8, 0x04))
        );
    }

    #[test]
    fn mixin_override_precedence() {
        let input = r#"
            @code {
                font-weight: normal
                font-slant: normal
            }

            block-code : @code {
                font-weight: bold
            }
        "#;

        let sheet = resolve(input).expect("resolve must succeed");
        let rule = &sheet.rules[0];

        // The usage site overrides font-weight, but inherits font-slant unchanged.
        assert_eq!(
            rule.declarations
                .iter()
                .find(|(k, _)| k == "font-weight")
                .map(|(_, v)| v.clone()),
            Some(Value::Keyword("bold".to_string()))
        );
        assert_eq!(
            rule.declarations
                .iter()
                .find(|(k, _)| k == "font-slant")
                .map(|(_, v)| v.clone()),
            Some(Value::Keyword("normal".to_string()))
        );
    }

    /// Robustness check one layer up from T-4-03's grammar-only test: every real fixture must
    /// not just *parse*, but fully *resolve* (variables + mixins) without error.
    #[test]
    fn resolves_all_real_fixtures_without_error() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/ulss");
        for entry in std::fs::read_dir(dir).expect("fixtures dir must exist") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_some_and(|e| e == "ulss") {
                let content = std::fs::read_to_string(&path).expect("readable");
                let result = resolve(&content);
                assert!(result.is_ok(), "{}: {:?}", path.display(), result.err());
            }
        }
    }

    /// T-4-06: `table-cell :header` in the real `manuscript.ulss` sets `font-weight: bold`
    /// (confirmed verbatim, lines 169-174) — the cascade must produce that computed value.
    #[test]
    fn table_cell_header_is_bold_per_manuscript_ulss() {
        use crate::select::PathNode;

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/ulss/manuscript.ulss"
        );
        let content = std::fs::read_to_string(path).expect("manuscript.ulss must be readable");
        let stylesheet = resolve(&content).expect("manuscript.ulss must resolve");

        let header_cell = PathNode::new("table-cell").with_pseudo("header");
        let computed = cascade(&stylesheet, &[header_cell], None);

        assert_eq!(
            computed.get("font-weight"),
            Some(&Value::Keyword("bold".to_string()))
        );
    }
}
