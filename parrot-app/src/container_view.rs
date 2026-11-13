//! Continuous "sheet flow" container view (T-10-06, ADR-7): per the confirmed nib inventory
//! (`ULContainerTableViewController.nib`, `ULSheetTableContainerCellView.nib`, inventory §14),
//! Parrot' real editing paradigm is a vertically-stacked list of per-sheet editor cells in one
//! scroll region when a Group/Project/Filter is selected — not one sheet at a time. This is
//! additive over the existing single-sheet editor widget (M2), not a rewrite of it: `ui.rs`
//! repeats that widget once per child sheet, with a divider between entries.
//!
//! `layout_metrics` is the headlessly-testable half, matching this project's established
//! rendering-test convention (`render.rs`'s `layout_paragraph`/`shape_paragraph`: pure
//! arithmetic, no `egui::Ui`). It takes each sheet's own rendered height — which only `ui.rs`
//! knows, from actually laying out that sheet's editor widget — and returns where each editor
//! instance and each divider sits in the scroll region's vertical flow.

use parrot_core::{Library, Node, Sheet};

/// Node path type, matching `crate::state::NodePath` — kept as a plain alias rather than a
/// `use` to avoid a dependency on `crate::state` from this otherwise `AppState`-independent
/// module (it only ever takes `&Node`/`&Library`, never `&AppState`).
pub type NodePath = Vec<usize>;

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerLayoutMetrics {
    /// Top y-offset of each sheet's editor instance, in the same order as the input heights.
    pub editor_positions: Vec<f32>,
    /// Y-offset of each divider — one between every adjacent pair of sheets, so
    /// `divider_positions.len() == editor_positions.len().saturating_sub(1)`.
    pub divider_positions: Vec<f32>,
}

/// Computes vertical layout for a stack of `sheet_heights.len()` editor instances separated by
/// a `divider_height`-tall divider each. Pure arithmetic — the real heights come from actually
/// laying out each sheet's editor widget, not from anything in this module.
pub fn layout_metrics(sheet_heights: &[f32], divider_height: f32) -> ContainerLayoutMetrics {
    let mut editor_positions = Vec::with_capacity(sheet_heights.len());
    let mut divider_positions = Vec::with_capacity(sheet_heights.len().saturating_sub(1));
    let mut y = 0.0;
    for (i, &height) in sheet_heights.iter().enumerate() {
        editor_positions.push(y);
        y += height;
        if i + 1 < sheet_heights.len() {
            divider_positions.push(y);
            y += divider_height;
        }
    }
    ContainerLayoutMetrics {
        editor_positions,
        divider_positions,
    }
}

/// The sheets the sheet-flow view shows for `node`: a Group/Project's own direct Sheet children
/// (nested Groups are not flattened into this view — ADR-7 scopes it to one container's own
/// sheets, matching the nib-confirmed "this container's sheet list" shape; see DECISIONS.md) or
/// a Filter's live query result. A bare Sheet has no children of its own kind here.
pub fn child_sheets<'a>(node: &'a Node, library: &'a Library) -> Vec<&'a Sheet> {
    match node {
        Node::Group(g) => g.children.iter().filter_map(as_sheet).collect(),
        Node::Project(p) => p.children.iter().filter_map(as_sheet).collect(),
        Node::Filter(f) => parrot_core::filter::evaluate(f, library),
        Node::Sheet(_) => Vec::new(),
    }
}

fn as_sheet(node: &Node) -> Option<&Sheet> {
    match node {
        Node::Sheet(s) => Some(s),
        _ => None,
    }
}

/// Same sheets as [`child_sheets`], paired with each one's absolute path into `library` — what
/// `ui.rs` actually needs to make every entry in the flow editable (committing edits back via
/// `AppState::set_sheet_content`). `own_path` is `node`'s own path (empty for the library root);
/// a Filter's results already carry absolute paths (`evaluate_with_paths` searches the whole
/// library, not relative to the filter node), so `own_path` is unused in that branch.
pub fn child_sheets_with_paths<'a>(
    node: &'a Node,
    own_path: &NodePath,
    library: &'a Library,
) -> Vec<(NodePath, &'a Sheet)> {
    match node {
        Node::Group(g) => indexed_sheets(&g.children, own_path),
        Node::Project(p) => indexed_sheets(&p.children, own_path),
        Node::Filter(f) => parrot_core::filter::evaluate_with_paths(f, library),
        Node::Sheet(_) => Vec::new(),
    }
}

fn indexed_sheets<'a>(children: &'a [Node], own_path: &NodePath) -> Vec<(NodePath, &'a Sheet)> {
    children
        .iter()
        .enumerate()
        .filter_map(|(i, child)| {
            as_sheet(child).map(|s| {
                let mut path = own_path.clone();
                path.push(i);
                (path, s)
            })
        })
        .collect()
}

/// One entry in the sheet-flow view's vertical stack (T-12-07, ui-description §5.2): each
/// sheet's editor, with a thin full-width divider between adjacent sheets — none before the
/// first or after the last. `ui.rs` iterates this to draw the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowItem {
    /// Index into the flow's sheet list.
    Sheet(usize),
    Divider,
}

pub fn flow_items(sheet_count: usize) -> Vec<FlowItem> {
    let mut items = Vec::with_capacity((2 * sheet_count).saturating_sub(1));
    for i in 0..sheet_count {
        if i > 0 {
            items.push(FlowItem::Divider);
        }
        items.push(FlowItem::Sheet(i));
    }
    items
}

/// The flow view's aggregate word-count badge (§5.2): the sum of every visible sheet's own
/// `parrot_core::statistics::word_count`.
pub fn aggregate_word_count(sheets: &[&Sheet]) -> u32 {
    sheets
        .iter()
        .map(|s| parrot_core::statistics::word_count(&s.content))
        .fold(0u32, u32::saturating_add)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_sheet(title: &str, text: &str) -> Sheet {
        Sheet {
            title: title.to_string(),
            content: parrot_core::SheetContent::new(vec![parrot_core::Paragraph {
                tag: None,
                inlines: vec![parrot_core::Inline::Text(text.to_string())],
            }]),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        }
    }

    fn three_sheet_container() -> Node {
        Node::Group(parrot_core::Group {
            title: "Flow".to_string(),
            goal: None,
            children: vec![
                Node::Sheet(text_sheet("One", "one two three")),
                Node::Sheet(text_sheet("Two", "four five")),
                Node::Sheet(text_sheet("Three", "six")),
            ],
        })
    }

    #[test]
    fn divider_count_matches_sheet_count_minus_one() {
        let container = three_sheet_container();
        let library = Library::default();
        let sheets = child_sheets(&container, &library);
        let items = flow_items(sheets.len());
        let dividers = items.iter().filter(|i| **i == FlowItem::Divider).count();
        assert_eq!(dividers, 2);
        assert_eq!(
            items,
            vec![
                FlowItem::Sheet(0),
                FlowItem::Divider,
                FlowItem::Sheet(1),
                FlowItem::Divider,
                FlowItem::Sheet(2),
            ]
        );
        assert!(flow_items(0).is_empty());
        assert_eq!(flow_items(1), vec![FlowItem::Sheet(0)]);
    }

    #[test]
    fn aggregate_word_count_sums_all_sheets() {
        let container = three_sheet_container();
        let library = Library::default();
        let sheets = child_sheets(&container, &library);
        let each: u32 = sheets
            .iter()
            .map(|s| parrot_core::statistics::word_count(&s.content))
            .sum();
        assert_eq!(each, 6);
        assert_eq!(aggregate_word_count(&sheets), each);
    }

    #[test]
    fn layout_metrics_for_three_sheet_fixture() {
        let heights = [100.0, 50.0, 200.0];
        let metrics = layout_metrics(&heights, 8.0);

        assert_eq!(
            metrics.editor_positions.len(),
            3,
            "one editor instance per sheet in the fixture"
        );
        assert_eq!(
            metrics.divider_positions.len(),
            2,
            "one divider between each adjacent pair of 3 sheets"
        );
        assert_eq!(metrics.editor_positions, vec![0.0, 108.0, 166.0]);
        assert_eq!(metrics.divider_positions, vec![100.0, 158.0]);
    }

    #[test]
    fn child_sheets_of_a_group_excludes_nested_groups() {
        use parrot_core::Group;

        fn sheet(title: &str) -> Node {
            Node::Sheet(Sheet {
                title: title.to_string(),
                content: parrot_core::SheetContent::new(vec![]),
                goal: None,
                keywords: Vec::new(),
                attachments: Vec::new(),
                glued: false,
                draft: false,
            })
        }

        let group = Node::Group(Group {
            title: "Drafts".to_string(),
            goal: None,
            children: vec![
                sheet("A"),
                Node::Group(Group {
                    title: "Nested".to_string(),
                    goal: None,
                    children: vec![sheet("Hidden")],
                }),
                sheet("B"),
            ],
        });

        let library = Library::default();
        let titles: Vec<&str> = child_sheets(&group, &library)
            .iter()
            .map(|s| s.title.as_str())
            .collect();
        assert_eq!(titles, vec!["A", "B"]);
    }

    #[test]
    fn child_sheets_with_paths_skips_nested_group_indices_correctly() {
        use parrot_core::Group;

        fn sheet(title: &str) -> Node {
            Node::Sheet(Sheet {
                title: title.to_string(),
                content: parrot_core::SheetContent::new(vec![]),
                goal: None,
                keywords: Vec::new(),
                attachments: Vec::new(),
                glued: false,
                draft: false,
            })
        }

        let group = Node::Group(Group {
            title: "Drafts".to_string(),
            goal: None,
            children: vec![
                sheet("A"),
                Node::Group(Group {
                    title: "Nested".to_string(),
                    goal: None,
                    children: vec![sheet("Hidden")],
                }),
                sheet("B"),
            ],
        });

        let library = Library::default();
        let own_path = vec![2]; // the Group's own path, as if it were child 2 of some parent
        let results = child_sheets_with_paths(&group, &own_path, &library);

        let paths: Vec<NodePath> = results.iter().map(|(p, _)| p.clone()).collect();
        // "A" is child 0, the nested Group is child 1 (skipped, not a Sheet), "B" is child 2.
        assert_eq!(paths, vec![vec![2, 0], vec![2, 2]]);
    }
}
