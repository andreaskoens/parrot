//! Goal setup/edit flow (T-10-03): a plain-data builder for T-10-02's `Goal` model, same
//! architecture as `filters_ui::FilterBuilder` — `ui.rs` binds widgets directly to this struct's
//! fields and calls `build()`/`save_to`, so the flow itself stays headlessly testable at the
//! `AppState` level. Per inventory §12's confirmed `addCountingGoal:`/`editGoal:` UI surface.
//! Sharing ("I hit my goal" image/text, `ULGoalSharingProvider`) is excluded — network-shaped,
//! same disposition as WordPress/Medium export.

use crate::state::{AppState, StateError};
use parrot_core::{Goal, GoalCounter, GoalKind, Node};

#[derive(Debug, Clone)]
pub struct GoalSetupFlow {
    pub kind: GoalKind,
    pub counter: GoalCounter,
    /// Text field backing `Goal::target` — parsed on `build()`; an empty/invalid field becomes
    /// `0` rather than failing the flow (a goal can always be edited again afterward).
    pub target: String,
    pub deadline: String,
    pub write_on_deadline: bool,
}

impl Default for GoalSetupFlow {
    fn default() -> Self {
        Self {
            kind: GoalKind::Simple,
            counter: GoalCounter::WordCount,
            target: String::new(),
            deadline: String::new(),
            write_on_deadline: false,
        }
    }
}

impl GoalSetupFlow {
    /// Populates this flow's fields from an existing `Goal` (opening the edit flow on a node
    /// that already has one) — the inverse of `build()`.
    pub fn from_goal(goal: &Goal) -> Self {
        Self {
            kind: goal.kind,
            counter: goal.counter,
            target: goal.target.to_string(),
            deadline: goal.deadline.clone().unwrap_or_default(),
            write_on_deadline: goal.write_on_deadline,
        }
    }

    /// Builds the `Goal` this flow's fields currently describe.
    pub fn build(&self) -> Goal {
        Goal {
            kind: self.kind,
            counter: self.counter,
            target: self.target.trim().parse().unwrap_or(0),
            deadline: (!self.deadline.trim().is_empty()).then(|| self.deadline.trim().to_string()),
            write_on_deadline: self.write_on_deadline,
        }
    }

    /// Builds and saves this goal onto the node at `path`.
    pub fn save_to(&self, state: &mut AppState, path: &[usize]) -> Result<(), StateError> {
        state.set_goal(path, Some(self.build()))
    }
}

/// Counts `node`'s plain-text units for `counter` (T-10-03's per-row progress indicator) — a
/// Sheet counts its own content; a Group/Project counts every descendant Sheet's content
/// combined, since a goal on a container tracks the whole container's writing, not one sheet.
pub fn count_units(node: &Node, counter: GoalCounter) -> u32 {
    let text = flatten_plain_text(node);
    let count = match counter {
        GoalCounter::WordCount => text.split_whitespace().count(),
        GoalCounter::CharCount => text.chars().count(),
    };
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn flatten_plain_text(node: &Node) -> String {
    match node {
        Node::Sheet(s) => s.content.plain_text(),
        Node::Group(g) => g
            .children
            .iter()
            .map(flatten_plain_text)
            .collect::<Vec<_>>()
            .join(" "),
        Node::Project(p) => p
            .children
            .iter()
            .map(flatten_plain_text)
            .collect::<Vec<_>>()
            .join(" "),
        Node::Filter(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Group, Inline, Paragraph, Sheet, SheetContent};

    #[test]
    fn setup_flow_output_matches_direct_construction() {
        let flow = GoalSetupFlow {
            kind: GoalKind::Daily,
            counter: GoalCounter::CharCount,
            target: "500".to_string(),
            deadline: "2026-12-31".to_string(),
            write_on_deadline: true,
        };

        let direct = Goal {
            kind: GoalKind::Daily,
            counter: GoalCounter::CharCount,
            target: 500,
            deadline: Some("2026-12-31".to_string()),
            write_on_deadline: true,
        };

        assert_eq!(flow.build(), direct);

        let mut state = AppState::new();
        state.create_project("P");
        state
            .create_sheet(&[0], "Untitled")
            .expect("create_sheet must succeed at root");
        flow.save_to(&mut state, &[0, 0])
            .expect("save_to must succeed on a Sheet");
        match state.node(&[0, 0]).expect("sheet node must exist") {
            Node::Sheet(s) => assert_eq!(s.goal, Some(direct)),
            other => panic!("expected a Sheet node, got {other:?}"),
        }
    }

    #[test]
    fn empty_fields_build_a_zero_target_goal_with_no_deadline() {
        let flow = GoalSetupFlow::default();
        let built = flow.build();
        assert_eq!(built.target, 0);
        assert_eq!(built.deadline, None);
    }

    #[test]
    fn from_goal_round_trips_through_build() {
        let goal = Goal {
            kind: GoalKind::Simple,
            counter: GoalCounter::WordCount,
            target: 1200,
            deadline: Some("2026-06-15".to_string()),
            write_on_deadline: false,
        };
        assert_eq!(GoalSetupFlow::from_goal(&goal).build(), goal);
    }

    fn sheet_with_words(words: &[&str]) -> Node {
        Node::Sheet(Sheet {
            title: "s".to_string(),
            content: SheetContent::new(vec![Paragraph {
                tag: None,
                inlines: vec![Inline::Text(words.join(" "))],
            }]),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        })
    }

    #[test]
    fn count_units_counts_a_sheets_own_words() {
        let sheet = sheet_with_words(&["one", "two", "three"]);
        assert_eq!(count_units(&sheet, GoalCounter::WordCount), 3);
    }

    #[test]
    fn count_units_sums_a_groups_descendant_sheets() {
        let group = Node::Group(Group {
            title: "g".to_string(),
            children: vec![
                sheet_with_words(&["one", "two"]),
                sheet_with_words(&["three", "four", "five"]),
            ],
            goal: None,
        });
        assert_eq!(count_units(&group, GoalCounter::WordCount), 5);
    }
}
