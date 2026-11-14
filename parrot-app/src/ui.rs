//! `egui`/`eframe` app shell (M12, ui-description §2): menu bar and toolbar on top, then the
//! four panels — Library (P1), Sheet List (P2), Editor (P3), Inspector (P4) — and a status bar.
//! Wraps `AppState` and owns no document state itself, only UI-transient state (selection,
//! editor buffers, dialog fields). Every decision with a testable rule lives in its own module
//! (`layout`, `sidebar`, `sheet_list`, `editor`, `container_view`, `inspector`, `toolbar`,
//! `export_modal`, `annotations`, `badges`, `tokens`); this file paints and dispatches.

use crate::annotations::{self, Annotation, AnnotationKind};
use crate::badges::{badges_for, paint_badges, Badge, BADGE_SLOT};
use crate::char_counter::{self, CounterUi, FrequencyKind};
use crate::container_view::{aggregate_word_count, flow_items, FlowItem};
use crate::editor::{self, EditorStyle};
use crate::editor_keys;
use crate::editor_state::EditorState;
use crate::export_modal::{ExportModal, PreviewBody, PreviewTheme, QuickExportError};
use crate::export_ui::{ExportFormat, PageSize};
use crate::filters_ui::FilterBuilder;
use crate::goal_ui::GoalSetupFlow;
use crate::icons;
use crate::inspector::{self, InspectorTab, MediaEntry};
use crate::keyword_manager;
use crate::language::Language;
use crate::layout::{Panel, PanelLayout, INSPECTOR_WIDTH, LIBRARY_WIDTH, SHEET_LIST_WIDTH};
use crate::os::{NativeOs, Os};
use crate::preferences::Preferences;
use crate::search_ui::SearchUiState;
use crate::selection::{self, DraggedNodes, DropZone};
use crate::sheet_list::{self, RunPosition, SheetRow};
use crate::sidebar::{self, ContextAction, SidebarRow, SidebarSelection, SmartFilter, UiIntent};
use crate::state::{AppState, NodeMove, NodePath, StateError};
use crate::style_picker::StylePicker;
use crate::tokens::{self, TypeToken};
use crate::toolbar::{self, ParagraphStyle, ToolbarAction};
use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat, TextWrapping};
use egui::{Align, Align2, Color32, Id, Layout, Margin, Rect, RichText, Sense, Ui};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// How long the writer must stop typing before the library saves itself.
const AUTOSAVE_IDLE: Duration = Duration::from_secs(60);
/// How long "Saved." / "Auto saved." stays in the status bar before it goes idle.
const SAVE_NOTICE: Duration = Duration::from_secs(10);
const IDLE_STATUS: &str = "Idle";
const UNSAVED_STATUS: &str = "Changes unsaved";
use parrot_core::{Node, Sheet};

/// Installs fonts and the token-based visuals. Call from the eframe creation context so both
/// are in place before the first frame (a `FontFamily::Name` used before `set_fonts` takes
/// effect would panic).
pub fn setup(ctx: &egui::Context) {
    install_fonts(ctx);
    tokens::apply_visuals(ctx);
}

/// Loads the Windows system UI faces (Segoe UI regular/bold, Consolas) when present — read
/// from the local Fonts folder, nothing bundled — and registers [`tokens::FAMILY_BOLD`]. Where
/// a face is missing (e.g. a Linux dev build) egui's own fonts stand in.
fn install_fonts(ctx: &egui::Context) {
    fn load(
        fonts: &mut egui::FontDefinitions,
        dir: &std::path::Path,
        name: &str,
        file: &str,
    ) -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => {
                fonts
                    .font_data
                    .insert(name.to_string(), egui::FontData::from_owned(bytes));
                true
            }
            Err(_) => false,
        }
    }
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    let regular = load(&mut fonts, &dir, "system-regular", "segoeui.ttf");
    let bold = load(&mut fonts, &dir, "system-bold", "segoeuib.ttf");
    let mono = load(&mut fonts, &dir, "system-mono", "consola.ttf");

    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    if regular {
        proportional.insert(0, "system-regular".to_string());
    }
    // Phosphor's glyphs sit in the Private Use Area, so as a fallback it never shadows a text
    // face; the bold family below clones this list and so inherits the icons too.
    fonts.font_data.insert(
        "phosphor".to_string(),
        egui_phosphor::Variant::Regular.font_data(),
    );
    proportional.push("phosphor".to_string());
    let mut bold_family = proportional.clone();
    if bold {
        bold_family.insert(0, "system-bold".to_string());
    }
    fonts.families.insert(
        egui::FontFamily::Name(tokens::FAMILY_BOLD.into()),
        bold_family,
    );
    if mono {
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "system-mono".to_string());
    }
    ctx.set_fonts(fonts);
}

/// The `TextEdit` id for a sheet's editor — shared by single-sheet and sheet-flow mode, so the
/// toolbar can find the active editor's cursor either way.
/// Export dialog geometry (pending the design-token pass, docs/design_tokens_proposal.md).
const EXPORT_OPTIONS_WIDTH: f32 = 220.0;
const EXPORT_PREVIEW_WIDTH: f32 = 360.0;
const EXPORT_PREVIEW_HEIGHT: f32 = 460.0;
const PT_PER_MM: f32 = 72.0 / 25.4;
/// Where the editor bar and its formatting buttons were last laid out (read by tests).
const FORMAT_BAR_LAYOUT: &str = "format_bar_layout";

fn editor_id(path: &NodePath) -> Id {
    Id::new(("sheet-editor", path.clone()))
}

fn panel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .inner_margin(Margin::symmetric(
            tokens::PANEL_PADDING_X,
            tokens::PANEL_PADDING_Y,
        ))
}

fn text(s: impl Into<String>, token: TypeToken, color: Color32) -> RichText {
    RichText::new(s).font(token.font_id()).color(color)
}

/// A one-row galley truncated with an ellipsis at `max_width` (§3.2 labels, §4.2 titles).
fn truncated(
    ui: &Ui,
    s: &str,
    token: TypeToken,
    color: Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(
        s.to_string(),
        TextFormat {
            font_id: token.font_id(),
            color,
            ..Default::default()
        },
    );
    job.wrap = TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

/// How a library row is marked: the selection itself, or the container holding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Highlight {
    None,
    Selected,
    Container,
}

/// The inline rename field (`id` must differ between panels showing the same node, or both
/// fields would take every keypress): focuses itself once when the rename starts, then reports
/// `(commit, cancel)` — Enter or clicking away commits, Esc cancels.
fn rename_field(
    ui: &mut Ui,
    buffer: &mut String,
    id: Id,
    width: f32,
    needs_focus: &mut bool,
) -> (bool, bool) {
    let response = ui.add(
        egui::TextEdit::singleline(buffer)
            .id(id)
            .desired_width(width),
    );
    // Focus once, when renaming starts: re-requesting it every frame would undo
    // Enter/click-away (so the name never committed) and steal focus from every other
    // text field while a rename was pending.
    if std::mem::take(needs_focus) {
        response.request_focus();
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        (false, true)
    } else {
        (response.lost_focus(), false)
    }
}

/// One §3.2 tree row: selection/hover fill, optional disclosure triangle, icon, truncated
/// label, right-aligned badges. Returns the row response and whether the triangle was clicked.
fn tree_row(
    ui: &mut Ui,
    depth: usize,
    icon: &str,
    label: &str,
    disclosure: Option<bool>,
    highlight: Highlight,
    badges: &[Badge],
) -> (egui::Response, bool) {
    let selected = highlight == Highlight::Selected;
    // Draggable so library nodes can be dropped elsewhere; a plain click still registers.
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), tokens::TREE_ROW_HEIGHT),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let fill = rect.shrink2(egui::vec2(2.0, 1.0));
    let fill_color = match highlight {
        Highlight::Selected => Some(tokens::ACCENT),
        Highlight::Container => Some(tokens::ACCENT_MUTED),
        Highlight::None if response.hovered() => Some(tokens::BG_ELEVATED),
        Highlight::None => None,
    };
    if let Some(color) = fill_color {
        painter.rect_filled(fill, tokens::SELECTION_RADIUS, color);
    }
    let color = if highlight != Highlight::None {
        tokens::TEXT_PRIMARY
    } else {
        tokens::TEXT_SECONDARY
    };
    let cy = rect.center().y;
    let mut x = rect.left() + tokens::GRID + depth as f32 * tokens::INDENT;
    let mut disclosure_clicked = false;
    if let Some(expanded) = disclosure {
        let tri = Rect::from_center_size(egui::pos2(x + 4.0, cy), egui::vec2(12.0, 12.0));
        disclosure_clicked = ui
            .interact(tri, response.id.with("disclosure"), Sense::click())
            .clicked();
        painter.text(
            tri.center(),
            Align2::CENTER_CENTER,
            if expanded {
                icons::EXPANDED
            } else {
                icons::COLLAPSED
            },
            tokens::UI_CAPTION.font_id(),
            if selected {
                color
            } else {
                tokens::TEXT_TERTIARY
            },
        );
    }
    x += 3.0 * tokens::GRID;
    painter.text(
        egui::pos2(x + 6.0, cy),
        Align2::CENTER_CENTER,
        icon,
        tokens::UI_CAPTION.font_id(),
        color,
    );
    x += 4.0 * tokens::GRID + 2.0;
    let right = rect.right() - tokens::GRID;
    let label_width = (right - badges.len() as f32 * BADGE_SLOT - x).max(0.0);
    let galley = truncated(ui, label, tokens::UI_LABEL, color, label_width);
    painter.galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, color);
    paint_badges(&painter, right, cy, badges);
    (response, disclosure_clicked)
}

/// A glued-sheet card's corners for a row at `run`: only the card's outer corners round.
fn run_rounding(run: RunPosition) -> egui::Rounding {
    let r = tokens::SELECTION_RADIUS;
    let (top, bottom) = match run {
        RunPosition::Alone => (r, r),
        RunPosition::First => (r, 0.0),
        RunPosition::Middle => (0.0, 0.0),
        RunPosition::Last => (0.0, r),
    };
    egui::Rounding {
        nw: top,
        ne: top,
        sw: bottom,
        se: bottom,
    }
}

/// One §4.2 sheet-list row: title, 2-line preview, badges bottom-right. Glued sheets share a
/// card; `in_open_run` marks the other sheets of the open sheet's run; `indent` nests the row
/// under its section when a whole project is listed.
fn sheet_row(
    ui: &mut Ui,
    row: &SheetRow,
    selected: bool,
    in_open_run: bool,
    indent: f32,
    renaming: bool,
) -> egui::Response {
    let (full, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), tokens::SHEET_ROW_HEIGHT),
        Sense::click_and_drag(),
    );
    let rect = Rect::from_min_max(full.min + egui::vec2(indent, 0.0), full.max);
    let painter = ui.painter_at(rect);
    let fill = if selected {
        Some(tokens::ACCENT)
    } else if in_open_run {
        Some(tokens::ACCENT_MUTED)
    } else if response.hovered() {
        Some(tokens::BG_ELEVATED)
    } else if row.run != RunPosition::Alone {
        Some(tokens::BG_MID)
    } else {
        None
    };
    if let Some(fill) = fill {
        painter.rect_filled(rect, run_rounding(row.run), fill);
    }
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(1.0_f32, tokens::DIVIDER),
    );
    let pad = 2.0 * tokens::GRID;
    let inner = rect.shrink(pad);
    let title_color = if selected || in_open_run {
        tokens::TEXT_PRIMARY
    } else {
        tokens::TEXT_SECONDARY
    };
    let title = truncated(
        ui,
        &row.title,
        tokens::UI_LABEL_STRONG,
        title_color,
        inner.width(),
    );
    let title_height = title.size().y;
    if !renaming {
        painter.galley(inner.left_top(), title, title_color);
    }
    let preview = ui.fonts(|f| {
        f.layout_job(sheet_list::preview_layout_job(
            &row.preview,
            inner.width(),
            selected,
        ))
    });
    painter.galley(
        inner.left_top() + egui::vec2(0.0, title_height + tokens::GRID),
        preview,
        tokens::TEXT_TERTIARY,
    );
    paint_badges(&painter, inner.right(), inner.bottom() - 6.0, &row.badges);
    response
}

/// A row painted this frame that a dragged node can be dropped on.
#[derive(Debug, Clone)]
struct DropRow {
    path: NodePath,
    rect: Rect,
    /// Groups/projects take drops into themselves as well as before/after.
    container: bool,
    child_count: usize,
}

/// A Glue/Unglue Sheets menu choice and the sheets it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GlueAction {
    Glue(Vec<NodePath>),
    Unglue(Vec<NodePath>),
}

/// Which editor buffer a sheet path's edits live in.
enum BufferSlot {
    Single,
    Flow,
}

pub struct ParrotApp {
    state: AppState,
    fonts_ready: bool,
    layout: PanelLayout,
    sidebar: Option<SidebarSelection>,
    /// The sheet open in single-sheet mode (§5.1); `None` shows the sheet flow (§5.2).
    selected: Option<NodePath>,
    /// Sheet-list selection in list order: `[selected]` for one sheet; two or more show
    /// one after another in the editor and are what Export exports.
    sheet_selection: Vec<NodePath>,
    /// Where Shift-click ranges start, in the sheet list and the library tree.
    sheet_anchor: Option<NodePath>,
    tree_anchor: Option<NodePath>,
    /// A sheet the library tree scrolls to on its next paint (it was just selected).
    reveal_in_tree: Option<NodePath>,
    /// Drop targets painted this frame (tree and sheet-list rows), for drag and drop.
    drop_rows: Vec<DropRow>,
    editor_buffer: String,
    /// Per-sheet buffers for the sheet-flow view, keyed by sheet path.
    container_buffers: BTreeMap<NodePath, String>,
    /// The sheet whose editor last had keyboard focus — the toolbar's formatting target.
    active_editor: Option<NodePath>,
    show_markdown: bool,
    focus_mode: bool,
    text_checking: bool,
    /// Proofing cards the user dismissed this session: (sheet, finding fingerprint).
    dismissed_findings: BTreeSet<(NodePath, String)>,
    /// The expanded proofing card: (sheet, finding start byte).
    proofing_selected: Option<(NodePath, usize)>,
    proofing_filter: Option<AnnotationKind>,
    inspector_tab: InspectorTab,
    collapsed: BTreeSet<NodePath>,
    rename: Option<(NodePath, String)>,
    /// Set when a rename starts; the field takes keyboard focus once, then lets it go.
    rename_needs_focus: bool,
    /// When the writer last edited a sheet's text, while those edits are still unsaved.
    unsaved_since_edit: Option<std::time::Instant>,
    /// Ctrl+A in the sheet flow selected every sheet's text (one editor each, so egui itself
    /// can only select within one): Ctrl+C then copies them all.
    flow_all_selected: bool,
    /// When the system's wheel-scroll setting was last read (it can change while we run).
    scroll_speed_read_at: Option<std::time::Instant>,
    /// Text to put on the clipboard at the next frame (intents have no egui context).
    pending_clipboard: Option<String>,
    /// The library as last saved/opened (`None` = take a snapshot next frame); the status bar
    /// shows "Changes unsaved" while the library differs from it.
    saved_snapshot: Option<parrot_core::Library>,
    unsaved_changes: bool,
    /// When the status last turned to "Saved." / "Auto saved."; it falls back to idle later.
    save_notice_at: Option<std::time::Instant>,
    pending_jump: Option<(NodePath, usize)>,
    link_url: Option<String>,
    show_paragraph_picker: bool,
    library_path_input: String,
    show_library_dialog: bool,
    status: String,
    style_picker: StylePicker,
    search: SearchUiState,
    filter_builder: FilterBuilder,
    filter_parent: Option<NodePath>,
    confirm_empty_trash: bool,
    preferences: Preferences,
    export: ExportModal,
    goal_edit_target: Option<NodePath>,
    goal_flow: GoalSetupFlow,
    show_keyword_manager: bool,
    keyword_rename_target: Option<String>,
    keyword_rename_buffer: String,
    keyword_new: String,
    info_target: Option<NodePath>,
    attachment_note_text: String,
    today: String,
    /// File dialogs and shell launches; a `FakeOs` in tests.
    os: Box<dyn Os>,
    /// The most recent export's file, for "Show in Folder".
    last_export_path: Option<PathBuf>,
    /// Where the last opened library is remembered for the next launch; `None` in tests.
    recent_library_file: Option<PathBuf>,
    /// Where the last iCloud library root is remembered (macOS only); `None` in tests.
    #[allow(dead_code)]
    recent_icloud_file: Option<PathBuf>,
    counter: CounterUi,
}

impl Default for ParrotApp {
    fn default() -> Self {
        Self {
            state: AppState::new(),
            fonts_ready: false,
            layout: PanelLayout::default(),
            sidebar: None,
            selected: None,
            sheet_selection: Vec::new(),
            sheet_anchor: None,
            tree_anchor: None,
            reveal_in_tree: None,
            drop_rows: Vec::new(),
            editor_buffer: String::new(),
            container_buffers: BTreeMap::new(),
            active_editor: None,
            show_markdown: false,
            focus_mode: false,
            text_checking: true,
            dismissed_findings: BTreeSet::new(),
            proofing_selected: None,
            proofing_filter: None,
            inspector_tab: InspectorTab::Statistics,
            collapsed: BTreeSet::new(),
            rename: None,
            rename_needs_focus: false,
            unsaved_since_edit: None,
            pending_clipboard: None,
            scroll_speed_read_at: None,
            flow_all_selected: false,
            saved_snapshot: None,
            unsaved_changes: false,
            save_notice_at: None,
            pending_jump: None,
            link_url: None,
            show_paragraph_picker: false,
            library_path_input: String::new(),
            show_library_dialog: false,
            status: "No library open.".to_string(),
            style_picker: StylePicker::new(),
            search: SearchUiState::default(),
            filter_builder: FilterBuilder::default(),
            filter_parent: None,
            confirm_empty_trash: false,
            preferences: Preferences::default(),
            export: ExportModal::default(),
            goal_edit_target: None,
            goal_flow: GoalSetupFlow::default(),
            show_keyword_manager: false,
            keyword_rename_target: None,
            keyword_rename_buffer: String::new(),
            keyword_new: String::new(),
            info_target: None,
            attachment_note_text: String::new(),
            today: sidebar::today_iso(),
            os: Box::new(NativeOs),
            last_export_path: None,
            recent_library_file: None,
            recent_icloud_file: None,
            counter: CounterUi::default(),
        }
    }
}

impl ParrotApp {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the app inside eframe's creation context, installing fonts/visuals up front,
    /// and reopens the last library.
    pub fn with_context(ctx: &egui::Context) -> Self {
        setup(ctx);
        let mut app = Self {
            fonts_ready: true,
            recent_library_file: crate::recent_library::default_file(),
            recent_icloud_file: crate::recent_library::default_icloud_file(),
            ..Self::default()
        };
        app.init_icloud_path();
        app.reopen_last_library();
        app
    }

    /// Points `state.icloud_path` at the remembered iCloud root, else the
    /// default `~/.../CloudDocs/Parrot` when iCloud Drive is available.
    /// Non-macOS builds leave it `None` so all iCloud UI stays hidden.
    fn init_icloud_path(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let remembered = self
                .recent_icloud_file
                .as_deref()
                .and_then(crate::recent_library::recall_icloud);
            let resolved = remembered.or_else(crate::icloud::default_icloud_library_dir);
            self.state.icloud_path = resolved;
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.state.icloud_path = None;
        }
    }

    /// Opens the library open when the app last ran, if it's still there.
    fn reopen_last_library(&mut self) {
        // On macOS the remembered iCloud root wins when the local recall is
        // gone: the iCloud store is the library there.
        #[cfg(target_os = "macos")]
        {
            if self
                .recent_library_file
                .as_deref()
                .and_then(crate::recent_library::recall)
                .is_none()
            {
                if let Some(icloud) = self
                    .recent_icloud_file
                    .as_deref()
                    .and_then(crate::recent_library::recall_icloud)
                {
                    if parrot_container::tree::has_library_entries(&icloud) {
                        self.library_path_input = icloud.display().to_string();
                        self.open_library();
                        return;
                    }
                }
            }
        }
        let last = self
            .recent_library_file
            .as_deref()
            .and_then(crate::recent_library::recall);
        if let Some(path) = last {
            self.library_path_input = path.display().to_string();
            self.open_library();
        }
    }

    /// Remembers the current library to reopen next launch.
    fn remember_library(&mut self) {
        if let (Some(file), Some(library)) = (&self.recent_library_file, &self.state.library_path) {
            // Best effort: failing to remember shouldn't fail the open itself.
            let _ = crate::recent_library::remember(file, library);
        }
        #[cfg(target_os = "macos")]
        if let (Some(file), Some(icloud)) = (&self.recent_icloud_file, &self.state.icloud_path) {
            let _ = crate::recent_library::remember(file, icloud);
        }
    }

    // --- Library, preferences -----------------------------------------------------------

    fn preferences_path(&self) -> Option<PathBuf> {
        self.state
            .library_path
            .as_ref()
            .map(|p| p.join(".parrot-preferences.toml"))
    }

    /// Loads the library's preferences and applies its default style (T-7-04).
    fn load_preferences(&mut self) {
        if let Some(path) = self.preferences_path() {
            let legacy = path.with_file_name(".ulysses-preferences.toml");
            let loaded = Preferences::load(&path).or_else(|e| {
                if legacy.exists() {
                    Preferences::load(&legacy)
                } else {
                    Err(e)
                }
            });
            if let Ok(prefs) = loaded {
                self.preferences = prefs;
                self.preferences.apply_to(&mut self.style_picker);
            }
        }
    }

    fn save_preferences(&mut self) {
        self.preferences.default_style_name = self.style_picker.selected_name.clone();
        self.preferences.export_style_name = self.export.style_name.clone();
        self.preferences.export_css_path = self.export.custom_css_path.clone();
        if let Some(path) = self.preferences_path() {
            self.status = match self.preferences.save(&path) {
                Ok(()) => "Preferences saved.".to_string(),
                Err(e) => format!("Error saving preferences: {e}"),
            };
        }
    }

    fn create_library(&mut self) {
        let path = PathBuf::from(self.library_path_input.trim());
        // Saving an empty tree here would set the existing sheets aside as stale.
        if parrot_container::tree::has_library_entries(&path) {
            self.status = format!(
                "Error: {} already holds a library — use Open Library.",
                path.display()
            );
            return;
        }
        self.state.create_library(&path);
        self.saved_snapshot = None;
        self.status = match self.state.save() {
            Ok(()) => {
                self.remember_library();
                format!("Created library at {}", path.display())
            }
            Err(e) => format!("Error creating library: {e}"),
        };
        self.show_shelf();
        self.load_preferences();
    }

    fn open_library(&mut self) {
        let path = PathBuf::from(self.library_path_input.trim());
        self.saved_snapshot = None;
        self.status = match self.state.open_library(&path) {
            Ok(adopted) => {
                self.remember_library();
                if adopted {
                    format!(
                        "Opened library at {}. Sheets outside any project were moved into the \"{}\" project.",
                        path.display(),
                        crate::state::UNFILED_PROJECT_TITLE
                    )
                } else {
                    format!("Opened library at {}", path.display())
                }
            }
            Err(e) => format!("Error opening library: {e}"),
        };
        self.show_shelf();
        self.load_preferences();
    }

    /// The Library shelf: the first page, with every project as a card.
    fn show_shelf(&mut self) {
        self.reset_selection();
        self.sidebar = None;
    }

    fn on_shelf(&self) -> bool {
        self.sidebar.is_none() && !crate::first_run::should_prompt(&self.state)
    }

    /// Goes into project `path` from the shelf: lists it and expands it in the tree.
    fn open_project(&mut self, path: NodePath) {
        self.collapsed.remove(&path);
        self.reveal(&path);
        self.select_sidebar(SidebarSelection::Node(path));
    }

    /// Creates a project, opens it and starts renaming it.
    fn new_project(&mut self) {
        self.new_project_in_section(parrot_core::Section::Local);
    }

    /// Creates a project in `section`, opens it and starts renaming it.
    /// On non-macOS builds `ICloud` falls back to `Local` (iCloud UI hidden).
    fn new_project_in_section(&mut self, section: parrot_core::Section) {
        #[cfg(not(target_os = "macos"))]
        let section = {
            let _ = section;
            parrot_core::Section::Local
        };
        let path = self
            .state
            .create_project_in_section(sidebar::NEW_PROJECT_TITLE, section);
        // Persist immediately so a new iCloud project creates the iCloud store.
        let _ = self.state.save();
        self.open_project(path.clone());
        self.handle_intent(UiIntent::BeginRename(path));
    }

    /// The project the writer is working in: the one holding the open sheet or the selected
    /// library node.
    fn current_project(&self) -> Option<NodePath> {
        let path = self.selected.clone().or_else(|| match &self.sidebar {
            Some(SidebarSelection::Node(p)) => Some(p.clone()),
            Some(SidebarSelection::Nodes(v)) => v.first().cloned(),
            _ => None,
        })?;
        self.state.project_of(&path)
    }

    fn save(&mut self) {
        self.save_as(false);
    }

    fn save_as(&mut self, auto: bool) {
        self.unsaved_since_edit = None;
        self.saved_snapshot = None;
        self.unsaved_changes = false;
        self.save_notice_at = None;
        self.status = match self.state.save() {
            Ok(()) => {
                self.save_notice_at = Some(std::time::Instant::now());
                if auto { "Auto saved." } else { "Saved." }.to_string()
            }
            Err(StateError::NoLibraryPath) => {
                "Nothing to save — create a library first.".to_string()
            }
            Err(e) => format!("Error saving: {e}"),
        };
    }

    // --- Selection ------------------------------------------------------------------------

    /// Clears everything path-based. Called after any tree mutation that can shift sibling
    /// indices (paths are positional), so a stale path never silently points at another node.
    fn reset_selection(&mut self) {
        self.selected = None;
        self.sheet_selection.clear();
        self.sheet_anchor = None;
        self.tree_anchor = None;
        self.reveal_in_tree = None;
        self.active_editor = None;
        self.container_buffers.clear();
        self.rename = None;
        self.pending_jump = None;
        self.collapsed.clear();
        self.info_target = None;
        self.goal_edit_target = None;
        self.export.close();
        if matches!(
            self.sidebar,
            Some(SidebarSelection::Node(_) | SidebarSelection::Nodes(_))
        ) {
            self.sidebar = None;
        }
    }

    fn select_sidebar(&mut self, selection: SidebarSelection) {
        self.container_buffers.clear();
        self.selected = None;
        self.sheet_selection.clear();
        self.sheet_anchor = None;
        self.active_editor = None;
        if let SidebarSelection::Node(path) = &selection {
            if matches!(self.state.node(path), Ok(Node::Sheet(_))) {
                // A sheet inside a group lists its group, with the sheet selected in it.
                let path = path.clone();
                self.sidebar = Some(match self.sheet_container(&path) {
                    Some(parent) => SidebarSelection::Node(parent),
                    None => selection,
                });
                self.open_sheet(path);
                return;
            }
        }
        self.sidebar = Some(selection);
    }

    /// The group or project directly holding sheet `path`, if any.
    fn sheet_container(&self, path: &NodePath) -> Option<NodePath> {
        let parent = path.split_last()?.1;
        matches!(
            self.state.node(parent),
            Ok(Node::Group(_) | Node::Project(_))
        )
        .then(|| parent.to_vec())
    }

    /// The library nodes selected in the tree: the selected sheets when the sidebar lists
    /// their container, else the sidebar's nodes.
    fn tree_selection(&self) -> Vec<NodePath> {
        let nodes = self
            .sidebar
            .as_ref()
            .map(SidebarSelection::nodes)
            .unwrap_or_default();
        let listed_in_container = !self.sheet_selection.is_empty()
            && self
                .sheet_selection
                .iter()
                .all(|p| self.sheet_container(p).is_some_and(|c| nodes == [c]));
        if listed_in_container {
            self.sheet_selection.clone()
        } else {
            nodes
        }
    }

    /// Opens `path` in single-sheet mode (§5.1), or, for a glued sheet, its whole run in the
    /// sheet flow with `path` focused.
    fn open_sheet(&mut self, path: NodePath) {
        let run = self.state.glue_run(&path);
        if run.len() > 1 {
            self.clear_sheet_selection();
            for p in &run {
                self.reveal(p);
            }
            self.reveal_in_tree = Some(path.clone());
            self.pending_jump = Some((path.clone(), 0));
            self.sheet_anchor = Some(path.clone());
            self.active_editor = Some(path);
            self.sheet_selection = run;
            return;
        }
        if let Ok(Node::Sheet(sheet)) = self.state.node(&path) {
            self.editor_buffer = editor::content_to_buffer(&sheet.content);
            self.container_buffers.clear();
            self.active_editor = Some(path.clone());
            self.sheet_selection = vec![path.clone()];
            self.sheet_anchor = Some(path.clone());
            self.reveal(&path);
            self.selected = Some(path);
        }
    }

    /// Shows `path` in the library tree: expands every group above it and scrolls to it.
    fn reveal(&mut self, path: &NodePath) {
        for depth in 1..path.len() {
            self.collapsed.remove(&path[..depth]);
        }
        self.reveal_in_tree = Some(path.clone());
    }

    /// The selected sheets are marked as the selection in the tree too; the sidebar node
    /// listing them then shows as their container.
    fn tree_highlight(&self, path: &NodePath) -> Highlight {
        if self.sheet_selection.contains(path) {
            return Highlight::Selected;
        }
        let in_sidebar = self
            .sidebar
            .as_ref()
            .is_some_and(|s| s.nodes().contains(path));
        match (in_sidebar, self.sheet_selection.is_empty()) {
            (false, _) => Highlight::None,
            (true, true) => Highlight::Selected,
            (true, false) => Highlight::Container,
        }
    }

    /// Back from one or several selected sheets to the container's sheet flow.
    fn clear_sheet_selection(&mut self) {
        self.selected = None;
        self.sheet_selection.clear();
        self.container_buffers.clear();
    }

    /// A click on library node `path` with `modifiers` (plain, Ctrl, Shift, Ctrl+Shift).
    fn click_tree_node(&mut self, path: &NodePath, modifiers: egui::Modifiers) {
        let order = sidebar::visible_tree_paths(&self.state.library, &self.collapsed);
        let current = self.tree_selection();
        let chosen = selection::click(&current, self.tree_anchor.as_ref(), &order, path, modifiers);
        let anchor = selection::next_anchor(self.tree_anchor.as_ref(), &order, path, modifiers);
        match SidebarSelection::of_nodes(chosen) {
            Some(sel) => self.select_sidebar(sel),
            None => {
                self.clear_sheet_selection();
                self.active_editor = None;
                self.sidebar = None;
            }
        }
        self.tree_anchor = Some(anchor);
    }

    /// A click on sheet-list row `path`, `order` being the list's rows.
    fn click_sheet_row(&mut self, path: &NodePath, order: &[NodePath], modifiers: egui::Modifiers) {
        let chosen = selection::click(
            &self.sheet_selection,
            self.sheet_anchor.as_ref(),
            order,
            path,
            modifiers,
        );
        let anchor = selection::next_anchor(self.sheet_anchor.as_ref(), order, path, modifiers);
        match chosen.as_slice() {
            [one] => self.open_sheet(one.clone()),
            _ => {
                self.clear_sheet_selection();
                self.active_editor = None;
                for path in &chosen {
                    self.reveal(path);
                }
                self.reveal_in_tree = chosen.first().cloned();
                self.sheet_selection = chosen;
            }
        }
        self.sheet_anchor = Some(anchor);
    }

    /// Whether `path` belongs to a glued run that's open in the editor.
    fn open_glue_run(&self, path: &NodePath) -> bool {
        self.sheet_selection.len() > 1
            && self.sheet_selection.contains(path)
            && self.state.glue_run(path) == self.sheet_selection
    }

    /// Glue/Unglue Sheets for the selected sheets — or, from a row's context menu, for that
    /// row (the selection, when it's part of it). Returns what was clicked.
    fn glue_menu_items(&self, ui: &mut Ui, row: Option<&NodePath>) -> Option<GlueAction> {
        let targets = match row {
            Some(p) if !self.sheet_selection.contains(p) => vec![p.clone()],
            _ => self.sheet_selection.clone(),
        };
        let mut runs: Vec<Vec<NodePath>> = Vec::new();
        for p in &targets {
            let run = self.state.glue_run(p);
            if !run.is_empty() && !runs.contains(&run) {
                runs.push(run);
            }
        }
        let can_glue = runs.len() > 1;
        let can_unglue = runs.iter().any(|r| r.len() > 1);
        let mut chosen = None;
        if ui
            .add_enabled(can_glue, egui::Button::new("Glue Sheets"))
            .clicked()
        {
            chosen = Some(GlueAction::Glue(targets.clone()));
            ui.close_menu();
        }
        if ui
            .add_enabled(can_unglue, egui::Button::new("Unglue Sheets"))
            .clicked()
        {
            chosen = Some(GlueAction::Unglue(targets));
            ui.close_menu();
        }
        chosen
    }

    fn run_glue(&mut self, action: GlueAction) {
        match action {
            GlueAction::Glue(paths) => match self.state.glue_sheets(&paths) {
                Ok(mv) => {
                    self.follow_move(&mv);
                    let glued = mv.new_paths();
                    self.status = format!("Glued {} sheets.", glued.len());
                    let focus = self.active_editor.clone().filter(|p| glued.contains(p));
                    self.open_sheet(focus.unwrap_or_else(|| glued[0].clone()));
                }
                Err(_) => self.status = "Select sheets from two or more places to glue.".into(),
            },
            GlueAction::Unglue(paths) => {
                let focus = self
                    .active_editor
                    .clone()
                    .or_else(|| paths.first().cloned());
                self.state.unglue_sheets(&paths);
                self.status = "Unglued.".to_string();
                if let Some(p) = focus {
                    self.open_sheet(p);
                }
            }
        }
    }

    fn sheet(&self, path: &NodePath) -> Option<&Sheet> {
        match self.state.node(path) {
            Ok(Node::Sheet(s)) => Some(s),
            _ => None,
        }
    }

    /// Sheets the editor currently shows: the open sheet, or the flow's sheets.
    fn visible_sheet_paths(&self) -> Vec<NodePath> {
        if self.sheet_selection.len() > 1 {
            return self.sheet_selection.clone();
        }
        if let Some(path) = &self.selected {
            return vec![path.clone()];
        }
        match &self.sidebar {
            Some(SidebarSelection::Smart(SmartFilter::Trash)) | None => Vec::new(),
            Some(selection) => sheet_list::listed_paths(&self.state, selection, &self.today),
        }
    }

    /// The sheet export/keyword/attachment actions target: the open sheet, else the flow's
    /// focused sheet.
    fn target_sheet(&self) -> Option<NodePath> {
        self.selected
            .clone()
            .or_else(|| self.active_editor.clone())
            .filter(|p| self.sheet(p).is_some())
    }

    // --- Editing --------------------------------------------------------------------------

    fn buffer_slot(&self, path: &NodePath) -> Option<BufferSlot> {
        if self.selected.as_ref() == Some(path) {
            Some(BufferSlot::Single)
        } else if self.container_buffers.contains_key(path) {
            Some(BufferSlot::Flow)
        } else {
            None
        }
    }

    fn commit_buffer(&mut self, path: &NodePath) {
        let text = match self.buffer_slot(path) {
            Some(BufferSlot::Single) => &self.editor_buffer,
            Some(BufferSlot::Flow) => &self.container_buffers[path],
            None => return,
        };
        let content = editor::buffer_to_content(text);
        let _ = self.state.set_sheet_content(path, content);
        self.unsaved_since_edit = Some(std::time::Instant::now());
    }

    /// Makes a wheel notch scroll as far as the system's setting says (read again every few
    /// seconds, so changing it in Settings applies without a restart).
    fn follow_system_scroll_speed(&mut self, ctx: &egui::Context) {
        const REREAD: Duration = Duration::from_secs(3);
        if self
            .scroll_speed_read_at
            .is_some_and(|t| t.elapsed() < REREAD)
        {
            return;
        }
        self.scroll_speed_read_at = Some(std::time::Instant::now());
        if let Some(points) = crate::scroll_speed::system_points_per_notch() {
            if ctx.options(|o| o.line_scroll_speed) != points {
                ctx.options_mut(|o| o.line_scroll_speed = points);
            }
        }
    }

    /// Notices edits (to text or structure) by comparing with the last saved library, and
    /// retires the "Saved" status after [`SAVE_NOTICE`].
    fn track_changes(&mut self, ctx: &egui::Context) {
        if self.state.library_path.is_some() {
            match &self.saved_snapshot {
                None => {
                    self.saved_snapshot = Some(self.state.library.clone());
                    self.unsaved_changes = false;
                }
                Some(saved) => {
                    self.unsaved_changes = *saved != self.state.library;
                    if self.unsaved_changes && self.unsaved_since_edit.is_none() {
                        self.unsaved_since_edit = Some(std::time::Instant::now());
                    }
                }
            }
        }
        if let Some(at) = self.save_notice_at {
            if at.elapsed() >= SAVE_NOTICE {
                self.save_notice_at = None;
                self.status = IDLE_STATUS.to_string();
            } else {
                ctx.request_repaint_after(SAVE_NOTICE.saturating_sub(at.elapsed()));
            }
        }
    }

    /// Saves once the writer hasn't typed for [`AUTOSAVE_IDLE`]. Returns whether it saved.
    fn autosave_if_idle(&mut self, now: std::time::Instant) -> bool {
        let Some(last_edit) = self.unsaved_since_edit else {
            return false;
        };
        if now.saturating_duration_since(last_edit) < AUTOSAVE_IDLE {
            return false;
        }
        self.unsaved_since_edit = None;
        if self.state.library_path.is_none() {
            return false;
        }
        self.save_as(true);
        true
    }

    /// Runs a toolbar formatting edit on the active editor: its text and egui's stored
    /// selection go into an `EditorState`, `edit` runs, and the result (text + selection) is
    /// written back and committed.
    fn apply_format(&mut self, ctx: &egui::Context, edit: impl FnOnce(&mut EditorState)) {
        let Some(path) = self.active_editor.clone() else {
            self.status = "Click into a sheet first.".to_string();
            return;
        };
        let id = editor_id(&path);
        let buffer = match self.buffer_slot(&path) {
            Some(BufferSlot::Single) => &mut self.editor_buffer,
            Some(BufferSlot::Flow) => match self.container_buffers.get_mut(&path) {
                Some(b) => b,
                None => return,
            },
            None => return,
        };
        let mut editor = EditorState::new(buffer.clone());
        let mut te_state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        if let Some(range) = te_state.cursor.char_range() {
            editor.set_selection(range.secondary.index, range.primary.index);
        }
        edit(&mut editor);
        *buffer = editor.text().to_string();
        let sel = editor.selection();
        te_state.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(sel.head),
            secondary: CCursor::new(sel.anchor),
        }));
        te_state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        self.commit_buffer(&path);
    }

    // --- Actions --------------------------------------------------------------------------

    fn dispatch_toolbar(&mut self, ctx: &egui::Context, action: ToolbarAction) {
        match action {
            ToolbarAction::ToggleSidebar => self.layout.toggle_sidebar(),
            ToolbarAction::ToggleEditorOnly => self.layout.toggle_editor_only(),
            ToolbarAction::ToggleMarkup => self.show_markdown = !self.show_markdown,
            ToolbarAction::Bold => self.apply_format(ctx, toolbar::bold),
            ToolbarAction::Italic => self.apply_format(ctx, toolbar::italic),
            ToolbarAction::Link => self.link_url = Some("https://".to_string()),
            ToolbarAction::ParagraphStyle => self.show_paragraph_picker = true,
            ToolbarAction::Export => self.open_export(),
            ToolbarAction::QuickExport => self.quick_export(),
            ToolbarAction::ToggleInspector => self.layout.toggle_inspector(),
        }
    }

    fn toolbar_active(&self, action: ToolbarAction) -> bool {
        match action {
            ToolbarAction::ToggleSidebar => self.layout.is_visible(Panel::Library),
            ToolbarAction::ToggleEditorOnly => self.layout.editor_only,
            ToolbarAction::ToggleMarkup => self.show_markdown,
            ToolbarAction::ToggleInspector => self.layout.is_visible(Panel::Inspector),
            _ => false,
        }
    }

    /// What Export exports, in order, and the export's name: several selected sheets; else
    /// the open sheet; else everything in several selected library nodes; else the flow's
    /// focused sheet; else the sheets of the one selected group/project/filter.
    fn export_scope(&self) -> Option<(Vec<NodePath>, String)> {
        let paths = if self.sheet_selection.len() > 1 {
            self.sheet_selection.clone()
        } else if let Some(path) = &self.selected {
            vec![path.clone()]
        } else if let Some(SidebarSelection::Nodes(_)) = &self.sidebar {
            self.visible_sheet_paths()
        } else if let Some(path) = self.target_sheet() {
            vec![path]
        } else if let Some(SidebarSelection::Node(_)) = &self.sidebar {
            self.visible_sheet_paths()
        } else {
            Vec::new()
        };
        let paths: Vec<NodePath> = paths
            .into_iter()
            .filter(|p| self.sheet(p).is_some())
            .collect();
        if paths.is_empty() {
            return None;
        }
        // One sheet keeps its own title; several are named after what's selected.
        let names: Vec<String> = match &self.sidebar {
            Some(SidebarSelection::Nodes(nodes)) if paths.len() > 1 => nodes
                .iter()
                .map(|n| {
                    sheet_list::container_header(&self.state, &SidebarSelection::Node(n.clone()))
                })
                .collect(),
            Some(sel) if paths.len() > 1 => vec![sheet_list::container_header(&self.state, sel)],
            _ => vec![self
                .sheet(&paths[0])
                .map(|s| s.title.clone())
                .unwrap_or_default()],
        };
        let mut title = names
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" + ");
        if names.len() > 3 {
            title.push_str(" + …");
        }
        Some((paths, title))
    }

    /// The sheets at `paths` glued into one document.
    fn glued_content(&self, paths: &[NodePath]) -> parrot_core::SheetContent {
        let contents: Vec<&parrot_core::SheetContent> = paths
            .iter()
            .filter_map(|p| self.sheet(p).map(|s| &s.content))
            .collect();
        crate::export_ui::glue(&contents)
    }

    fn open_export(&mut self) {
        let Some((paths, title)) = self.export_scope() else {
            self.status = "Nothing to export — select a sheet first.".to_string();
            return;
        };
        let folder = self
            .state
            .library_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        // The export style is the dialog's own setting (persisted in preferences),
        // not the editor's ULSS style — the old coupling never changed any output.
        self.export.style_name = self.preferences.export_style_name.clone();
        self.export.custom_css_path = self.preferences.export_css_path.clone();
        self.export.open_for(paths, &title, folder.as_deref());
    }

    fn sheets_label(title: &str, count: usize) -> String {
        if count == 1 {
            format!("\"{title}\"")
        } else {
            format!("{count} sheets ({title})")
        }
    }

    fn run_export(&mut self) {
        let paths = self.export.targets.clone();
        if paths.is_empty() || paths.iter().any(|p| self.sheet(p).is_none()) {
            self.status = "Nothing to export — the sheet no longer exists.".to_string();
            self.export.close();
            return;
        }
        let content = self.glued_content(&paths);
        let what = Self::sheets_label(&self.export.title, paths.len());
        let (result, warning) = self.export.export(&content);
        self.status = match result {
            Ok(dest) => {
                let mut status = format!(
                    "Exported {what} as {} to {}",
                    self.export.format.label(),
                    dest.display()
                );
                if let Some(note) = warning {
                    status.push_str(&format!(" ({note})"));
                }
                self.last_export_path = Some(dest);
                status
            }
            Err(e) => format!("Error exporting: {e}"),
        };
    }

    fn quick_export(&mut self) {
        let Some((paths, title)) = self.export_scope() else {
            self.status = "Nothing to export — select a sheet first.".to_string();
            return;
        };
        let content = self.glued_content(&paths);
        match self.export.quick_export(&content, &title) {
            Ok(dest) => {
                self.status = format!(
                    "Quick-exported {} to {}",
                    Self::sheets_label(&title, paths.len()),
                    dest.display()
                );
                self.last_export_path = Some(dest);
            }
            Err(QuickExportError::NoPreviousExport) => self.open_export(),
            Err(e) => self.status = format!("Error exporting: {e}"),
        }
    }

    fn run_context_action(&mut self, path: &NodePath, action: ContextAction) {
        let structural = matches!(
            action,
            ContextAction::NewGroup
                | ContextAction::NewSheet
                | ContextAction::Duplicate
                | ContextAction::MoveToTrash
                | ContextAction::MoveToICloud
                | ContextAction::MoveToLocal
        );
        match sidebar::apply_context_action(&mut self.state, path, action) {
            Ok(intent) => {
                if structural {
                    self.reset_selection();
                    if action == ContextAction::MoveToTrash {
                        self.status = "Moved to Trash.".to_string();
                    }
                }
                self.handle_intent(intent);
                if matches!(action, ContextAction::NewGroup | ContextAction::NewSheet) {
                    let parent = sidebar::creation_parent(&self.state.library, path);
                    self.begin_rename_of_last_child(&parent);
                }
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    fn handle_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::None => {}
            UiIntent::BeginRename(path) => {
                let title = match self.state.node(&path) {
                    Ok(Node::Sheet(s)) => s.title.clone(),
                    Ok(Node::Group(g)) => g.title.clone(),
                    Ok(Node::Project(p)) => p.title.clone(),
                    Ok(Node::Filter(f)) => f.title.clone(),
                    Err(_) => return,
                };
                self.reveal(&path);
                self.rename = Some((path, title));
                self.rename_needs_focus = true;
            }
            UiIntent::OpenFilterBuilder => {
                self.filter_builder = FilterBuilder::default();
                self.filter_parent = Some(Vec::new());
            }
            UiIntent::OpenExport(path) => {
                self.open_sheet(path);
                self.open_export();
            }
            UiIntent::OpenGoal(path) => {
                let goal = match self.state.node(&path) {
                    Ok(Node::Sheet(s)) => s.goal.clone(),
                    Ok(Node::Group(g)) => g.goal.clone(),
                    Ok(Node::Project(p)) => p.goal.clone(),
                    _ => None,
                };
                self.goal_flow = goal
                    .as_ref()
                    .map(GoalSetupFlow::from_goal)
                    .unwrap_or_default();
                self.goal_edit_target = Some(path);
                self.show_inspector_tab(InspectorTab::Statistics);
            }
            UiIntent::OpenKeywords(path) => {
                self.open_sheet(path);
                self.show_inspector_tab(InspectorTab::Keywords);
            }
            UiIntent::OpenInfo(path) => self.info_target = Some(path),
            UiIntent::CopyFolderPath(path) => {
                // Pending renames only reach the disk on a save, so save first.
                if self.unsaved_changes {
                    self.save();
                }
                match self.state.node_folder(&path) {
                    Some(folder) => {
                        let folder = folder.display().to_string();
                        self.status = format!("Copied {folder}");
                        self.pending_clipboard = Some(folder);
                    }
                    None => {
                        self.status = "Error: save the library to give it a folder.".to_string();
                    }
                }
            }
        }
    }

    /// Applies the pending inline rename. A blank name keeps the old one.
    fn commit_rename(&mut self) {
        if let Some((path, title)) = self.rename.take() {
            let title = title.trim();
            if !title.is_empty() {
                if let Err(e) = self.state.rename_node(&path, title) {
                    self.status = format!("Error: {e}");
                }
            }
        }
    }

    fn show_inspector_tab(&mut self, tab: InspectorTab) {
        self.inspector_tab = tab;
        if !self.layout.is_visible(Panel::Inspector) {
            self.layout.toggle_inspector();
        }
    }

    /// Where new sheets and sections go: beside/inside the selected library node, else the
    /// current project. `None` when there's no project to put them in.
    fn creation_parent(&self) -> Option<NodePath> {
        match &self.sidebar {
            Some(SidebarSelection::Node(path)) if !path.is_empty() => {
                let parent = sidebar::creation_parent(&self.state.library, path);
                if parent.is_empty() {
                    self.current_project()
                } else {
                    Some(parent)
                }
            }
            _ => self.current_project(),
        }
    }

    fn new_root_node(&mut self, group: bool) {
        let Some(parent) = self.creation_parent() else {
            self.status = format!("Error: {}", StateError::OutsideProject);
            return;
        };
        let result = if group {
            self.state.create_group(&parent, sidebar::NEW_GROUP_TITLE)
        } else {
            self.state.create_sheet(&parent, sidebar::NEW_SHEET_TITLE)
        };
        match result {
            Ok(()) => self.begin_rename_of_last_child(&parent),
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    /// Starts renaming the node just appended to `parent`.
    fn begin_rename_of_last_child(&mut self, parent: &NodePath) {
        let count = match self.state.node(parent) {
            Ok(Node::Group(g)) => g.children.len(),
            Ok(Node::Project(p)) => p.children.len(),
            _ => return,
        };
        if count > 0 {
            let mut path = parent.clone();
            path.push(count - 1);
            self.handle_intent(UiIntent::BeginRename(path));
        }
    }

    // --- Drag and drop --------------------------------------------------------------------

    /// Runs after the panels: while library nodes are dragged, marks where they'd land on the
    /// row under the pointer; on release, moves them there.
    fn finish_drag(&mut self, ctx: &egui::Context) {
        let Some(dragged) = egui::DragAndDrop::payload::<DraggedNodes>(ctx) else {
            return;
        };
        let hit = ctx.input(|i| i.pointer.latest_pos()).and_then(|pos| {
            let row = self.drop_rows.iter().find(|r| r.rect.contains(pos))?;
            let fraction = (pos.y - row.rect.top()) / row.rect.height().max(1.0);
            Some((row.clone(), selection::drop_zone(fraction, row.container)))
        });
        // Dropping a row on itself (or into itself) does nothing.
        let hit = hit.filter(|(row, _)| !dragged.0.iter().any(|d| row.path.starts_with(d)));
        if let Some((row, zone)) = &hit {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                Id::new("drop-indicator"),
            ));
            let stroke = egui::Stroke::new(2.0_f32, tokens::ACCENT);
            let _ = match zone {
                DropZone::Before => painter.hline(row.rect.x_range(), row.rect.top(), stroke),
                DropZone::After => painter.hline(row.rect.x_range(), row.rect.bottom(), stroke),
                DropZone::Into => {
                    painter.rect_stroke(row.rect.shrink(1.0), tokens::SELECTION_RADIUS, stroke)
                }
            };
        }
        if ctx.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ctx);
            if let Some((row, zone)) = hit {
                let (parent, index) = selection::drop_destination(&row.path, zone, row.child_count);
                self.drop_nodes(&dragged.0, &parent, index);
            }
        }
    }

    /// Moves `nodes` to `parent` at `index`, keeping selections on the same nodes.
    fn drop_nodes(&mut self, nodes: &[NodePath], parent: &[usize], index: usize) {
        match self.state.move_nodes(nodes, parent, index) {
            Ok(mv) => {
                let n = mv.new_paths().len();
                self.follow_move(&mv);
                self.status = format!("Moved {n} item{}.", if n == 1 { "" } else { "s" });
            }
            Err(StateError::InvalidPath) => {
                self.status =
                    "Error: can't move there — a section can't go inside itself.".to_string()
            }
            Err(e) => self.status = format!("Error: can't move there — {e}"),
        }
    }

    /// Re-points path-based UI state after a move, so it still refers to the same nodes.
    fn follow_move(&mut self, mv: &NodeMove) {
        let remap =
            |paths: &[NodePath]| -> Vec<NodePath> { paths.iter().map(|p| mv.remap(p)).collect() };
        self.sidebar = match self.sidebar.take() {
            Some(SidebarSelection::Node(p)) => Some(SidebarSelection::Node(mv.remap(&p))),
            Some(SidebarSelection::Nodes(v)) => {
                let mut v = remap(&v);
                v.sort();
                Some(SidebarSelection::Nodes(v))
            }
            other => other,
        };
        self.collapsed = self.collapsed.iter().map(|p| mv.remap(p)).collect();
        self.tree_anchor = self.tree_anchor.as_ref().map(|p| mv.remap(p));
        self.sheet_anchor = self.sheet_anchor.as_ref().map(|p| mv.remap(p));
        self.selected = self.selected.as_ref().map(|p| mv.remap(p));
        self.active_editor = self.active_editor.as_ref().map(|p| mv.remap(p));
        // Sheets moved out of the listed container drop out of its selection.
        let listed = match &self.sidebar {
            Some(sel) => sheet_list::listed_paths(&self.state, sel, &self.today),
            None => Vec::new(),
        };
        let mut sheets = remap(&self.sheet_selection);
        sheets.retain(|p| listed.contains(p));
        sheets.sort_by_key(|p| listed.iter().position(|l| l == p));
        self.sheet_selection = sheets;
        if self
            .selected
            .as_ref()
            .is_some_and(|p| !self.sheet_selection.contains(p))
        {
            self.selected = None;
        }
        self.container_buffers.clear();
        self.rename = None;
        self.pending_jump = None;
        self.info_target = None;
        self.goal_edit_target = None;
        self.export.close();
    }

    // --- OS integration -------------------------------------------------------------------

    /// Fills the library path field from a folder picker; false when cancelled.
    fn browse_library_folder(&mut self) -> bool {
        let start = PathBuf::from(self.library_path_input.trim());
        let start = (!self.library_path_input.trim().is_empty()).then_some(start.as_path());
        match self.os.pick_folder("Choose a library folder", start) {
            Some(folder) => {
                self.library_path_input = folder.display().to_string();
                true
            }
            None => false,
        }
    }

    fn new_library_via_dialog(&mut self) {
        if self.browse_library_folder() {
            self.create_library();
        }
    }

    fn open_library_via_dialog(&mut self) {
        if self.browse_library_folder() {
            self.open_library();
        }
    }

    /// Replaces the export destination with a Save As choice, keeping the format's extension.
    fn browse_export_destination(&mut self) {
        let format = self.export.format;
        let filters: [crate::os::FileFilter; 1] = [(format.label(), &[format.extension()])];
        let default = PathBuf::from(&self.export.destination);
        if let Some(mut dest) = self.os.save_file("Export As", &default, &filters) {
            if dest.extension().is_none() {
                dest.set_extension(format.extension());
            }
            self.export.destination = dest.display().to_string();
        }
    }

    /// Picks a custom CSS file for the export dialog and validates it reads as text.
    /// The file is re-read on every export, so outside edits apply without re-picking.
    fn browse_export_css(&mut self) {
        const CSS_FILTERS: [crate::os::FileFilter; 1] = [("Cascading Style Sheets", &["css"])];
        match self.os.pick_file("Custom Export CSS", &CSS_FILTERS) {
            None => {}
            Some(path) => match std::fs::read_to_string(&path) {
                Ok(_) => {
                    self.status = format!("Custom style: {}", path.display());
                    self.export.custom_css_path = Some(path);
                    self.save_preferences();
                }
                Err(e) => {
                    self.status = format!("Couldn't read {}: {e}", path.display());
                }
            },
        }
    }

    fn reveal_last_export(&mut self) {
        if let Some(path) = self.last_export_path.clone() {
            if let Err(e) = self.os.reveal_path(&path) {
                self.status = format!("Error showing {}: {e}", path.display());
            }
        }
    }

    fn attach_file_via_dialog(&mut self, path: &NodePath) {
        if let Some(file) = self.os.pick_file("Attach File", &[]) {
            match crate::attachments::attach_file(
                &mut self.state,
                path,
                &file.display().to_string(),
            ) {
                Ok(()) => self.status = format!("Attached {}", file.display()),
                Err(e) => self.status = format!("Error attaching: {e}"),
            }
        }
    }

    fn open_attachment(&mut self, file_name: &str) {
        let path = PathBuf::from(file_name);
        if !path.exists() {
            self.status = format!("Error: {file_name} no longer exists.");
        } else if let Err(e) = self.os.open_path(&path) {
            self.status = format!("Error opening {file_name}: {e}");
        }
    }

    fn import_path(&mut self, file: &std::path::Path) {
        let Some(parent) = self.creation_parent() else {
            self.status = format!("Error importing: {}", StateError::OutsideProject);
            return;
        };
        self.status = match crate::import_ui::import_file(&mut self.state, &parent, file) {
            Ok(1) => format!("Imported {}", file.display()),
            Ok(n) => format!("Imported {n} sheets from {}", file.display()),
            Err(e) => format!("Error importing {}: {e}", file.display()),
        };
    }

    fn import_via_dialog(&mut self) {
        if let Some(file) = self
            .os
            .pick_file("Import", crate::import_ui::IMPORT_FILTERS)
        {
            self.import_path(&file);
        }
    }

    fn import_textbundle_via_dialog(&mut self) {
        if let Some(folder) = self.os.pick_folder("Import TextBundle", None) {
            self.import_path(&folder);
        }
    }

    /// Whether the editor shows several sheets one after another (the sheet flow) and one of
    /// their editors has keyboard focus.
    fn flow_editor_focused(&self, ctx: &egui::Context) -> bool {
        if self.selected.is_some() {
            return false;
        }
        let paths = self.visible_sheet_paths();
        paths.len() > 1
            && paths
                .iter()
                .any(|p| ctx.memory(|m| m.has_focus(editor_id(p))))
    }

    /// Ctrl+A selects the text of every sheet in the flow; while that selection stands, Ctrl+C
    /// copies it all (sheets separated by a blank line) and any other input drops it.
    fn flow_selection_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Event, Key, Modifiers};
        if self.flow_all_selected && !self.flow_editor_focused(ctx) {
            self.flow_all_selected = false;
        }
        if self.flow_all_selected {
            let copy = ctx.input_mut(|i| {
                let before = i.events.len();
                i.events.retain(|e| !matches!(e, Event::Copy));
                i.events.len() != before
            });
            if copy {
                let paths = self.visible_sheet_paths();
                let text: Vec<&str> = paths
                    .iter()
                    .filter_map(|p| self.container_buffers.get(p))
                    .map(String::as_str)
                    .collect();
                ctx.copy_text(text.join("\n\n"));
                self.status = format!("Copied {} sheets.", text.len());
            }
            let dismissed = ctx.input(|i| {
                i.events.iter().any(|e| match e {
                    Event::Text(_) | Event::Paste(_) | Event::Cut => true,
                    Event::PointerButton { pressed, .. } => *pressed,
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => !(modifiers.command && matches!(key, Key::C | Key::A)),
                    _ => false,
                })
            });
            if dismissed {
                self.flow_all_selected = false;
            }
        }
        if self.flow_editor_focused(ctx)
            && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A))
        {
            self.flow_all_selected = true;
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let pressed = |key| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, key));
        self.flow_selection_shortcuts(ctx);
        if pressed(Key::S) {
            self.save();
        }
        if pressed(Key::B) {
            self.apply_format(ctx, toolbar::bold);
        }
        if pressed(Key::I) {
            self.apply_format(ctx, toolbar::italic);
        }
        if pressed(Key::N) {
            let result = match self.creation_parent() {
                Some(parent) => crate::shortcuts::new_sheet(&mut self.state, &parent),
                None => Err(StateError::OutsideProject),
            };
            match result {
                Ok(()) => self.status = "New sheet created.".to_string(),
                Err(e) => self.status = format!("Error: {e}"),
            }
        }
        let open_run_sheet = self.active_editor.clone().filter(|p| self.open_glue_run(p));
        if let Some(current) = self.selected.clone().or(open_run_sheet) {
            if pressed(Key::ArrowDown) {
                if let Some(next) = crate::shortcuts::navigate_next_sheet(&self.state, &current) {
                    self.open_sheet(next);
                }
            }
            if pressed(Key::ArrowUp) {
                if let Some(prev) = crate::shortcuts::navigate_previous_sheet(&self.state, &current)
                {
                    self.open_sheet(prev);
                }
            }
        }
    }

    // --- Top: menu bar and toolbar --------------------------------------------------------

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu")
            .frame(panel_frame(tokens::BG_PANEL).inner_margin(Margin::symmetric(8.0, 2.0)))
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New Library…").clicked() {
                            ui.close_menu();
                            self.new_library_via_dialog();
                        }
                        if ui.button("Open Library…").clicked() {
                            ui.close_menu();
                            self.open_library_via_dialog();
                        }
                        if ui.button("New Project").clicked() {
                            self.new_project();
                            ui.close_menu();
                        }
                        if ui.button("Library by Path…").clicked() {
                            self.show_library_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("Save    Ctrl+S").clicked() {
                            self.save();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Export…").clicked() {
                            self.open_export();
                            ui.close_menu();
                        }
                        if ui.button("Quick Export").clicked() {
                            self.quick_export();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                self.last_export_path.is_some(),
                                egui::Button::new("Show Last Export in Folder"),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                            self.reveal_last_export();
                        }
                        ui.separator();
                        if ui.button("Import…").clicked() {
                            ui.close_menu();
                            self.import_via_dialog();
                        }
                        if ui.button("Import TextBundle…").clicked() {
                            ui.close_menu();
                            self.import_textbundle_via_dialog();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        ui.checkbox(&mut self.text_checking, "Check Spelling and Grammar");
                        ui.separator();
                        if let Some(glue) = self.glue_menu_items(ui, None) {
                            self.run_glue(glue);
                        }
                    });
                    ui.menu_button("Markup", |ui| {
                        if ui.button("Bold    Ctrl+B").clicked() {
                            self.apply_format(ui.ctx(), toolbar::bold);
                            ui.close_menu();
                        }
                        if ui.button("Italic    Ctrl+I").clicked() {
                            self.apply_format(ui.ctx(), toolbar::italic);
                            ui.close_menu();
                        }
                        if ui.button("Link…").clicked() {
                            self.link_url = Some("https://".to_string());
                            ui.close_menu();
                        }
                        if ui.button("Paragraph Style…").clicked() {
                            self.show_paragraph_picker = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.checkbox(&mut self.show_markdown, "Show Markdown");
                    });
                    ui.menu_button("View", |ui| {
                        if ui.button("Library & Sheets").clicked() {
                            self.layout.toggle_sidebar();
                            ui.close_menu();
                        }
                        if ui.button("Inspector").clicked() {
                            self.layout.toggle_inspector();
                            ui.close_menu();
                        }
                        if ui.button("Editor Only").clicked() {
                            self.layout.toggle_editor_only();
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.checkbox(&mut self.focus_mode, "Focus Mode");
                        if ui.button("Statistics").clicked() {
                            self.show_inspector_tab(InspectorTab::Statistics);
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Sheet", |ui| {
                        if ui.button("New Sheet").clicked() {
                            self.new_root_node(false);
                            ui.close_menu();
                        }
                        if ui.button("New Section").clicked() {
                            self.new_root_node(true);
                            ui.close_menu();
                        }
                        if ui.button("New Filter…").clicked() {
                            self.handle_intent(UiIntent::OpenFilterBuilder);
                            ui.close_menu();
                        }
                        ui.separator();
                        let target = self.target_sheet();
                        if ui
                            .add_enabled(target.is_some(), egui::Button::new("Goal…"))
                            .clicked()
                        {
                            if let Some(path) = target.clone() {
                                self.handle_intent(UiIntent::OpenGoal(path));
                            }
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(target.is_some(), egui::Button::new("Keywords"))
                            .clicked()
                        {
                            self.show_inspector_tab(InspectorTab::Keywords);
                            ui.close_menu();
                        }
                        if ui.button("Keyword Manager…").clicked() {
                            self.show_keyword_manager = true;
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(target.is_some(), egui::Button::new("Attachments"))
                            .clicked()
                        {
                            self.show_inspector_tab(InspectorTab::Media);
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Style", |ui| {
                        let current = self.style_picker.selected_name.clone();
                        for name in self.style_picker.available_names().collect::<Vec<_>>() {
                            if ui.radio(current.as_deref() == Some(name), name).clicked() {
                                self.style_picker.select(name);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("Set as Default").clicked() {
                            self.save_preferences();
                            ui.close_menu();
                        }
                    });
                });
            });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        let mut clicked = None;
        egui::TopBottomPanel::top("toolbar")
            .frame(panel_frame(tokens::BG_PANEL).inner_margin(Margin::symmetric(8.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for action in ToolbarAction::WINDOW_LEFT {
                        if self.toolbar_button(ui, action) {
                            clicked = Some(action);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for action in ToolbarAction::WINDOW_RIGHT.into_iter().rev() {
                            if self.toolbar_button(ui, action) {
                                clicked = Some(action);
                            }
                        }
                    });
                });
            });
        if let Some(action) = clicked {
            self.dispatch_toolbar(ctx, action);
        }
    }

    fn toolbar_button(&self, ui: &mut Ui, action: ToolbarAction) -> bool {
        self.toolbar_button_rect(ui, action).0
    }

    /// A toolbar button: whether it was clicked, and where it was laid out.
    fn toolbar_button_rect(&self, ui: &mut Ui, action: ToolbarAction) -> (bool, Rect) {
        let label = RichText::new(action.icon()).size(15.0);
        let enabled = !action.needs_editor() || self.active_editor.is_some();
        let response = ui
            .add_enabled(
                enabled,
                egui::SelectableLabel::new(self.toolbar_active(action), label),
            )
            .on_hover_text(action.tooltip());
        (response.clicked(), response.rect)
    }

    // --- Panel 1: Library -----------------------------------------------------------------

    fn library_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut add_project_row_done = false;
                for row in sidebar::root_rows(&self.state.library) {
                    match row {
                        SidebarRow::Shelf => {
                            let highlight = if self.on_shelf() {
                                Highlight::Selected
                            } else {
                                Highlight::None
                            };
                            let (response, _) =
                                tree_row(ui, 0, icons::SHELF, "Library", None, highlight, &[]);
                            if response.on_hover_text("All projects").clicked() {
                                self.show_shelf();
                            }
                        }
                        SidebarRow::Smart(smart) => self.smart_row(ui, smart),
                        SidebarRow::Heading(label) => {
                            if label == sidebar::FILTERS_HEADING {
                                add_project_row_done = true;
                                self.add_project_row(ui);
                            }
                            ui.add_space(3.0 * tokens::GRID);
                            ui.horizontal(|ui| {
                                ui.add_space(tokens::GRID);
                                ui.label(text(
                                    label.to_uppercase(),
                                    tokens::UI_SECTION,
                                    tokens::TEXT_QUATERNARY,
                                ));
                                if label == sidebar::PROJECTS_HEADING {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        self.add_menu(ui, parrot_core::Section::Local);
                                    });
                                }
                                #[cfg(target_os = "macos")]
                                if label == sidebar::ICLOUD_HEADING {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        self.add_menu(ui, parrot_core::Section::ICloud);
                                    });
                                }
                            });
                            ui.add_space(tokens::GRID);
                        }
                        SidebarRow::Node(path) => self.tree_node(ui, path, 0),
                    }
                }
                if !add_project_row_done {
                    self.add_project_row(ui);
                }
            });
    }

    /// The "+ New Project" row closing the project list.
    fn add_project_row(&mut self, ui: &mut Ui) {
        let (response, _) = tree_row(ui, 0, icons::ADD, "New Project", None, Highlight::None, &[]);
        if response.on_hover_text("Create a new project").clicked() {
            self.new_project();
        }
    }

    /// The "+" beside the PROJECTS heading: a new project, or a section, sheet or filter in
    /// the current project.
    fn add_menu(&mut self, ui: &mut Ui, section: parrot_core::Section) {
        let in_project = self.creation_parent().is_some();
        let new_project_label = match section {
            parrot_core::Section::ICloud => "New iCloud Project",
            parrot_core::Section::Local => "New Project",
        };
        ui.menu_button(
            text(icons::ADD, tokens::UI_LABEL, tokens::TEXT_SECONDARY),
            |ui| {
                if ui.button(new_project_label).clicked() {
                    self.new_project_in_section(section);
                    ui.close_menu();
                }
                ui.separator();
                let hint = "Select a project first";
                if ui
                    .add_enabled(in_project, egui::Button::new("New Section"))
                    .on_disabled_hover_text(hint)
                    .clicked()
                {
                    self.new_root_node(true);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(in_project, egui::Button::new("New Sheet"))
                    .on_disabled_hover_text(hint)
                    .clicked()
                {
                    self.new_root_node(false);
                    ui.close_menu();
                }
                if ui.button("New Filter…").clicked() {
                    self.handle_intent(UiIntent::OpenFilterBuilder);
                    ui.close_menu();
                }
            },
        )
        .response
        .on_hover_text("New project, section, sheet or filter");
    }

    fn smart_row(&mut self, ui: &mut Ui, smart: SmartFilter) {
        let selected = self.sidebar == Some(SidebarSelection::Smart(smart));
        let label = match smart {
            SmartFilter::Trash if !self.state.trash.is_empty() => {
                format!("{} ({})", smart.label(), self.state.trash.len())
            }
            _ => smart.label().to_string(),
        };
        let highlight = match (selected, self.sheet_selection.is_empty()) {
            (false, _) => Highlight::None,
            (true, true) => Highlight::Selected,
            (true, false) => Highlight::Container,
        };
        let (response, _) = tree_row(ui, 0, smart.icon(), &label, None, highlight, &[]);
        if response.clicked() {
            self.select_sidebar(SidebarSelection::Smart(smart));
        }
        if smart == SmartFilter::Trash {
            response.context_menu(|ui| {
                if ui
                    .add_enabled(
                        !self.state.trash.is_empty(),
                        egui::Button::new("Empty Trash…"),
                    )
                    .clicked()
                {
                    self.confirm_empty_trash = true;
                    ui.close_menu();
                }
            });
        }
    }

    fn tree_node(&mut self, ui: &mut Ui, path: NodePath, depth: usize) {
        let Ok(node) = self.state.node(&path) else {
            return;
        };
        let (icon, title, child_count) = match node {
            Node::Sheet(s) => (icons::SHEET, s.title.clone(), None),
            Node::Group(g) => (icons::GROUP, g.title.clone(), Some(g.children.len())),
            Node::Project(p) => (icons::PROJECT, p.title.clone(), Some(p.children.len())),
            Node::Filter(f) => (icons::SEARCH, f.title.clone(), None),
        };
        let badges = badges_for(node);
        let actions = ContextAction::for_node(node);
        let container = matches!(node, Node::Group(_) | Node::Project(_));
        let expanded = !self.collapsed.contains(&path);

        let renaming = matches!(&self.rename, Some((p, _)) if *p == path);
        if renaming {
            let mut commit = false;
            let mut cancel = false;
            // Laid out like `tree_row` (same indent, disclosure and icon slots) so the field
            // lines up with the row it replaces.
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), tokens::TREE_ROW_HEIGHT),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add_space(tokens::GRID + depth as f32 * tokens::INDENT + 3.0 * tokens::GRID);
                    ui.label(text(icon, tokens::UI_CAPTION, tokens::TEXT_SECONDARY));
                    if let Some((_, buffer)) = &mut self.rename {
                        (commit, cancel) = rename_field(
                            ui,
                            buffer,
                            Id::new(("rename", &path)),
                            f32::INFINITY,
                            &mut self.rename_needs_focus,
                        );
                    }
                },
            );
            if commit {
                self.commit_rename();
            } else if cancel {
                self.rename = None;
            }
        } else {
            let (response, toggled) = tree_row(
                ui,
                depth,
                icon,
                &title,
                child_count.map(|_| expanded),
                self.tree_highlight(&path),
                &badges,
            );
            if self.reveal_in_tree.as_ref() == Some(&path) {
                response.scroll_to_me(None);
                self.reveal_in_tree = None;
            }
            if toggled {
                if expanded {
                    self.collapsed.insert(path.clone());
                } else {
                    self.collapsed.remove(&path);
                }
            } else if response.double_clicked() {
                self.handle_intent(UiIntent::BeginRename(path.clone()));
            } else if response.clicked() {
                let modifiers = ui.input(|i| i.modifiers);
                self.click_tree_node(&path, modifiers);
            }
            if response.drag_started() {
                let current = self
                    .sidebar
                    .as_ref()
                    .map(SidebarSelection::nodes)
                    .unwrap_or_default();
                let rows = selection::dragged_rows(&current, &path);
                egui::DragAndDrop::set_payload(ui.ctx(), DraggedNodes(rows));
            }
            self.drop_rows.push(DropRow {
                path: path.clone(),
                rect: response.rect,
                container,
                child_count: child_count.unwrap_or(0),
            });
            let mut chosen = None;
            response.context_menu(|ui| {
                for action in &actions {
                    if *action == ContextAction::GetInfo || *action == ContextAction::Export {
                        ui.separator();
                    }
                    if ui.button(action.label()).clicked() {
                        chosen = Some(*action);
                        ui.close_menu();
                    }
                }
            });
            if let Some(action) = chosen {
                self.run_context_action(&path, action);
                return;
            }
        }

        if let (Some(count), true) = (child_count, expanded) {
            for i in 0..count {
                let mut child = path.clone();
                child.push(i);
                self.tree_node(ui, child, depth + 1);
            }
        }
    }

    // --- Panel 2: Sheet list --------------------------------------------------------------

    fn sheet_list_panel(&mut self, ui: &mut Ui) {
        let Some(selection) = self.sidebar.clone() else {
            ui.label(text(
                "Select an item in the library.",
                tokens::UI_CAPTION,
                tokens::TEXT_TERTIARY,
            ));
            return;
        };
        let header = sheet_list::container_header(&self.state, &selection);
        let header = ui.add(
            egui::Label::new(text(header, tokens::UI_HEADER, tokens::TEXT_PRIMARY))
                .truncate()
                .sense(Sense::click()),
        );
        if header.clicked() {
            // Back from the selected sheet(s) to the container's sheet flow.
            self.clear_sheet_selection();
        }
        ui.add_space(tokens::GRID);
        ui.add(
            egui::TextEdit::singleline(&mut self.search.query)
                .hint_text(format!("{} Search library", icons::SEARCH))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(2.0 * tokens::GRID);

        if selection == SidebarSelection::Smart(SmartFilter::Trash) && self.search.query.is_empty()
        {
            self.trash_list(ui);
            return;
        }
        let paths: Vec<NodePath> = if self.search.query.is_empty() {
            sheet_list::listed_paths(&self.state, &selection, &self.today)
        } else {
            self.search
                .execute_with_paths(&self.state.library)
                .into_iter()
                .map(|(p, _)| p)
                .collect()
        };
        let rows = sheet_list::rows_with(&self.state, &paths, &|p| self.language_of(p));
        let footer = sheet_list::footer_label(&self.state, &selection, rows.len());
        let order: Vec<NodePath> = rows.iter().map(|r| r.path.clone()).collect();
        // A listed project shows its sections as headings, with their sheets indented below.
        let sections_from = match &selection {
            SidebarSelection::Node(p)
                if self.search.query.is_empty()
                    && matches!(self.state.node(p), Ok(Node::Project(_))) =>
            {
                Some(p.len())
            }
            _ => None,
        };
        let can_add = self.search.query.is_empty()
            && matches!(&selection, SidebarSelection::Node(p) if !matches!(self.state.node(p), Ok(Node::Filter(_))))
            && self.creation_parent().is_some();
        let mut shown_sections: BTreeSet<NodePath> = BTreeSet::new();
        let mut add_sheet = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for row in &rows {
                    let mut indent = 0.0;
                    if let Some(base) = sections_from {
                        for len in base + 1..row.path.len() {
                            let section = row.path[..len].to_vec();
                            if shown_sections.insert(section.clone()) {
                                if let Ok(Node::Group(g)) = self.state.node(&section) {
                                    ui.horizontal(|ui| {
                                        ui.add_space(
                                            tokens::GRID + (len - base - 1) as f32 * tokens::INDENT,
                                        );
                                        ui.label(text(
                                            g.title.to_uppercase(),
                                            tokens::UI_SECTION,
                                            tokens::TEXT_QUATERNARY,
                                        ));
                                    });
                                    ui.add_space(tokens::GRID);
                                }
                            }
                        }
                        indent = (row.path.len() - base - 1) as f32 * tokens::INDENT;
                    }
                    let open_run = self.open_glue_run(&row.path);
                    let active = self.active_editor.as_ref() == Some(&row.path);
                    let selected = (self.sheet_selection.contains(&row.path) && !open_run)
                        || (open_run && active)
                        || (self.sheet_selection.is_empty() && active);
                    let renaming = matches!(&self.rename, Some((p, _)) if *p == row.path);
                    let response =
                        sheet_row(ui, row, selected, open_run && !active, indent, renaming);
                    if renaming {
                        // The card stays; the field takes the title's place inside it.
                        let pad = 2.0 * tokens::GRID;
                        let field = Rect::from_min_size(
                            response.rect.min + egui::vec2(indent + pad, pad),
                            egui::vec2(response.rect.width() - indent - 2.0 * pad, 20.0),
                        );
                        let (mut commit, mut cancel) = (false, false);
                        if let Some((_, buffer)) = &mut self.rename {
                            let id = Id::new(("sheet-list-rename", &row.path));
                            let mut field_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(field)
                                    .layout(Layout::left_to_right(Align::Center)),
                            );
                            (commit, cancel) = rename_field(
                                &mut field_ui,
                                buffer,
                                id,
                                field.width(),
                                &mut self.rename_needs_focus,
                            );
                        }
                        if commit {
                            self.commit_rename();
                        } else if cancel {
                            self.rename = None;
                        }
                    } else if response.double_clicked() {
                        self.handle_intent(UiIntent::BeginRename(row.path.clone()));
                    } else if response.clicked() {
                        let modifiers = ui.input(|i| i.modifiers);
                        self.click_sheet_row(&row.path, &order, modifiers);
                    }
                    if response.drag_started() {
                        let rows = selection::dragged_rows(&self.sheet_selection, &row.path);
                        egui::DragAndDrop::set_payload(ui.ctx(), DraggedNodes(rows));
                    }
                    self.drop_rows.push(DropRow {
                        path: row.path.clone(),
                        rect: response.rect,
                        container: false,
                        child_count: 0,
                    });
                    let mut chosen = None;
                    let mut glue = None;
                    response.context_menu(|ui| {
                        if let Ok(node) = self.state.node(&row.path) {
                            for action in ContextAction::for_node(node) {
                                if ui.button(action.label()).clicked() {
                                    chosen = Some(action);
                                    ui.close_menu();
                                }
                            }
                        }
                        ui.separator();
                        glue = self.glue_menu_items(ui, Some(&row.path));
                    });
                    if let Some(action) = chosen {
                        self.run_context_action(&row.path, action);
                        return;
                    }
                    if let Some(glue) = glue {
                        self.run_glue(glue);
                        return;
                    }
                }
                if can_add {
                    let (response, _) =
                        tree_row(ui, 0, icons::ADD, "New Sheet", None, Highlight::None, &[]);
                    add_sheet = response.on_hover_text("Create a new sheet").clicked();
                }
                if let Some(label) = footer {
                    ui.add_space(4.0 * tokens::GRID);
                    ui.vertical_centered(|ui| {
                        ui.label(text(label, tokens::UI_CAPTION, tokens::TEXT_QUATERNARY));
                    });
                }
            });
        if add_sheet {
            self.new_root_node(false);
        }
    }

    fn trash_list(&mut self, ui: &mut Ui) {
        let mut restore = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, node) in self.state.trash.iter().enumerate() {
                    let (icon, title) = match node {
                        Node::Sheet(s) => (icons::SHEET, s.title.as_str()),
                        Node::Group(g) => (icons::GROUP, g.title.as_str()),
                        Node::Project(p) => (icons::PROJECT, p.title.as_str()),
                        Node::Filter(f) => (icons::SEARCH, f.title.as_str()),
                    };
                    let (response, _) = tree_row(ui, 0, icon, title, None, Highlight::None, &[]);
                    response.context_menu(|ui| {
                        if ui.button("Put Back").clicked() {
                            restore = Some(i);
                            ui.close_menu();
                        }
                    });
                    if response.double_clicked() {
                        restore = Some(i);
                    }
                }
                if self.state.trash.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.label(text(
                            sheet_list::EMPTY_SHEET_LABEL,
                            tokens::UI_CAPTION,
                            tokens::TEXT_QUATERNARY,
                        ));
                    });
                } else {
                    ui.add_space(2.0 * tokens::GRID);
                    ui.label(text(
                        "Double-click or right-click → Put Back to restore.",
                        tokens::UI_CAPTION,
                        tokens::TEXT_QUATERNARY,
                    ));
                    if ui.small_button("Empty Trash…").clicked() {
                        self.confirm_empty_trash = true;
                    }
                }
            });
        if let Some(i) = restore {
            let project = self
                .current_project()
                .or_else(|| self.state.projects().into_iter().next());
            self.status = match self.state.restore_from_trash(i, project.as_deref()) {
                Ok(path) => match path.as_slice() {
                    [_] => "Restored to the library.".to_string(),
                    _ => "Restored into the project.".to_string(),
                },
                Err(e) => format!("Error: {e}"),
            };
        }
    }

    // --- Panel 3: Editor ------------------------------------------------------------------

    /// The sheet area's chrome around the editor: Export/Quick Export top-left; the bottom
    /// bar with the formatting buttons left and the word-count pill right. Clicking the pill
    /// opens Statistics & Goals; the findings mark beside it opens Spelling & Grammar.
    fn sheet_chrome(&mut self, ui: &mut Ui, words: u32, has_errors: bool) {
        let mut clicked = None;
        let mut target = None;
        egui::TopBottomPanel::top("sheet_header")
            .frame(
                egui::Frame::none()
                    .inner_margin(Margin::symmetric(3.0 * tokens::GRID, tokens::GRID)),
            )
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    for action in ToolbarAction::SHEET_AREA {
                        if self.toolbar_button(ui, action) {
                            clicked = Some(action);
                        }
                    }
                });
            });
        egui::TopBottomPanel::bottom("editor_footer")
            .frame(
                egui::Frame::none()
                    .inner_margin(Margin::symmetric(3.0 * tokens::GRID, tokens::GRID)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    // Centered in the bar: egui lays out left to right, so the buttons are
                    // offset by half the free width, using the width they took last frame.
                    let width_id = ui.id().with("format_bar_width");
                    let last_width: f32 = ui.data(|d| d.get_temp(width_id)).unwrap_or(0.0);
                    ui.add_space(((ui.available_width() - last_width) / 2.0).max(0.0));
                    let mut buttons: Option<Rect> = None;
                    for action in ToolbarAction::FORMAT_BAR {
                        let (pressed, rect) = self.toolbar_button_rect(ui, action);
                        buttons = Some(buttons.map_or(rect, |r| r.union(rect)));
                        if pressed {
                            clicked = Some(action);
                        }
                    }
                    let width = buttons.map_or(0.0, |r| r.width());
                    if let Some(buttons) = buttons {
                        let bar = ui.max_rect();
                        ui.data_mut(|d| d.insert_temp(Id::new(FORMAT_BAR_LAYOUT), (bar, buttons)));
                    }
                    if (width - last_width).abs() > 0.5 {
                        ui.data_mut(|d| d.insert_temp(width_id, width));
                        ui.ctx().request_repaint();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let pill = egui::Button::new(text(
                            editor::word_count_label(words),
                            tokens::UI_CAPTION,
                            tokens::TEXT_TERTIARY,
                        ))
                        .fill(tokens::BG_ELEVATED)
                        .rounding(tokens::BADGE_RADIUS);
                        if ui
                            .add(pill)
                            .on_hover_text("Open statistics and goals")
                            .clicked()
                        {
                            target = Some(InspectorTab::Statistics);
                        }
                        if has_errors
                            && ui
                                .add(
                                    egui::Button::new(text(
                                        icons::WARNING,
                                        tokens::UI_CAPTION,
                                        tokens::SPELLING,
                                    ))
                                    .frame(false),
                                )
                                .on_hover_text("Show spelling and grammar findings")
                                .clicked()
                        {
                            target = Some(InspectorTab::Proofing);
                        }
                    });
                });
            });
        if let Some(tab) = target {
            self.show_inspector_tab(tab);
        }
        if let Some(action) = clicked {
            let ctx = ui.ctx().clone();
            self.dispatch_toolbar(&ctx, action);
        }
    }

    fn editor_panel(&mut self, ui: &mut Ui) {
        if crate::first_run::should_prompt(&self.state) {
            self.welcome(ui);
            return;
        }
        if self.sheet_selection.len() > 1 {
            let paths = self.visible_sheet_paths();
            self.sheet_flow(ui, &paths);
            return;
        }
        if let Some(path) = self.selected.clone() {
            if self.sheet(&path).is_some() {
                self.single_sheet(ui, &path);
            } else {
                self.selected = None;
            }
            return;
        }
        match self.sidebar.clone() {
            Some(SidebarSelection::Smart(SmartFilter::Trash)) => {
                self.centered_caption(ui, &format!("Trash — {} item(s)", self.state.trash.len()));
            }
            Some(_) => {
                let paths = self.visible_sheet_paths();
                if paths.is_empty() {
                    self.centered_caption(ui, "This project or section has no sheets yet.");
                } else {
                    self.sheet_flow(ui, &paths);
                }
            }
            None => self.shelf(ui),
        }
    }

    /// The Library shelf: every project as a book-like card; clicking one goes into it.
    /// On macOS the cards group under local and iCloud headings so the separate
    /// iCloud library is visible on the landing page too.
    fn shelf(&mut self, ui: &mut Ui) {
        let items = crate::shelf::items(&self.state);
        #[cfg(target_os = "macos")]
        let show_sections = true;
        #[cfg(not(target_os = "macos"))]
        let show_sections = false;
        let mut open = None;
        let mut action = None;
        let mut create_local = false;
        #[cfg(target_os = "macos")]
        let mut create_icloud = false;
        egui::ScrollArea::vertical()
            .id_salt("shelf")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::none()
                    .inner_margin(Margin::same(10.0 * tokens::GRID))
                    .show(ui, |ui| {
                        ui.label(text("Library", tokens::HEADING_2, tokens::TEXT_PRIMARY));
                        let words: u32 = items.iter().map(|i| i.words).sum();
                        ui.label(text(
                            format!(
                                "{} {} · {} words",
                                items.len(),
                                if items.len() == 1 {
                                    "project"
                                } else {
                                    "projects"
                                },
                                inspector::format_count(words)
                            ),
                            tokens::UI_CAPTION,
                            tokens::TEXT_TERTIARY,
                        ));
                        ui.add_space(6.0 * tokens::GRID);
                        if show_sections {
                            #[allow(clippy::single_match)]
                            match self.shelf_section(
                                ui,
                                &items,
                                parrot_core::Section::Local,
                                "Projects",
                                &mut open,
                                &mut action,
                            ) {
                                true => create_local = true,
                                false => {}
                            }
                            #[cfg(target_os = "macos")]
                            if self.shelf_section(
                                ui,
                                &items,
                                parrot_core::Section::ICloud,
                                "iCloud",
                                &mut open,
                                &mut action,
                            ) {
                                create_icloud = true;
                            }
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing =
                                    egui::vec2(6.0 * tokens::GRID, 6.0 * tokens::GRID);
                                for item in &items {
                                    let response = shelf_card(ui, item);
                                    if response.clicked() {
                                        open = Some(item.path.clone());
                                    }
                                    response.context_menu(|ui| {
                                        if let Ok(node) = self.state.node(&item.path) {
                                            for a in ContextAction::for_node(node) {
                                                if ui.button(a.label()).clicked() {
                                                    action = Some((item.path.clone(), a));
                                                    ui.close_menu();
                                                }
                                            }
                                        }
                                    });
                                }
                                if new_project_card(ui).clicked() {
                                    create_local = true;
                                }
                            });
                        }
                    });
            });
        if let Some(path) = open {
            self.open_project(path);
        } else if let Some((path, a)) = action {
            self.run_context_action(&path, a);
        } else if create_local {
            self.new_project_in_section(parrot_core::Section::Local);
        }
        #[cfg(target_os = "macos")]
        if create_icloud {
            self.new_project_in_section(parrot_core::Section::ICloud);
        }
    }

    /// One shelf section: heading + that section's project cards + a New Project
    /// card. Returns true when the New card was clicked.
    fn shelf_section(
        &mut self,
        ui: &mut Ui,
        items: &[crate::shelf::ShelfItem],
        section: parrot_core::Section,
        heading: &str,
        open: &mut Option<NodePath>,
        action: &mut Option<(NodePath, ContextAction)>,
    ) -> bool {
        let mut create = false;
        ui.label(text(
            heading.to_uppercase(),
            tokens::UI_SECTION,
            tokens::TEXT_QUATERNARY,
        ));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0 * tokens::GRID, 6.0 * tokens::GRID);
            for item in items.iter().filter(|i| i.section == section) {
                let response = shelf_card(ui, item);
                if response.clicked() {
                    *open = Some(item.path.clone());
                }
                response.context_menu(|ui| {
                    if let Ok(node) = self.state.node(&item.path) {
                        for a in ContextAction::for_node(node) {
                            if ui.button(a.label()).clicked() {
                                *action = Some((item.path.clone(), a));
                                ui.close_menu();
                            }
                        }
                    }
                });
            }
            if new_project_card(ui).clicked() {
                create = true;
            }
        });
        ui.add_space(6.0 * tokens::GRID);
        create
    }

    fn centered_caption(&self, ui: &mut Ui, caption: &str) {
        ui.centered_and_justified(|ui| {
            ui.label(text(caption, tokens::UI_LABEL, tokens::TEXT_QUATERNARY));
        });
    }

    fn welcome(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0 * tokens::GRID);
            ui.label(text(
                "Welcome to Parrot",
                tokens::HEADING_2,
                tokens::TEXT_PRIMARY,
            ));
            ui.add_space(2.0 * tokens::GRID);
            ui.label(text(
                "Enter a folder path to create a new library there, or to open an existing one.",
                tokens::BODY,
                tokens::TEXT_SECONDARY,
            ));
            ui.add_space(4.0 * tokens::GRID);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 480.0).max(0.0) / 2.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.library_path_input)
                        .hint_text(r"C:\Users\you\Documents\Library")
                        .desired_width(400.0),
                );
                if ui
                    .button(format!("{} Browse…", icons::BROWSE))
                    .on_hover_text("Choose a folder")
                    .clicked()
                {
                    self.browse_library_folder();
                }
            });
            ui.add_space(2.0 * tokens::GRID);
            ui.horizontal(|ui| {
                let ready = !self.library_path_input.trim().is_empty();
                ui.add_space((ui.available_width() - 220.0).max(0.0) / 2.0);
                if ui
                    .add_enabled(ready, egui::Button::new("New Library"))
                    .clicked()
                {
                    self.create_library();
                }
                if ui
                    .add_enabled(ready, egui::Button::new("Open Library"))
                    .clicked()
                {
                    self.open_library();
                }
            });
        });
    }

    fn editor_style<'a>(
        &'a self,
        annotations: &'a [Annotation],
        alphas: Option<Vec<f32>>,
    ) -> EditorStyle<'a> {
        EditorStyle {
            show_markdown: self.show_markdown,
            reveal: None,
            paragraph_alphas: alphas,
            annotations,
            stylesheet: self.style_picker.active_stylesheet.as_ref(),
        }
    }

    /// Focus Mode's per-paragraph fades from the editor's stored cursor (T-10-09).
    fn focus_alphas(&self, ctx: &egui::Context, path: &NodePath, buffer: &str) -> Option<Vec<f32>> {
        if !self.focus_mode {
            return None;
        }
        let cursor = egui::text_edit::TextEditState::load(ctx, editor_id(path))
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index)
            .and_then(|c| buffer.char_indices().nth(c).map(|(b, _)| b))
            .unwrap_or(buffer.len());
        let starts: Vec<usize> = editor::paragraph_spans(buffer)
            .into_iter()
            .map(|s| s.start)
            .collect();
        Some(crate::focus_mode::paragraph_alphas(&starts, cursor))
    }

    fn single_sheet(&mut self, ui: &mut Ui, path: &NodePath) {
        let annotations = if self.text_checking {
            annotations::check_in(&self.editor_buffer, self.language_of(path))
        } else {
            Vec::new()
        };
        self.sheet_chrome(
            ui,
            editor::buffer_word_count(&self.editor_buffer),
            !annotations.is_empty(),
        );
        let alphas = self.focus_alphas(ui.ctx(), path, &self.editor_buffer);
        let jump = match &self.pending_jump {
            Some((p, paragraph)) if p == path => Some(*paragraph),
            _ => None,
        };
        let mut buffer = std::mem::take(&mut self.editor_buffer);
        let mut output = None;
        egui::ScrollArea::vertical()
            .id_salt("single-sheet")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                text_column(ui, |ui, width| {
                    let style = self.editor_style(&annotations, alphas.clone());
                    output = Some(sheet_editor(
                        ui,
                        editor_id(path),
                        &mut buffer,
                        &style,
                        width,
                        jump,
                        false,
                    ));
                });
            });
        self.editor_buffer = buffer;
        if jump.is_some() {
            self.pending_jump = None;
        }
        if let Some(out) = output {
            if out.has_focus {
                self.active_editor = Some(path.clone());
            }
            if out.changed {
                self.commit_buffer(path);
            }
            if let Some((a, b)) = out.split {
                self.split_sheet(path, a, b);
            }
        }
    }

    fn sheet_flow(&mut self, ui: &mut Ui, paths: &[NodePath]) {
        let sheets: Vec<(NodePath, String)> = paths
            .iter()
            .filter_map(|p| self.sheet(p).map(|s| (p.clone(), s.title.clone())))
            .collect();
        for (path, _) in &sheets {
            if !self.container_buffers.contains_key(path) {
                if let Some(sheet) = self.sheet(path) {
                    let buffer = editor::content_to_buffer(&sheet.content);
                    self.container_buffers.insert(path.clone(), buffer);
                }
            }
        }
        let refs: Vec<&Sheet> = sheets.iter().filter_map(|(p, _)| self.sheet(p)).collect();
        let words = aggregate_word_count(&refs);
        let checks: Vec<Vec<Annotation>> = sheets
            .iter()
            .map(|(p, _)| {
                if self.text_checking {
                    annotations::check_in(&self.container_buffers[p], self.language_of(p))
                } else {
                    Vec::new()
                }
            })
            .collect();
        self.sheet_chrome(ui, words, checks.iter().any(|c| !c.is_empty()));

        let ctx = ui.ctx().clone();
        let active = self.active_editor.clone();
        // Focus Mode in the flow fades whole sheets other than the active one (T-11-03).
        let alphas: Vec<Option<Vec<f32>>> = sheets
            .iter()
            .map(|(p, _)| {
                if !self.focus_mode || active.is_none() {
                    None
                } else if active.as_ref() == Some(p) {
                    self.focus_alphas(&ctx, p, &self.container_buffers[p])
                } else {
                    let n = editor::paragraph_spans(&self.container_buffers[p]).len();
                    Some(vec![crate::focus_mode::FADED_ALPHA; n])
                }
            })
            .collect();
        let jump = self.pending_jump.clone();
        let mut buffers = std::mem::take(&mut self.container_buffers);
        let mut changed = Vec::new();
        let mut focused = None;
        let mut split: Option<(NodePath, (usize, usize))> = None;
        egui::ScrollArea::vertical()
            .id_salt("sheet-flow")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                text_column(ui, |ui, width| {
                    for item in flow_items(sheets.len()) {
                        match item {
                            FlowItem::Divider => {
                                ui.add_space(6.0 * tokens::GRID);
                                let y = ui.cursor().top();
                                let x = ui.max_rect().x_range();
                                ui.painter().hline(
                                    (x.min - editor::SIDE_MARGIN)..=(x.max + editor::SIDE_MARGIN),
                                    y,
                                    egui::Stroke::new(1.0_f32, tokens::DIVIDER),
                                );
                                ui.add_space(6.0 * tokens::GRID);
                            }
                            FlowItem::Sheet(i) => {
                                let (path, title) = &sheets[i];
                                ui.label(text(title, tokens::UI_CAPTION, tokens::TEXT_QUATERNARY));
                                let Some(buffer) = buffers.get_mut(path) else {
                                    continue;
                                };
                                let style = self.editor_style(&checks[i], alphas[i].clone());
                                let sheet_jump = match &jump {
                                    Some((p, paragraph)) if p == path => Some(*paragraph),
                                    _ => None,
                                };
                                let out = sheet_editor(
                                    ui,
                                    editor_id(path),
                                    buffer,
                                    &style,
                                    width,
                                    sheet_jump,
                                    self.flow_all_selected,
                                );
                                if out.changed {
                                    changed.push(path.clone());
                                }
                                if let Some(range) = out.split {
                                    split = Some((path.clone(), range));
                                }
                                if out.has_focus {
                                    focused = Some(path.clone());
                                }
                            }
                        }
                    }
                });
            });
        self.container_buffers = buffers;
        if jump.is_some() {
            self.pending_jump = None;
        }
        if focused.is_some() {
            self.active_editor = focused;
        }
        for path in changed {
            self.commit_buffer(&path);
        }
        if let Some((path, (a, b))) = split {
            self.split_sheet(&path, a, b);
        }
    }

    /// Splits the sheet at `path` around the selected chars `[a, b)`: the text before, the
    /// selection and the text after become separate sheets, in that order (a missing before
    /// or after makes no sheet; selecting everything does nothing). The first part stays in
    /// the original sheet; the others follow it with titles taken from their first line.
    fn split_sheet(&mut self, path: &NodePath, a: usize, b: usize) {
        let text = match self.buffer_slot(path) {
            Some(BufferSlot::Single) => self.editor_buffer.clone(),
            Some(BufferSlot::Flow) => self.container_buffers[path].clone(),
            None => return,
        };
        let Some((parts, selection)) = editor::split_around_selection(&text, a, b) else {
            return;
        };
        let Some(original) = self.sheet(path).cloned() else {
            return;
        };
        let _ = self
            .state
            .set_sheet_content(path, editor::buffer_to_content(&parts[0]));
        let mut at = path.clone();
        for part in &parts[1..] {
            let sheet = Sheet {
                title: split_title(part),
                content: editor::buffer_to_content(part),
                goal: None,
                keywords: original.keywords.clone(),
                attachments: Vec::new(),
                glued: false,
                draft: original.draft,
            };
            if let Err(e) = self.state.insert_sheet_after(&at, sheet) {
                self.status = format!("Error: {e}");
                return;
            }
            if let Some(last) = at.last_mut() {
                *last += 1;
            }
        }
        let mut opened = path.clone();
        if let Some(last) = opened.last_mut() {
            *last += selection;
        }
        self.reset_selection();
        self.select_sidebar(SidebarSelection::Node(opened));
        self.status = format!("Split into {} sheets.", parts.len());
    }

    // --- Character Counter (Inspector tab) -------------------------------------------------

    /// The Character Counter (`char_counter`) over the editor's selected text, or — with
    /// nothing selected — every sheet the editor shows: counts, readability, time
    /// estimates, platform limits and the frequency table.
    fn counter_tab(&mut self, ui: &mut Ui) {
        let (source, scope) = self.counter_source(ui.ctx());
        self.counter.set_text(source);
        ui.label(text(
            scope.label(),
            tokens::UI_CAPTION,
            tokens::TEXT_TERTIARY,
        ));
        ui.add_space(tokens::GRID);
        let stats = self.counter.stats().clone();
        ui.label(text(
            char_counter::status_line(&stats),
            tokens::UI_LABEL_STRONG,
            tokens::TEXT_PRIMARY,
        ));
        ui.add_space(3.0 * tokens::GRID);
        counter_stats(ui, &stats);
        counter_limits(ui, stats.characters);
        ui.add_space(3.0 * tokens::GRID);
        self.counter_frequency(ui);
    }

    /// What the counter counts: the active editor's selection, else the editor's sheets.
    fn counter_source(&self, ctx: &egui::Context) -> (String, char_counter::CountScope) {
        let visible = self.visible_sheet_paths();
        let selection = self
            .active_editor
            .as_ref()
            .filter(|p| visible.contains(p))
            .and_then(|p| self.selected_text(ctx, p));
        let sheets: Vec<String> = visible
            .iter()
            .filter_map(|p| self.sheet(p))
            .map(|s| char_counter::sheet_text(&s.content))
            .collect();
        char_counter::source_text(selection.as_deref(), &sheets)
    }

    /// The plain text selected in `path`'s editor, if any (markup characters dropped).
    fn selected_text(&self, ctx: &egui::Context, path: &NodePath) -> Option<String> {
        let buffer = match self.buffer_slot(path)? {
            BufferSlot::Single => &self.editor_buffer,
            BufferSlot::Flow => self.container_buffers.get(path)?,
        };
        let range = egui::text_edit::TextEditState::load(ctx, editor_id(path))?
            .cursor
            .char_range()?;
        let (a, b) = (
            range.primary.index.min(range.secondary.index),
            range.primary.index.max(range.secondary.index),
        );
        if a == b {
            return None;
        }
        let selected: String = buffer.chars().skip(a).take(b - a).collect();
        Some(char_counter::sheet_text(&editor::buffer_to_content(
            &selected,
        )))
    }

    fn counter_frequency(&mut self, ui: &mut Ui) {
        ui.label(text(
            "TOP FREQUENCY",
            tokens::UI_SECTION,
            tokens::TEXT_QUATERNARY,
        ));
        ui.add_space(tokens::GRID);
        ui.horizontal_wrapped(|ui| {
            for kind in FrequencyKind::ALL {
                if ui
                    .selectable_label(self.counter.kind == kind, kind.label())
                    .clicked()
                {
                    self.counter.kind = kind;
                }
            }
            ui.add_space(2.0 * tokens::GRID);
            ui.label(text("Top", tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
            egui::ComboBox::from_id_salt("counter-top")
                .selected_text(self.counter.top.to_string())
                .width(56.0)
                .show_ui(ui, |ui| {
                    for n in char_counter::TOP_CHOICES {
                        ui.selectable_value(&mut self.counter.top, n, n.to_string());
                    }
                });
            ui.add_enabled(
                self.counter.kind != FrequencyKind::Character,
                egui::Checkbox::new(&mut self.counter.exclude_grammar, "Exclude grammar words"),
            );
        });
        ui.add_space(tokens::GRID);
        let entries = self.counter.frequency().to_vec();
        if entries.is_empty() {
            Self::no_target(ui, "Nothing to count yet.");
            return;
        }
        egui::Grid::new("counter-frequency")
            .num_columns(3)
            .striped(true)
            .spacing([24.0, 4.0])
            .show(ui, |ui| {
                for (label, align) in [
                    ("Term", Align::LEFT),
                    ("Count", Align::RIGHT),
                    ("Density", Align::RIGHT),
                ] {
                    ui.with_layout(Layout::top_down(align), |ui| {
                        ui.label(text(label, tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
                    });
                }
                ui.end_row();
                for entry in &entries {
                    ui.label(text(&entry.term, tokens::UI_LABEL, tokens::TEXT_PRIMARY));
                    ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                        ui.label(text(
                            entry.count.to_string(),
                            tokens::UI_LABEL,
                            tokens::TEXT_SECONDARY,
                        ));
                    });
                    ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                        ui.label(text(
                            format!("{:.1}%", entry.percent),
                            tokens::UI_LABEL,
                            tokens::TEXT_SECONDARY,
                        ));
                    });
                    ui.end_row();
                }
            });
    }

    // --- Spelling & Grammar (Inspector tab) ------------------------------------------------

    /// The editor's text for `path`: its live buffer, else the stored sheet.
    fn sheet_buffer(&self, path: &NodePath) -> Option<String> {
        match self.buffer_slot(path) {
            Some(BufferSlot::Single) => Some(self.editor_buffer.clone()),
            Some(BufferSlot::Flow) => self.container_buffers.get(path).cloned(),
            None => self
                .sheet(path)
                .map(|s| editor::content_to_buffer(&s.content)),
        }
    }

    /// Every finding across the editor's sheets as one list, in editor (flow) order, each
    /// tagged with its sheet's path and title.
    fn proofing_findings(&self) -> Vec<(NodePath, String, Annotation)> {
        self.visible_sheet_paths()
            .into_iter()
            .filter_map(|p| {
                let title = self.sheet(&p)?.title.clone();
                let found = annotations::check_in(&self.sheet_buffer(&p)?, self.language_of(&p));
                Some(
                    found
                        .into_iter()
                        .map(move |a| (p.clone(), title.clone(), a)),
                )
            })
            .flatten()
            .collect()
    }

    /// The proofing language of the project holding `path`, from its meta file.
    fn language_of(&self, path: &NodePath) -> Language {
        match self.state.project_of(path).map(|p| self.state.node(&p)) {
            Some(Ok(Node::Project(p))) => p
                .language
                .as_deref()
                .and_then(Language::from_code)
                .unwrap_or_default(),
            _ => Language::default(),
        }
    }

    /// A language list for the project of the sheets in view; picking one stores it in the
    /// project and saves the library (the project's meta file).
    fn language_picker(&mut self, ui: &mut Ui) {
        let Some(project) = self
            .visible_sheet_paths()
            .into_iter()
            .next()
            .and_then(|p| self.state.project_of(&p))
        else {
            return;
        };
        let current = self.language_of(&project);
        let mut chosen = current;
        ui.horizontal(|ui| {
            ui.label(text("Language", tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
            egui::ComboBox::from_id_salt("proofing-language")
                .selected_text(current.label())
                .show_ui(ui, |ui| {
                    for lang in Language::ALL {
                        ui.selectable_value(&mut chosen, lang, lang.label());
                    }
                });
        });
        if !current.has_dictionary() {
            Self::no_target(
                ui,
                "No dictionary for this language: spelling check is off.",
            );
        }
        if chosen != current {
            let _ = self
                .state
                .set_project_language(&project, Some(chosen.code().to_string()));
            if self.state.library_path.is_some() {
                self.saved_snapshot = None;
                self.status = match self.state.save() {
                    Ok(()) => format!("Project language: {}", chosen.label()),
                    Err(e) => format!("Error saving library: {e}"),
                };
            }
        }
    }

    /// Replaces `range` of `path`'s text with `fix` (accepting a proofing card).
    fn apply_fix(&mut self, path: &NodePath, range: std::ops::Range<usize>, fix: &str) {
        let Some(mut buffer) = self.sheet_buffer(path) else {
            return;
        };
        if range.end > buffer.len()
            || !buffer.is_char_boundary(range.start)
            || !buffer.is_char_boundary(range.end)
        {
            return;
        }
        buffer.replace_range(range, fix);
        match self.buffer_slot(path) {
            Some(BufferSlot::Single) => self.editor_buffer = buffer,
            Some(BufferSlot::Flow) => {
                self.container_buffers.insert(path.clone(), buffer);
            }
            None => {
                let _ = self
                    .state
                    .set_sheet_content(path, editor::buffer_to_content(&buffer));
                return;
            }
        }
        self.commit_buffer(path);
    }

    /// Stable identity of a finding that survives edits elsewhere in the sheet.
    fn finding_key(buffer: &str, a: &Annotation) -> String {
        let (before, span, after) = annotations::context(buffer, &a.byte_range, 12);
        format!("{}|{before}|{span}|{after}", a.title)
    }

    /// Spelling and grammar findings for every sheet in the editor (§9.3) as cards: title,
    /// the text around the finding, and Accept / Dismiss on the open card. Clicking a card
    /// selects the finding in its sheet's editor.
    fn proofing_tab(&mut self, ui: &mut Ui) {
        if !self.text_checking {
            Self::no_target(ui, "Spelling and grammar checking is off.");
            ui.checkbox(&mut self.text_checking, "Check Spelling and Grammar");
            return;
        }
        let sheets = self.visible_sheet_paths().len();
        if sheets == 0 {
            Self::no_target(ui, "Open a sheet to check it.");
            return;
        }
        self.language_picker(ui);
        ui.add_space(tokens::GRID);
        let buffers: BTreeMap<NodePath, String> = self
            .visible_sheet_paths()
            .into_iter()
            .filter_map(|p| Some((p.clone(), self.sheet_buffer(&p)?)))
            .collect();
        let findings: Vec<(NodePath, String, Annotation)> = self
            .proofing_findings()
            .into_iter()
            .filter(|(p, _, a)| {
                buffers.get(p).is_none_or(|b| {
                    !self
                        .dismissed_findings
                        .contains(&(p.clone(), Self::finding_key(b, a)))
                })
            })
            .collect();
        if findings.is_empty() {
            Self::no_target(ui, "No spelling or grammar findings.");
            return;
        }
        let count = |kind| findings.iter().filter(|(_, _, a)| a.kind == kind).count();
        let (spelling, grammar) = (
            count(AnnotationKind::Spelling),
            count(AnnotationKind::Grammar),
        );
        ui.horizontal_wrapped(|ui| {
            for (label, filter) in [
                (format!("All {}", spelling + grammar), None),
                (
                    format!("Spelling {spelling}"),
                    Some(AnnotationKind::Spelling),
                ),
                (format!("Grammar {grammar}"), Some(AnnotationKind::Grammar)),
            ] {
                if ui
                    .selectable_label(self.proofing_filter == filter, label)
                    .clicked()
                {
                    self.proofing_filter = filter;
                }
            }
        });
        if sheets > 1 {
            Self::no_target(ui, &format!("Across {sheets} sheets"));
        }
        ui.add_space(tokens::GRID);

        let mut jump = None;
        let mut accept = None;
        let mut dismiss = None;
        for (path, title, a) in findings
            .iter()
            .filter(|(_, _, a)| self.proofing_filter.is_none_or(|f| f == a.kind))
        {
            let Some(buffer) = buffers.get(path) else {
                continue;
            };
            let open = self.proofing_selected.as_ref() == Some(&(path.clone(), a.byte_range.start));
            let (before, span, after) = annotations::context(buffer, &a.byte_range, 24);
            let color = annotations::annotation_color(a.kind);
            let frame = egui::Frame::none()
                .fill(tokens::SURFACE_INSET)
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    if open { color } else { tokens::SURFACE_INSET },
                ))
                .rounding(8.0)
                .inner_margin(egui::Margin::same(2.0 * tokens::GRID));
            frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut clicked = false;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icons::PROOFING).color(color));
                    clicked |= ui
                        .add(
                            egui::Label::new(
                                text(&a.title, tokens::UI_LABEL, tokens::TEXT_SECONDARY).strong(),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_text(if sheets > 1 {
                            title.as_str()
                        } else {
                            &a.message
                        })
                        .clicked();
                });
                let mut job = egui::text::LayoutJob::default();
                let font = tokens::UI_CAPTION.font_id();
                let plain = egui::TextFormat::simple(font.clone(), tokens::TEXT_TERTIARY);
                let mut mark = egui::TextFormat::simple(font, tokens::TEXT_SECONDARY);
                mark.background = color.gamma_multiply(0.35);
                job.append(&before, 0.0, plain.clone());
                job.append(&span, 0.0, mark);
                job.append(&after, 0.0, plain);
                clicked |= ui
                    .add(egui::Label::new(job).sense(Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked();
                if open {
                    ui.add_space(tokens::GRID);
                    ui.horizontal(|ui| {
                        if let Some(fix) = &a.fix {
                            if ui
                                .add(
                                    egui::Button::new(text(
                                        "Accept",
                                        tokens::UI_LABEL,
                                        tokens::TEXT_PRIMARY,
                                    ))
                                    .fill(tokens::ACCENT),
                                )
                                .clicked()
                            {
                                accept = Some((path.clone(), a.byte_range.clone(), fix.clone()));
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            dismiss = Some((path.clone(), Self::finding_key(buffer, a)));
                        }
                    });
                }
                if clicked {
                    jump = Some((path.clone(), a.byte_range.clone()));
                }
            });
            ui.add_space(tokens::GRID);
        }
        if let Some((path, range, fix)) = accept {
            self.apply_fix(&path, range, &fix);
            self.proofing_selected = None;
        }
        if let Some(key) = dismiss {
            self.dismissed_findings.insert(key);
            self.proofing_selected = None;
        }
        if let Some((path, range)) = jump {
            self.proofing_selected = Some((path.clone(), range.start));
            if let Some(buffer) = self.sheet_buffer(&path) {
                self.jump_to_finding(ui.ctx(), &path, &buffer, range);
            }
        }
    }

    /// Scrolls `path`'s editor to the finding at `range` (bytes of `buffer`) and selects it.
    fn jump_to_finding(
        &mut self,
        ctx: &egui::Context,
        path: &NodePath,
        buffer: &str,
        range: std::ops::Range<usize>,
    ) {
        self.pending_jump = Some((path.clone(), annotations::paragraph_of(buffer, range.start)));
        let chars = |byte: usize| buffer[..byte].chars().count();
        let id = editor_id(path);
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(chars(range.start)),
            CCursor::new(chars(range.end)),
        )));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        self.active_editor = Some(path.clone());
    }

    // --- Panel 4: Inspector ---------------------------------------------------------------

    fn inspector_panel(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            for tab in InspectorTab::ALL {
                let label = RichText::new(tab.icon()).size(tokens::INSPECTOR_ICON_SIZE);
                if ui
                    .add(egui::SelectableLabel::new(self.inspector_tab == tab, label))
                    .on_hover_text(tab.label())
                    .clicked()
                {
                    self.inspector_tab = tab;
                }
            }
        });
        ui.separator();
        ui.label(text(
            self.inspector_tab.label().to_uppercase(),
            tokens::UI_SECTION,
            tokens::TEXT_QUATERNARY,
        ));
        ui.add_space(2.0 * tokens::GRID);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.inspector_tab {
                InspectorTab::Statistics => self.statistics_tab(ui),
                InspectorTab::Keywords => self.keywords_tab(ui),
                InspectorTab::Outline => self.outline_tab(ui),
                InspectorTab::Links => self.links_tab(ui),
                InspectorTab::Media => self.media_tab(ui),
                InspectorTab::Counter => self.counter_tab(ui),
                InspectorTab::Proofing => self.proofing_tab(ui),
            });
    }

    fn no_target(ui: &mut Ui, what: &str) {
        ui.label(text(what, tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
    }

    /// Statistics & Goals: the shown sheets' figures, the goal of the open sheet (or the
    /// selected container) with its setup form in place, then the library's session chart.
    fn statistics_tab(&mut self, ui: &mut Ui) {
        self.progress_figures(ui);
        ui.add_space(4.0 * tokens::GRID);
        ui.label(text("GOAL", tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
        ui.add_space(tokens::GRID);
        self.goal_section(ui);
        ui.add_space(4.0 * tokens::GRID);
        ui.label(text(
            "SESSIONS",
            tokens::UI_SECTION,
            tokens::TEXT_QUATERNARY,
        ));
        ui.add_space(tokens::GRID);
        self.session_chart(ui);
    }

    fn progress_figures(&self, ui: &mut Ui) {
        let paths = self.visible_sheet_paths();
        let sheets: Vec<&Sheet> = paths.iter().filter_map(|p| self.sheet(p)).collect();
        if sheets.is_empty() {
            Self::no_target(ui, "Open a sheet to see its progress.");
            return;
        }
        let p = inspector::progress(&sheets);
        egui::Grid::new("progress")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("Characters", inspector::format_count(p.characters)),
                    ("Words", inspector::format_count(p.words)),
                    (
                        "Reading time",
                        inspector::format_reading_time(p.reading_seconds),
                    ),
                ] {
                    ui.label(text(label, tokens::UI_LABEL, tokens::TEXT_TERTIARY));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(text(value, tokens::UI_LABEL, tokens::TEXT_PRIMARY));
                    });
                    ui.end_row();
                }
            });
    }

    /// The node whose goal the tab shows: the open sheet, else the selected container.
    fn goal_node(&self) -> Option<NodePath> {
        self.selected.clone().or_else(|| match &self.sidebar {
            Some(SidebarSelection::Node(p)) => Some(p.clone()),
            _ => None,
        })
    }

    fn goal_section(&mut self, ui: &mut Ui) {
        if let Some(target) = self.goal_edit_target.clone() {
            self.goal_form(ui, &target);
            return;
        }
        let Some(path) = self.goal_node() else {
            Self::no_target(
                ui,
                "Open a sheet or select a section or project to set a goal.",
            );
            return;
        };
        let ring = self.state.node(&path).ok().and_then(|node| {
            badges_for(node).into_iter().find_map(|b| match b {
                Badge::GoalRing { fraction, band } => Some((fraction, band)),
                _ => None,
            })
        });
        match ring {
            Some((fraction, band)) => {
                paint_large_goal_ring(ui, fraction, band);
                if ui.small_button("Edit Goal").clicked() {
                    self.handle_intent(UiIntent::OpenGoal(path));
                }
            }
            None => {
                if matches!(self.state.node(&path), Ok(Node::Filter(_))) {
                    Self::no_target(ui, "Filters can't have goals.");
                } else if ui.small_button("Add Goal…").clicked() {
                    self.handle_intent(UiIntent::OpenGoal(path));
                }
            }
        }
    }

    /// The goal setup form, in place in the tab (no dialog).
    fn goal_form(&mut self, ui: &mut Ui, target: &NodePath) {
        let mut close = false;
        egui::Grid::new("goal_fields")
            .num_columns(2)
            .spacing([2.0 * tokens::GRID, tokens::GRID])
            .show(ui, |ui| {
                ui.label(text("Type", tokens::UI_LABEL, tokens::TEXT_TERTIARY));
                egui::ComboBox::from_id_salt("goal_kind")
                    .selected_text(match self.goal_flow.kind {
                        parrot_core::GoalKind::Simple => "Simple",
                        parrot_core::GoalKind::Daily => "Daily",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.goal_flow.kind,
                            parrot_core::GoalKind::Simple,
                            "Simple",
                        );
                        ui.selectable_value(
                            &mut self.goal_flow.kind,
                            parrot_core::GoalKind::Daily,
                            "Daily",
                        );
                    });
                ui.end_row();
                ui.label(text("Counts", tokens::UI_LABEL, tokens::TEXT_TERTIARY));
                egui::ComboBox::from_id_salt("goal_counter")
                    .selected_text(match self.goal_flow.counter {
                        parrot_core::GoalCounter::WordCount => "Words",
                        parrot_core::GoalCounter::CharCount => "Characters",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.goal_flow.counter,
                            parrot_core::GoalCounter::WordCount,
                            "Words",
                        );
                        ui.selectable_value(
                            &mut self.goal_flow.counter,
                            parrot_core::GoalCounter::CharCount,
                            "Characters",
                        );
                    });
                ui.end_row();
                ui.label(text("Target", tokens::UI_LABEL, tokens::TEXT_TERTIARY));
                ui.add(
                    egui::TextEdit::singleline(&mut self.goal_flow.target)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
                ui.label(text("Deadline", tokens::UI_LABEL, tokens::TEXT_TERTIARY));
                ui.add(
                    egui::TextEdit::singleline(&mut self.goal_flow.deadline)
                        .hint_text("YYYY-MM-DD")
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
            });
        ui.checkbox(
            &mut self.goal_flow.write_on_deadline,
            "Lock sheet on deadline",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save").clicked() {
                match self.goal_flow.save_to(&mut self.state, target) {
                    Ok(()) => close = true,
                    Err(e) => self.status = format!("Error setting goal: {e}"),
                }
            }
            if ui.button("Remove").clicked() {
                let _ = self.state.set_goal(target, None);
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
        if close {
            self.goal_edit_target = None;
        }
    }

    fn session_chart(&mut self, ui: &mut Ui) {
        ui.label(text(
            format!(
                "{} words in library",
                inspector::format_count(self.state.total_word_count())
            ),
            tokens::UI_LABEL,
            tokens::TEXT_PRIMARY,
        ));
        ui.add_space(2.0 * tokens::GRID);
        ui.label(text(
            "Words written per session. A session is a manual counter — there's no calendar here.",
            tokens::UI_CAPTION,
            tokens::TEXT_TERTIARY,
        ));
        ui.add_space(2.0 * tokens::GRID);
        let series = self.state.activity.series().to_vec();
        let max = series.iter().map(|p| p.written).max().unwrap_or(0).max(1);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), Sense::hover());
        let bar = (rect.width() / series.len().max(1) as f32).min(16.0);
        for (i, point) in series.iter().enumerate() {
            let h = rect.height() * point.written as f32 / max as f32;
            let x = rect.left() + i as f32 * bar;
            ui.painter().rect_filled(
                Rect::from_min_max(
                    egui::pos2(x + 1.0, rect.bottom() - h),
                    egui::pos2(x + bar - 1.0, rect.bottom()),
                ),
                1.0,
                tokens::ACCENT,
            );
        }
        for point in &series {
            ui.label(text(
                format!("Session {}: {} words", point.day, point.written),
                tokens::UI_CAPTION,
                tokens::TEXT_SECONDARY,
            ));
        }
        ui.add_space(2.0 * tokens::GRID);
        if ui.small_button("Start New Session").clicked() {
            self.state.start_new_day();
        }
    }

    fn keywords_tab(&mut self, ui: &mut Ui) {
        let Some(path) = self.target_sheet() else {
            Self::no_target(ui, "Open a sheet to edit its keywords.");
            return;
        };
        let tags = self
            .sheet(&path)
            .map(inspector::keywords)
            .unwrap_or_default();
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for tag in &tags {
                let chip = egui::Button::new(text(
                    format!("{tag}  {}", icons::REMOVE),
                    tokens::UI_CAPTION,
                    tokens::TEXT_PRIMARY,
                ))
                .fill(tokens::BG_ELEVATED)
                .rounding(tokens::BADGE_RADIUS);
                if ui.add(chip).on_hover_text("Remove keyword").clicked() {
                    remove = Some(tag.clone());
                }
            }
        });
        if let Some(tag) = remove {
            let _ = keyword_manager::remove(&mut self.state, &path, &tag);
        }
        ui.add_space(2.0 * tokens::GRID);
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.keyword_new)
                    .hint_text("Keyword")
                    .desired_width(ui.available_width() - 32.0),
            );
            let submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ready = !self.keyword_new.trim().is_empty();
            if (ui
                .add_enabled(ready, egui::Button::new(icons::ADD))
                .clicked()
                || submit)
                && ready
            {
                let _ = keyword_manager::add(&mut self.state, &path, self.keyword_new.trim());
                self.keyword_new.clear();
            }
        });
        ui.add_space(2.0 * tokens::GRID);
        if ui.small_button("Keyword Manager…").clicked() {
            self.show_keyword_manager = true;
        }
    }

    fn outline_tab(&mut self, ui: &mut Ui) {
        let paths = self.visible_sheet_paths();
        let sheets: Vec<(NodePath, &Sheet)> = paths
            .iter()
            .filter_map(|p| self.sheet(p).map(|s| (p.clone(), s)))
            .collect();
        let entries = inspector::outline(&sheets);
        if entries.is_empty() {
            Self::no_target(ui, "No headings.");
            return;
        }
        let mut jump = None;
        for entry in entries {
            ui.horizontal(|ui| {
                ui.add_space(f32::from(entry.level.saturating_sub(1)) * tokens::INDENT);
                let color = if entry.level == 1 {
                    tokens::TEXT_PRIMARY
                } else {
                    tokens::TEXT_SECONDARY
                };
                if ui
                    .add(
                        egui::Label::new(text(&entry.text, tokens::UI_LABEL, color))
                            .truncate()
                            .sense(Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    jump = Some((entry.sheet.clone(), entry.paragraph));
                }
            });
        }
        if let Some(target) = jump {
            self.pending_jump = Some(target);
        }
    }

    fn links_tab(&mut self, ui: &mut Ui) {
        let paths = self.visible_sheet_paths();
        let sheets: Vec<&Sheet> = paths.iter().filter_map(|p| self.sheet(p)).collect();
        let links = inspector::links(&sheets);
        if links.is_empty() {
            Self::no_target(ui, "No links.");
            return;
        }
        for link in links {
            let response = ui
                .add(
                    egui::Label::new(text(&link.label, tokens::UI_LABEL, tokens::ACCENT_LIGHT))
                        .truncate()
                        .sense(Sense::click()),
                )
                .on_hover_text(format!("{}\nClick to copy the address", link.url));
            ui.label(text(&link.url, tokens::UI_CAPTION, tokens::TEXT_QUATERNARY));
            ui.add_space(tokens::GRID);
            if response.clicked() {
                ui.ctx().copy_text(link.url.clone());
                self.status = format!("Copied {}", link.url);
            }
        }
    }

    fn media_tab(&mut self, ui: &mut Ui) {
        let Some(path) = self.target_sheet() else {
            Self::no_target(ui, "Open a sheet to see its media and attachments.");
            return;
        };
        let entries = self.sheet(&path).map(inspector::media).unwrap_or_default();
        let mut remove = None;
        let mut open = None;
        for entry in &entries {
            ui.horizontal(|ui| {
                ui.label(text(entry.icon(), tokens::UI_LABEL, tokens::TEXT_SECONDARY));
                ui.add(
                    egui::Label::new(text(
                        entry.label(),
                        tokens::UI_LABEL,
                        tokens::TEXT_SECONDARY,
                    ))
                    .truncate(),
                );
                if let MediaEntry::Attachment { index, attachment } = entry {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button(icons::DELETE)
                            .on_hover_text("Remove attachment")
                            .clicked()
                        {
                            remove = Some(*index);
                        }
                        if let parrot_core::Attachment::File { file_name } = attachment {
                            if ui
                                .small_button(icons::OPEN_EXTERNAL)
                                .on_hover_text("Open with its default app")
                                .clicked()
                            {
                                open = Some(file_name.clone());
                            }
                        }
                    });
                }
            });
        }
        if entries.is_empty() {
            Self::no_target(ui, "No images or attachments.");
        }
        if let Some(index) = remove {
            let _ = self.state.remove_attachment(&path, index);
        }
        if let Some(file_name) = open {
            self.open_attachment(&file_name);
        }
        ui.add_space(4.0 * tokens::GRID);
        ui.label(text(
            "ADD ATTACHMENT",
            tokens::UI_SECTION,
            tokens::TEXT_QUATERNARY,
        ));
        ui.add(egui::TextEdit::singleline(&mut self.attachment_note_text).hint_text("Note text"));
        if ui
            .add_enabled(
                !self.attachment_note_text.is_empty(),
                egui::Button::new("Add Note"),
            )
            .clicked()
        {
            let _ =
                crate::attachments::attach_note(&mut self.state, &path, &self.attachment_note_text);
            self.attachment_note_text.clear();
        }
        if ui.button(format!("{} Add File…", icons::BROWSE)).clicked() {
            self.attach_file_via_dialog(&path);
        }
    }

    // --- Dialogs --------------------------------------------------------------------------

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.export_window(ctx);
        self.link_window(ctx);
        self.paragraph_window(ctx);
        self.library_window(ctx);
        self.filter_window(ctx);
        self.keyword_manager_window(ctx);
        self.info_window(ctx);
    }

    fn modal(title: &str) -> egui::Window<'_> {
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        if !self.export.open {
            return;
        }
        let content = self.glued_content(&self.export.targets);
        let mut export_clicked = false;
        let mut browse = false;
        let mut css_browse = false;
        let mut open = true;
        let scope = Self::sheets_label(&self.export.title, self.export.targets.len());
        Self::modal("Export").open(&mut open).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(EXPORT_OPTIONS_WIDTH);
                    self.export_options(
                        ui,
                        &scope,
                        &mut browse,
                        &mut css_browse,
                        &mut export_clicked,
                    );
                });
                ui.separator();
                ui.vertical(|ui| self.export_preview(ui, &content));
            });
        });
        if !open {
            self.export.close();
        }
        if browse {
            self.browse_export_destination();
        }
        if css_browse {
            self.browse_export_css();
        }
        if export_clicked {
            self.run_export();
        }
    }

    /// The export dialog's left column: format, style, page settings, destination and Export.
    fn export_options(
        &mut self,
        ui: &mut Ui,
        scope: &str,
        browse: &mut bool,
        css_browse: &mut bool,
        export_clicked: &mut bool,
    ) {
        ui.label(text(
            format!("Exporting {scope}"),
            tokens::UI_CAPTION,
            tokens::TEXT_TERTIARY,
        ));
        ui.add_space(2.0 * tokens::GRID);
        ui.label(text("FORMAT", tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
        ui.horizontal(|ui| {
            for format in ExportFormat::OFFERED {
                if ui
                    .selectable_label(
                        self.export.format == format,
                        text(format.label(), tokens::UI_LABEL, tokens::TEXT_PRIMARY),
                    )
                    .clicked()
                {
                    self.export.set_format(format);
                }
            }
        });
        if self.export.format == ExportFormat::Pdf {
            ui.add_space(2.0 * tokens::GRID);
            ui.label(text("PAGE", tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
            ui.horizontal_wrapped(|ui| {
                for size in PageSize::ALL {
                    if ui
                        .selectable_label(
                            self.export.page_size == size,
                            text(size.label(), tokens::UI_CAPTION, tokens::TEXT_SECONDARY),
                        )
                        .clicked()
                    {
                        self.export.page_size = size;
                    }
                }
            });
            // An `@page size` in the active style wins over the picker above
            // (preview and export both use it) — say so instead of letting the
            // picker look live when it isn't.
            if let Some(forced) = self.export.active_page_override() {
                ui.label(text(
                    format!(
                        "Style sets {} — page selection ignored",
                        ExportModal::page_override_label(forced)
                    ),
                    tokens::UI_CAPTION,
                    tokens::TEXT_QUATERNARY,
                ));
            }
        }
        ui.add_space(2.0 * tokens::GRID);
        ui.label(text("STYLE", tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
        ui.horizontal(|ui| {
            let current = self
                .export
                .active_style_label()
                .unwrap_or_else(|| "Built-in".to_string());
            egui::ComboBox::from_id_salt("export-style")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for name in crate::export_styles::available_names() {
                        let value = if name == crate::export_styles::BUILT_IN_NAME {
                            None
                        } else {
                            Some(name.to_string())
                        };
                        if ui
                            .selectable_label(
                                self.export.style_name == value
                                    && self.export.custom_css_path.is_none(),
                                name,
                            )
                            .clicked()
                        {
                            self.export.style_name = value;
                            self.export.custom_css_path = None;
                            self.save_preferences();
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            if ui
                .button(text(
                    format!("{} Custom CSS…", crate::icons::FILE),
                    tokens::UI_CAPTION,
                    tokens::TEXT_SECONDARY,
                ))
                .clicked()
            {
                *css_browse = true;
            }
            if self.export.custom_css_path.is_some() {
                ui.label(text(
                    self.export
                        .custom_css_path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map_or_else(
                            || "Custom CSS".to_string(),
                            |n| format!("Custom: {}", n.to_string_lossy()),
                        ),
                    tokens::UI_CAPTION,
                    tokens::TEXT_TERTIARY,
                ));
                if ui
                    .button(text(
                        format!("{} Clear", crate::icons::REMOVE),
                        tokens::UI_CAPTION,
                        tokens::TEXT_SECONDARY,
                    ))
                    .clicked()
                {
                    self.export.custom_css_path = None;
                    self.save_preferences();
                }
            }
        });
        ui.label(text(
            "Plain text ignores styling.",
            tokens::UI_CAPTION,
            tokens::TEXT_QUATERNARY,
        ));
        ui.add_space(2.0 * tokens::GRID);
        ui.label(text("PREVIEW", tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
        let theme = match self.export.theme {
            PreviewTheme::Light => "Light page",
            PreviewTheme::Dark => "Dark page",
        };
        if ui
            .button(text(
                format!("{} {theme}", self.export.theme.icon()),
                tokens::UI_CAPTION,
                tokens::TEXT_SECONDARY,
            ))
            .on_hover_text("Preview only — the exported file is unchanged")
            .clicked()
        {
            self.export.theme = self.export.theme.toggled();
        }
        ui.add_space(2.0 * tokens::GRID);
        ui.label(text("SAVE TO", tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
        ui.add(
            egui::TextEdit::singleline(&mut self.export.destination).desired_width(f32::INFINITY),
        );
        if ui
            .button(text(
                format!("{} Browse…", icons::BROWSE),
                tokens::UI_CAPTION,
                tokens::TEXT_SECONDARY,
            ))
            .clicked()
        {
            *browse = true;
        }
        ui.add_space(4.0 * tokens::GRID);
        let button = egui::Button::new(text(
            "Export",
            tokens::UI_LABEL_STRONG,
            tokens::TEXT_PRIMARY,
        ))
        .fill(tokens::ACCENT)
        .min_size(egui::vec2(ui.available_width(), 7.0 * tokens::GRID));
        *export_clicked = ui
            .add_enabled(!self.export.destination.trim().is_empty(), button)
            .clicked();
    }

    /// The export dialog's live preview: the exact Markdown text, or the PDF's own pages
    /// (same wrapping, sizes and breaks as the file) drawn to scale.
    fn export_preview(&self, ui: &mut Ui, content: &parrot_core::SheetContent) {
        let preview = self.export.preview(content);
        let (page_fill, ink) = match preview.input.theme {
            PreviewTheme::Light => (tokens::PAGE_LIGHT, tokens::PAGE_LIGHT_TEXT),
            PreviewTheme::Dark => (tokens::BG_DEEP, tokens::TEXT_SECONDARY),
        };
        egui::Frame::none()
            .fill(tokens::BG_MID)
            .inner_margin(Margin::same(3.0 * tokens::GRID))
            .show(ui, |ui| {
                ui.set_width(EXPORT_PREVIEW_WIDTH);
                egui::ScrollArea::vertical()
                    .id_salt("export-preview")
                    .max_height(EXPORT_PREVIEW_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| match &preview.body {
                        PreviewBody::Text(markdown) => {
                            egui::Frame::none()
                                .fill(page_fill)
                                .inner_margin(Margin::same(4.0 * tokens::GRID))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add(egui::Label::new(text(
                                        markdown,
                                        tokens::MONO_BLOCK,
                                        ink,
                                    )));
                                });
                        }
                        PreviewBody::Pages(pages) => {
                            let (w_mm, h_mm) = preview.page_mm;
                            let scale = (ui.available_width() / w_mm).max(0.1);
                            for page in pages {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(w_mm * scale, h_mm * scale),
                                    Sense::hover(),
                                );
                                let painter = ui.painter_at(rect);
                                painter.rect_filled(rect, 0.0, page_fill);
                                for line in page {
                                    let size = line.size_pt * scale / PT_PER_MM;
                                    let family = if line.bold {
                                        egui::FontFamily::Name(tokens::FAMILY_BOLD.into())
                                    } else {
                                        egui::FontFamily::Proportional
                                    };
                                    let color = line
                                        .color
                                        .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
                                        .unwrap_or(ink);
                                    painter.text(
                                        rect.left_top()
                                            + egui::vec2(line.left_mm * scale, line.top_mm * scale),
                                        Align2::LEFT_BOTTOM,
                                        &line.text,
                                        egui::FontId::new(size, family),
                                        color,
                                    );
                                }
                                ui.add_space(2.0 * tokens::GRID);
                            }
                        }
                    });
            });
    }

    fn link_window(&mut self, ctx: &egui::Context) {
        let Some(mut url) = self.link_url.take() else {
            return;
        };
        let mut insert = false;
        let mut cancel = false;
        Self::modal("Insert Link").show(ctx, |ui| {
            ui.label(text("Address", tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
            let response = ui.add(egui::TextEdit::singleline(&mut url).desired_width(320.0));
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                insert = true;
            }
            ui.horizontal(|ui| {
                insert |= ui.button("Insert").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if insert {
            self.apply_format(ctx, |e| toolbar::link(e, url.trim()));
        } else if !cancel {
            self.link_url = Some(url);
        }
    }

    fn paragraph_window(&mut self, ctx: &egui::Context) {
        if !self.show_paragraph_picker {
            return;
        }
        let mut chosen = None;
        let mut open = true;
        Self::modal("Paragraph Style")
            .open(&mut open)
            .show(ctx, |ui| {
                for style in ParagraphStyle::ALL {
                    if ui.button(style.label()).clicked() {
                        chosen = Some(style);
                    }
                }
            });
        if let Some(style) = chosen {
            self.apply_format(ctx, |e| toolbar::set_paragraph_style(e, style));
            open = false;
        }
        self.show_paragraph_picker = open;
    }

    fn library_window(&mut self, ctx: &egui::Context) {
        if !self.show_library_dialog {
            return;
        }
        let mut open = true;
        let mut action = None;
        #[cfg(target_os = "macos")]
        let mut icloud_reveal = false;
        Self::modal("Library").open(&mut open).show(ctx, |ui| {
            ui.label(text(
                "Library folder",
                tokens::UI_CAPTION,
                tokens::TEXT_TERTIARY,
            ));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.library_path_input).desired_width(360.0),
                );
                if ui.button(format!("{} Browse…", icons::BROWSE)).clicked() {
                    action = Some(None);
                }
            });
            ui.horizontal(|ui| {
                let ready = !self.library_path_input.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("New Library"))
                    .clicked()
                {
                    action = Some(Some(true));
                }
                if ui
                    .add_enabled(ready, egui::Button::new("Open Library"))
                    .clicked()
                {
                    action = Some(Some(false));
                }
            });
            #[cfg(target_os = "macos")]
            {
                ui.separator();
                ui.label(text(
                    "iCloud library (this Mac only)",
                    tokens::UI_CAPTION,
                    tokens::TEXT_TERTIARY,
                ));
                let icloud_label = self
                    .state
                    .icloud_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Not set up".to_string());
                ui.label(text(
                    icloud_label,
                    tokens::UI_LABEL,
                    tokens::TEXT_SECONDARY,
                ));
                ui.horizontal(|ui| {
                    if ui.button("Use Default iCloud Folder").clicked() {
                        if let Some(dir) = crate::icloud::default_icloud_library_dir() {
                            std::fs::create_dir_all(&dir).ok();
                            self.state.icloud_path = Some(dir.clone());
                            self.remember_library();
                            let _ = self.state.save();
                            self.status = format!("iCloud library at {}", dir.display());
                        } else {
                            self.status =
                                "iCloud Drive not available on this Mac.".to_string();
                        }
                    }
                    if ui.button("Reveal in Finder").clicked() {
                        icloud_reveal = true;
                    }
                });
                ui.label(text(
                    "Projects under the iCloud heading save here and sync via iCloud Drive. No account needed beyond iCloud Drive.",
                    tokens::UI_CAPTION,
                    tokens::TEXT_TERTIARY,
                ));
            }
        });
        match action {
            Some(Some(true)) => {
                self.create_library();
                open = false;
            }
            Some(Some(false)) => {
                self.open_library();
                open = false;
            }
            Some(None) => {
                self.browse_library_folder();
            }
            None => {}
        }
        #[cfg(target_os = "macos")]
        if icloud_reveal {
            if let Some(dir) = self.state.icloud_path.clone() {
                let _ = self.os.reveal_path(&dir);
            }
        }
        self.show_library_dialog = open;
    }

    fn filter_window(&mut self, ctx: &egui::Context) {
        let Some(parent) = self.filter_parent.clone() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        Self::modal("New Filter").open(&mut open).show(ctx, |ui| {
            egui::Grid::new("filter_fields")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, field) in [
                        ("Title", &mut self.filter_builder.title),
                        ("Text contains", &mut self.filter_builder.keyword),
                        ("Keyword", &mut self.filter_builder.keyword_tag),
                    ] {
                        ui.label(text(label, tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
                        ui.text_edit_singleline(field);
                        ui.end_row();
                    }
                });
            save = ui.button("Save Filter").clicked();
        });
        if save {
            match self.filter_builder.save_to(&mut self.state, &parent) {
                Ok(()) => {
                    self.filter_builder = FilterBuilder::default();
                    open = false;
                }
                Err(e) => self.status = format!("Error saving filter: {e}"),
            }
        }
        if !open {
            self.filter_parent = None;
        }
    }

    fn keyword_manager_window(&mut self, ctx: &egui::Context) {
        if !self.show_keyword_manager {
            return;
        }
        let mut open = true;
        Self::modal("Keyword Manager")
            .open(&mut open)
            .show(ctx, |ui| {
                let all = keyword_manager::list_all(&self.state);
                if all.is_empty() {
                    ui.label(text(
                        "No keywords in this library yet.",
                        tokens::UI_CAPTION,
                        tokens::TEXT_TERTIARY,
                    ));
                }
                for keyword in all {
                    ui.horizontal(|ui| {
                        ui.label(&keyword);
                        if ui
                            .small_button(icons::RENAME)
                            .on_hover_text("Rename everywhere")
                            .clicked()
                        {
                            self.keyword_rename_target = Some(keyword.clone());
                            self.keyword_rename_buffer = keyword.clone();
                        }
                        if ui
                            .small_button(icons::DELETE)
                            .on_hover_text("Delete everywhere")
                            .clicked()
                        {
                            keyword_manager::delete(&mut self.state, &keyword);
                        }
                    });
                }
                if let Some(target) = self.keyword_rename_target.clone() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!("Rename \"{target}\" to:"));
                        ui.text_edit_singleline(&mut self.keyword_rename_buffer);
                        let new_name = self.keyword_rename_buffer.trim().to_string();
                        if ui
                            .add_enabled(!new_name.is_empty(), egui::Button::new("Save"))
                            .clicked()
                        {
                            keyword_manager::rename(&mut self.state, &target, &new_name);
                            self.keyword_rename_target = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.keyword_rename_target = None;
                        }
                    });
                }
            });
        if !open {
            self.show_keyword_manager = false;
            self.keyword_rename_target = None;
        }
    }

    fn info_window(&mut self, ctx: &egui::Context) {
        let Some(path) = self.info_target.clone() else {
            return;
        };
        let Ok(node) = self.state.node(&path) else {
            self.info_target = None;
            return;
        };
        let (kind, title) = match node {
            Node::Sheet(s) => ("Sheet", s.title.clone()),
            Node::Group(g) => ("Section", g.title.clone()),
            Node::Project(p) => ("Project", p.title.clone()),
            Node::Filter(f) => ("Filter", f.title.clone()),
        };
        let words = crate::goal_ui::count_units(node, parrot_core::GoalCounter::WordCount);
        let chars = crate::goal_ui::count_units(node, parrot_core::GoalCounter::CharCount);
        let mut rows = vec![
            ("Kind", kind.to_string()),
            ("Words", inspector::format_count(words)),
            ("Characters", inspector::format_count(chars)),
        ];
        match node {
            Node::Sheet(s) => {
                rows.push(("Keywords", s.keywords.join(", ")));
                rows.push(("Attachments", s.attachments.len().to_string()));
            }
            Node::Group(g) => rows.push(("Items", g.children.len().to_string())),
            Node::Project(p) => rows.push(("Items", p.children.len().to_string())),
            Node::Filter(f) => rows.push((
                "Matches",
                parrot_core::filter::evaluate(f, &self.state.library)
                    .len()
                    .to_string(),
            )),
        }
        let mut open = true;
        Self::modal(&title).open(&mut open).show(ctx, |ui| {
            egui::Grid::new("info")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for (label, value) in &rows {
                        ui.label(text(*label, tokens::UI_CAPTION, tokens::TEXT_TERTIARY));
                        ui.label(text(value, tokens::UI_LABEL, tokens::TEXT_PRIMARY));
                        ui.end_row();
                    }
                });
        });
        if !open {
            self.info_target = None;
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(panel_frame(tokens::BG_PANEL).inner_margin(Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.confirm_empty_trash {
                        ui.label(text(
                            "Permanently delete everything in the Trash? This cannot be undone.",
                            tokens::UI_CAPTION,
                            tokens::TEXT_PRIMARY,
                        ));
                        if ui.small_button("Empty Trash").clicked() {
                            self.state.empty_trash();
                            self.confirm_empty_trash = false;
                            self.status = "Trash emptied.".to_string();
                        }
                        if ui.small_button("Cancel").clicked() {
                            self.confirm_empty_trash = false;
                        }
                        return;
                    }
                    // Error-prefixed status text gets the error color (T-9-03 convention).
                    if let Some(name) = self.state.library_path.as_ref().and_then(|p| p.file_name())
                    {
                        ui.label(text(
                            name.to_string_lossy(),
                            tokens::UI_CAPTION,
                            tokens::TEXT_SECONDARY,
                        ));
                        ui.label(text("·", tokens::UI_CAPTION, tokens::TEXT_QUATERNARY));
                    }
                    let error = self.status.starts_with("Error");
                    let color = if error {
                        tokens::ERROR
                    } else {
                        tokens::TEXT_TERTIARY
                    };
                    let shown = if self.unsaved_changes && !error {
                        UNSAVED_STATUS
                    } else {
                        self.status.as_str()
                    };
                    ui.label(text(shown, tokens::UI_CAPTION, color));
                    let exported = self.status.starts_with("Exported")
                        || self.status.starts_with("Quick-exported");
                    if exported
                        && self.last_export_path.is_some()
                        && ui.small_button("Show in Folder").clicked()
                    {
                        self.reveal_last_export();
                    }
                    if let Some(path) = &self.state.library_path {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(text(
                                path.display().to_string(),
                                tokens::UI_CAPTION,
                                tokens::TEXT_QUATERNARY,
                            ));
                        });
                    }
                });
            });
    }
}

/// Shelf card size: a portrait "book" (pending the token pass).
const SHELF_CARD: egui::Vec2 = egui::vec2(168.0, 216.0);

/// One project on the shelf: a spine stripe on the left, the title, its figures, and the goal
/// ring when the project has a goal.
fn shelf_card(ui: &mut Ui, item: &crate::shelf::ShelfItem) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(SHELF_CARD, Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if response.hovered() {
        tokens::BG_ELEVATED
    } else {
        tokens::BG_MID
    };
    painter.rect_filled(rect, tokens::SELECTION_RADIUS, fill);
    let spine = Rect::from_min_max(
        rect.left_top(),
        rect.left_bottom() + egui::vec2(2.0 * tokens::GRID, 0.0),
    );
    painter.rect_filled(
        spine,
        egui::Rounding {
            nw: tokens::SELECTION_RADIUS,
            sw: tokens::SELECTION_RADIUS,
            ..Default::default()
        },
        tokens::ACCENT,
    );
    let inner = rect.shrink(4.0 * tokens::GRID);
    let inner = Rect::from_min_max(
        inner.left_top() + egui::vec2(2.0 * tokens::GRID, 0.0),
        inner.right_bottom(),
    );
    let title = ui.fonts(|f| {
        f.layout(
            item.title.clone(),
            tokens::UI_HEADER.font_id(),
            tokens::TEXT_PRIMARY,
            inner.width(),
        )
    });
    painter.galley(inner.left_top(), title, tokens::TEXT_PRIMARY);
    let summary = ui.fonts(|f| {
        f.layout(
            item.summary(),
            tokens::UI_CAPTION.font_id(),
            tokens::TEXT_TERTIARY,
            inner.width(),
        )
    });
    let summary_top = inner.bottom() - summary.size().y;
    painter.galley(
        egui::pos2(inner.left(), summary_top),
        summary,
        tokens::TEXT_TERTIARY,
    );
    if let Some((fraction, band)) = item.goal {
        let center = egui::pos2(
            inner.right() - tokens::GOAL_RING_DIAMETER / 2.0,
            summary_top - 2.0 * tokens::GRID - tokens::GOAL_RING_DIAMETER / 2.0,
        );
        crate::badges::paint_goal_ring(&painter, center, fraction, band);
    }
    response.on_hover_text(format!("Open {}", item.title))
}

/// The shelf's last slot: creates a project.
fn new_project_card(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(SHELF_CARD, Sense::click());
    let painter = ui.painter_at(rect);
    let color = if response.hovered() {
        tokens::TEXT_SECONDARY
    } else {
        tokens::TEXT_QUATERNARY
    };
    painter.rect_stroke(
        rect.shrink(0.5),
        tokens::SELECTION_RADIUS,
        egui::Stroke::new(1.0_f32, tokens::DIVIDER),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_BOTTOM,
        icons::ADD,
        tokens::HEADING_2.font_id(),
        color,
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 2.0 * tokens::GRID),
        Align2::CENTER_TOP,
        "New Project",
        tokens::UI_LABEL.font_id(),
        color,
    );
    response
}

/// §6.1 large goal ring: track, clockwise progress arc from 12 o'clock, percentage inside.
fn paint_large_goal_ring(ui: &mut Ui, fraction: f32, band: crate::badges::GoalBand) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 80.0), Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    painter.circle_stroke(
        center,
        31.0,
        egui::Stroke::new(2.5_f32, tokens::GOAL_INCOMPLETE),
    );
    let color = match band {
        crate::badges::GoalBand::Incomplete => tokens::TEXT_TERTIARY,
        crate::badges::GoalBand::Almost => tokens::GOAL_ALMOST,
        crate::badges::GoalBand::Complete => tokens::GOAL_COMPLETE,
    };
    if fraction > 0.0 {
        let steps = 64;
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
                center + 31.0 * egui::vec2(a.cos(), a.sin())
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(4.0_f32, color)));
    }
    painter.text(
        center,
        Align2::CENTER_CENTER,
        format!("{:.0}%", fraction * 100.0),
        tokens::UI_LABEL_STRONG.font_id(),
        tokens::TEXT_PRIMARY,
    );
}

/// The counter's stats panel: four labelled groups of figures in a two-column grid.
fn counter_stats(ui: &mut Ui, s: &char_counter::TextStats) {
    let n = |v: usize| inspector::format_count(u32::try_from(v).unwrap_or(u32::MAX));
    let f = |v: f32| format!("{v:.2}");
    let groups: [(&str, Vec<(&str, String)>); 4] = [
        (
            "COUNTS",
            vec![
                ("Characters (including spaces)", n(s.characters)),
                ("Characters (without spaces)", n(s.characters_no_spaces)),
                ("Words", n(s.words)),
                ("Alphanumeric characters", n(s.alphanumeric)),
                ("Alpha characters", n(s.alpha)),
                ("Numeric characters", n(s.numeric)),
                ("Sentences", n(s.sentences)),
                ("Paragraphs", n(s.paragraphs)),
                ("Unique words", n(s.unique_words)),
            ],
        ),
        (
            "SYLLABLES & LENGTH",
            vec![
                ("Syllables", n(s.syllables)),
                ("Monosyllabic words (1 syllable)", n(s.monosyllabic_words)),
                ("Polysyllabic words (≥3 syllables)", n(s.polysyllabic_words)),
                ("Syllables per word", f(s.syllables_per_word)),
                ("Short words (≤3 characters)", n(s.short_words)),
                ("Long words (≥7 characters)", n(s.long_words)),
                ("Average word length (characters)", f(s.average_word_length)),
                (
                    "Average sentence length (words)",
                    f(s.average_sentence_length),
                ),
            ],
        ),
        (
            "READABILITY",
            vec![
                ("Difficult words (approx.)", n(s.difficult_words)),
                ("Dale-Chall score (approx.)", f(s.dale_chall_score)),
                (
                    "Dale-Chall level (approx.)",
                    char_counter::dale_chall_level(s.dale_chall_score, s.words).to_string(),
                ),
            ],
        ),
        (
            "TIME",
            vec![
                (
                    "Reading time (200 wpm)",
                    char_counter::format_duration(s.reading_seconds),
                ),
                (
                    "Speaking time (125 wpm)",
                    char_counter::format_duration(s.speaking_seconds),
                ),
            ],
        ),
    ];
    for (title, rows) in groups {
        ui.label(text(title, tokens::UI_SECTION, tokens::TEXT_QUATERNARY));
        ui.add_space(tokens::GRID);
        for (label, value) in rows {
            stat_row(ui, label, &value, tokens::TEXT_PRIMARY);
        }
        ui.add_space(3.0 * tokens::GRID);
    }
}

/// One counter figure in the narrow Inspector: label left, value right-aligned.
fn stat_row(ui: &mut Ui, label: &str, value: &str, color: Color32) -> egui::Response {
    ui.horizontal(|ui| {
        let label = ui.add(egui::Label::new(text(
            label,
            tokens::UI_CAPTION,
            tokens::TEXT_TERTIARY,
        )));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(text(value, tokens::UI_LABEL, color));
        });
        label
    })
    .inner
}

/// Per-platform limits against the current character count; over-limit rows turn red.
fn counter_limits(ui: &mut Ui, characters: usize) {
    ui.label(text(
        "PLATFORM LIMITS",
        tokens::UI_SECTION,
        tokens::TEXT_QUATERNARY,
    ));
    ui.add_space(tokens::GRID);
    for limit in char_counter::PLATFORM_LIMITS {
        let color = if characters > limit.limit {
            tokens::ERROR
        } else {
            tokens::TEXT_PRIMARY
        };
        let value = format!(
            "{} / {}",
            inspector::format_count(u32::try_from(characters).unwrap_or(u32::MAX)),
            inspector::format_count(u32::try_from(limit.limit).unwrap_or(u32::MAX))
        );
        let label = stat_row(ui, limit.name, &value, color);
        if let Some(note) = limit.note {
            label.on_hover_text(note);
        }
    }
}

/// Output of one sheet's editor.
struct SheetEditorOutput {
    changed: bool,
    has_focus: bool,
    /// "Split Sheet" was chosen in the right-click menu: the selection, as char indices.
    split: Option<(usize, usize)>,
}

/// §12.4: the centered inset column — 640px max text width plus side margins, filled with
/// `SURFACE_INSET`. `content` gets the text width.
fn text_column(ui: &mut Ui, content: impl FnOnce(&mut Ui, f32)) {
    let width = editor::text_column_width(ui.available_width());
    ui.vertical_centered(|ui| {
        ui.add_space(4.0 * tokens::GRID);
        egui::Frame::none()
            .fill(tokens::SURFACE_INSET)
            .inner_margin(Margin::symmetric(
                editor::SIDE_MARGIN,
                editor::COLUMN_PADDING_Y,
            ))
            .show(ui, |ui| {
                ui.set_width(width);
                ui.with_layout(Layout::top_down(Align::LEFT), |ui| content(ui, width));
            });
        ui.add_space(4.0 * tokens::GRID);
    });
}

/// Runs the note-editor key behaviours (`editor_keys`: Enter, Tab, Backspace, Space) on the
/// focused editor before its text field sees the keys. Returns whether the buffer changed.
fn apply_editor_keys(ctx: &egui::Context, id: Id, buffer: &mut String) -> bool {
    use egui::Event;
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let Some(range) = state.cursor.char_range() else {
        return false;
    };
    let selection = (range.secondary.index, range.primary.index);
    let text: &str = buffer;
    let edit = ctx.input_mut(|i| {
        // Pasted Markdown comes in line by line: give each heading, item and quote its block.
        for event in i.events.iter_mut() {
            if let Event::Paste(pasted) = event {
                if pasted.contains('\n') {
                    *pasted = crate::markdown_blocks::normalize_blocks(pasted);
                }
            }
        }
        for index in 0..i.events.len() {
            let plain = |m: &egui::Modifiers| !m.command && !m.alt && !m.ctrl;
            let result = match &i.events[index] {
                Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if plain(modifiers) && !modifiers.shift => editor_keys::enter(text, selection),
                Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if plain(modifiers) => Some(editor_keys::tab(text, selection, modifiers.shift)),
                Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    modifiers,
                    ..
                } if plain(modifiers) && !modifiers.shift => {
                    editor_keys::backspace(text, selection)
                }
                Event::Text(typed) if typed == " " => editor_keys::space(text, selection),
                _ => None,
            };
            if result.is_some() {
                i.events.remove(index);
                return result;
            }
        }
        None
    });
    let Some(edit) = edit else {
        return false;
    };
    *buffer = edit.text;
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(edit.anchor),
        CCursor::new(edit.head),
    )));
    state.store(ctx, id);
    true
}

/// Draws list bullets/numbers and divider rules over the invisible Markdown markers.
fn paint_block_decorations(
    ui: &Ui,
    output: &egui::text_edit::TextEditOutput,
    buffer: &str,
    editing: Option<&std::ops::Range<usize>>,
) {
    let painter = ui.painter();
    let column = output.response.rect.x_range();
    for (byte, decoration) in editor::block_decorations(buffer) {
        // The paragraph being edited shows its own marker instead.
        if editing.is_some_and(|r| r.start <= byte && byte <= r.end) {
            continue;
        }
        let char_index = buffer[..byte].chars().count();
        let rect = output
            .galley
            .pos_from_ccursor(CCursor::new(char_index))
            .translate(output.galley_pos.to_vec2());
        match decoration {
            editor::BlockDecoration::Rule => {
                painter.hline(
                    column,
                    rect.center().y,
                    egui::Stroke::new(1.0_f32, tokens::TEXT_QUATERNARY),
                );
            }
            editor::BlockDecoration::Bullet(_) | editor::BlockDecoration::Number(_) => {
                let label = match decoration {
                    editor::BlockDecoration::Number(n) => n,
                    editor::BlockDecoration::Bullet(level) => {
                        editor::BlockDecoration::bullet_glyph(level).to_string()
                    }
                    editor::BlockDecoration::Rule => unreachable!(),
                };
                painter.text(
                    egui::pos2(rect.left() - 3.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    label,
                    tokens::BODY.font_id(),
                    tokens::TEXT_SECONDARY,
                );
            }
        }
    }
}

/// A new sheet's title from its text: the first line without its block prefix, cut to a
/// reasonable length.
fn split_title(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    let line = line.trim_start_matches(|c: char| {
        matches!(c, '#' | '>' | '*' | '%' | '`') || c.is_whitespace()
    });
    let title: String = line.chars().take(40).collect();
    if title.trim().is_empty() {
        sidebar::NEW_SHEET_TITLE.to_string()
    } else {
        title.trim().to_string()
    }
}

/// One sheet's styled `TextEdit` plus its §12.10 paragraph numbers; scrolls to paragraph
/// `jump` (an Outline click) when given.
fn sheet_editor(
    ui: &mut Ui,
    id: Id,
    buffer: &mut String,
    style: &EditorStyle,
    width: f32,
    jump: Option<usize>,
    highlight_all: bool,
) -> SheetEditorOutput {
    // Live preview: the paragraph the caret is in shows its Markdown, the rest shows formatted.
    let mut style = style.clone();
    if !style.show_markdown && ui.memory(|m| m.has_focus(id)) {
        style.reveal = egui::text_edit::TextEditState::load(ui.ctx(), id)
            .and_then(|s| s.cursor.char_range())
            .and_then(|r| {
                let caret = buffer
                    .chars()
                    .take(r.primary.index)
                    .map(char::len_utf8)
                    .sum();
                editor::paragraph_at(buffer, caret)
            });
    }
    let style = &style;
    let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
        let job = editor::layout_job(text, style, wrap_width);
        ui.fonts(|f| f.layout_job(job))
    };
    // egui collapses the selection on any press, the right button included — which would
    // leave nothing to split — so remember it before the editor sees a right-click.
    let ctx = ui.ctx().clone();
    let selection_before_press = if ctx.input(|i| i.pointer.secondary_pressed()) {
        egui::text_edit::TextEditState::load(&ctx, id).and_then(|s| s.cursor.char_range())
    } else {
        None
    };
    let edited_by_key = ctx.memory(|m| m.has_focus(id)) && apply_editor_keys(&ctx, id, buffer);
    let output = egui::TextEdit::multiline(buffer)
        .id(id)
        .lock_focus(true)
        .frame(false)
        .desired_width(width)
        .desired_rows(3)
        .hint_text("Start writing…")
        .layouter(&mut layouter)
        .show(ui);

    // Right-click on a selection offers to split the sheet around it; with the whole sheet
    // selected there's nothing to split, so the entry is disabled.
    let mut split = None;
    let mut selection = output
        .cursor_range
        .map(|r| (r.primary.ccursor.index, r.secondary.ccursor.index))
        .filter(|(a, b)| a != b);
    if let Some(range) = selection_before_press.filter(|r| r.primary != r.secondary) {
        if output.response.hovered() {
            let mut state = output.state.clone();
            state.cursor.set_char_range(Some(range));
            state.store(&ctx, id);
            selection = Some((range.primary.index, range.secondary.index));
        }
    }
    if let Some((a, b)) = selection {
        let can_split = editor::split_around_selection(buffer, a, b).is_some();
        output.response.context_menu(|ui| {
            if ui
                .add_enabled(can_split, egui::Button::new("Split Sheet"))
                .on_disabled_hover_text("The whole sheet is selected")
                .clicked()
            {
                split = Some((a, b));
                ui.close_menu();
            }
        });
    }

    let painter = ui.painter();
    let numbers = editor::paragraph_gutter_labels(buffer);
    let spans = editor::paragraph_spans(buffer);
    for (i, (byte, label)) in numbers.iter().enumerate() {
        let char_index = buffer[..*byte].chars().count();
        let row = output.galley.pos_from_ccursor(CCursor::new(char_index));
        let rect = row.translate(output.galley_pos.to_vec2());
        painter.text(
            egui::pos2(
                output.galley_pos.x - editor::PARAGRAPH_NUMBER_OFFSET,
                rect.top() + 2.0,
            ),
            Align2::RIGHT_TOP,
            label,
            tokens::UI_CAPTION.font_id(),
            tokens::TEXT_QUATERNARY,
        );
        if jump == Some(i) && i < spans.len() {
            ui.scroll_to_rect(rect, Some(Align::TOP));
        }
    }
    if !style.show_markdown {
        paint_block_decorations(ui, &output, buffer, style.reveal.as_ref());
    }
    if highlight_all {
        // Selected along with the other sheets by Ctrl+A: highlight every row of text.
        let color = ui.visuals().selection.bg_fill.gamma_multiply(0.5);
        for row in &output.galley.rows {
            let rect = row.rect.translate(output.galley_pos.to_vec2());
            painter.rect_filled(rect, 0.0, color);
        }
    }
    SheetEditorOutput {
        changed: output.response.changed() || edited_by_key,
        has_focus: output.response.has_focus(),
        split,
    }
}

impl eframe::App for ParrotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_ready {
            // Fonts take effect next frame; drawing now could reference the unregistered bold
            // family, so this one frame is skipped.
            setup(ctx);
            self.fonts_ready = true;
            ctx.request_repaint();
            return;
        }
        self.frame_ui(ctx);
    }
}

impl ParrotApp {
    /// One frame of the whole window — everything `update` draws once fonts are in place.
    fn frame_ui(&mut self, ctx: &egui::Context) {
        self.drop_rows.clear();
        self.track_changes(ctx);
        self.follow_system_scroll_speed(ctx);
        if let Some(text) = self.pending_clipboard.take() {
            ctx.copy_text(text);
        }
        if let Some(last_edit) = self.unsaved_since_edit {
            if !self.autosave_if_idle(std::time::Instant::now()) {
                ctx.request_repaint_after(AUTOSAVE_IDLE.saturating_sub(last_edit.elapsed()));
            }
        }
        self.handle_shortcuts(ctx);
        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.dialogs(ctx);

        let sidebar = self.layout.is_visible(Panel::Library);
        // The shelf has no sheet to list or inspect, so it takes the whole width.
        let shelf = self.on_shelf();
        egui::SidePanel::left("library")
            .default_width(LIBRARY_WIDTH)
            .width_range(LIBRARY_WIDTH * 0.75..=LIBRARY_WIDTH * 2.0)
            .frame(panel_frame(tokens::BG_PANEL))
            .show_animated(ctx, sidebar, |ui| self.library_panel(ui));
        egui::SidePanel::left("sheet_list")
            .default_width(SHEET_LIST_WIDTH)
            .width_range(SHEET_LIST_WIDTH * 0.75..=SHEET_LIST_WIDTH * 1.6)
            .frame(panel_frame(tokens::BG_MID))
            .show_animated(
                ctx,
                self.layout.is_visible(Panel::SheetList) && !shelf,
                |ui| self.sheet_list_panel(ui),
            );
        egui::SidePanel::right("inspector")
            .default_width(INSPECTOR_WIDTH)
            .width_range(INSPECTOR_WIDTH * 0.8..=INSPECTOR_WIDTH * 1.6)
            .frame(panel_frame(tokens::BG_PANEL))
            .show_animated(
                ctx,
                self.layout.is_visible(Panel::Inspector) && !shelf,
                |ui| self.inspector_panel(ui),
            );
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(tokens::BG_DEEP))
            .show(ctx, |ui| self.editor_panel(ui));
        self.finish_drag(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs whole frames of the real UI headlessly — every panel, with a populated library —
    /// to catch panics (font families, slicing, borrow logic) no unit test would reach.
    #[test]
    fn full_frames_render_headlessly() {
        let ctx = egui::Context::default();
        let mut app = ParrotApp::default();
        let dir = tempfile::tempdir().expect("tempdir");
        app.library_path_input = dir.path().join("lib").display().to_string();
        // `eframe::App::update` needs an `eframe::Frame`, which only exists inside a running
        // native loop; `frame_ui` is everything it draws.
        let frame = |app: &mut ParrotApp, ctx: &egui::Context| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        };
        setup(&ctx);
        frame(&mut app, &ctx); // fonts apply from here on
        frame(&mut app, &ctx); // welcome screen
        app.create_library();
        frame(&mut app, &ctx); // empty shelf
        app.state.create_project("Novel");
        app.state.create_group(&[0], "Celeste").expect("group");
        app.state.create_sheet(&[0, 0], "Finais").expect("sheet");
        app.state.create_sheet(&[0, 0], "Materiais").expect("sheet");
        app.state
            .set_sheet_content(
                &[0, 0, 0],
                editor::buffer_to_content(
                    "# Title\n\nSome **bold** [link](https://x.y) tpyo.\n\n## Part",
                ),
            )
            .expect("content");
        app.focus_mode = true;
        for tab in InspectorTab::ALL {
            app.inspector_tab = tab;
            app.select_sidebar(SidebarSelection::Node(vec![0, 0]));
            frame(&mut app, &ctx); // sheet flow
            app.open_sheet(vec![0, 0, 0]);
            frame(&mut app, &ctx); // single sheet
        }
        app.open_export();
        assert!(app.export.open);
        for format in ExportFormat::ALL {
            app.export.set_format(format);
            frame(&mut app, &ctx);
        }
        app.show_markdown = true;
        app.link_url = Some("https://example.org".to_string());
        app.show_paragraph_picker = true;
        app.info_target = Some(vec![0, 0]);
        app.show_keyword_manager = true;
        app.handle_intent(UiIntent::OpenGoal(vec![0, 0]));
        app.handle_intent(UiIntent::OpenFilterBuilder);
        frame(&mut app, &ctx);
        app.run_context_action(&vec![0, 0, 1], ContextAction::MoveToTrash);
        app.select_sidebar(SidebarSelection::Smart(SmartFilter::Trash));
        frame(&mut app, &ctx);
        assert_eq!(app.state.trash.len(), 1);
        // Several sheets, several library nodes, and the counter over them.
        app.select_sidebar(SidebarSelection::Node(vec![0, 0]));
        app.sheet_selection = vec![vec![0, 0, 0]];
        app.click_sheet_row(&vec![0, 0, 0], &[vec![0, 0, 0]], egui::Modifiers::NONE);
        app.inspector_tab = InspectorTab::Counter;
        for kind in FrequencyKind::ALL {
            app.counter.kind = kind;
            frame(&mut app, &ctx);
        }
        app.select_sidebar(SidebarSelection::Nodes(vec![vec![0, 0], vec![0, 1]]));
        frame(&mut app, &ctx);
    }

    #[test]
    fn toolbar_format_round_trips_through_text_edit_state() {
        let ctx = egui::Context::default();
        let mut app = ParrotApp::default();
        app.state.create_project("P");
        app.state.create_sheet(&[0], "S").expect("sheet");
        app.state
            .set_sheet_content(&[0, 0], editor::buffer_to_content("make this bold"))
            .expect("content");
        app.open_sheet(vec![0, 0]);
        let id = editor_id(&vec![0, 0]);
        let mut te = egui::text_edit::TextEditState::default();
        te.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(9),
            secondary: CCursor::new(5),
        }));
        te.store(&ctx, id);
        app.dispatch_toolbar(&ctx, ToolbarAction::Bold);
        assert_eq!(app.editor_buffer, "make **this** bold");
        let Ok(Node::Sheet(sheet)) = app.state.node(&[0, 0]) else {
            panic!("sheet");
        };
        assert_eq!(
            sheet.content,
            editor::buffer_to_content("make **this** bold")
        );
        app.dispatch_toolbar(&ctx, ToolbarAction::ToggleSidebar);
        assert!(!app.layout.is_visible(Panel::Library));
        assert!(!app.layout.is_visible(Panel::SheetList));
        assert!(app.layout.is_visible(Panel::Inspector));
    }

    /// A library with project `P[Section "G"[Sheet "A", Sheet "B"], Sheet "C"]`, saved to a tempdir.
    fn populated_app() -> (ParrotApp, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut app = ParrotApp {
            library_path_input: dir.path().join("lib").display().to_string(),
            ..ParrotApp::default()
        };
        app.create_library();
        app.state.create_project("P");
        app.state.create_group(&[0], "G").expect("group");
        app.state.create_sheet(&[0, 0], "A").expect("sheet");
        app.state.create_sheet(&[0, 0], "B").expect("sheet");
        app.state.create_sheet(&[0], "C").expect("sheet");
        app.state
            .set_sheet_content(&[0, 0, 0], editor::buffer_to_content("alpha text"))
            .expect("content");
        (app, dir)
    }

    fn sheet_title(app: &ParrotApp, path: &[usize]) -> String {
        match app.state.node(path) {
            Ok(Node::Sheet(s)) => s.title.clone(),
            Ok(Node::Group(g)) => g.title.clone(),
            other => panic!("unexpected node {other:?}"),
        }
    }

    /// Runs `handle_shortcuts` on one frame whose input is a single Ctrl+`key` press.
    fn press_ctrl(app: &mut ParrotApp, ctx: &egui::Context, key: egui::Key) {
        let input = egui::RawInput {
            modifiers: egui::Modifiers::COMMAND,
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.handle_shortcuts(ctx));
    }

    fn select(ctx: &egui::Context, path: &NodePath, anchor: usize, head: usize) {
        let mut te = egui::text_edit::TextEditState::default();
        te.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(head),
            secondary: CCursor::new(anchor),
        }));
        te.store(ctx, editor_id(path));
    }

    #[test]
    fn keyboard_shortcuts_drive_real_key_events() {
        let ctx = egui::Context::default();
        let (mut app, dir) = populated_app();
        app.select_sidebar(SidebarSelection::Node(vec![0, 0, 0]));
        assert_eq!(app.selected, Some(vec![0, 0, 0]));

        press_ctrl(&mut app, &ctx, egui::Key::ArrowDown);
        assert_eq!(
            app.selected,
            Some(vec![0, 0, 1]),
            "Ctrl+Down opens the next sheet"
        );
        press_ctrl(&mut app, &ctx, egui::Key::ArrowUp);
        assert_eq!(
            app.selected,
            Some(vec![0, 0, 0]),
            "Ctrl+Up opens the previous sheet"
        );

        select(&ctx, &vec![0, 0, 0], 0, 5);
        press_ctrl(&mut app, &ctx, egui::Key::B);
        assert_eq!(app.editor_buffer, "**alpha** text");
        select(&ctx, &vec![0, 0, 0], 2, 7);
        press_ctrl(&mut app, &ctx, egui::Key::B);
        assert_eq!(
            app.editor_buffer, "alpha text",
            "Ctrl+B toggles bold back off"
        );
        select(&ctx, &vec![0, 0, 0], 6, 10);
        press_ctrl(&mut app, &ctx, egui::Key::I);
        assert_eq!(app.editor_buffer, "alpha *text*");

        // Ctrl+N adds a sheet where new nodes go: inside the open sheet's section.
        let parent = app.creation_parent().expect("in a project");
        assert_eq!(parent, vec![0, 0]);
        let before = app.state.child_count(&parent).expect("section");
        press_ctrl(&mut app, &ctx, egui::Key::N);
        assert_eq!(app.state.child_count(&parent), Ok(before + 1));

        press_ctrl(&mut app, &ctx, egui::Key::S);
        assert_eq!(app.status, "Saved.");
        let reopened = parrot_container::tree::open_library(&dir.path().join("lib"))
            .expect("saved library reopens");
        let Node::Project(project) = &reopened.children[0] else {
            panic!("project");
        };
        let Node::Group(section) = &project.children[0] else {
            panic!("section");
        };
        assert_eq!(
            section.children.len(),
            before + 1,
            "the new sheet was saved"
        );
    }

    #[test]
    fn every_toolbar_action_has_its_effect() {
        let ctx = egui::Context::default();
        let (mut app, _dir) = populated_app();
        app.open_sheet(vec![0, 0, 0]);

        assert!(!app.show_markdown, "formatted text by default");
        app.dispatch_toolbar(&ctx, ToolbarAction::ToggleMarkup);
        assert!(app.show_markdown);
        app.dispatch_toolbar(&ctx, ToolbarAction::ToggleMarkup);
        assert!(!app.show_markdown);

        app.dispatch_toolbar(&ctx, ToolbarAction::ToggleEditorOnly);
        assert!(app.layout.editor_only);
        assert!(!app.layout.is_visible(Panel::Library));
        app.dispatch_toolbar(&ctx, ToolbarAction::ToggleEditorOnly);
        assert!(app.layout.is_visible(Panel::Library));

        let inspector = app.layout.is_visible(Panel::Inspector);
        app.dispatch_toolbar(&ctx, ToolbarAction::ToggleInspector);
        assert_ne!(app.layout.is_visible(Panel::Inspector), inspector);

        select(&ctx, &vec![0, 0, 0], 0, 5);
        app.dispatch_toolbar(&ctx, ToolbarAction::Italic);
        assert_eq!(app.editor_buffer, "*alpha* text");

        app.dispatch_toolbar(&ctx, ToolbarAction::Link);
        assert_eq!(app.link_url.as_deref(), Some("https://"));
        app.dispatch_toolbar(&ctx, ToolbarAction::ParagraphStyle);
        assert!(app.show_paragraph_picker);

        // Quick Export before any export falls through to the full modal.
        app.dispatch_toolbar(&ctx, ToolbarAction::QuickExport);
        assert!(app.export.open);
        app.export.close();
        app.dispatch_toolbar(&ctx, ToolbarAction::Export);
        assert!(app.export.open);
        assert_eq!(app.export.targets, vec![vec![0, 0, 0]]);
    }

    #[test]
    fn formatting_without_an_editor_reports_instead_of_editing() {
        let ctx = egui::Context::default();
        let (mut app, _dir) = populated_app();
        app.dispatch_toolbar(&ctx, ToolbarAction::Bold);
        assert_eq!(app.status, "Click into a sheet first.");
        app.dispatch_toolbar(&ctx, ToolbarAction::Export);
        assert!(!app.export.open);
        assert!(app.status.starts_with("Nothing to export"));
    }

    #[test]
    fn link_and_paragraph_style_apply_to_the_selection() {
        let ctx = egui::Context::default();
        let (mut app, _dir) = populated_app();
        app.open_sheet(vec![0, 0, 0]);
        select(&ctx, &vec![0, 0, 0], 0, 5);
        app.apply_format(&ctx, |e| toolbar::link(e, "https://x.y"));
        assert_eq!(app.editor_buffer, "[alpha](https://x.y) text");
        app.apply_format(&ctx, |e| {
            toolbar::set_paragraph_style(e, ParagraphStyle::ALL[1])
        });
        assert_ne!(app.editor_buffer, "[alpha](https://x.y) text");
        assert!(app.editor_buffer.ends_with("[alpha](https://x.y) text"));
    }

    #[test]
    fn context_menu_structural_actions_mutate_and_reset_paths() {
        let (mut app, _dir) = populated_app();
        app.select_sidebar(SidebarSelection::Node(vec![0, 0, 1]));
        app.collapsed.insert(vec![0, 0]);
        app.info_target = Some(vec![0, 0, 1]);

        app.run_context_action(&vec![0, 0, 0], ContextAction::Duplicate);
        assert_eq!(
            sheet_title(&app, &[0, 0, 1]),
            "A",
            "copy lands right after the original"
        );
        assert_eq!(app.selected, None, "positional selection is dropped");
        assert!(
            app.collapsed.is_empty(),
            "positional collapse state is dropped"
        );
        assert_eq!(app.info_target, None, "dialogs on shifted paths close");

        app.run_context_action(&vec![0, 0, 0], ContextAction::NewSheet);
        app.run_context_action(&vec![0, 0], ContextAction::NewGroup);
        let Ok(Node::Group(g)) = app.state.node(&[0, 0]) else {
            panic!("group");
        };
        assert_eq!(g.children.len(), 5, "A, A copy, B, new sheet, new group");

        app.run_context_action(&vec![0, 1], ContextAction::MoveToTrash);
        assert_eq!(app.state.trash.len(), 1);
        assert_eq!(app.status, "Moved to Trash.");
    }

    #[test]
    fn context_menu_dialog_actions_open_their_dialogs() {
        let (mut app, _dir) = populated_app();
        app.run_context_action(&vec![0, 0, 0], ContextAction::Rename);
        assert_eq!(app.rename, Some((vec![0, 0, 0], "A".to_string())));
        app.run_context_action(&vec![0, 0, 0], ContextAction::AddGoal);
        assert_eq!(app.goal_edit_target, Some(vec![0, 0, 0]));
        app.run_context_action(&vec![0, 0, 0], ContextAction::EditKeywords);
        assert_eq!(app.inspector_tab, InspectorTab::Keywords);
        assert!(app.layout.is_visible(Panel::Inspector));
        app.run_context_action(&vec![0, 0, 0], ContextAction::GetInfo);
        assert_eq!(app.info_target, Some(vec![0, 0, 0]));
        app.run_context_action(&vec![0, 0, 0], ContextAction::Export);
        assert!(app.export.open);
        app.run_context_action(&vec![0, 0], ContextAction::NewFilter);
        assert_eq!(app.filter_parent, Some(Vec::new()));
    }

    #[test]
    fn editing_goal_prefills_the_existing_goal() {
        let (mut app, _dir) = populated_app();
        app.goal_flow.target = "750".to_string();
        app.goal_flow
            .save_to(&mut app.state, &[0, 0, 0])
            .expect("goal");
        app.goal_flow = GoalSetupFlow::default();
        app.handle_intent(UiIntent::OpenGoal(vec![0, 0, 0]));
        assert_eq!(app.goal_flow.target, "750");
    }

    #[test]
    fn export_writes_the_file_and_quick_export_repeats_it() {
        let (mut app, dir) = populated_app();
        app.open_sheet(vec![0, 0, 0]);
        app.open_export();
        app.export.set_format(ExportFormat::ALL[0]);
        let dest = dir.path().join("out").join("first");
        std::fs::create_dir_all(dest.parent().expect("parent")).expect("mkdir");
        app.export.destination = dest
            .with_extension(app.export.format.extension())
            .display()
            .to_string();
        app.run_export();
        assert!(app.status.starts_with("Exported"), "{}", app.status);
        assert!(!app.export.open);
        assert!(PathBuf::from(&app.export.destination).is_file());

        app.open_sheet(vec![0, 0, 1]);
        app.quick_export();
        assert!(app.status.starts_with("Quick-exported"), "{}", app.status);
        assert!(dir
            .path()
            .join("out")
            .join(format!("B.{}", app.export.format.extension()))
            .is_file());
    }

    #[test]
    fn new_library_refuses_to_overwrite_an_existing_one() {
        let (mut app, dir) = populated_app();
        app.save();
        assert_eq!(app.status, "Saved.");
        app.create_library();
        assert!(app.status.starts_with("Error"), "{}", app.status);
        assert_eq!(
            app.state.library.children.len(),
            1,
            "library left untouched"
        );
        app.open_library();
        assert!(app.status.starts_with("Opened"), "{}", app.status);
        assert_eq!(app.state.library.children.len(), 1);
        assert!(dir
            .path()
            .join("lib")
            .join("P.ulproject")
            .join("C.parrot")
            .is_dir());
    }

    #[test]
    fn trash_does_not_leak_into_another_library() {
        let (mut app, dir) = populated_app();
        app.run_context_action(&vec![0, 1], ContextAction::MoveToTrash);
        assert_eq!(app.state.trash.len(), 1);
        app.library_path_input = dir.path().join("other").display().to_string();
        app.create_library();
        assert!(app.state.trash.is_empty());
    }

    #[test]
    fn renaming_then_saving_does_not_resurrect_the_old_name() {
        let (mut app, dir) = populated_app();
        app.save();
        app.state.rename_node(&[0, 1], "Renamed").expect("rename");
        app.run_context_action(&vec![0, 0, 1], ContextAction::MoveToTrash);
        app.save();
        assert_eq!(app.status, "Saved.");
        app.open_library();
        let Ok(Node::Group(g)) = app.state.node(&[0, 0]) else {
            panic!("group");
        };
        assert_eq!(g.children.len(), 1, "trashed sheet stays gone");
        assert_eq!(sheet_title(&app, &[0, 1]), "Renamed");
        assert_eq!(app.state.library.children.len(), 1);
        assert!(dir.path().join("lib").join(".removed").is_dir());
    }

    #[test]
    fn saving_with_a_filter_succeeds() {
        let (mut app, _dir) = populated_app();
        app.handle_intent(UiIntent::OpenFilterBuilder);
        app.filter_builder.title = "Has alpha".to_string();
        app.filter_builder.keyword = "alpha".to_string();
        app.filter_builder
            .save_to(&mut app.state, &[0])
            .expect("filter");
        app.save();
        assert_eq!(app.status, "Saved.");
    }

    #[test]
    fn keyword_tab_and_manager_operations() {
        let (mut app, _dir) = populated_app();
        app.open_sheet(vec![0, 0, 0]);
        keyword_manager::add(&mut app.state, &[0, 0, 0], "draft").expect("add");
        keyword_manager::add(&mut app.state, &[0, 0, 1], "draft").expect("add");
        keyword_manager::rename(&mut app.state, "draft", "final");
        assert_eq!(keyword_manager::list_all(&app.state), ["final"]);
        keyword_manager::delete(&mut app.state, "final");
        assert!(keyword_manager::list_all(&app.state).is_empty());
    }

    fn with_fake_os(
        app: &mut ParrotApp,
        answers: impl IntoIterator<Item = Option<PathBuf>>,
    ) -> std::rc::Rc<crate::os::FakeOs> {
        let os = std::rc::Rc::new(crate::os::FakeOs::answering(answers));
        app.os = Box::new(os.clone());
        os
    }

    #[test]
    fn library_dialogs_create_and_open_from_the_picked_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib = dir.path().join("picked");
        let mut app = ParrotApp::default();
        let os = with_fake_os(&mut app, [None, Some(lib.clone()), Some(lib.clone())]);

        app.new_library_via_dialog();
        assert_eq!(
            app.state.library_path, None,
            "cancelled picker changes nothing"
        );
        app.new_library_via_dialog();
        assert_eq!(app.state.library_path.as_deref(), Some(lib.as_path()));
        app.state.create_project("P");
        app.state.create_sheet(&[0], "S").expect("sheet");
        app.save();

        app.state = AppState::new();
        app.open_library_via_dialog();
        assert!(app.status.starts_with("Opened"), "{}", app.status);
        assert_eq!(app.state.library.children.len(), 1);
        assert_eq!(os.calls.borrow().len(), 3);
    }

    #[test]
    fn export_browse_sets_destination_and_reveal_shows_it() {
        let (mut app, dir) = populated_app();
        let chosen = dir.path().join("picked-name");
        let os = with_fake_os(&mut app, [Some(chosen.clone())]);
        app.open_sheet(vec![0, 0, 0]);
        app.open_export();
        app.browse_export_destination();
        let ext = app.export.format.extension();
        assert_eq!(
            PathBuf::from(&app.export.destination),
            chosen.with_extension(ext),
            "the format's extension is added"
        );
        assert!(os.calls.borrow()[0].starts_with("save_file: Export As"));
        app.run_export();
        assert!(chosen.with_extension(ext).is_file());
        assert_eq!(app.last_export_path, Some(chosen.with_extension(ext)));
        app.reveal_last_export();
        assert_eq!(
            os.calls.borrow().last().map(String::as_str),
            Some(format!("reveal: {}", chosen.with_extension(ext).display()).as_str())
        );
    }

    #[test]
    fn attachments_come_from_a_file_picker_and_open_externally() {
        let (mut app, dir) = populated_app();
        let file = dir.path().join("reference.pdf");
        std::fs::write(&file, b"%PDF").expect("write");
        let os = with_fake_os(&mut app, [None, Some(file.clone())]);

        app.attach_file_via_dialog(&vec![0, 0, 0]);
        assert!(
            !app.state.has_attachments(&[0, 0, 0]),
            "cancel attaches nothing"
        );
        app.attach_file_via_dialog(&vec![0, 0, 0]);
        assert!(app.state.has_attachments(&[0, 0, 0]));
        let name = file.display().to_string();
        app.open_attachment(&name);
        assert_eq!(
            os.calls.borrow().last().map(String::as_str),
            Some(format!("open: {name}").as_str())
        );
        std::fs::remove_file(&file).expect("rm");
        app.open_attachment(&name);
        assert!(app.status.starts_with("Error"), "{}", app.status);
    }

    #[test]
    fn import_dialog_adds_sheets_where_new_nodes_go() {
        let (mut app, dir) = populated_app();
        let md = dir.path().join("Imported Draft.md");
        std::fs::write(&md, "# Hello\n\nWorld").expect("write");
        let os = with_fake_os(&mut app, [None, Some(md.clone())]);
        app.select_sidebar(SidebarSelection::Node(vec![0, 0]));

        app.import_via_dialog();
        let Ok(Node::Group(g)) = app.state.node(&[0, 0]) else {
            panic!("group");
        };
        assert_eq!(g.children.len(), 2, "cancel imports nothing");
        app.import_via_dialog();
        assert_eq!(app.status, format!("Imported {}", md.display()));
        assert_eq!(sheet_title(&app, &[0, 0, 2]), "Imported Draft");
        assert!(os.calls.borrow()[0].starts_with("pick_file: Import"));
    }

    const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
    const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;

    /// [`populated_app`] plus `G/Nested[D]` and two more root sheets: the tree is
    /// `G[A, B, Nested[D]], C, E, F`.
    fn bigger_app() -> (ParrotApp, tempfile::TempDir) {
        let (mut app, dir) = populated_app();
        app.state.create_group(&[0, 0], "Nested").expect("nested");
        app.state.create_sheet(&[0, 0, 2], "D").expect("D");
        app.state.create_sheet(&[0], "E").expect("E");
        app.state.create_sheet(&[0], "F").expect("F");
        app.state
            .set_sheet_content(&[0, 0, 2, 0], editor::buffer_to_content("deep words here"))
            .expect("content");
        (app, dir)
    }

    fn export_to(app: &mut ParrotApp, dir: &std::path::Path, name: &str) -> String {
        app.open_export();
        assert!(app.export.open, "{}", app.status);
        app.export.set_format(ExportFormat::PlainText);
        app.export.destination = dir.join(format!("{name}.txt")).display().to_string();
        app.run_export();
        assert!(app.status.starts_with("Exported"), "{}", app.status);
        std::fs::read_to_string(dir.join(format!("{name}.txt"))).expect("exported")
    }

    #[test]
    fn sheet_list_supports_ctrl_shift_and_ctrl_shift_clicks() {
        let (mut app, dir) = bigger_app();
        app.select_sidebar(SidebarSelection::Smart(SmartFilter::All));
        let order = sheet_list::listed_paths(
            &app.state,
            &SidebarSelection::Smart(SmartFilter::All),
            &app.today,
        );
        assert_eq!(order.len(), 6);
        app.click_sheet_row(&order[0], &order, egui::Modifiers::NONE);
        assert_eq!(
            app.selected,
            Some(order[0].clone()),
            "plain click opens one sheet"
        );
        app.click_sheet_row(&order[2], &order, CTRL);
        assert_eq!(
            app.sheet_selection,
            vec![order[0].clone(), order[2].clone()]
        );
        assert_eq!(app.selected, None, "several sheets: no single open sheet");
        assert_eq!(app.visible_sheet_paths(), app.sheet_selection);
        app.click_sheet_row(&order[4], &order, SHIFT);
        assert_eq!(
            app.sheet_selection,
            order[2..=4].to_vec(),
            "range from the anchor"
        );
        app.click_sheet_row(&order[0], &order, CTRL | SHIFT);
        assert_eq!(app.sheet_selection, order[0..=4].to_vec(), "range added");
        app.click_sheet_row(&order[1], &order, CTRL);
        assert!(!app.sheet_selection.contains(&order[1]), "ctrl toggles off");

        // The editor shows them one after another; Export glues exactly these.
        let ctx = egui::Context::default();
        setup(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert_eq!(app.container_buffers.len(), app.sheet_selection.len());
        app.click_sheet_row(&order[0], &order, egui::Modifiers::NONE);
        app.click_sheet_row(&order[3], &order, CTRL);
        let exported = export_to(&mut app, dir.path(), "two");
        assert!(exported.contains("alpha text"), "{exported}");
        assert!(!exported.contains("deep words"), "{exported}");
        assert!(app.status.contains("2 sheets"), "{}", app.status);
    }

    #[test]
    fn several_groups_show_and_export_their_full_content() {
        let (mut app, dir) = bigger_app();
        // G (with its nested group) and C, by Ctrl-click in the tree.
        app.click_tree_node(&vec![0, 0], egui::Modifiers::NONE);
        app.click_tree_node(&vec![0, 1], CTRL);
        assert_eq!(
            app.sidebar,
            Some(SidebarSelection::Nodes(vec![vec![0, 0], vec![0, 1]]))
        );
        assert_eq!(
            app.visible_sheet_paths(),
            vec![vec![0, 0, 0], vec![0, 0, 1], vec![0, 0, 2, 0], vec![0, 1]]
        );
        let exported = export_to(&mut app, dir.path(), "groups");
        assert!(exported.contains("alpha text") && exported.contains("deep words"));
        assert!(app.export.title.contains("G + C"), "{}", app.export.title);
        // Shift extends over the visible tree rows; Ctrl toggles back to one node.
        app.click_tree_node(&vec![0, 3], SHIFT);
        let Some(SidebarSelection::Nodes(nodes)) = &app.sidebar else {
            panic!("expected several nodes");
        };
        assert_eq!(nodes.first(), Some(&vec![0, 1]));
        assert_eq!(nodes.last(), Some(&vec![0, 3]));
        app.click_tree_node(&vec![0, 1], egui::Modifiers::NONE);
        app.click_tree_node(&vec![0, 2], CTRL);
        app.click_tree_node(&vec![0, 2], CTRL);
        // C sits directly in the project, so its project lists with C selected (NAV-05).
        assert_eq!(app.sidebar, Some(SidebarSelection::Node(vec![0])));
    }

    /// Drives real pointer events: press on `from`, drag past egui's threshold, move to `to`,
    /// release — what a mouse drag does.
    fn drag(app: &mut ParrotApp, ctx: &egui::Context, from: egui::Pos2, to: egui::Pos2) {
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.frame_ui(ctx));
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(vec![egui::Event::PointerMoved(from)]);
        run(vec![button(from, true)]);
        run(vec![egui::Event::PointerMoved(
            from + egui::vec2(0.0, 12.0),
        )]);
        run(vec![egui::Event::PointerMoved(to)]);
        run(vec![button(to, false)]);
        run(Vec::new());
    }

    /// A painted row's rect: the sheet list's (`in_list`, right of the library panel) or the
    /// library tree's.
    fn row_rect(app: &ParrotApp, path: &[usize], in_list: bool) -> Rect {
        app.drop_rows
            .iter()
            .filter(|r| r.path == path && (r.rect.left() > LIBRARY_WIDTH * 0.75) == in_list)
            .map(|r| r.rect)
            .next()
            .unwrap_or_else(|| panic!("no row for {path:?}"))
    }

    #[test]
    fn dragging_tree_rows_moves_nodes_into_groups_and_reorders() {
        let (mut app, _dir) = bigger_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let render = |app: &mut ParrotApp| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.frame_ui(ctx));
        };
        render(&mut app);
        render(&mut app);
        // C (root) into the middle of G: appended to G.
        let c = row_rect(&app, &[0, 1], false);
        let g = row_rect(&app, &[0, 0], false);
        drag(&mut app, &ctx, c.center(), g.center());
        assert_eq!(sheet_title(&app, &[0, 0, 3]), "C", "{}", app.status);
        assert_eq!(app.state.child_count(&[0]).expect("root"), 3);

        // F dropped on the top edge of E: reordered before it.
        render(&mut app);
        let f = row_rect(&app, &[0, 2], false);
        let e = row_rect(&app, &[0, 1], false);
        drag(
            &mut app,
            &ctx,
            f.center(),
            e.left_top() + egui::vec2(20.0, 2.0),
        );
        assert_eq!(sheet_title(&app, &[0, 1]), "F");
        assert_eq!(sheet_title(&app, &[0, 2]), "E");

        // A group dropped into itself is refused and changes nothing.
        render(&mut app);
        let g = row_rect(&app, &[0, 0], false);
        let nested = row_rect(&app, &[0, 0, 2], false);
        drag(&mut app, &ctx, g.center(), nested.center());
        assert_eq!(sheet_title(&app, &[0, 0, 2]), "Nested");
    }

    #[test]
    fn dragging_selected_sheet_list_rows_moves_them_together() {
        let (mut app, _dir) = bigger_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let render = |app: &mut ParrotApp| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.frame_ui(ctx));
        };
        // G lists A and B: drag B above A in the sheet list.
        app.click_tree_node(&vec![0, 0], egui::Modifiers::NONE);
        render(&mut app);
        render(&mut app);
        let a = row_rect(&app, &[0, 0, 0], true);
        let b = row_rect(&app, &[0, 0, 1], true);
        drag(
            &mut app,
            &ctx,
            b.center(),
            a.left_top() + egui::vec2(20.0, 2.0),
        );
        assert_eq!(sheet_title(&app, &[0, 0, 0]), "B", "{}", app.status);
        assert_eq!(
            app.sidebar,
            Some(SidebarSelection::Node(vec![0, 0])),
            "still on G"
        );

        // Select both sheets and drag them onto a root sheet in the tree: they leave G.
        let order = vec![vec![0, 0, 0], vec![0, 0, 1]];
        app.click_sheet_row(&order[0], &order, egui::Modifiers::NONE);
        app.click_sheet_row(&order[1], &order, SHIFT);
        render(&mut app);
        let from = row_rect(&app, &[0, 0, 1], true);
        let e = row_rect(&app, &[0, 2], false);
        drag(
            &mut app,
            &ctx,
            from.center(),
            e.left_bottom() + egui::vec2(20.0, -2.0),
        );
        assert_eq!(
            app.state.child_count(&[0, 0]).expect("G"),
            1,
            "only Nested left"
        );
        assert_eq!(sheet_title(&app, &[0, 3]), "B");
        assert_eq!(sheet_title(&app, &[0, 4]), "A");
        assert!(
            app.sheet_selection.is_empty(),
            "moved out of the listed group"
        );
    }

    #[test]
    fn reordering_survives_save_and_reopen() {
        let (mut app, _dir) = bigger_app();
        app.drop_nodes(&[vec![0, 3]], &[0], 0);
        app.save();
        app.open_library();
        assert_eq!(sheet_title(&app, &[0, 0]), "F");
        assert_eq!(sheet_title(&app, &[0, 1]), "G");
    }

    #[test]
    fn character_counter_counts_the_selection_else_the_open_sheets() {
        let (mut app, _dir) = bigger_app();
        assert_eq!(InspectorTab::Counter.icon(), icons::CHARACTER_COUNTER);
        assert!(!sidebar::root_rows(&app.state.library)
            .iter()
            .any(|r| matches!(r, SidebarRow::Heading("Tools"))));
        let ctx = egui::Context::default();
        app.select_sidebar(SidebarSelection::Nodes(vec![vec![0, 0], vec![0, 1]]));
        let (text, scope) = app.counter_source(&ctx);
        assert_eq!(scope, char_counter::CountScope::Sheets(4));
        assert_eq!(char_counter::analyze(&text).words, 5, "{text:?}");

        app.open_sheet(vec![0, 0, 0]);
        assert_eq!(
            app.counter_source(&ctx).1,
            char_counter::CountScope::Sheets(1)
        );
        select(&ctx, &vec![0, 0, 0], 0, 5);
        let (text, scope) = app.counter_source(&ctx);
        assert_eq!(
            (text.as_str(), scope),
            ("alpha", char_counter::CountScope::Selection)
        );
        // The tab paints and caches the figures.
        app.inspector_tab = InspectorTab::Counter;
        setup(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert_eq!(app.counter.text(), "alpha");
        assert_eq!(app.counter.stats().words, 1);
    }

    #[test]
    fn spelling_and_grammar_findings_live_in_the_inspector() {
        let (mut app, _dir) = bigger_app();
        assert_eq!(InspectorTab::Proofing.icon(), icons::PROOFING);
        let ctx = egui::Context::default();
        setup(&ctx);
        app.open_sheet(vec![0, 0, 2, 0]);
        app.editor_buffer = "Fine start.\n\nthe qwzx here.".to_string();
        app.commit_buffer(&vec![0, 0, 2, 0]);
        let findings = app.proofing_findings();
        assert!(findings.iter().all(|(p, _, _)| p == &vec![0, 0, 2, 0]));
        let buffer = app.sheet_buffer(&vec![0, 0, 2, 0]).expect("buffer");
        let found: Vec<Annotation> = findings.into_iter().map(|(_, _, a)| a).collect();
        assert!(found.iter().any(|a| a.kind == AnnotationKind::Spelling));
        assert!(found.iter().any(|a| a.kind == AnnotationKind::Grammar));

        // The footer's findings mark opens the tab, which paints the findings.
        app.show_inspector_tab(InspectorTab::Proofing);
        assert_eq!(app.inspector_tab, InspectorTab::Proofing);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));

        // Clicking a finding selects it in the editor and scrolls to its paragraph.
        let qwzx = found
            .iter()
            .find(|a| &buffer[a.byte_range.clone()] == "qwzx")
            .expect("qwzx flagged")
            .byte_range
            .clone();
        app.jump_to_finding(&ctx, &vec![0, 0, 2, 0], &buffer, qwzx);
        assert_eq!(app.pending_jump, Some((vec![0, 0, 2, 0], 1)));
        let range = egui::text_edit::TextEditState::load(&ctx, editor_id(&vec![0, 0, 2, 0]))
            .and_then(|s| s.cursor.char_range())
            .expect("selection");
        assert_eq!((range.secondary.index, range.primary.index), (17, 21));

        // Several sheets: one merged list in editor order; checking off empties the tab.
        app.select_sidebar(SidebarSelection::Nodes(vec![vec![0, 0], vec![0, 1]]));
        let order = app.visible_sheet_paths();
        let merged = app.proofing_findings();
        let expected: usize = order
            .iter()
            .filter_map(|p| app.sheet_buffer(p))
            .map(|b| annotations::check(&b).len())
            .sum();
        assert_eq!(merged.len(), expected);
        let rank = |p: &NodePath| order.iter().position(|q| q == p);
        assert!(merged.windows(2).all(|w| rank(&w[0].0) <= rank(&w[1].0)));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        app.text_checking = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert!(!app.text_checking, "the tab only offers the toggle");
    }

    #[test]
    fn opening_a_sheet_reveals_it_inside_its_group_in_the_tree() {
        let (mut app, _dir) = bigger_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        app.collapsed.insert(vec![0, 0]);
        app.collapsed.insert(vec![0, 0, 2]);
        // Open the nested sheet D from the All list: G and Nested expand to show it.
        app.select_sidebar(SidebarSelection::Smart(SmartFilter::All));
        let order = sheet_list::listed_paths(
            &app.state,
            &SidebarSelection::Smart(SmartFilter::All),
            &app.today,
        );
        app.click_sheet_row(&vec![0, 0, 2, 0], &order, egui::Modifiers::NONE);
        assert!(app.collapsed.is_empty(), "{:?}", app.collapsed);
        assert_eq!(app.tree_highlight(&vec![0, 0, 2, 0]), Highlight::Selected);
        assert_eq!(app.tree_highlight(&vec![0, 0]), Highlight::None);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert!(
            app.drop_rows.iter().any(|r| r.path == vec![0, 0, 2, 0]),
            "the sheet's tree row is painted"
        );
        assert_eq!(app.reveal_in_tree, None, "scrolled to once");

        // Opening a sheet of the selected group: the sheet is the selection, the group its
        // container.
        app.click_tree_node(&vec![0, 0], egui::Modifiers::NONE);
        assert_eq!(app.tree_highlight(&vec![0, 0]), Highlight::Selected);
        app.open_sheet(vec![0, 0, 1]);
        assert_eq!(app.tree_highlight(&vec![0, 0, 1]), Highlight::Selected);
        assert_eq!(app.tree_highlight(&vec![0, 0]), Highlight::Container);
        assert_eq!(app.tree_highlight(&vec![0, 0, 0]), Highlight::None);
        // Several selected sheets are all marked.
        let order = vec![vec![0, 0, 0], vec![0, 0, 1]];
        app.click_sheet_row(&vec![0, 0, 0], &order, CTRL);
        assert_eq!(app.tree_highlight(&vec![0, 0, 0]), Highlight::Selected);
        assert_eq!(app.tree_highlight(&vec![0, 0, 1]), Highlight::Selected);
    }

    #[test]
    fn clicking_a_grouped_sheet_in_the_tree_lists_its_group() {
        let (mut app, _dir) = bigger_app();
        app.click_tree_node(&vec![0, 0, 2, 0], egui::Modifiers::NONE);
        assert_eq!(app.sidebar, Some(SidebarSelection::Node(vec![0, 0, 2])));
        assert_eq!(app.selected, Some(vec![0, 0, 2, 0]));
        assert_eq!(app.sheet_selection, vec![vec![0, 0, 2, 0]]);
        assert_eq!(app.tree_highlight(&vec![0, 0, 2, 0]), Highlight::Selected);
        assert_eq!(app.tree_highlight(&vec![0, 0, 2]), Highlight::Container);
        // Ctrl-click adds to the clicked sheet, not to its listed group.
        app.click_tree_node(&vec![0, 0, 0], CTRL);
        assert_eq!(
            app.sidebar,
            Some(SidebarSelection::Nodes(vec![
                vec![0, 0, 0],
                vec![0, 0, 2, 0]
            ]))
        );
    }

    #[test]
    fn glued_sheets_open_together_from_the_tree_and_the_sheet_list() {
        let (mut app, _dir) = bigger_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        app.select_sidebar(SidebarSelection::Node(vec![0, 0]));
        let order = vec![vec![0, 0, 0], vec![0, 0, 1]];
        app.click_sheet_row(&order[0], &order, egui::Modifiers::NONE);
        app.click_sheet_row(&order[1], &order, CTRL);
        app.run_glue(GlueAction::Glue(app.sheet_selection.clone()));
        assert_eq!(app.state.glue_run(&[0, 0, 0]), order, "{}", app.status);

        // Clicking either sheet, in the tree or the list, opens both with it focused.
        app.click_tree_node(&vec![0, 0, 1], egui::Modifiers::NONE);
        assert_eq!(app.sidebar, Some(SidebarSelection::Node(vec![0, 0])));
        assert_eq!(app.sheet_selection, order);
        assert_eq!(app.active_editor, Some(vec![0, 0, 1]));
        assert_eq!(app.selected, None);
        assert!(app.open_glue_run(&vec![0, 0, 0]));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert_eq!(
            app.container_buffers.len(),
            2,
            "both sheets are in the editor"
        );
        app.click_sheet_row(&order[0], &order, egui::Modifiers::NONE);
        assert_eq!(app.sheet_selection, order);
        assert_eq!(app.active_editor, Some(vec![0, 0, 0]));

        // Unglued, a click opens just the one sheet again.
        app.run_glue(GlueAction::Unglue(order.clone()));
        assert_eq!(app.state.glue_run(&[0, 0, 1]), vec![vec![0, 0, 1]]);
        app.click_sheet_row(&order[1], &order, egui::Modifiers::NONE);
        assert_eq!(app.selected, Some(vec![0, 0, 1]));
        assert_eq!(app.sheet_selection, vec![vec![0, 0, 1]]);
    }

    #[test]
    fn the_last_library_reopens_on_the_next_launch() {
        let (mut app, dir) = populated_app();
        let file = dir.path().join("config").join("last-library.txt");
        app.recent_library_file = Some(file.clone());
        app.save();
        app.open_library();
        assert!(file.is_file(), "{}", app.status);

        // A fresh launch with the same config file opens that library straight away.
        let mut next = ParrotApp {
            recent_library_file: Some(file),
            ..ParrotApp::default()
        };
        next.reopen_last_library();
        assert_eq!(next.state.library_path, app.state.library_path);
        assert_eq!(next.state.library.children.len(), 1, "the one project");
        assert!(!crate::first_run::should_prompt(&next.state));

        // Without a remembered library the app starts at the welcome screen as before.
        let mut fresh = ParrotApp::default();
        fresh.reopen_last_library();
        assert!(crate::first_run::should_prompt(&fresh.state));
    }

    fn key(key: egui::Key) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn autosave_waits_for_a_minute_without_typing() {
        let (mut app, _dir) = populated_app();
        let start = std::time::Instant::now();
        assert!(!app.autosave_if_idle(start), "nothing edited yet");
        app.unsaved_since_edit = Some(start);
        assert!(!app.autosave_if_idle(start + AUTOSAVE_IDLE / 2));
        assert!(app.unsaved_since_edit.is_some(), "still pending");
        assert!(app.autosave_if_idle(start + AUTOSAVE_IDLE));
        assert_eq!(app.unsaved_since_edit, None);
        assert_eq!(app.status, "Auto saved.");
    }

    #[test]
    fn status_tracks_unsaved_changes_and_saved_notice() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        app.save();
        app.track_changes(&ctx);
        assert!(!app.unsaved_changes);
        assert_eq!(app.status, "Saved.");
        app.state.create_sheet(&[0], "New").expect("sheet");
        app.track_changes(&ctx);
        assert!(app.unsaved_changes, "a change shows as unsaved");
        app.save();
        app.track_changes(&ctx);
        assert!(!app.unsaved_changes);
        app.save_notice_at = Some(std::time::Instant::now() - SAVE_NOTICE);
        app.track_changes(&ctx);
        assert_eq!(app.status, IDLE_STATUS);
    }

    #[test]
    fn new_items_start_in_rename_mode() {
        let (mut app, _dir) = populated_app();
        app.run_context_action(&vec![0, 0], ContextAction::NewSheet);
        let count = match app.state.node(&[0, 0]) {
            Ok(Node::Group(g)) => g.children.len(),
            _ => panic!("group"),
        };
        assert_eq!(
            app.rename.as_ref().map(|(p, _)| p.clone()),
            Some(vec![0, 0, count - 1])
        );
    }

    #[test]
    fn splitting_a_sheet_separates_before_selection_and_after() {
        let (mut app, _dir) = populated_app();
        app.state
            .set_sheet_content(&[0, 0, 0], editor::buffer_to_content("one\n\ntwo\n\nthree"))
            .expect("content");
        app.open_sheet(vec![0, 0, 0]);
        let before = match app.state.node(&[0, 0]) {
            Ok(Node::Group(g)) => g.children.len(),
            _ => panic!("group"),
        };
        app.split_sheet(&vec![0, 0, 0], 5, 8);
        let text = |app: &ParrotApp, i: usize| {
            editor::content_to_buffer(&app.sheet(&vec![0, 0, i]).expect("sheet").content)
        };
        assert_eq!(text(&app, 0), "one");
        assert_eq!(text(&app, 1), "two");
        assert_eq!(text(&app, 2), "three");
        let after = match app.state.node(&[0, 0]) {
            Ok(Node::Group(g)) => g.children.len(),
            _ => panic!("group"),
        };
        assert_eq!(after, before + 2);
        assert_eq!(app.selected, Some(vec![0, 0, 1]));
        // Selecting everything leaves the sheet alone.
        app.open_sheet(vec![0, 0, 0]);
        app.split_sheet(&vec![0, 0, 0], 0, 3);
        assert_eq!(app.status, "Split into 3 sheets.");
    }

    #[test]
    fn ctrl_a_in_the_flow_selects_every_sheet_and_ctrl_c_copies_them() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        let frame = |app: &mut ParrotApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            ctx.run(input, |ctx| app.frame_ui(ctx))
        };
        frame(&mut app, vec![]); // fonts apply
        app.select_sidebar(SidebarSelection::Node(vec![0]));
        let paths = app.visible_sheet_paths();
        assert!(paths.len() > 1, "the project flows several sheets");
        frame(&mut app, vec![]);
        ctx.memory_mut(|m| m.request_focus(editor_id(&paths[0])));
        frame(&mut app, vec![]);
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(&mut app, vec![key(egui::Key::A, true)]);
        assert!(app.flow_all_selected);
        let out = frame(&mut app, vec![egui::Event::Copy]);
        assert!(!out.platform_output.copied_text.is_empty());
        assert!(app.flow_all_selected, "copying keeps the selection");
        frame(&mut app, vec![egui::Event::Text("x".into())]);
        assert!(!app.flow_all_selected, "typing drops it");
    }

    #[test]
    fn enter_and_tab_in_a_list_continue_and_nest_items() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        let frame = |app: &mut ParrotApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.frame_ui(ctx));
        };
        let key = |key, shift| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        };
        app.state
            .set_sheet_content(&[0, 0, 0], editor::buffer_to_content("- one"))
            .expect("content");
        frame(&mut app, vec![]); // fonts
        app.open_sheet(vec![0, 0, 0]);
        frame(&mut app, vec![]);
        let id = editor_id(&vec![0, 0, 0]);
        ctx.memory_mut(|m| m.request_focus(id));
        let mut state = egui::text_edit::TextEditState::default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(5))));
        state.store(&ctx, id);
        frame(&mut app, vec![]);
        frame(&mut app, vec![key(egui::Key::Enter, false)]);
        assert_eq!(app.editor_buffer, "- one\n\n- ");
        frame(&mut app, vec![egui::Event::Text("two".into())]);
        frame(&mut app, vec![key(egui::Key::Tab, false)]);
        assert_eq!(app.editor_buffer, "- one\n\n  - two");
        frame(&mut app, vec![key(egui::Key::Tab, true)]);
        assert_eq!(app.editor_buffer, "- one\n\n- two");
    }

    #[test]
    fn opening_an_imported_sheet_fills_the_editor_with_its_formatted_text() {
        let (mut app, _dir) = populated_app();
        let content = parrot_import::markdown::import_markdown("# T\n\n- a\n  - b\n\n---\n\nend\n");
        app.state
            .set_sheet_content(&[0, 0, 0], content)
            .expect("content");
        app.open_sheet(vec![0, 0, 0]);
        assert_eq!(app.editor_buffer, "# T\n\n- a\n\n  - b\n\n----\n\nend");
        // Several sheets opened together (the flow) get the same text.
        app.select_sidebar(SidebarSelection::Node(vec![0, 0]));
        let ctx = egui::Context::default();
        setup(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert_eq!(
            app.container_buffers
                .get(&vec![0, 0, 0])
                .map(String::as_str),
            Some("# T\n\n- a\n\n  - b\n\n----\n\nend")
        );
    }

    #[test]
    fn pasting_markdown_gives_each_block_its_own_paragraph() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        let frame = |app: &mut ParrotApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.frame_ui(ctx));
        };
        app.state
            .set_sheet_content(&[0, 0, 0], editor::buffer_to_content(""))
            .expect("content");
        frame(&mut app, vec![]);
        app.open_sheet(vec![0, 0, 0]);
        frame(&mut app, vec![]);
        let id = editor_id(&vec![0, 0, 0]);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::Paste("## Heading\n- one\n- two\nplain".into())],
        );
        // An unindented line after an item continues it, as in Markdown.
        assert_eq!(app.editor_buffer, "## Heading\n\n- one\n\n- two\nplain");
    }

    #[test]
    fn a_project_lists_every_sheet_including_its_sections() {
        let (app, _dir) = populated_app();
        let listed =
            sheet_list::listed_paths(&app.state, &SidebarSelection::Node(vec![0]), &app.today);
        assert_eq!(
            listed,
            sheet_list::all_sheets(app.state.node(&[0]).unwrap(), &vec![0])
        );
        assert!(listed.iter().any(|p| p.len() > 2), "nested sheets included");
    }

    #[test]
    fn inline_rename_commits_on_enter_and_lets_go_of_focus() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        app.collapsed.insert(vec![0, 0]);
        app.run_context_action(&vec![0, 0, 0], ContextAction::Rename);
        assert!(
            !app.collapsed.contains(&vec![0, 0]),
            "renaming reveals the row"
        );
        let field = Id::new(("rename", &vec![0, 0usize, 0]));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert!(ctx.memory(|m| m.has_focus(field)), "the field takes focus");
        if let Some((_, buffer)) = &mut app.rename {
            *buffer = "  Renamed ".to_string();
        }
        let _ = ctx.run(key(egui::Key::Enter), |ctx| app.frame_ui(ctx));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert_eq!(sheet_title(&app, &[0, 0, 0]), "Renamed");
        assert_eq!(app.rename, None);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        assert!(!ctx.memory(|m| m.has_focus(field)));
    }

    #[test]
    fn inline_rename_escape_cancels_and_blank_keeps_the_name() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        app.run_context_action(&vec![0, 0, 0], ContextAction::Rename);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        if let Some((_, buffer)) = &mut app.rename {
            *buffer = "Discarded".to_string();
        }
        let _ = ctx.run(key(egui::Key::Escape), |ctx| app.frame_ui(ctx));
        assert_eq!(app.rename, None);
        assert_eq!(sheet_title(&app, &[0, 0, 0]), "A");

        app.rename = Some((vec![0, 0, 0], "   ".to_string()));
        app.commit_rename();
        assert_eq!(sheet_title(&app, &[0, 0, 0]), "A");
    }

    #[test]
    fn goals_are_set_up_inside_the_statistics_tab() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        app.open_sheet(vec![0, 0, 0]);
        app.run_context_action(&vec![0, 0, 0], ContextAction::AddGoal);
        assert_eq!(app.inspector_tab, InspectorTab::Statistics);
        assert!(app.layout.is_visible(Panel::Inspector));
        assert_eq!(app.goal_edit_target, Some(vec![0, 0, 0]));
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        app.goal_flow.target = "500".to_string();
        app.goal_flow
            .save_to(&mut app.state, &[0, 0, 0])
            .expect("goal");
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
    }

    #[test]
    fn open_files_is_gone_from_the_sidebar() {
        assert!(SmartFilter::ALL.iter().all(|s| s.label() != "Open Files"));
    }

    #[test]
    fn formatting_buttons_sit_centered_in_the_editor_bar() {
        let (mut app, _dir) = populated_app();
        let ctx = egui::Context::default();
        setup(&ctx);
        app.open_sheet(vec![0, 0, 0]);
        // The first frame measures the buttons, the next ones center them.
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        }
        let (bar, buttons): (Rect, Rect) = ctx
            .data(|d| d.get_temp(Id::new(FORMAT_BAR_LAYOUT)))
            .expect("editor bar laid out");
        assert!(buttons.width() > 0.0);
        assert!(
            (buttons.center().x - bar.center().x).abs() < 1.0,
            "buttons {buttons:?} centered in {bar:?}"
        );
    }

    #[test]
    fn the_app_opens_on_the_project_shelf() {
        let (mut app, _dir) = populated_app();
        app.state.create_project("Essays");
        app.save();
        app.open_library();
        assert!(app.on_shelf(), "opening a library lands on the shelf");
        let ctx = egui::Context::default();
        setup(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.frame_ui(ctx));
        let shelf = crate::shelf::items(&app.state);
        assert_eq!(
            shelf.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            ["P", "Essays"]
        );
        // A card goes straight into its project; the Library row comes back.
        app.open_project(shelf[1].path.clone());
        assert_eq!(app.sidebar, Some(SidebarSelection::Node(vec![1])));
        assert!(!app.on_shelf());
        app.show_shelf();
        assert!(app.on_shelf());
    }

    #[test]
    fn projects_are_managed_and_sheets_stay_inside_them() {
        let (mut app, _dir) = populated_app();
        app.new_project();
        let new = vec![1];
        assert_eq!(
            sheet_or_project_title(&app, &new),
            sidebar::NEW_PROJECT_TITLE
        );
        assert_eq!(app.sidebar, Some(SidebarSelection::Node(new.clone())));
        assert_eq!(
            app.rename.as_ref().map(|(p, _)| p),
            Some(&new),
            "renames right away"
        );
        app.rename = Some((new.clone(), "Essays".to_string()));
        app.commit_rename();
        assert_eq!(sheet_or_project_title(&app, &new), "Essays");
        // New sheets go into the selected project; with no project there's nowhere to go.
        app.new_root_node(false);
        assert_eq!(app.state.child_count(&new), Ok(1));
        app.show_shelf();
        app.new_root_node(false);
        assert!(app.status.starts_with("Error"), "{}", app.status);
        assert_eq!(app.state.library.children.len(), 2);
        // Trash and restore: a sheet goes back into a project, never the root.
        app.run_context_action(&vec![1, 0], ContextAction::MoveToTrash);
        app.select_sidebar(SidebarSelection::Smart(SmartFilter::Trash));
        assert_eq!(app.state.restore_from_trash(0, Some(&[1])), Ok(vec![1, 0]));
        // Drafts are a classification on the sheet.
        app.run_context_action(&vec![1, 0], ContextAction::MarkDraft);
        assert!(matches!(app.state.node(&[1, 0]), Ok(Node::Sheet(s)) if s.draft));
        assert!(badges_for(app.state.node(&[1, 0]).expect("sheet")).contains(&Badge::Draft));
    }

    fn sheet_or_project_title(app: &ParrotApp, path: &[usize]) -> String {
        match app.state.node(path) {
            Ok(Node::Project(p)) => p.title.clone(),
            Ok(Node::Sheet(s)) => s.title.clone(),
            other => panic!("unexpected node {other:?}"),
        }
    }
}
