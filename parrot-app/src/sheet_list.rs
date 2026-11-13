//! Panel 2 sheet list (T-12-05, ui-description §4): which sheets a sidebar selection lists, the
//! row model (title, two-line preview, status badges), the bold container header (§4.1) and the
//! empty-container footer. `ui.rs` paints rows straight from [`SheetRow`]; glued sheets listed
//! one after the other share one card ([`RunPosition`]).

use crate::badges::{badges_for, Badge};
use crate::sidebar::{SidebarSelection, SmartFilter};
use crate::state::{AppState, NodePath};
use crate::tokens;
use parrot_core::{Node, Sheet, SheetContent};

/// §4.2 placeholder for a sheet without a title, and the §4.2 footer for a container with no
/// sheets (the confirmed `emptySheetLabel` outlet).
pub const EMPTY_SHEET_LABEL: &str = "Empty Sheet";

#[derive(Debug, Clone, PartialEq)]
pub struct SheetRow {
    pub path: NodePath,
    pub title: String,
    pub preview: String,
    pub badges: Vec<Badge>,
    pub run: RunPosition,
}

/// Where a row sits in a card of glued sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPosition {
    Alone,
    First,
    Middle,
    Last,
}

impl RunPosition {
    fn of(glued_to_previous: bool, next_glued_to_this: bool) -> Self {
        match (glued_to_previous, next_glued_to_this) {
            (false, false) => RunPosition::Alone,
            (false, true) => RunPosition::First,
            (true, true) => RunPosition::Middle,
            (true, false) => RunPosition::Last,
        }
    }
}

/// The sheet list's 2-line preview: the first two non-empty paragraphs of the sheet's
/// plain-text projection, one line each.
pub fn preview_snippet(content: &SheetContent) -> String {
    content
        .paragraphs
        .iter()
        .map(|p| p.plain_text())
        .filter(|line| !line.trim().is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn row_for(path: NodePath, sheet: &Sheet, language: crate::language::Language) -> SheetRow {
    let node = Node::Sheet(sheet.clone());
    let mut badges = badges_for(&node);
    // The error dot (§4.2) uses the same checker as the editor's annotations (T-12-11).
    if !crate::annotations::check_in(&sheet.content.plain_text(), language).is_empty() {
        badges.push(Badge::Error);
    }
    SheetRow {
        path,
        title: if sheet.title.trim().is_empty() {
            EMPTY_SHEET_LABEL.to_string()
        } else {
            sheet.title.clone()
        },
        preview: preview_snippet(&sheet.content),
        badges,
        run: RunPosition::Alone,
    }
}

/// The sheet paths Panel 2 lists for `selection`: a container's own sheets (same set as the
/// sheet-flow view, T-10-06), a filter's matches, a smart filter's results, or a directly
/// selected sheet by itself. Several selected nodes list their *full* content — every sheet
/// in each group, nested groups included — one after the other, each sheet once.
pub fn listed_paths(state: &AppState, selection: &SidebarSelection, today: &str) -> Vec<NodePath> {
    match selection {
        SidebarSelection::Smart(smart) => smart.sheet_paths(&state.library, today),
        SidebarSelection::Nodes(paths) => {
            let mut out: Vec<NodePath> = Vec::new();
            for path in paths {
                let found = match state.node(path) {
                    Ok(Node::Filter(_)) => {
                        listed_paths(state, &SidebarSelection::Node(path.clone()), today)
                    }
                    Ok(node) => all_sheets(node, path),
                    Err(_) => Vec::new(),
                };
                for p in found {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
            out
        }
        SidebarSelection::Node(path) => match state.node(path) {
            Ok(Node::Sheet(_)) => vec![path.clone()],
            // A project lists every sheet in it, sections included.
            Ok(node @ Node::Project(_)) => all_sheets(node, path),
            Ok(node) => crate::container_view::child_sheets_with_paths(node, path, &state.library)
                .into_iter()
                .map(|(p, _)| p)
                .collect(),
            Err(_) => Vec::new(),
        },
    }
}

/// Every sheet at or below `node` (at `path`), in tree order.
pub fn all_sheets(node: &Node, path: &NodePath) -> Vec<NodePath> {
    let children = match node {
        Node::Sheet(_) => return vec![path.clone()],
        Node::Filter(_) => return Vec::new(),
        Node::Group(g) => &g.children,
        Node::Project(p) => &p.children,
    };
    children
        .iter()
        .enumerate()
        .flat_map(|(i, child)| {
            let mut p = path.clone();
            p.push(i);
            all_sheets(child, &p)
        })
        .collect()
}

pub fn rows(state: &AppState, paths: &[NodePath]) -> Vec<SheetRow> {
    rows_with(state, paths, &|_| Default::default())
}

/// [`rows`], with each sheet's error dot checked in the language `language_of` gives its path.
pub fn rows_with(
    state: &AppState,
    paths: &[NodePath],
    language_of: &dyn Fn(&NodePath) -> crate::language::Language,
) -> Vec<SheetRow> {
    let mut rows: Vec<SheetRow> = paths
        .iter()
        .filter_map(|p| match state.node(p) {
            Ok(Node::Sheet(s)) => Some(row_for(p.clone(), s, language_of(p))),
            _ => None,
        })
        .collect();
    // Row `i` continues a card when it's glued to the row listed right above it.
    let continues: Vec<bool> = (0..rows.len())
        .map(|i| i > 0 && glued_to(state, &rows[i - 1].path, &rows[i].path))
        .collect();
    for (i, row) in rows.iter_mut().enumerate() {
        row.run = RunPosition::of(continues[i], continues.get(i + 1) == Some(&true));
    }
    rows
}

/// Whether sheet `next` is glued to sheet `prev`.
fn glued_to(state: &AppState, prev: &NodePath, next: &NodePath) -> bool {
    let (Some((&i, pp)), Some((&j, np))) = (prev.split_last(), next.split_last()) else {
        return false;
    };
    pp == np && i + 1 == j && matches!(state.node(next), Ok(Node::Sheet(s)) if s.glued)
}

/// §4.1 header: the selected Panel-1 node's (or smart filter's) name.
pub fn container_header(state: &AppState, selection: &SidebarSelection) -> String {
    match selection {
        SidebarSelection::Smart(smart) => smart.label().to_string(),
        SidebarSelection::Nodes(paths) => format!("{} items selected", paths.len()),
        SidebarSelection::Node(path) => match state.node(path) {
            Ok(Node::Sheet(s)) => s.title.clone(),
            Ok(Node::Group(g)) => g.title.clone(),
            Ok(Node::Project(p)) => p.title.clone(),
            Ok(Node::Filter(f)) => f.title.clone(),
            Err(_) => String::new(),
        },
    }
}

/// The footer label, shown only when the container lists no sheets. The Trash lists its own
/// entries rather than sheets, so it's empty only when the trash itself is.
pub fn footer_label(
    state: &AppState,
    selection: &SidebarSelection,
    row_count: usize,
) -> Option<&'static str> {
    let empty = match selection {
        SidebarSelection::Smart(SmartFilter::Trash) => state.trash.is_empty(),
        _ => row_count == 0,
    };
    empty.then_some(EMPTY_SHEET_LABEL)
}

/// Builds the §12.3 truncated layout for a row's preview: caption size, tertiary color,
/// at most two rows, ellipsis on overflow.
pub fn preview_layout_job(preview: &str, max_width: f32, selected: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        preview.to_string(),
        egui::TextFormat {
            font_id: tokens::UI_CAPTION.font_id(),
            color: if selected {
                tokens::TEXT_PRIMARY
            } else {
                tokens::TEXT_TERTIARY
            },
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: 2,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph};

    fn para(text: &str) -> Paragraph {
        Paragraph {
            tag: None,
            inlines: vec![Inline::Text(text.to_string())],
        }
    }

    fn fixture() -> AppState {
        let mut state = AppState::new();
        state.create_project("Celeste");
        state.create_sheet(&[0], "Finais").expect("sheet");
        state.create_project("Empty");
        let content = SheetContent::new(vec![
            para("First line of the body."),
            para(""),
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Second ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                ],
            },
            para("Third, not shown."),
        ]);
        state.set_sheet_content(&[0, 0], content).expect("content");
        state
    }

    #[test]
    fn preview_snippet_is_first_two_lines() {
        let state = fixture();
        let rows = rows(&state, &[vec![0, 0]]);
        assert_eq!(rows.len(), 1);
        let Ok(Node::Sheet(sheet)) = state.node(&[0, 0]) else {
            panic!("fixture sheet missing");
        };
        let projection: Vec<String> = sheet
            .content
            .paragraphs
            .iter()
            .map(|p| p.plain_text())
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(rows[0].preview, projection[..2].join("\n"));
        assert_eq!(rows[0].preview, "First line of the body.\nSecond bold");
        assert_eq!(rows[0].title, "Finais");
    }

    #[test]
    fn header_matches_selected_container_name() {
        let state = fixture();
        let group = SidebarSelection::Node(vec![0]);
        assert_eq!(container_header(&state, &group), "Celeste");
        assert_eq!(
            container_header(&state, &SidebarSelection::Smart(SmartFilter::All)),
            "All"
        );
        assert_eq!(listed_paths(&state, &group, "2026-10-31"), vec![vec![0, 0]]);
    }

    #[test]
    fn several_nodes_list_their_full_content_once() {
        let mut state = fixture();
        state.create_group(&[0], "Nested").expect("nested");
        state.create_sheet(&[0, 1], "Deep").expect("deep");
        state.create_sheet(&[1], "Other").expect("other");
        // Celeste[Finais, Nested[Deep]], Empty[Other], plus Finais selected on its own too.
        let selection = SidebarSelection::Nodes(vec![vec![0], vec![0, 0], vec![1]]);
        assert_eq!(
            listed_paths(&state, &selection, "2026-10-31"),
            vec![vec![0, 0], vec![0, 1, 0], vec![1, 0]]
        );
        assert_eq!(container_header(&state, &selection), "3 items selected");
    }

    #[test]
    fn empty_footer_only_when_empty() {
        let state = fixture();
        let full = SidebarSelection::Node(vec![0]);
        let empty = SidebarSelection::Node(vec![1]);
        let full_rows = rows(&state, &listed_paths(&state, &full, "2026-10-31"));
        let empty_rows = rows(&state, &listed_paths(&state, &empty, "2026-10-31"));
        assert_eq!(footer_label(&state, &full, full_rows.len()), None);
        assert_eq!(
            footer_label(&state, &empty, empty_rows.len()),
            Some(EMPTY_SHEET_LABEL)
        );
    }

    #[test]
    fn glued_sheets_share_one_card() {
        let mut state = fixture();
        for title in ["B", "C", "D"] {
            state.create_sheet(&[0], title).expect("sheet");
        }
        state
            .glue_sheets(&[vec![0, 0], vec![0, 1], vec![0, 2]])
            .expect("glue");
        let listed = listed_paths(&state, &SidebarSelection::Node(vec![0]), "2026-10-31");
        let runs: Vec<RunPosition> = rows(&state, &listed).iter().map(|r| r.run).collect();
        use RunPosition::*;
        assert_eq!(runs, [First, Middle, Last, Alone]);
        // A run member listed on its own is a card of one.
        let alone = rows(&state, &[vec![0, 1]]);
        assert_eq!(alone[0].run, Alone);
    }
}
