//! The Library shelf: the app's first page, one card per project so the writer can go straight
//! into one. Each card's figures come from the project's subtree; `ui.rs` only paints them.

use crate::badges::{badges_for, Badge, GoalBand};
use crate::state::{AppState, NodePath};
use parrot_core::Node;

/// One project card.
#[derive(Debug, Clone, PartialEq)]
pub struct ShelfItem {
    pub path: NodePath,
    pub title: String,
    pub section: parrot_core::Section,
    pub sheets: usize,
    pub drafts: usize,
    pub sections: usize,
    pub words: u32,
    /// The project's own goal progress, when it has a goal.
    pub goal: Option<(f32, GoalBand)>,
}

impl ShelfItem {
    /// The card's figure line, e.g. "12 sheets · 3 drafts · 4,210 words".
    pub fn summary(&self) -> String {
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut parts = vec![plural(self.sheets, "sheet", "sheets")];
        if self.drafts > 0 {
            parts.push(plural(self.drafts, "draft", "drafts"));
        }
        parts.push(format!(
            "{} {}",
            crate::inspector::format_count(self.words),
            if self.words == 1 { "word" } else { "words" }
        ));
        parts.join(" · ")
    }
}

/// Every project in library order.
pub fn items(state: &AppState) -> Vec<ShelfItem> {
    state
        .projects()
        .into_iter()
        .filter_map(|path| {
            let node = state.node(&path).ok()?;
            let Node::Project(project) = node else {
                return None;
            };
            let mut item = ShelfItem {
                path,
                title: project.title.clone(),
                section: project.section,
                sheets: 0,
                drafts: 0,
                sections: 0,
                words: 0,
                goal: badges_for(node).into_iter().find_map(|b| match b {
                    Badge::GoalRing { fraction, band } => Some((fraction, band)),
                    _ => None,
                }),
            };
            tally(&project.children, &mut item);
            Some(item)
        })
        .collect()
}

fn tally(nodes: &[Node], item: &mut ShelfItem) {
    for node in nodes {
        match node {
            Node::Sheet(s) => {
                item.sheets += 1;
                item.drafts += usize::from(s.draft);
                item.words += parrot_core::statistics::word_count(&s.content);
            }
            Node::Group(g) => {
                item.sections += 1;
                tally(&g.children, item);
            }
            Node::Project(p) => tally(&p.children, item),
            Node::Filter(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_card_per_project_with_its_subtree_figures() {
        let mut state = AppState::new();
        let novel = state.create_project("Novel");
        state.create_group(&novel, "Part 1").expect("section");
        state.create_sheet(&[0, 0], "Ch 1").expect("sheet");
        state
            .set_sheet_content(
                &[0, 0, 0],
                crate::editor::buffer_to_content("one two three"),
            )
            .expect("content");
        state.create_sheet(&novel, "Notes").expect("sheet");
        state.set_draft(&[0, 1], true).expect("draft");
        state.create_project("Essays");

        let shelf = items(&state);
        assert_eq!(shelf.len(), 2);
        assert_eq!(shelf[0].title, "Novel");
        assert_eq!(
            (
                shelf[0].sheets,
                shelf[0].drafts,
                shelf[0].sections,
                shelf[0].words
            ),
            (2, 1, 1, 3)
        );
        assert_eq!(shelf[0].summary(), "2 sheets · 1 draft · 3 words");
        assert_eq!(shelf[1].summary(), "0 sheets · 0 words");
        assert_eq!(shelf[1].path, vec![1]);
    }
}
