//! Library tree node types (feature inventory §2: `ULLibrary`/`ULSheet`/`ULGroup`/`ULProject`/
//! `ULFilter`). Project is a Group-like directory variant (T-1-05); `Filter`'s real query model
//! lives in `crate::filter` (T-1-06) and is just referenced here.

use crate::ast::SheetContent;
use crate::attachment::Attachment;
use crate::filter::Filter;
use crate::goal::Goal;

/// A single document — the atomic editing unit (`ULSheet`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub title: String,
    pub content: SheetContent,
    /// T-10-02: a Sheet/Group/Project may carry its own writing goal (inventory §12).
    pub goal: Option<Goal>,
    /// T-10-04: free-form tags (inventory §15's confirmed Keyword Manager) — see `crate::keyword`.
    pub keywords: Vec<String>,
    /// T-10-11: sheet-level notes/files (inventory §15) — see `crate::attachment`.
    pub attachments: Vec<Attachment>,
    /// Glued to the previous sibling sheet: a run of glued sheets lists as one card and opens
    /// together in the editor (Parrot' Glue Sheets).
    pub glued: bool,
    /// A Draft Sheet: raw material, fragments, references or working notes rather than part
    /// of the finished text. A classification only; the sheet behaves like any other.
    pub draft: bool,
}

/// A folder grouping child nodes (`ULGroup`). Directory-package convention per Decision D2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub title: String,
    pub children: Vec<Node>,
    pub goal: Option<Goal>,
}

/// A workspace unit above groups (`ULProject`). Directory-package convention per Decision D2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub title: String,
    pub children: Vec<Node>,
    pub goal: Option<Goal>,
    /// Proofing language code (e.g. `en-US`, `de`), persisted in the project's `.project-meta`
    /// file. `None` = the app default. Opaque to this crate.
    pub language: Option<String>,
}

/// A node in the library tree (`ULContainer`'s concrete variants).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Sheet(Sheet),
    Group(Group),
    Project(Project),
    Filter(Filter),
}

/// The whole document tree/workspace (`ULLibrary`): a root directory containing top-level nodes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Library {
    pub children: Vec<Node>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SheetContent;

    #[test]
    fn node_enum_round_trips_through_match() {
        let sheet = Node::Sheet(Sheet {
            title: "Untitled".to_string(),
            content: SheetContent::new(vec![]),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        });
        let group = Node::Group(Group {
            title: "Drafts".to_string(),
            goal: None,
            children: vec![sheet],
        });

        // Exhaustive match (no catch-all) so a future missing-variant is a compile error.
        let kind = match &group {
            Node::Sheet(_) => "sheet",
            Node::Group(_) => "group",
            Node::Project(_) => "project",
            Node::Filter(_) => "filter",
        };
        assert_eq!(kind, "group");
    }
}
