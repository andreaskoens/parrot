//! Filters UI (T-7-01): a plain-data visual-builder for T-1-06's `Filter` model. `ui.rs` binds
//! widgets directly to this struct's fields and calls `build()`/`save_to` — the builder itself
//! has no `egui` dependency, so it's headlessly testable at the `AppState` level, matching this
//! project's established UI architecture (logic in plain Rust, `ui.rs` as a thin shell).

use crate::state::{AppState, StateError};
use parrot_core::{DateRange, Filter, NodeKindMatch};

#[derive(Debug, Clone)]
pub struct FilterBuilder {
    pub title: String,
    pub keyword: String,
    /// T-10-04: matches sheets carrying this exact Keyword tag — see `Filter::keyword_tag`.
    pub keyword_tag: String,
    pub node_kind: NodeKindMatch,
    pub date_after: String,
    pub date_before: String,
}

impl Default for FilterBuilder {
    fn default() -> Self {
        Self {
            title: String::new(),
            keyword: String::new(),
            keyword_tag: String::new(),
            node_kind: NodeKindMatch::Any,
            date_after: String::new(),
            date_before: String::new(),
        }
    }
}

impl FilterBuilder {
    /// Builds the `Filter` this UI state currently describes. An empty text field maps to
    /// `None` rather than `Some("")`, matching how `Filter`'s own fields are optional.
    pub fn build(&self) -> Filter {
        let date_range = match (self.date_after.is_empty(), self.date_before.is_empty()) {
            (true, true) => None,
            _ => Some(DateRange {
                after: (!self.date_after.is_empty()).then(|| self.date_after.clone()),
                before: (!self.date_before.is_empty()).then(|| self.date_before.clone()),
            }),
        };
        Filter {
            title: self.title.clone(),
            keyword: (!self.keyword.is_empty()).then(|| self.keyword.clone()),
            keyword_tag: (!self.keyword_tag.is_empty()).then(|| self.keyword_tag.clone()),
            node_kind: self.node_kind,
            date_range,
        }
    }

    /// Builds and saves this filter into `state`'s tree under `parent`.
    pub fn save_to(&self, state: &mut AppState, parent: &[usize]) -> Result<(), StateError> {
        state.create_filter(parent, self.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::Node;

    #[test]
    fn builder_output_matches_direct_construction() {
        let builder = FilterBuilder {
            title: "Needs review".to_string(),
            keyword: "TODO".to_string(),
            keyword_tag: "draft".to_string(),
            node_kind: NodeKindMatch::Sheet,
            date_after: "2026-01-01".to_string(),
            date_before: String::new(),
        };

        let direct = Filter {
            title: "Needs review".to_string(),
            keyword: Some("TODO".to_string()),
            keyword_tag: Some("draft".to_string()),
            node_kind: NodeKindMatch::Sheet,
            date_range: Some(DateRange {
                after: Some("2026-01-01".to_string()),
                before: None,
            }),
        };

        assert_eq!(builder.build(), direct);

        let mut state = AppState::new();
        builder
            .save_to(&mut state, &[])
            .expect("save_to must succeed at root");
        match state.node(&[0]).expect("filter node must exist") {
            Node::Filter(f) => assert_eq!(f, &direct),
            other => panic!("expected a Filter node, got {other:?}"),
        }
    }

    #[test]
    fn empty_fields_build_a_minimal_filter() {
        let builder = FilterBuilder::default();
        let built = builder.build();
        assert_eq!(built.keyword, None);
        assert_eq!(built.keyword_tag, None);
        assert_eq!(built.date_range, None);
        assert_eq!(built.node_kind, NodeKindMatch::Any);
    }
}
