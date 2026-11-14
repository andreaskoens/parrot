//! Plain Rust application state — no `egui`/`eframe` dependency, so the tree-mutation and save
//! logic is unit-testable headlessly (T-0-09's explicit requirement). The UI layer (`ui.rs`)
//! only ever calls into this; it owns no document state of its own.

use parrot_core::{
    ActivityTracker, Attachment, Filter, Goal, Group, Library, Node, Project, Sheet, SheetContent,
};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::personal_dictionary::{PersonalDictionary, FILE_NAME as DICTIONARY_FILE};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateError {
    #[error("no node at that path")]
    InvalidPath,
    #[error("node at that path is not a Sheet")]
    NotASheet,
    #[error("node at that path is not a Project")]
    NotAProject,
    #[error("a Filter node cannot carry a Goal")]
    CannotHaveGoal,
    #[error("no library path has been set; create or open a library first")]
    NoLibraryPath,
    #[error("could not write the library: {0}")]
    Io(String),
    #[error("sheets and sections live inside a project; create or select a project first")]
    OutsideProject,
    #[error("projects live at the top of the library")]
    ProjectNotAtRoot,
}

/// Title of the project that collects sheets and sections found outside any project when a
/// library is opened.
pub const UNFILED_PROJECT_TITLE: &str = "Unfiled";

/// Where a node may live: projects only at the library root, sheets and sections only inside
/// a project (at any depth), filters anywhere.
fn check_placement(node: &Node, parent: &[usize]) -> Result<(), StateError> {
    match node {
        Node::Project(_) if !parent.is_empty() => Err(StateError::ProjectNotAtRoot),
        Node::Sheet(_) | Node::Group(_) if parent.is_empty() => Err(StateError::OutsideProject),
        _ => Ok(()),
    }
}

/// A path into the library tree: a sequence of child indices from the root. An empty path means
/// the library root itself (valid as a parent path, not as a node path).
pub type NodePath = Vec<usize>;

#[derive(Default)]
pub struct AppState {
    pub library: Library,
    pub library_path: Option<PathBuf>,
    /// Separate iCloud library root (`~/Library/Mobile Documents/.../Parrot`
    /// on macOS). `None` on Windows/Linux or when iCloud Drive is off.
    pub icloud_path: Option<PathBuf>,
    /// Writing-activity series (T-10-10) across the whole library. `day` is an opaque session
    /// counter this app bumps itself (`start_new_day`) — there's no real calendar source here
    /// (no `chrono` dependency), see `parrot_core::statistics`'s own doc.
    pub activity: ActivityTracker,
    activity_day: u32,
    /// Words the user added via the proofing UI, accepted by the spellchecker in every
    /// language. Loaded from `Dictionary.txt` on open, written by
    /// [`AppState::save_personal_dictionary`].
    pub personal_dictionary: PersonalDictionary,
    /// Nodes moved to the Trash smart filter (T-12-03, ui-description §3.1), most recent last.
    /// Session-only: not written by `save` — see DECISIONS.md.
    pub trash: Vec<Node>,
}

/// A completed [`AppState::move_nodes`]: which nodes moved (old paths, tree order) and where
/// they now start, so path-based UI state can follow the nodes instead of their old slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMove {
    roots: Vec<NodePath>,
    parent: NodePath,
    index: usize,
}

impl NodeMove {
    /// The moved nodes' new paths, in order.
    pub fn new_paths(&self) -> Vec<NodePath> {
        (self.index..self.index + self.roots.len())
            .map(|i| {
                let mut p = self.parent.clone();
                p.push(i);
                p
            })
            .collect()
    }

    /// Where the node that was at `old` is now.
    pub fn remap(&self, old: &[usize]) -> NodePath {
        if let Some(k) = self.roots.iter().position(|r| old.starts_with(r)) {
            let mut p = self.parent.clone();
            p.push(self.index + k);
            p.extend_from_slice(&old[self.roots[k].len()..]);
            return p;
        }
        let mut p = Self::after_removal(&self.roots, old);
        let level = self.parent.len();
        if p.len() > level && p.starts_with(&self.parent) && p[level] >= self.index {
            p[level] += self.roots.len();
        }
        p
    }

    /// `path` (outside every root) once `roots` are removed: each removed earlier sibling of
    /// the path or of one of its ancestors shifts that level left by one.
    fn after_removal(roots: &[NodePath], path: &[usize]) -> NodePath {
        let mut p = path.to_vec();
        for r in roots {
            let (&i, r_parent) = r.split_last().expect("non-empty");
            let level = r_parent.len();
            if level < path.len() && path.starts_with(r_parent) && i < path[level] {
                p[level] -= 1;
            }
        }
        p
    }
}

impl AppState {
    pub fn new() -> Self {
        let mut state = Self::default();
        state.activity.start_session(0, 0);
        state
    }

    /// Total word count across every Sheet in the library, recursively — what the Statistics
    /// panel shows as "today's" running total, and what `activity` tracks the delta of.
    pub fn total_word_count(&self) -> u32 {
        fn count(nodes: &[Node]) -> u32 {
            nodes
                .iter()
                .map(|n| match n {
                    Node::Sheet(s) => parrot_core::statistics::word_count(&s.content),
                    Node::Group(g) => count(&g.children),
                    Node::Project(p) => count(&p.children),
                    Node::Filter(_) => 0,
                })
                .sum()
        }
        count(&self.library.children)
    }

    fn record_activity(&mut self) {
        let total = self.total_word_count();
        self.activity.record(total);
    }

    /// Starts a new activity-tracking session/day (T-10-10) — there's no real calendar here, so
    /// this app only ever advances "days" when the user (or, later, a real clock) says so.
    pub fn start_new_day(&mut self) {
        self.activity_day += 1;
        let total = self.total_word_count();
        self.activity.start_session(self.activity_day, total);
    }

    /// Starts a brand-new, empty library rooted at `path` (T-0-09 / T-7-05's first-run flow).
    pub fn create_library(&mut self, path: &Path) {
        self.library = Library::default();
        self.library_path = Some(path.to_path_buf());
        self.personal_dictionary = PersonalDictionary::default();
        self.trash.clear();
    }

    /// Loads an existing library tree from `path` (feature inventory §2, via parrot-container).
    /// Returns whether loose sheets/sections had to be moved into an Unfiled project.
    /// When `icloud_path` is set, iCloud projects stored there are unioned in too
    /// (their `section` is forced to `ICloud` — location is authoritative).
    pub fn open_library(
        &mut self,
        path: &Path,
    ) -> Result<bool, parrot_container::tree::TreeIoError> {
        self.library = parrot_container::tree::open_library(path)?;
        self.library_path = Some(path.to_path_buf());
        // Personal words are auxiliary: a missing or unreadable file means an empty
        // dictionary, never a failed open (libraries predate this feature).
        self.personal_dictionary =
            PersonalDictionary::load(&path.join(DICTIONARY_FILE)).unwrap_or_default();
        self.trash.clear();
        let mut adopted = self.adopt_loose_nodes();
        if let Some(icloud) = self.icloud_path.clone() {
            if icloud != path {
                if let Ok(added) = self.load_icloud_projects(&icloud) {
                    adopted |= added;
                }
            }
        }
        Ok(adopted)
    }

    /// Loads iCloud-section projects from `icloud_root` and appends them.
    /// Missing folder = no iCloud projects yet, not an error for callers that
    /// care (`open_library` ignores the error; Settings surfaces it).
    pub fn load_icloud_projects(
        &mut self,
        icloud_root: &Path,
    ) -> Result<bool, parrot_container::tree::TreeIoError> {
        if !icloud_root.is_dir() {
            return Ok(false);
        }
        let remote = parrot_container::tree::open_library(icloud_root)?;
        let mut added = false;
        for node in remote.children {
            match node {
                Node::Project(mut p) => {
                    p.section = parrot_core::Section::ICloud;
                    self.library.children.push(Node::Project(p));
                    added = true;
                }
                // Filters are global: keep the local set, skip remote ones.
                Node::Filter(_) => {}
                // A bare sheet/section in the iCloud store (hand-copied in):
                // adopt it under an Unfiled iCloud project.
                Node::Sheet(_) | Node::Group(_) => {
                    let target = self.unfiled_icloud_project();
                    if let Ok(Node::Project(p)) = self.node_mut(&target) {
                        p.children.push(node);
                        added = true;
                    }
                }
            }
        }
        Ok(added)
    }

    fn unfiled_icloud_project(&mut self) -> NodePath {
        for (i, child) in self.library.children.iter().enumerate() {
            if let Node::Project(p) = child {
                if p.title == UNFILED_PROJECT_TITLE && p.section.is_icloud() {
                    return vec![i];
                }
            }
        }
        self.create_project_in_section(UNFILED_PROJECT_TITLE, parrot_core::Section::ICloud)
    }

    /// The iCloud-section projects, in library order.
    pub fn icloud_projects(&self) -> Vec<NodePath> {
        self.library
            .children
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n, Node::Project(p) if p.section.is_icloud()))
            .map(|(i, _)| vec![i])
            .collect()
    }

    /// The local-section projects, in library order.
    pub fn local_projects(&self) -> Vec<NodePath> {
        self.library
            .children
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                matches!(n, Node::Project(p) if !p.section.is_icloud())
                    || matches!(n, Node::Filter(_))
            })
            .map(|(i, _)| vec![i])
            .collect()
    }

    /// Moves the project at `path` to the iCloud section (stays in place in
    /// the tree; `save` writes it to the iCloud store).
    pub fn move_project_to_icloud(&mut self, path: &[usize]) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Project(p) => {
                p.section = parrot_core::Section::ICloud;
                Ok(())
            }
            _ => Err(StateError::NotAProject),
        }
    }

    /// Moves the project at `path` back to the local section.
    pub fn move_project_to_local(&mut self, path: &[usize]) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Project(p) => {
                p.section = parrot_core::Section::Local;
                Ok(())
            }
            _ => Err(StateError::NotAProject),
        }
    }

    /// Moves sheets and sections sitting at the library root (libraries written before
    /// projects were required) into an [`UNFILED_PROJECT_TITLE`] project at the end. Returns
    /// whether anything moved.
    pub fn adopt_loose_nodes(&mut self) -> bool {
        let (loose, kept): (Vec<Node>, Vec<Node>) = std::mem::take(&mut self.library.children)
            .into_iter()
            .partition(|n| matches!(n, Node::Sheet(_) | Node::Group(_)));
        self.library.children = kept;
        if loose.is_empty() {
            return false;
        }
        self.library.children.push(Node::Project(Project {
            title: UNFILED_PROJECT_TITLE.to_string(),
            children: loose,
            goal: None,
            language: None,
            section: parrot_core::Section::Local,
        }));
        true
    }

    /// Adds a new, empty project at the end of the library root.
    pub fn create_project(&mut self, title: &str) -> NodePath {
        self.create_project_in_section(title, parrot_core::Section::Local)
    }

    /// Adds a new, empty project in `section` at the end of the library root.
    pub fn create_project_in_section(
        &mut self,
        title: &str,
        section: parrot_core::Section,
    ) -> NodePath {
        self.library.children.push(Node::Project(Project {
            title: title.to_string(),
            children: Vec::new(),
            goal: None,
            language: None,
            section,
        }));
        vec![self.library.children.len() - 1]
    }

    /// Root indices of every project, in library order — the shelf.
    pub fn projects(&self) -> Vec<NodePath> {
        self.library
            .children
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n, Node::Project(_)))
            .map(|(i, _)| vec![i])
            .collect()
    }

    /// The project `path` lies in (the project itself for a project path).
    pub fn project_of(&self, path: &[usize]) -> Option<NodePath> {
        let root = path.first()?;
        matches!(self.library.children.get(*root), Some(Node::Project(_))).then(|| vec![*root])
    }

    /// Marks the sheet at `path` as a Draft Sheet, or clears the mark.
    pub fn set_draft(&mut self, path: &[usize], draft: bool) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                s.draft = draft;
                Ok(())
            }
            _ => Err(StateError::NotASheet),
        }
    }

    /// The folder the node at `path` is stored in under the library folder, as of the next
    /// save. `None` without a library folder or for a node that isn't stored (filters).
    /// iCloud-section projects resolve under the iCloud store; everything else
    /// under the local library folder.
    pub fn node_folder(&self, path: &[usize]) -> Option<PathBuf> {
        let section = match self.node(path).ok()? {
            Node::Project(p) => Some(p.section),
            _ => self
                .project_of(path)
                .and_then(|pp| match self.node(&pp).ok()? {
                    Node::Project(p) => Some(p.section),
                    _ => None,
                }),
        };
        let root = if section == Some(parrot_core::Section::ICloud) {
            self.icloud_path.as_ref().or(self.library_path.as_ref())?
        } else {
            self.library_path.as_ref()?
        };
        // Partitioned save keeps each section in its own root, so resolve the
        // disk path against a section-filtered view of the tree.
        let view = self.section_view(section.unwrap_or(parrot_core::Section::Local));
        let mut index_map: Vec<usize> = Vec::new();
        for (i, child) in self.library.children.iter().enumerate() {
            let keep = match child {
                Node::Project(p) => p.section == section.unwrap_or(parrot_core::Section::Local),
                Node::Filter(_) => section == Some(parrot_core::Section::Local),
                _ => false,
            };
            if keep {
                index_map.push(i);
            }
        }
        let local_index = index_map
            .iter()
            .position(|&i| Some(i) == path.first().copied())?;
        let mut view_path = vec![local_index];
        view_path.extend_from_slice(&path[1..]);
        parrot_container::tree::node_disk_path(root, &view, &view_path)
    }

    /// A filtered `Library` holding only one section's projects (plus filters
    /// for the local view, which is where filters live).
    fn section_view(&self, section: parrot_core::Section) -> Library {
        Library {
            children: self
                .library
                .children
                .iter()
                .filter(|n| match n {
                    Node::Project(p) => p.section == section,
                    Node::Filter(_) => section == parrot_core::Section::Local,
                    _ => false,
                })
                .cloned()
                .collect(),
        }
    }

    /// Persists the current in-memory tree. Local projects go to `library_path`;
    /// iCloud projects go to `icloud_path` when set, else alongside local so
    /// nothing is lost on machines without iCloud Drive.
    pub fn save(&self) -> Result<(), StateError> {
        let local_path = self
            .library_path
            .as_ref()
            .ok_or(StateError::NoLibraryPath)?;
        // The iCloud store opened directly as the library: one root holds both
        // sections, so save the merged tree once instead of partitioning a
        // store against itself (which would set the other half aside).
        if self.icloud_path.as_ref() == Some(local_path) {
            return parrot_container::tree::save_library(local_path, &self.library)
                .map_err(|e| StateError::Io(e.to_string()));
        }
        let local_view = self.section_view(parrot_core::Section::Local);
        parrot_container::tree::save_library(local_path, &local_view)
            .map_err(|e| StateError::Io(e.to_string()))?;
        let icloud_view = self.section_view(parrot_core::Section::ICloud);
        let has_icloud = icloud_view
            .children
            .iter()
            .any(|n| matches!(n, Node::Project(_)));
        if let Some(icloud_path) = self.icloud_path.as_ref() {
            if has_icloud || icloud_path.is_dir() {
                parrot_container::tree::save_library(icloud_path, &icloud_view)
                    .map_err(|e| StateError::Io(e.to_string()))?;
            }
        } else if has_icloud {
            // No iCloud store configured: keep iCloud-section projects in the
            // local folder (with `section=icloud` in their meta) so a later
            // move to iCloud Drive can pick them up.
            let mut merged = local_view;
            merged.children.extend(icloud_view.children);
            parrot_container::tree::save_library(local_path, &merged)
                .map_err(|e| StateError::Io(e.to_string()))?;
        }
        Ok(())
    }

    /// Adds `word` to the personal dictionary (case-insensitive). Returns whether it is
    /// newly added; call [`AppState::save_personal_dictionary`] to persist it.
    pub fn add_personal_word(&mut self, word: &str) -> bool {
        self.personal_dictionary.add(word)
    }

    /// Persists the personal dictionary to `Dictionary.txt` in the library folder.
    /// Writes nothing when empty — and removes a stale file — so wordless libraries stay
    /// clean and old libraries never gain the file until a word is added.
    pub fn save_personal_dictionary(&self) -> Result<(), StateError> {
        let root = self
            .library_path
            .as_ref()
            .ok_or(StateError::NoLibraryPath)?;
        let path = root.join(DICTIONARY_FILE);
        if self.personal_dictionary.is_empty() {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| StateError::Io(e.to_string()))?;
            }
            return Ok(());
        }
        self.personal_dictionary
            .save(&path)
            .map_err(|e| StateError::Io(e.to_string()))
    }

    pub fn create_group(&mut self, parent: &[usize], title: &str) -> Result<(), StateError> {
        let group = Node::Group(Group {
            title: title.to_string(),
            children: Vec::new(),
            goal: None,
        });
        self.add_child(parent, group)
    }

    pub fn create_sheet(&mut self, parent: &[usize], title: &str) -> Result<(), StateError> {
        self.add_child(
            parent,
            Node::Sheet(Sheet {
                title: title.to_string(),
                content: SheetContent::new(Vec::new()),
                goal: None,
                keywords: Vec::new(),
                attachments: Vec::new(),
                glued: false,
                draft: false,
            }),
        )
    }

    /// Appends `node` to `parent`'s children, if it may live there.
    pub fn add_child(&mut self, parent: &[usize], node: Node) -> Result<(), StateError> {
        check_placement(&node, parent)?;
        self.children_mut(parent)?.push(node);
        Ok(())
    }

    /// Inserts `sheet` right after the node at `path`, in the same parent.
    pub fn insert_sheet_after(&mut self, path: &[usize], sheet: Sheet) -> Result<(), StateError> {
        let (&idx, parent) = path.split_last().ok_or(StateError::InvalidPath)?;
        let children = self.children_mut(parent)?;
        if idx >= children.len() {
            return Err(StateError::InvalidPath);
        }
        children.insert(idx + 1, Node::Sheet(sheet));
        self.record_activity();
        Ok(())
    }

    /// Saves a filter built by the Filters UI (T-7-01) into the tree as a `Node::Filter`.
    pub fn create_filter(&mut self, parent: &[usize], filter: Filter) -> Result<(), StateError> {
        let children = self.children_mut(parent)?;
        children.push(Node::Filter(filter));
        Ok(())
    }

    /// Renames any node kind (every `Node` variant carries a `title`) — used by the tree UI's
    /// inline rename field, which doesn't care what kind of node it's editing.
    pub fn rename_node(&mut self, path: &[usize], new_title: &str) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => s.title = new_title.to_string(),
            Node::Group(g) => g.title = new_title.to_string(),
            Node::Project(p) => p.title = new_title.to_string(),
            Node::Filter(f) => f.title = new_title.to_string(),
        }
        Ok(())
    }

    /// Sets the proofing language code of the project at `path` (`None` = app default).
    pub fn set_project_language(
        &mut self,
        path: &[usize],
        language: Option<String>,
    ) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Project(p) => {
                p.language = language;
                Ok(())
            }
            _ => Err(StateError::NotAProject),
        }
    }

    /// Sets (or clears, via `None`) the Goal on a Sheet/Group/Project node (T-10-03). A `Filter`
    /// node has no `goal` field — `CannotHaveGoal` rather than silently doing nothing.
    pub fn set_goal(&mut self, path: &[usize], goal: Option<Goal>) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                s.goal = goal;
                Ok(())
            }
            Node::Group(g) => {
                g.goal = goal;
                Ok(())
            }
            Node::Project(p) => {
                p.goal = goal;
                Ok(())
            }
            Node::Filter(_) => Err(StateError::CannotHaveGoal),
        }
    }

    /// Assigns a Keyword tag to the Sheet at `path` (T-10-05).
    pub fn assign_keyword(&mut self, path: &[usize], keyword: &str) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                parrot_core::keyword::assign(&mut s.keywords, keyword);
                Ok(())
            }
            _ => Err(StateError::NotASheet),
        }
    }

    /// Removes a Keyword tag from the Sheet at `path` (T-10-05).
    pub fn unassign_keyword(&mut self, path: &[usize], keyword: &str) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                parrot_core::keyword::unassign(&mut s.keywords, keyword);
                Ok(())
            }
            _ => Err(StateError::NotASheet),
        }
    }

    /// Adds an Attachment to the Sheet at `path` (T-10-11).
    pub fn add_attachment(
        &mut self,
        path: &[usize],
        attachment: Attachment,
    ) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                s.attachments.push(attachment);
                Ok(())
            }
            _ => Err(StateError::NotASheet),
        }
    }

    /// Removes the Attachment at `index` from the Sheet at `path` (T-10-11). Out-of-range
    /// `index` is a no-op, not an error — the UI always re-reads the current list before
    /// offering an index to remove, so this should never actually happen.
    pub fn remove_attachment(&mut self, path: &[usize], index: usize) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                if index < s.attachments.len() {
                    s.attachments.remove(index);
                }
                Ok(())
            }
            _ => Err(StateError::NotASheet),
        }
    }

    /// Whether the Sheet at `path` has any Attachments — the sidebar badge's own state
    /// (T-10-11's "badge reflects presence" requirement).
    pub fn has_attachments(&self, path: &[usize]) -> bool {
        matches!(self.node(path), Ok(Node::Sheet(s)) if !s.attachments.is_empty())
    }

    /// Replaces a Sheet's content (used by the editor after re-parsing the typed buffer).
    pub fn set_sheet_content(
        &mut self,
        path: &[usize],
        content: SheetContent,
    ) -> Result<(), StateError> {
        match self.node_mut(path)? {
            Node::Sheet(s) => {
                s.content = content;
                self.record_activity();
                Ok(())
            }
            _ => Err(StateError::NotASheet),
        }
    }

    /// Moves the node at `from` to become the last child of `to_parent` (T-7-03).
    ///
    /// KNOWN LIMITATION: `to_parent` is resolved *after* `from` is removed — if `to_parent`'s
    /// own path runs through the same array `from` was removed from (e.g. a sibling-shift
    /// within one parent), compute `to_parent` against the post-removal tree, not the
    /// pre-removal one. Not an issue for the common case (moving into a different subtree).
    pub fn move_node(&mut self, from: &[usize], to_parent: &[usize]) -> Result<(), StateError> {
        let (&idx, parent) = from.split_last().ok_or(StateError::InvalidPath)?;
        check_placement(self.node(from)?, to_parent)?;
        let node = unglued(self.take_child(parent, idx)?);
        self.children_mut(to_parent)?.push(node);
        Ok(())
    }

    /// Moves `sources` so they sit consecutively, in tree order, in `parent` starting at
    /// `index` — both counted *before* the move, as a drop indicator shows them. A source
    /// inside another source moves along with it; a container can't move into itself or its
    /// own descendants. The returned [`NodeMove`] maps any old path to its new one.
    pub fn move_nodes(
        &mut self,
        sources: &[NodePath],
        parent: &[usize],
        index: usize,
    ) -> Result<NodeMove, StateError> {
        let mut sources: Vec<NodePath> = sources.to_vec();
        sources.sort();
        sources.dedup();
        let roots: Vec<NodePath> = sources
            .iter()
            .filter(|s| !sources.iter().any(|o| o != *s && s.starts_with(o)))
            .cloned()
            .collect();
        if roots.iter().any(|s| s.is_empty() || parent.starts_with(s)) {
            return Err(StateError::InvalidPath);
        }
        for s in &roots {
            self.node(s)?;
        }
        let mut target = parent.to_vec();
        target.push(index);
        let target = NodeMove::after_removal(&roots, &target);
        let new_parent_len = target.len() - 1;
        for s in &roots {
            // Placement depends only on whether the destination is the root, which removal
            // can't change.
            check_placement(self.node(s)?, &target[..new_parent_len])?;
        }
        if index > self.children(parent)?.len() {
            return Err(StateError::InvalidPath);
        }
        let (&new_index, new_parent) = target.split_last().expect("non-empty");
        // Reverse tree order: removing a node only shifts paths after it, already removed.
        let mut nodes = Vec::with_capacity(roots.len());
        for s in roots.iter().rev() {
            let (&i, s_parent) = s.split_last().expect("non-empty");
            nodes.push(self.take_child(s_parent, i)?);
        }
        nodes.reverse();
        let children = self.children_mut(new_parent)?;
        let count = nodes.len();
        for (offset, node) in nodes.into_iter().enumerate() {
            // A moved sheet stays glued only to the sheet it was glued to, moved along with it.
            let with_its_run =
                offset > 0 && was_previous_sibling(&roots[offset - 1], &roots[offset]);
            let node = if with_its_run { node } else { unglued(node) };
            children.insert(new_index + offset, node);
        }
        if let Some(Node::Sheet(next)) = children.get_mut(new_index + count) {
            next.glued = false;
        }
        Ok(NodeMove {
            roots,
            parent: new_parent.to_vec(),
            index: new_index,
        })
    }

    /// Duplicates the node at `path`, inserting the copy immediately after the original (T-7-03).
    pub fn duplicate_node(&mut self, path: &[usize]) -> Result<(), StateError> {
        let (&idx, parent) = path.split_last().ok_or(StateError::InvalidPath)?;
        let children = self.children_mut(parent)?;
        if idx >= children.len() {
            return Err(StateError::InvalidPath);
        }
        let mut clone = children[idx].clone();
        // A copy made inside a glued run stays in it.
        let next_glued = matches!(children.get(idx + 1), Some(Node::Sheet(s)) if s.glued);
        if let Node::Sheet(s) = &mut clone {
            s.glued = s.glued || next_glued;
        }
        children.insert(idx + 1, clone);
        Ok(())
    }

    pub fn delete_node(&mut self, path: &[usize]) -> Result<(), StateError> {
        let (&idx, parent) = path.split_last().ok_or(StateError::InvalidPath)?;
        self.take_child(parent, idx)?;
        Ok(())
    }

    /// Moves the node at `path` into [`AppState::trash`] — the sidebar's delete action
    /// (T-12-03): recoverable via [`AppState::restore_from_trash`], unlike [`AppState::delete_node`].
    pub fn move_to_trash(&mut self, path: &[usize]) -> Result<(), StateError> {
        let (&idx, parent) = path.split_last().ok_or(StateError::InvalidPath)?;
        let node = unglued(self.take_child(parent, idx)?);
        self.trash.push(node);
        Ok(())
    }

    /// Moves trash entry `index` back: a project or filter to the end of the library root, a
    /// sheet or section to the end of `project` (its original parent may no longer exist).
    /// Returns where it landed.
    pub fn restore_from_trash(
        &mut self,
        index: usize,
        project: Option<&[usize]>,
    ) -> Result<NodePath, StateError> {
        let node = self.trash.get(index).ok_or(StateError::InvalidPath)?;
        let parent = match node {
            Node::Project(_) | Node::Filter(_) => Vec::new(),
            _ => project.ok_or(StateError::OutsideProject)?.to_vec(),
        };
        check_placement(node, &parent)?;
        let count = self.children(&parent)?.len();
        let node = self.trash.remove(index);
        self.children_mut(&parent)?.push(unglued(node));
        let mut path = parent;
        path.push(count);
        Ok(path)
    }

    /// Permanently discards everything in the trash.
    pub fn empty_trash(&mut self) {
        self.trash.clear();
    }

    /// The run of glued sheets holding sheet `path`, in order — just `path` for a sheet glued
    /// to nothing, empty for anything that isn't a sheet.
    pub fn glue_run(&self, path: &[usize]) -> Vec<NodePath> {
        let Some((&i, parent)) = path.split_last() else {
            return Vec::new();
        };
        let Ok(children) = self.children(parent) else {
            return Vec::new();
        };
        if !matches!(children.get(i), Some(Node::Sheet(_))) {
            return Vec::new();
        }
        let glued = |j: usize| matches!(children.get(j), Some(Node::Sheet(s)) if s.glued);
        let mut start = i;
        while start > 0 && glued(start) {
            start -= 1;
        }
        let mut end = i;
        while glued(end + 1) {
            end += 1;
        }
        (start..=end)
            .map(|j| {
                let mut p = parent.to_vec();
                p.push(j);
                p
            })
            .collect()
    }

    /// Glues `paths` (sheets from two or more runs, a lone sheet being a run of its own) into one
    /// run: they move, whole runs at a time, to sit together in tree order where the first is.
    pub fn glue_sheets(&mut self, paths: &[NodePath]) -> Result<NodeMove, StateError> {
        let mut runs: Vec<Vec<NodePath>> = Vec::new();
        for p in paths {
            let run = self.glue_run(p);
            if run.is_empty() {
                return Err(StateError::NotASheet);
            }
            if !runs.contains(&run) {
                runs.push(run);
            }
        }
        if runs.len() < 2 {
            return Err(StateError::InvalidPath);
        }
        let mut sources: Vec<NodePath> = runs.concat();
        sources.sort();
        let (&index, parent) = sources[0].split_last().expect("non-empty");
        let parent = parent.to_vec();
        let mv = self.move_nodes(&sources, &parent, index)?;
        for p in mv.new_paths().iter().skip(1) {
            if let Ok(Node::Sheet(s)) = self.node_mut(p) {
                s.glued = true;
            }
        }
        Ok(mv)
    }

    /// Unglues each sheet in `paths` from the sheets before and after it.
    pub fn unglue_sheets(&mut self, paths: &[NodePath]) {
        for p in paths {
            let Some((&i, parent)) = p.split_last() else {
                continue;
            };
            let Ok(children) = self.children_mut(parent) else {
                continue;
            };
            for j in [i, i + 1] {
                if let Some(Node::Sheet(s)) = children.get_mut(j) {
                    s.glued = false;
                }
            }
        }
    }

    /// Looks up a node by path for read access (used by the UI to render the tree/editor).
    pub fn node(&self, path: &[usize]) -> Result<&Node, StateError> {
        let (&idx, parent) = path.split_last().ok_or(StateError::InvalidPath)?;
        self.children(parent)?
            .get(idx)
            .ok_or(StateError::InvalidPath)
    }

    /// How many children the container at `parent` has (the root for an empty path).
    pub fn child_count(&self, parent: &[usize]) -> Result<usize, StateError> {
        self.children(parent).map(Vec::len)
    }

    fn children(&self, path: &[usize]) -> Result<&Vec<Node>, StateError> {
        let mut children = &self.library.children;
        for &idx in path {
            children = match children.get(idx) {
                Some(Node::Group(g)) => &g.children,
                Some(Node::Project(p)) => &p.children,
                _ => return Err(StateError::InvalidPath),
            };
        }
        Ok(children)
    }

    fn children_mut(&mut self, path: &[usize]) -> Result<&mut Vec<Node>, StateError> {
        let mut children = &mut self.library.children;
        for &idx in path {
            children = match children.get_mut(idx) {
                Some(Node::Group(g)) => &mut g.children,
                Some(Node::Project(p)) => &mut p.children,
                _ => return Err(StateError::InvalidPath),
            };
        }
        Ok(children)
    }

    /// Removes child `i` of `parent`; when it headed a glued run, the next sheet heads it now.
    fn take_child(&mut self, parent: &[usize], i: usize) -> Result<Node, StateError> {
        let children = self.children_mut(parent)?;
        if i >= children.len() {
            return Err(StateError::InvalidPath);
        }
        let node = children.remove(i);
        if !matches!(&node, Node::Sheet(s) if s.glued) {
            if let Some(Node::Sheet(next)) = children.get_mut(i) {
                next.glued = false;
            }
        }
        Ok(node)
    }

    fn node_mut(&mut self, path: &[usize]) -> Result<&mut Node, StateError> {
        let (&idx, parent) = path.split_last().ok_or(StateError::InvalidPath)?;
        self.children_mut(parent)?
            .get_mut(idx)
            .ok_or(StateError::InvalidPath)
    }
}

/// `node`, no longer glued to whatever precedes it.
fn unglued(mut node: Node) -> Node {
    if let Node::Sheet(s) = &mut node {
        s.glued = false;
    }
    node
}

/// Whether `a` was the sibling right before `b`.
fn was_previous_sibling(a: &[usize], b: &[usize]) -> bool {
    match (a.split_last(), b.split_last()) {
        (Some((&i, pa)), Some((&j, pb))) => pa == pb && i + 1 == j,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_group() {
        let mut state = AppState::new();
        let project = state.create_project("P");
        state
            .create_group(&project, "Drafts")
            .expect("create_group must succeed in a project");
        match state.node(&[0, 0]).expect("section") {
            Node::Group(g) => assert_eq!(g.title, "Drafts"),
            other => panic!("expected a Group, got {other:?}"),
        }
    }

    #[test]
    fn sheets_and_sections_live_only_inside_projects() {
        let mut state = AppState::new();
        assert_eq!(
            state.create_sheet(&[], "Loose"),
            Err(StateError::OutsideProject)
        );
        assert_eq!(
            state.create_group(&[], "Loose"),
            Err(StateError::OutsideProject)
        );
        assert!(state.library.children.is_empty());
        let a = state.create_project("A");
        let b = state.create_project("B");
        state.create_sheet(&a, "S").expect("sheet in a project");
        // Nothing moves to the root, and projects don't nest.
        assert_eq!(
            state.move_nodes(&[vec![0, 0]], &[], 0),
            Err(StateError::OutsideProject)
        );
        assert_eq!(
            state.move_nodes(std::slice::from_ref(&b), &a, 0),
            Err(StateError::ProjectNotAtRoot)
        );
        // Between projects is fine; filters may sit at the root.
        state.move_nodes(&[vec![0, 0]], &b, 0).expect("move");
        assert_eq!(titles(&state, &b), ["S"]);
        let filter = Filter {
            title: "F".to_string(),
            keyword: None,
            keyword_tag: None,
            node_kind: parrot_core::filter::NodeKindMatch::Sheet,
            date_range: None,
        };
        state.create_filter(&[], filter).expect("filter at root");
        assert_eq!(state.projects(), vec![vec![0], vec![1]]);
        assert_eq!(state.project_of(&[1, 0]), Some(vec![1]));
        assert_eq!(state.project_of(&[2]), None, "a filter isn't a project");
    }

    #[test]
    fn opening_a_library_moves_loose_sheets_into_an_unfiled_project() {
        let mut state = AppState::new();
        state.library.children = vec![
            Node::Project(Project {
                title: "Novel".to_string(),
                children: Vec::new(),
                goal: None,
                language: None,
                section: parrot_core::Section::Local,
            }),
            Node::Sheet(Sheet {
                title: "Loose".to_string(),
                content: SheetContent::new(Vec::new()),
                goal: None,
                keywords: Vec::new(),
                attachments: Vec::new(),
                glued: false,
                draft: false,
            }),
        ];
        assert!(state.adopt_loose_nodes());
        assert_eq!(titles(&state, &[]), ["Novel", UNFILED_PROJECT_TITLE]);
        assert_eq!(titles(&state, &[1]), ["Loose"]);
        assert!(!state.adopt_loose_nodes(), "nothing left to move");
    }

    #[test]
    fn draft_marks_only_sheets() {
        let mut state = AppState::new();
        let p = state.create_project("P");
        state.create_sheet(&p, "S").expect("sheet");
        state.set_draft(&[0, 0], true).expect("draft");
        assert!(matches!(state.node(&[0, 0]), Ok(Node::Sheet(s)) if s.draft));
        assert_eq!(state.set_draft(&p, true), Err(StateError::NotASheet));
    }

    #[test]
    fn rename_sheet() {
        let mut state = AppState::new();
        state.create_project("P");
        state
            .create_sheet(&[0], "Untitled")
            .expect("create_sheet must succeed in a project");
        state
            .rename_node(&[0, 0], "My First Sheet")
            .expect("rename_node must succeed on a Sheet");
        match state.node(&[0, 0]).expect("node must exist") {
            Node::Sheet(s) => assert_eq!(s.title, "My First Sheet"),
            other => panic!("expected a Sheet, got {other:?}"),
        }
    }

    #[test]
    fn delete_node() {
        let mut state = AppState::new();
        state.create_project("P");
        state
            .create_group(&[0], "Drafts")
            .expect("create_group must succeed");
        state
            .create_sheet(&[0, 0], "A")
            .expect("create_sheet must succeed");
        state
            .create_sheet(&[0, 0], "B")
            .expect("create_sheet must succeed");
        assert_eq!(state.node(&[0, 0]).map(|_| ()), Ok(()));

        state
            .delete_node(&[0, 0, 0])
            .expect("delete_node must succeed");

        match state.node(&[0, 0]).expect("group must still exist") {
            Node::Group(g) => {
                assert_eq!(g.children.len(), 1);
                match &g.children[0] {
                    Node::Sheet(s) => assert_eq!(s.title, "B", "the remaining sheet must be B"),
                    other => panic!("expected a Sheet, got {other:?}"),
                }
            }
            other => panic!("expected a Group, got {other:?}"),
        }
    }

    #[test]
    fn trash_round_trip() {
        let mut state = AppState::new();
        let p = state.create_project("P");
        state.create_sheet(&p, "A").expect("create_sheet");
        state.create_sheet(&p, "B").expect("create_sheet");
        state.move_to_trash(&[0, 0]).expect("move_to_trash");
        assert_eq!(titles(&state, &p), ["B"]);
        assert_eq!(state.trash.len(), 1);
        // A sheet needs a project to go back into.
        assert_eq!(
            state.restore_from_trash(0, None),
            Err(StateError::OutsideProject)
        );
        let path = state.restore_from_trash(0, Some(&p)).expect("restore");
        assert_eq!(path, vec![0, 1]);
        assert!(state.trash.is_empty());
        match state.node(&[0, 1]).expect("restored node") {
            Node::Sheet(s) => assert_eq!(s.title, "A"),
            other => panic!("expected a Sheet, got {other:?}"),
        }
        assert_eq!(
            state.restore_from_trash(0, Some(&p)),
            Err(StateError::InvalidPath)
        );
        // A project goes back to the root.
        state.move_to_trash(&p).expect("trash project");
        assert_eq!(state.restore_from_trash(0, None), Ok(vec![0]));
    }

    #[test]
    fn personal_dictionary_round_trips_through_the_library_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut state = AppState::new();
        state.create_library(dir.path());
        assert!(state.add_personal_word("Affini"));
        assert!(
            !state.add_personal_word("AFFINI"),
            "same word in other case is not new"
        );
        state.save_personal_dictionary().expect("save dictionary");
        assert!(dir.path().join("Dictionary.txt").is_file());
        state.save().expect("save library");
        // A fresh open picks the words back up.
        let mut reopened = AppState::new();
        reopened.open_library(dir.path()).expect("reopen");
        assert!(reopened.personal_dictionary.contains("affini"));
        // Libraries written before this feature have no file: still an empty dictionary.
        let bare = tempfile::tempdir().expect("tempdir");
        let mut state = AppState::new();
        state.create_library(bare.path());
        state.save().expect("save library");
        let mut reopened = AppState::new();
        reopened.open_library(bare.path()).expect("reopen");
        assert!(reopened.personal_dictionary.is_empty());
        // And removing every word cleans the file back up.
        let mut state = AppState::new();
        state.create_library(dir.path());
        state.open_library(dir.path()).expect("reopen");
        state.personal_dictionary.remove("affini");
        state.save_personal_dictionary().expect("save dictionary");
        assert!(!dir.path().join("Dictionary.txt").exists());
        // No library, nowhere to save.
        assert_eq!(
            AppState::new().save_personal_dictionary(),
            Err(StateError::NoLibraryPath)
        );
    }

    fn titles(state: &AppState, parent: &[usize]) -> Vec<String> {
        state
            .children(parent)
            .expect("parent")
            .iter()
            .map(|n| match n {
                Node::Sheet(s) => s.title.clone(),
                Node::Group(g) => g.title.clone(),
                Node::Project(p) => p.title.clone(),
                Node::Filter(f) => f.title.clone(),
            })
            .collect()
    }

    /// Project [0]: [A, B, G[x, y], C].
    fn move_fixture() -> AppState {
        let mut state = AppState::new();
        state.create_project("P");
        state.create_sheet(&[0], "A").expect("A");
        state.create_sheet(&[0], "B").expect("B");
        state.create_group(&[0], "G").expect("G");
        state.create_sheet(&[0, 2], "x").expect("x");
        state.create_sheet(&[0, 2], "y").expect("y");
        state.create_sheet(&[0], "C").expect("C");
        state
    }

    #[test]
    fn move_nodes_reorders_within_a_parent() {
        let mut state = move_fixture();
        // C dropped before A.
        let mv = state.move_nodes(&[vec![0, 3]], &[0], 0).expect("move");
        assert_eq!(mv.new_paths(), vec![vec![0, 0]]);
        assert_eq!(mv.remap(&[0, 0]), vec![0, 1]);
        assert_eq!(mv.remap(&[0, 2, 1]), vec![0, 3, 1]);
        assert_eq!(titles(&state, &[0]), ["C", "A", "B", "G"]);
        // A dropped after G (index 4 counted before the move).
        let mv = state.move_nodes(&[vec![0, 1]], &[0], 4).expect("move");
        assert_eq!(mv.new_paths(), vec![vec![0, 3]]);
        assert_eq!(mv.remap(&[0, 3]), vec![0, 2], "G shifts left");
        assert_eq!(titles(&state, &[0]), ["C", "B", "G", "A"]);
    }

    #[test]
    fn move_nodes_moves_several_between_groups_keeping_order() {
        let mut state = move_fixture();
        // A and C into G between x and y; G's own path shifts from [2] to [1].
        let mv = state
            .move_nodes(&[vec![0, 3], vec![0, 0]], &[0, 2], 1)
            .expect("move");
        assert_eq!(mv.new_paths(), vec![vec![0, 1, 1], vec![0, 1, 2]]);
        assert_eq!(mv.remap(&[0, 2]), vec![0, 1], "G itself");
        assert_eq!(mv.remap(&[0, 2, 0]), vec![0, 1, 0], "x stays first");
        assert_eq!(
            mv.remap(&[0, 2, 1]),
            vec![0, 1, 3],
            "y after the inserted pair"
        );
        assert_eq!(mv.remap(&[0, 1]), vec![0, 0], "B");
        assert_eq!(titles(&state, &[0]), ["B", "G"]);
        assert_eq!(titles(&state, &[0, 1]), ["x", "A", "C", "y"]);
        // And back out to the root's start.
        state.move_nodes(&[vec![0, 1, 0]], &[0], 0).expect("out");
        assert_eq!(titles(&state, &[0]), ["x", "B", "G"]);
        assert_eq!(titles(&state, &[0, 2]), ["A", "C", "y"]);
    }

    #[test]
    fn move_nodes_rejects_moving_a_group_into_itself() {
        let mut state = move_fixture();
        assert_eq!(
            state.move_nodes(&[vec![0, 2]], &[0, 2], 0),
            Err(StateError::InvalidPath)
        );
        assert_eq!(
            state.move_nodes(&[vec![0, 9]], &[0], 0),
            Err(StateError::InvalidPath)
        );
        assert_eq!(
            state.move_nodes(&[vec![0, 0]], &[0, 0], 0),
            Err(StateError::InvalidPath),
            "a sheet is not a container"
        );
        // A child selected along with its group moves inside the group, not separately.
        let mv = state
            .move_nodes(&[vec![0, 2], vec![0, 2, 0]], &[0], 0)
            .expect("move");
        assert_eq!(mv.new_paths(), vec![vec![0, 0]]);
        assert_eq!(
            mv.remap(&[0, 2, 1]),
            vec![0, 0, 1],
            "follows its moved group"
        );
        assert_eq!(titles(&state, &[0]), ["G", "A", "B", "C"]);
        assert_eq!(titles(&state, &[0, 0]), ["x", "y"]);
    }

    #[test]
    fn delete_node_rejects_empty_path() {
        let mut state = AppState::new();
        assert_eq!(state.delete_node(&[]), Err(StateError::InvalidPath));
    }

    fn glue_fixture() -> AppState {
        let mut state = AppState::new();
        state.create_project("P");
        for title in ["A", "B", "C", "D"] {
            state.create_sheet(&[0], title).expect("sheet");
        }
        state
    }

    fn glued_titles(state: &AppState) -> Vec<String> {
        state
            .children(&[0])
            .expect("project")
            .iter()
            .map(|n| match n {
                Node::Sheet(s) if s.glued => format!("+{}", s.title),
                Node::Sheet(s) => s.title.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn glueing_sheets_moves_them_together_into_one_run() {
        let mut state = glue_fixture();
        let mv = state.glue_sheets(&[vec![0, 0], vec![0, 2]]).expect("glue");
        assert_eq!(mv.new_paths(), vec![vec![0, 0], vec![0, 1]]);
        assert_eq!(glued_titles(&state), ["A", "+C", "B", "D"]);
        assert_eq!(state.glue_run(&[0, 1]), vec![vec![0, 0], vec![0, 1]]);
        assert_eq!(state.glue_run(&[0, 2]), vec![vec![0, 2]]);
        // Glueing a run member to another sheet brings the whole run along.
        state.glue_sheets(&[vec![0, 3], vec![0, 1]]).expect("glue");
        assert_eq!(glued_titles(&state), ["A", "+C", "+D", "B"]);
        assert!(state.glue_sheets(&[vec![0, 0]]).is_err(), "one run alone");
        // Unglueing a sheet detaches it from both neighbours.
        state.unglue_sheets(&[vec![0, 1]]);
        assert_eq!(glued_titles(&state), ["A", "C", "D", "B"]);
    }

    #[test]
    fn runs_stay_consistent_when_their_sheets_move_or_go() {
        let mut state = glue_fixture();
        state
            .glue_sheets(&[vec![0, 0], vec![0, 1], vec![0, 2]])
            .expect("glue");
        // Removing the head: the next sheet heads the run.
        state.move_to_trash(&[0, 0]).expect("trash");
        assert_eq!(glued_titles(&state), ["B", "+C", "D"]);
        // Moving the whole run keeps it glued; what follows it at its new place isn't.
        state.glue_sheets(&[vec![0, 2], vec![0, 0]]).expect("glue");
        assert_eq!(glued_titles(&state), ["B", "+C", "+D"]);
        state
            .move_nodes(&[vec![0, 1], vec![0, 2]], &[0], 0)
            .expect("move");
        assert_eq!(glued_titles(&state), ["C", "+D", "B"]);
        // A sheet moved out on its own leaves its run; the sheet it was glued to stays.
        state.move_nodes(&[vec![0, 1]], &[0], 3).expect("move");
        assert_eq!(glued_titles(&state), ["C", "B", "D"]);
        // A copy made inside a run stays in it.
        state.glue_sheets(&[vec![0, 0], vec![0, 1]]).expect("glue");
        state.duplicate_node(&[0, 0]).expect("duplicate");
        assert_eq!(glued_titles(&state), ["C", "+C", "+B", "D"]);
        state.restore_from_trash(0, Some(&[0])).expect("restore");
        assert_eq!(glued_titles(&state).last().map(String::as_str), Some("A"));
    }

    #[test]
    fn icloud_projects_save_to_their_own_store_and_reload() {
        let local = tempfile::tempdir().expect("tempdir");
        let icloud = tempfile::tempdir().expect("tempdir");
        let mut state = AppState::new();
        state.library_path = Some(local.path().to_path_buf());
        state.icloud_path = Some(icloud.path().to_path_buf());
        state.create_project("Local Novel");
        state.create_project_in_section("iCloud Novel", parrot_core::Section::ICloud);
        state.create_sheet(&[0], "A").expect("sheet");
        state.create_sheet(&[1], "B").expect("sheet");
        state.save().expect("save");
        assert!(
            local.path().join("Local Novel.ulproject").is_dir(),
            "local project in the local store"
        );
        assert!(
            icloud.path().join("iCloud Novel.ulproject").is_dir(),
            "icloud project in the icloud store"
        );
        assert!(
            !local.path().join("iCloud Novel.ulproject").exists(),
            "icloud project not duplicated locally"
        );

        let mut reopened = AppState::new();
        reopened.library_path = Some(local.path().to_path_buf());
        reopened.icloud_path = Some(icloud.path().to_path_buf());
        reopened.open_library(local.path()).expect("open combined");
        assert_eq!(titles(&reopened, &[]), ["Local Novel", "iCloud Novel"]);
        assert_eq!(reopened.icloud_projects(), vec![vec![1]]);
        assert!(
            matches!(
                reopened.node(&[1]),
                Ok(Node::Project(p)) if p.section.is_icloud()
            ),
            "reloaded iCloud project keeps its section"
        );
    }

    #[test]
    fn moving_a_project_between_sections_rewrites_its_store() {
        let local = tempfile::tempdir().expect("tempdir");
        let icloud = tempfile::tempdir().expect("tempdir");
        let mut state = AppState::new();
        state.library_path = Some(local.path().to_path_buf());
        state.icloud_path = Some(icloud.path().to_path_buf());
        state.create_project("Nomad");
        state.save().expect("save");
        assert!(local.path().join("Nomad.ulproject").is_dir());
        state.move_project_to_icloud(&[0]).expect("move to icloud");
        state.save().expect("save");
        assert!(
            icloud.path().join("Nomad.ulproject").is_dir(),
            "project lands in the icloud store after the move"
        );
        state.move_project_to_local(&[0]).expect("move back");
        state.save().expect("save");
        assert!(
            local.path().join("Nomad.ulproject").is_dir(),
            "project returns to the local store"
        );
    }
}
