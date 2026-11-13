//! Selector matching engine (T-4-05): matches a resolved rule's selector (`resolve::SelectorToken`
//! sequence) against a node path — descendant combination (bare whitespace between two
//! selectors), the adjacent-sibling combinator (`+`), the universal selector (`*`), and
//! pseudo-class compounds (`table-cell :header`).

use crate::resolve::SelectorToken;

/// One node in the document path being matched against, identified by its ULSS selector name
/// (`"table-cell"`, `"paragraph"`, `"heading-3"`, ...) plus whichever pseudo-class states are
/// currently active on it (e.g. a header table-cell has `"header"` in `pseudo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PathNode {
    pub kind: String,
    pub pseudo: Vec<String>,
}

impl PathNode {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            pseudo: Vec::new(),
        }
    }

    pub fn with_pseudo(mut self, pseudo: impl Into<String>) -> Self {
        self.pseudo.push(pseudo.into());
        self
    }
}

enum StepKind {
    Universal,
    Named(String),
}

struct CompoundStep {
    kind: StepKind,
    pseudo: Vec<String>,
}

enum Combinator {
    Descendant,
    AdjacentSibling,
}

/// Returns `true` if `selector` matches the node at the end of `path` (root-to-target order),
/// given `preceding_sibling` (the node immediately before the target among its siblings, if
/// any — needed for the adjacent-sibling combinator).
pub fn matches(
    selector: &[SelectorToken],
    path: &[PathNode],
    preceding_sibling: Option<&PathNode>,
) -> bool {
    let steps = compile(selector);
    matches_compiled(&steps, path, preceding_sibling)
}

fn compile(tokens: &[SelectorToken]) -> Vec<(Option<Combinator>, CompoundStep)> {
    let mut steps: Vec<(Option<Combinator>, CompoundStep)> = Vec::new();
    let mut pending_combinator: Option<Combinator> = None;

    for token in tokens {
        match token {
            SelectorToken::Plain(name) => {
                let combinator = (!steps.is_empty())
                    .then(|| pending_combinator.take().unwrap_or(Combinator::Descendant));
                steps.push((
                    combinator,
                    CompoundStep {
                        kind: StepKind::Named(name.clone()),
                        pseudo: Vec::new(),
                    },
                ));
            }
            SelectorToken::Universal => {
                let combinator = (!steps.is_empty())
                    .then(|| pending_combinator.take().unwrap_or(Combinator::Descendant));
                steps.push((
                    combinator,
                    CompoundStep {
                        kind: StepKind::Universal,
                        pseudo: Vec::new(),
                    },
                ));
            }
            SelectorToken::Pseudo(name) => {
                if let Some((_, last)) = steps.last_mut() {
                    last.pseudo.push(name.clone());
                }
            }
            SelectorToken::AdjacentSibling => {
                pending_combinator = Some(Combinator::AdjacentSibling);
            }
            // A lone mixin reference is a mixin *definition* (T-4-04 filters these out of real
            // rules); defensively ignored here rather than panicking if one slips through.
            SelectorToken::MixinRef(_) => {}
        }
    }
    steps
}

fn matches_compiled(
    steps: &[(Option<Combinator>, CompoundStep)],
    path: &[PathNode],
    preceding_sibling: Option<&PathNode>,
) -> bool {
    let Some((_, last_step)) = steps.last() else {
        return false;
    };
    let Some(target) = path.last() else {
        return false;
    };
    if !step_matches(last_step, target) {
        return false;
    }
    if steps.len() == 1 {
        return true;
    }

    let rest = &steps[..steps.len() - 1];
    let (last_combinator, _) = &steps[steps.len() - 1];
    match last_combinator {
        Some(Combinator::AdjacentSibling) => match preceding_sibling {
            Some(sibling) => {
                let mut sibling_path = path[..path.len() - 1].to_vec();
                sibling_path.push(sibling.clone());
                matches_compiled(rest, &sibling_path, None)
            }
            None => false,
        },
        // Descendant (including the implicit case for the first step, which has no
        // combinator): any strict ancestor subpath may satisfy the remaining steps.
        _ => (0..path.len() - 1)
            .rev()
            .any(|i| matches_compiled(rest, &path[..=i], None)),
    }
}

fn step_matches(step: &CompoundStep, node: &PathNode) -> bool {
    let kind_matches = match &step.kind {
        StepKind::Universal => true,
        StepKind::Named(name) => *name == node.kind,
    };
    kind_matches && step.pseudo.iter().all(|p| node.pseudo.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::SelectorToken as T;

    #[test]
    fn descendant_combinator() {
        // "table-cell paragraph" — a paragraph that is a descendant of a table-cell.
        let selector = vec![
            T::Plain("table-cell".to_string()),
            T::Plain("paragraph".to_string()),
        ];
        let path = vec![
            PathNode::new("table"),
            PathNode::new("table-cell"),
            PathNode::new("paragraph"),
        ];
        assert!(matches(&selector, &path, None));

        // A paragraph with no table-cell ancestor must not match.
        let path_no_cell = vec![PathNode::new("block-quote"), PathNode::new("paragraph")];
        assert!(!matches(&selector, &path_no_cell, None));
    }

    #[test]
    fn adjacent_sibling_combinator() {
        // "paragraph + paragraph" — a paragraph immediately preceded by a sibling paragraph.
        let selector = vec![
            T::Plain("paragraph".to_string()),
            T::AdjacentSibling,
            T::Plain("paragraph".to_string()),
        ];
        let path = vec![PathNode::new("block-quote"), PathNode::new("paragraph")];
        let preceding = PathNode::new("paragraph");
        assert!(matches(&selector, &path, Some(&preceding)));

        // No preceding sibling at all: must not match.
        assert!(!matches(&selector, &path, None));

        // Preceding sibling is the wrong kind: must not match.
        let wrong_preceding = PathNode::new("heading-1");
        assert!(!matches(&selector, &path, Some(&wrong_preceding)));
    }

    #[test]
    fn universal_selector() {
        let selector = vec![T::Universal];
        assert!(matches(&selector, &[PathNode::new("paragraph")], None));
        assert!(matches(&selector, &[PathNode::new("table-cell")], None));
        assert!(matches(
            &selector,
            &[PathNode::new("anything-at-all")],
            None
        ));
    }

    #[test]
    fn pseudo_class_compound() {
        // "table-cell :header" — a table-cell that is also a header cell.
        let selector = vec![
            T::Plain("table-cell".to_string()),
            T::Pseudo("header".to_string()),
        ];
        let header_cell = PathNode::new("table-cell").with_pseudo("header");
        let plain_cell = PathNode::new("table-cell");

        assert!(matches(&selector, &[header_cell], None));
        assert!(!matches(&selector, &[plain_cell], None));
    }
}
