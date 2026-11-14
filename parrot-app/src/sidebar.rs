//! Library sidebar model (T-12-03, ui-description §3): the smart filters above the tree, the
//! non-interactive section labels, and the right-click context menu's actions. `ui.rs` paints
//! the rows; everything here is `egui`-free so the dispatch rules are testable headlessly.

use crate::state::{AppState, NodePath, StateError};
use parrot_core::filter::{DateRange, Filter, NodeKindMatch};
use parrot_core::{Library, Node};

/// Default titles shared by every "new node" entry point (sidebar `+` menu and context menu),
/// so both create identical nodes.
pub const NEW_GROUP_TITLE: &str = "New Section";
pub const NEW_PROJECT_TITLE: &str = "New Project";
pub const NEW_SHEET_TITLE: &str = "Untitled";

/// The §3.1 smart filters, in display order. (Parrot' "Open Files" is left out: this app
/// only edits sheets inside the library.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartFilter {
    All,
    LastSevenDays,
    Trash,
}

impl SmartFilter {
    pub const ALL: [SmartFilter; 3] = [
        SmartFilter::All,
        SmartFilter::LastSevenDays,
        SmartFilter::Trash,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SmartFilter::All => "All",
            SmartFilter::LastSevenDays => "Last 7 Days",
            SmartFilter::Trash => "Trash",
        }
    }

    /// §3.2: smart filters get their own glyphs, distinct from group/sheet/filter icons.
    pub fn icon(self) -> &'static str {
        match self {
            SmartFilter::All => crate::icons::ALL_SHEETS,
            SmartFilter::LastSevenDays => crate::icons::RECENT,
            SmartFilter::Trash => crate::icons::TRASH,
        }
    }

    /// The equivalent saved-filter query, for the two smart filters that are plain library
    /// queries. `today` is an ISO `YYYY-MM-DD` date (see [`today_iso`]).
    pub fn as_filter(self, today: &str) -> Option<Filter> {
        let base = Filter {
            title: self.label().to_string(),
            keyword: None,
            keyword_tag: None,
            node_kind: NodeKindMatch::Sheet,
            date_range: None,
        };
        match self {
            SmartFilter::All => Some(base),
            SmartFilter::LastSevenDays => Some(Filter {
                date_range: Some(DateRange {
                    after: Some(iso_days_before(today, 7)),
                    before: None,
                }),
                ..base
            }),
            SmartFilter::Trash => None,
        }
    }

    /// Sheets this smart filter lists in Panel 2, as library paths. Trash entries live outside
    /// the tree (`AppState::trash`) and have no path, so `Trash` is listed separately by the UI.
    pub fn sheet_paths(self, library: &Library, today: &str) -> Vec<NodePath> {
        match self.as_filter(today) {
            Some(filter) => parrot_core::filter::evaluate_with_paths(&filter, library)
                .into_iter()
                .map(|(path, _)| path)
                .collect(),
            None => Vec::new(),
        }
    }
}

/// One row of the sidebar's root level, in display order (§3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRow {
    /// "Library": back to the project shelf.
    Shelf,
    Smart(SmartFilter),
    /// §3.5 non-interactive heading.
    Heading(&'static str),
    /// A root-level library node (its children are expanded by the tree renderer).
    Node(NodePath),
}

/// The heading whose row carries the "+" new-item menu.
pub const PROJECTS_HEADING: &str = "Projects";
pub const FILTERS_HEADING: &str = "Filters";
/// macOS-only heading for iCloud Drive projects (hidden elsewhere).
pub const ICLOUD_HEADING: &str = "iCloud";

/// Root rows: Library (the shelf), the smart filters, every project under PROJECTS, then the
/// root-level saved filters under FILTERS when there are any. Sheets and sections live inside
/// projects; anything else at the root (from a library predating that rule) lists last.
///
/// On macOS, iCloud-section projects list under their own ICLOUD heading so one
/// library shows both stores; on other platforms everything lists under PROJECTS
/// (iCloud UI is hidden there).
pub fn root_rows(library: &Library) -> Vec<SidebarRow> {
    let mut rows = vec![SidebarRow::Shelf];
    rows.extend(SmartFilter::ALL.into_iter().map(SidebarRow::Smart));
    let kind = |i: &usize| match library.children[*i] {
        Node::Project(_) => 0,
        Node::Filter(_) => 1,
        _ => 2,
    };
    let mut order: Vec<usize> = (0..library.children.len()).collect();
    order.sort_by_key(|i| (kind(i), *i));
    #[cfg(target_os = "macos")]
    {
        let mut local: Vec<usize> = Vec::new();
        let mut icloud: Vec<usize> = Vec::new();
        let mut filters: Vec<usize> = Vec::new();
        let mut other: Vec<usize> = Vec::new();
        for i in order {
            match &library.children[i] {
                Node::Project(p) if p.section.is_icloud() => icloud.push(i),
                Node::Project(_) => local.push(i),
                Node::Filter(_) => filters.push(i),
                _ => other.push(i),
            }
        }
        rows.push(SidebarRow::Heading(PROJECTS_HEADING));
        for i in local {
            rows.push(SidebarRow::Node(vec![i]));
        }
        // Always show the iCloud heading on macOS (even when empty) so there is
        // a visible place to create the first iCloud project. Other platforms
        // never show it.
        rows.push(SidebarRow::Heading(ICLOUD_HEADING));
        for i in icloud {
            rows.push(SidebarRow::Node(vec![i]));
        }
        if !filters.is_empty() {
            rows.push(SidebarRow::Heading(FILTERS_HEADING));
            for i in filters {
                rows.push(SidebarRow::Node(vec![i]));
            }
        }
        for i in other {
            rows.push(SidebarRow::Node(vec![i]));
        }
        rows
    }
    #[cfg(not(target_os = "macos"))]
    {
        rows.push(SidebarRow::Heading(PROJECTS_HEADING));
        let mut filters_shown = false;
        for i in order {
            if kind(&i) == 1 && !filters_shown {
                rows.push(SidebarRow::Heading(FILTERS_HEADING));
                filters_shown = true;
            }
            rows.push(SidebarRow::Node(vec![i]));
        }
        rows
    }
}

/// Every library node row in display order — root rows as [`root_rows`] orders them, each
/// followed by its descendants unless `collapsed` hides them. The order Shift-click ranges
/// run over.
pub fn visible_tree_paths(
    library: &Library,
    collapsed: &std::collections::BTreeSet<NodePath>,
) -> Vec<NodePath> {
    fn walk(
        node: &Node,
        path: NodePath,
        collapsed: &std::collections::BTreeSet<NodePath>,
        out: &mut Vec<NodePath>,
    ) {
        let children = match node {
            Node::Group(g) => g.children.as_slice(),
            Node::Project(p) => p.children.as_slice(),
            _ => &[],
        };
        let expanded = !collapsed.contains(&path);
        out.push(path.clone());
        if expanded {
            for (i, child) in children.iter().enumerate() {
                let mut p = path.clone();
                p.push(i);
                walk(child, p, collapsed, out);
            }
        }
    }
    let mut out = Vec::new();
    for row in root_rows(library) {
        if let SidebarRow::Node(path) = row {
            walk(&library.children[path[0]], path, collapsed, &mut out);
        }
    }
    out
}

/// What Panel 1 currently has selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarSelection {
    Smart(SmartFilter),
    Node(NodePath),
    /// Two or more library nodes (Ctrl/Shift-click), in tree order.
    Nodes(Vec<NodePath>),
}

impl SidebarSelection {
    /// The library nodes selected, in tree order (none for a smart filter).
    pub fn nodes(&self) -> Vec<NodePath> {
        match self {
            SidebarSelection::Smart(_) => Vec::new(),
            SidebarSelection::Node(p) => vec![p.clone()],
            SidebarSelection::Nodes(v) => v.clone(),
        }
    }

    /// A selection of `nodes`: none, one node, or several.
    pub fn of_nodes(mut nodes: Vec<NodePath>) -> Option<SidebarSelection> {
        match nodes.len() {
            0 => None,
            1 => nodes.pop().map(SidebarSelection::Node),
            _ => Some(SidebarSelection::Nodes(nodes)),
        }
    }
}

/// The §3.4 context menu, plus Duplicate (an existing per-row action that would otherwise
/// have nowhere to live once the row buttons became a context menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    /// A new Section (`Node::Group`).
    NewGroup,
    NewSheet,
    NewFilter,
    Rename,
    Duplicate,
    MoveToTrash,
    Export,
    AddGoal,
    EditKeywords,
    MarkDraft,
    UnmarkDraft,
    GetInfo,
    /// Copies the project's folder path on disk to the clipboard.
    CopyFolderPath,
    /// Moves a project to the iCloud section (macOS only; hidden elsewhere).
    MoveToICloud,
    /// Moves a project back to the local section (macOS only; hidden elsewhere).
    MoveToLocal,
}

impl ContextAction {
    pub fn label(self) -> &'static str {
        match self {
            ContextAction::NewGroup => "New Section",
            ContextAction::NewSheet => "New Sheet",
            ContextAction::NewFilter => "New Filter",
            ContextAction::Rename => "Rename",
            ContextAction::Duplicate => "Duplicate",
            ContextAction::MoveToTrash => "Move to Trash",
            ContextAction::Export => "Export…",
            ContextAction::AddGoal => "Add Goal…",
            ContextAction::EditKeywords => "Add/Edit Keywords…",
            ContextAction::MarkDraft => "Mark as Draft",
            ContextAction::UnmarkDraft => "Unmark as Draft",
            ContextAction::GetInfo => "Get Info",
            ContextAction::CopyFolderPath => "Copy Folder Path",
            ContextAction::MoveToICloud => "Move to iCloud",
            ContextAction::MoveToLocal => "Move to Local",
        }
    }

    /// The actions offered for `node`, in §3.4 order. Keywords/Export are sheet-only (the
    /// keyword and export models are per-sheet); a Filter can't carry a Goal.
    /// iCloud moves are offered for projects only, and only on macOS.
    pub fn for_node(node: &Node) -> Vec<ContextAction> {
        use ContextAction::*;
        let mut actions = vec![
            NewGroup,
            NewSheet,
            NewFilter,
            Rename,
            Duplicate,
            MoveToTrash,
        ];
        if matches!(node, Node::Sheet(_)) {
            actions.push(Export);
        }
        if !matches!(node, Node::Filter(_)) {
            actions.push(AddGoal);
        }
        if let Node::Sheet(s) = node {
            actions.push(EditKeywords);
            actions.push(if s.draft { UnmarkDraft } else { MarkDraft });
        }
        if let Node::Project(p) = node {
            actions.push(CopyFolderPath);
            #[cfg(target_os = "macos")]
            {
                actions.push(if p.section.is_icloud() {
                    MoveToLocal
                } else {
                    MoveToICloud
                });
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = p.section;
            }
        }
        actions.push(GetInfo);
        actions
    }
}

/// A follow-up the UI must perform after [`apply_context_action`] — anything needing a dialog
/// or inline editor rather than an immediate tree mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiIntent {
    None,
    BeginRename(NodePath),
    OpenFilterBuilder,
    OpenExport(NodePath),
    OpenGoal(NodePath),
    OpenKeywords(NodePath),
    OpenInfo(NodePath),
    CopyFolderPath(NodePath),
}

/// The parent new nodes are created under when "New …" is chosen on `path`: inside it for a
/// Group/Project, beside it otherwise.
pub fn creation_parent(library: &Library, path: &[usize]) -> NodePath {
    let is_container = matches!(
        node_at(library, path),
        Some(Node::Group(_)) | Some(Node::Project(_))
    );
    if is_container {
        path.to_vec()
    } else {
        path[..path.len().saturating_sub(1)].to_vec()
    }
}

fn node_at<'a>(library: &'a Library, path: &[usize]) -> Option<&'a Node> {
    let (&last, parent) = path.split_last()?;
    let mut children = &library.children;
    for &i in parent {
        children = match children.get(i)? {
            Node::Group(g) => &g.children,
            Node::Project(p) => &p.children,
            _ => return None,
        };
    }
    children.get(last)
}

/// Runs `action` on the node at `path`. Tree mutations go through the same `AppState`
/// methods the non-menu entry points use (sidebar `+` menu, inline rename, delete).
pub fn apply_context_action(
    state: &mut AppState,
    path: &[usize],
    action: ContextAction,
) -> Result<UiIntent, StateError> {
    state.node(path)?;
    let parent = creation_parent(&state.library, path);
    Ok(match action {
        ContextAction::NewGroup => {
            state.create_group(&parent, NEW_GROUP_TITLE)?;
            UiIntent::None
        }
        ContextAction::NewSheet => {
            state.create_sheet(&parent, NEW_SHEET_TITLE)?;
            UiIntent::None
        }
        ContextAction::NewFilter => UiIntent::OpenFilterBuilder,
        ContextAction::Rename => UiIntent::BeginRename(path.to_vec()),
        ContextAction::Duplicate => {
            state.duplicate_node(path)?;
            UiIntent::None
        }
        ContextAction::MoveToTrash => {
            state.move_to_trash(path)?;
            UiIntent::None
        }
        ContextAction::Export => UiIntent::OpenExport(path.to_vec()),
        ContextAction::AddGoal => UiIntent::OpenGoal(path.to_vec()),
        ContextAction::EditKeywords => UiIntent::OpenKeywords(path.to_vec()),
        ContextAction::MarkDraft | ContextAction::UnmarkDraft => {
            state.set_draft(path, action == ContextAction::MarkDraft)?;
            UiIntent::None
        }
        ContextAction::GetInfo => UiIntent::OpenInfo(path.to_vec()),
        ContextAction::CopyFolderPath => UiIntent::CopyFolderPath(path.to_vec()),
        ContextAction::MoveToICloud => {
            state.move_project_to_icloud(path)?;
            // Persist the section change immediately so the project lands in
            // the iCloud store; a later save would do it anyway.
            let _ = state.save();
            UiIntent::None
        }
        ContextAction::MoveToLocal => {
            state.move_project_to_local(path)?;
            let _ = state.save();
            UiIntent::None
        }
    })
}

/// Today's date as ISO `YYYY-MM-DD` (UTC), from the system clock — no calendar crate needed
/// for a whole-day resolution.
pub fn today_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(i64::try_from(secs / 86_400).unwrap_or(0));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `iso` minus `days`, as ISO. An unparseable input is returned unchanged.
pub fn iso_days_before(iso: &str, days: i64) -> String {
    let parts: Vec<i64> = iso.split('-').filter_map(|p| p.parse().ok()).collect();
    let [y, m, d] = parts[..] else {
        return iso.to_string();
    };
    let (y, m, d) = civil_from_days(days_from_civil(y, m, d) - days);
    format!("{y:04}-{m:02}-{d:02}")
}

// Howard Hinnant's days-from-civil / civil-from-days (proleptic Gregorian, epoch 1970-01-01).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two projects: [0] holds one sheet; [1] holds Chapter 1 and a nested section.
    fn fixture() -> AppState {
        let mut state = AppState::new();
        state.create_project("Essays");
        state.create_sheet(&[0], "Loose").expect("sheet");
        state.create_project("Drafts");
        state.create_sheet(&[1], "Chapter 1").expect("sheet");
        state.create_group(&[1], "Nested").expect("group");
        state.create_sheet(&[1, 1], "Deep").expect("sheet");
        state
    }

    fn all_sheet_paths(nodes: &[Node], prefix: &mut NodePath, out: &mut Vec<NodePath>) {
        for (i, node) in nodes.iter().enumerate() {
            prefix.push(i);
            match node {
                Node::Sheet(_) => out.push(prefix.clone()),
                Node::Group(g) => all_sheet_paths(&g.children, prefix, out),
                Node::Project(p) => all_sheet_paths(&p.children, prefix, out),
                Node::Filter(_) => {}
            }
            prefix.pop();
        }
    }

    #[test]
    fn smart_filter_all_matches_full_library_scan() {
        let state = fixture();
        let mut expected = Vec::new();
        all_sheet_paths(&state.library.children, &mut Vec::new(), &mut expected);
        assert_eq!(expected.len(), 3);
        assert_eq!(
            SmartFilter::All.sheet_paths(&state.library, "2026-10-31"),
            expected
        );
    }

    #[test]
    fn smart_filter_last_7_days_matches_date_range_query() {
        let state = fixture();
        let direct = Filter {
            title: "direct".to_string(),
            keyword: None,
            keyword_tag: None,
            node_kind: NodeKindMatch::Sheet,
            date_range: Some(DateRange {
                after: Some("2026-10-24".to_string()),
                before: None,
            }),
        };
        let expected: Vec<NodePath> =
            parrot_core::filter::evaluate_with_paths(&direct, &state.library)
                .into_iter()
                .map(|(p, _)| p)
                .collect();
        let smart = SmartFilter::LastSevenDays
            .as_filter("2026-10-31")
            .expect("Last 7 Days is a library query");
        assert_eq!(smart.date_range, direct.date_range);
        assert_eq!(
            SmartFilter::LastSevenDays.sheet_paths(&state.library, "2026-10-31"),
            expected
        );
    }

    #[test]
    fn context_menu_action_dispatches_to_existing_handler() {
        // New Sheet on a sheet creates a sibling, exactly as `create_sheet` on its parent does.
        let mut via_menu = fixture();
        let mut direct = fixture();
        apply_context_action(&mut via_menu, &[1, 0], ContextAction::NewSheet).expect("menu");
        direct.create_sheet(&[1], NEW_SHEET_TITLE).expect("direct");
        assert_eq!(via_menu.library, direct.library);

        // New Group on a group nests inside it.
        apply_context_action(&mut via_menu, &[1], ContextAction::NewGroup).expect("menu");
        direct.create_group(&[1], NEW_GROUP_TITLE).expect("direct");
        assert_eq!(via_menu.library, direct.library);

        // Move to Trash == the sidebar's delete handler.
        apply_context_action(&mut via_menu, &[0], ContextAction::MoveToTrash).expect("menu");
        direct.move_to_trash(&[0]).expect("direct");
        assert_eq!(via_menu.library, direct.library);
        assert_eq!(via_menu.trash, direct.trash);

        // Duplicate == tree_ops' duplicate.
        apply_context_action(&mut via_menu, &[0], ContextAction::Duplicate).expect("menu");
        crate::tree_ops::duplicate_node(&mut direct, &[0]).expect("direct");
        assert_eq!(via_menu.library, direct.library);

        // Rename hands off to the inline editor, which commits via `rename_node`.
        assert_eq!(
            apply_context_action(&mut via_menu, &[0], ContextAction::Rename),
            Ok(UiIntent::BeginRename(vec![0]))
        );
        assert_eq!(
            apply_context_action(&mut via_menu, &[9], ContextAction::Rename),
            Err(StateError::InvalidPath)
        );
    }

    #[test]
    fn root_rows_order_and_sections() {
        let state = fixture();
        let rows = root_rows(&state.library);
        assert_eq!(rows[0], SidebarRow::Shelf);
        assert_eq!(rows[1..4], SmartFilter::ALL.map(SidebarRow::Smart));
        assert_eq!(rows[4], SidebarRow::Heading(PROJECTS_HEADING));
        #[cfg(target_os = "macos")]
        assert_eq!(
            rows[5..],
            [
                SidebarRow::Node(vec![0]),
                SidebarRow::Node(vec![1]),
                SidebarRow::Heading(ICLOUD_HEADING),
            ]
        );
        #[cfg(not(target_os = "macos"))]
        assert_eq!(
            rows[5..],
            [SidebarRow::Node(vec![0]), SidebarRow::Node(vec![1])]
        );
        assert!(!rows
            .iter()
            .any(|r| matches!(r, SidebarRow::Heading("Notes"))));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn root_rows_splits_icloud_projects() {
        let mut state = AppState::new();
        state.create_project("Local");
        state.create_project_in_section("Cloud", parrot_core::Section::ICloud);
        let rows = root_rows(&state.library);
        assert_eq!(
            rows,
            vec![
                SidebarRow::Shelf,
                SidebarRow::Smart(SmartFilter::All),
                SidebarRow::Smart(SmartFilter::LastSevenDays),
                SidebarRow::Smart(SmartFilter::Trash),
                SidebarRow::Heading(PROJECTS_HEADING),
                SidebarRow::Node(vec![0]),
                SidebarRow::Heading(ICLOUD_HEADING),
                SidebarRow::Node(vec![1]),
            ]
        );
    }

    #[test]
    fn visible_tree_paths_follow_display_order_and_collapse() {
        let state = fixture();
        let mut collapsed = std::collections::BTreeSet::new();
        let all = visible_tree_paths(&state.library, &collapsed);
        assert_eq!(
            all,
            vec![
                vec![0],
                vec![0, 0],
                vec![1],
                vec![1, 0],
                vec![1, 1],
                vec![1, 1, 0]
            ]
        );
        collapsed.insert(vec![1, 1]);
        let shown = visible_tree_paths(&state.library, &collapsed);
        assert_eq!(shown, all[..5].to_vec());
        collapsed.insert(vec![1]);
        assert_eq!(
            visible_tree_paths(&state.library, &collapsed),
            all[..3].to_vec()
        );
    }

    #[test]
    fn selection_of_nodes() {
        assert_eq!(SidebarSelection::of_nodes(Vec::new()), None);
        assert_eq!(
            SidebarSelection::of_nodes(vec![vec![1]]),
            Some(SidebarSelection::Node(vec![1]))
        );
        let two = SidebarSelection::of_nodes(vec![vec![0], vec![1]]).expect("two");
        assert_eq!(two.nodes(), vec![vec![0], vec![1]]);
        assert!(SidebarSelection::Smart(SmartFilter::All).nodes().is_empty());
    }

    #[test]
    fn context_actions_respect_node_kind() {
        let state = fixture();
        let sheet = state.node(&[0, 0]).expect("sheet");
        let group = state.node(&[1]).expect("group");
        assert!(ContextAction::for_node(sheet).contains(&ContextAction::EditKeywords));
        assert!(ContextAction::for_node(sheet).contains(&ContextAction::MarkDraft));
        assert!(!ContextAction::for_node(group).contains(&ContextAction::MarkDraft));
        assert!(!ContextAction::for_node(group).contains(&ContextAction::EditKeywords));
        assert!(ContextAction::for_node(group).contains(&ContextAction::AddGoal));
    }

    #[test]
    fn iso_date_arithmetic() {
        assert_eq!(iso_days_before("2026-10-31", 7), "2026-10-24");
        assert_eq!(iso_days_before("2026-03-03", 7), "2026-02-24");
        assert_eq!(iso_days_before("2024-03-01", 1), "2024-02-29");
        assert_eq!(iso_days_before("garbage", 7), "garbage");
        assert_eq!(today_iso().len(), 10);
    }
}
