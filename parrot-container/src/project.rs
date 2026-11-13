//! `.ulproject` directory package I/O — a Group-like directory variant per roadmap Decision D2.
//!
//! // ASSUMPTION(D2): no real `.ulproject` sample was found in the source inventory (only
//! // `.parrot` sheet samples) — this mirrors the confirmed directory-package convention by
//! // analogy, same as `.ulgroup` in tree.rs.

use crate::tree::{self, TreeIoError};
use parrot_core::Project;
use std::fs;
use std::path::Path;

/// Saves a `.ulproject` directory package at `path`.
pub fn save(path: &Path, project: &Project) -> Result<(), TreeIoError> {
    fs::create_dir_all(path)?;
    tree::write_project_meta(path, project.language.as_deref())?;
    tree::save_children(path, &project.children, path)
}

/// Reads a `.ulproject` directory package back from `path`. `title` is supplied by the caller
/// (it names the directory itself, the same convention `tree.rs` uses for Group/Sheet).
pub fn open(path: &Path, title: &str) -> Result<Project, TreeIoError> {
    Ok(Project {
        title: title.to_string(),
        children: tree::open_children(path)?,
        // T-10-02: Goal isn't persisted to disk yet — out of this task's scope (see
        // DECISIONS.md), same deferred-persistence pattern as filter.rs's `.ulfilter` note.
        goal: None,
        language: tree::read_project_meta(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Group, Inline, Node, Paragraph, Sheet, SheetContent, TagKind, TagMark};

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("My Project.ulproject");

        let project = Project {
            language: Some("de".to_string()),
            title: "My Project".to_string(),
            children: vec![
                Node::Group(Group {
                    title: "Drafts".to_string(),
                    goal: None,
                    children: vec![Node::Sheet(Sheet {
                        title: "Chapter 1".to_string(),
                        content: SheetContent::new(vec![Paragraph {
                            tag: Some(TagMark {
                                kind: TagKind::Heading(1),
                                literal: "# ".to_string(),
                            }),
                            inlines: vec![Inline::Text("Chapter 1".to_string())],
                        }]),
                        goal: None,
                        keywords: Vec::new(),
                        attachments: Vec::new(),
                        glued: false,
                        draft: false,
                    })],
                }),
                Node::Sheet(Sheet {
                    title: "Notes".to_string(),
                    content: SheetContent::new(vec![]),
                    goal: None,
                    keywords: Vec::new(),
                    attachments: Vec::new(),
                    glued: false,
                    draft: false,
                }),
            ],
            goal: None,
        };

        save(&path, &project).expect("save must succeed");
        let reopened = open(&path, "My Project").expect("open must succeed");

        assert_eq!(project, reopened);
        assert!(path.join("Drafts.ulgroup").is_dir());
        assert!(path.join("Notes.parrot").is_dir());
    }
}
