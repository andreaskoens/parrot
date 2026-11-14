//! Library/Group directory tree I/O: a Library root is a directory whose children are one
//! entry per node — `<title>.parrot` (a sheet package, T-0-06) or `<title>.ulgroup` (a nested
//! directory of its own children).
//!
//! // ASSUMPTION(D2): `.ulgroup`'s internal structure was never independently confirmed in the
//! // source inventory — only `.parrot` sample files were found there. This mirrors the
//! // confirmed `.parrot` directory-package convention by analogy, per roadmap Decision D2.
//!
//! Sibling order is kept in a `.order` sidecar per directory (one entry name per line), so a
//! drag-and-drop reorder survives a save/reopen. Entries the sidecar doesn't list (a library
//! from before it existed, a file copied in by hand) follow in file-name order.
//!
//! Glued sheets (`Sheet::glued`) are kept in a `.glued` sidecar per directory: the entry name
//! of each sheet glued to its previous sibling, one per line. Draft Sheets (`Sheet::draft`)
//! are listed the same way in `.draft`. Each sidecar is omitted when it would be empty.

use crate::sheet::{self, SheetIoError};
use parrot_core::{Group, Library, Node, Project, Section, Sheet};
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

const SHEET_SUFFIX: &str = ".parrot";
/// The sheet suffix from before the rename; still read, and rewritten as `.parrot` on save.
const LEGACY_SHEET_SUFFIX: &str = ".ulysses";
const GROUP_SUFFIX: &str = ".ulgroup";
const PROJECT_SUFFIX: &str = ".ulproject";
/// The per-directory sibling-order sidecar (a dotfile, so it's never read as a node).
const ORDER_FILE: &str = ".order";
/// The per-directory glued-sheets sidecar.
const GLUED_FILE: &str = ".glued";
/// The per-directory Draft Sheets sidecar.
const DRAFT_FILE: &str = ".draft";

#[derive(Debug, Error)]
pub enum TreeIoError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("sheet io error: {0}")]
    Sheet(#[from] SheetIoError),
}

/// Saves a Library's full tree under `root`.
///
/// File names come from node titles via [`entry_stem`], made unique per directory (two
/// "Untitled" sheets must not overwrite each other). Entries left over from an earlier save —
/// a renamed, moved or trashed node — are moved into `root/.removed/` rather than deleted, so
/// they neither reappear on the next open nor are lost outright. Filters aren't persisted yet
/// (T-1-06) and are skipped rather than failing the whole save.
pub fn save_library(root: &Path, library: &Library) -> Result<(), TreeIoError> {
    fs::create_dir_all(root)?;
    save_children(root, &library.children, root)
}

/// Reads a Library's full tree back from `root`.
pub fn open_library(root: &Path) -> Result<Library, TreeIoError> {
    Ok(Library {
        children: open_children(root)?,
    })
}

/// Whether `root` already holds library entries — `create_library` must not save an empty
/// tree over an existing library.
pub fn has_library_entries(root: &Path) -> bool {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| node_suffix(&e.file_name().to_string_lossy()).is_some())
        })
        .unwrap_or(false)
}

fn node_suffix(file_name: &str) -> Option<&'static str> {
    [
        SHEET_SUFFIX,
        LEGACY_SHEET_SUFFIX,
        GROUP_SUFFIX,
        PROJECT_SUFFIX,
    ]
    .into_iter()
    .find(|suffix| file_name.ends_with(suffix) && !file_name.starts_with('.'))
}

/// A title as a portable file-name stem: path separators and Windows-reserved characters
/// become `-`, leading dots (hidden, skipped on open) and trailing dots/spaces (stripped by
/// Windows) are removed, and reserved device names (`CON`, `NUL`, `COM1`…) get a `_` suffix.
pub fn entry_stem(title: &str) -> String {
    let replaced: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let mut stem = replaced
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string();
    if stem.trim().is_empty() {
        stem = "Untitled".to_string();
    }
    let upper = stem.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit());
    if device {
        stem.push('_');
    }
    stem
}

/// The entry name for `title` + `suffix`, made unique among the names already `written` in
/// its directory (case-insensitively: Windows treats "A" and "a" as the same entry).
fn unique_name(
    written: &mut std::collections::HashSet<String>,
    title: &str,
    suffix: &str,
) -> String {
    let stem = entry_stem(title);
    let mut name = format!("{stem}{suffix}");
    let mut n = 2;
    while !written.insert(name.to_lowercase()) {
        name = format!("{stem} {n}{suffix}");
        n += 1;
    }
    name
}

/// Where the node at `path` lives (or will live, once saved) under the library `root`:
/// the same directory names [`save_library`] gives it. `None` for a path that doesn't lead
/// to a sheet, section or project (filters aren't stored).
pub fn node_disk_path(
    root: &Path,
    library: &Library,
    path: &[usize],
) -> Option<std::path::PathBuf> {
    let mut dir = root.to_path_buf();
    let mut children = &library.children;
    for (depth, &index) in path.iter().enumerate() {
        let mut written = std::collections::HashSet::new();
        let mut name = None;
        for (i, node) in children.iter().enumerate() {
            let (title, suffix) = match node {
                Node::Sheet(s) => (&s.title, SHEET_SUFFIX),
                Node::Group(g) => (&g.title, GROUP_SUFFIX),
                Node::Project(p) => (&p.title, PROJECT_SUFFIX),
                Node::Filter(_) => {
                    if i == index {
                        return None;
                    }
                    continue;
                }
            };
            let unique = unique_name(&mut written, title, suffix);
            if i == index {
                name = Some(unique);
                break;
            }
        }
        dir.push(name?);
        if depth + 1 < path.len() {
            children = match children.get(index)? {
                Node::Group(g) => &g.children,
                Node::Project(p) => &p.children,
                _ => return None,
            };
        }
    }
    Some(dir)
}

pub(crate) fn save_children(dir: &Path, children: &[Node], root: &Path) -> Result<(), TreeIoError> {
    let mut written = std::collections::HashSet::new();
    let mut order = Vec::new();
    let mut glued: Vec<String> = Vec::new();
    let mut drafts: Vec<String> = Vec::new();
    let mut unique = |title: &str, suffix: &str| {
        let name = unique_name(&mut written, title, suffix);
        order.push(name.clone());
        dir.join(name)
    };
    for node in children {
        match node {
            Node::Sheet(s) => {
                let path = unique(&s.title, SHEET_SUFFIX);
                sheet::save(&path, &s.content, &s.attachments)?;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                if s.glued {
                    glued.extend(name.clone());
                }
                if s.draft {
                    drafts.extend(name);
                }
            }
            Node::Group(g) => {
                let path = unique(&g.title, GROUP_SUFFIX);
                fs::create_dir_all(&path)?;
                save_children(&path, &g.children, root)?;
            }
            Node::Project(p) => {
                let path = unique(&p.title, PROJECT_SUFFIX);
                fs::create_dir_all(&path)?;
                write_project_meta(&path, p.language.as_deref(), p.section)?;
                save_children(&path, &p.children, root)?;
            }
            Node::Filter(_) => {}
        }
    }
    let mut listing = order.join("\n");
    listing.push('\n');
    fs::write(dir.join(ORDER_FILE), listing)?;
    write_sidecar(&dir.join(GLUED_FILE), &glued)?;
    write_sidecar(&dir.join(DRAFT_FILE), &drafts)?;
    for entry in fs::read_dir(dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if node_suffix(&name).is_some() && !written.contains(&name.to_lowercase()) {
            set_aside(root, &entry.path(), &name)?;
        }
    }
    Ok(())
}

/// Per-project metadata sidecar (`key=value` lines). Keeps project settings out of the
/// directory name, which is only the title.
const PROJECT_META_FILE: &str = ".project-meta";

/// Writes the project's meta file in `dir`, or removes it when there is nothing to store.
/// `section=icloud` is written for iCloud projects; local is the default and writes no
/// section line so old libraries stay byte-identical.
pub(crate) fn write_project_meta(
    dir: &Path,
    language: Option<&str>,
    section: Section,
) -> Result<(), TreeIoError> {
    let path = dir.join(PROJECT_META_FILE);
    let mut lines = Vec::new();
    if let Some(l) = language.filter(|l| !l.trim().is_empty()) {
        lines.push(format!("language={}\n", l.trim()));
    }
    if section.is_icloud() {
        lines.push("section=icloud\n".to_string());
    }
    if lines.is_empty() {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    } else {
        fs::write(path, lines.concat())?;
    }
    Ok(())
}

/// The project's stored language code, if its meta file has one.
pub(crate) fn read_project_meta(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join(PROJECT_META_FILE)).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("language="))
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

/// The project's stored section; missing (old libraries) means `Local`.
pub(crate) fn read_project_section(dir: &Path) -> Section {
    let Ok(text) = fs::read_to_string(dir.join(PROJECT_META_FILE)) else {
        return Section::Local;
    };
    text.lines()
        .find_map(|l| l.strip_prefix("section="))
        .map(Section::from_label)
        .unwrap_or(Section::Local)
}

/// Writes `names` one per line to the sidecar at `path`, or removes it when there are none.
fn write_sidecar(path: &Path, names: &[String]) -> Result<(), TreeIoError> {
    if names.is_empty() {
        match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    } else {
        let mut listing = names.join("\n");
        listing.push('\n');
        fs::write(path, listing)?;
    }
    Ok(())
}

fn read_sidecar(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Moves a stale entry into `root/.removed/`, numbering it if that name is already taken.
fn set_aside(root: &Path, path: &Path, name: &str) -> Result<(), TreeIoError> {
    let bin = root.join(".removed");
    fs::create_dir_all(&bin)?;
    let mut dest = bin.join(name);
    let mut n = 2;
    while dest.exists() {
        dest = bin.join(format!("{n} {name}"));
        n += 1;
    }
    fs::rename(path, dest)?;
    Ok(())
}

pub(crate) fn open_children(dir: &Path) -> Result<Vec<Node>, TreeIoError> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, io::Error>>()?;
    let order: Vec<String> = fs::read_to_string(dir.join(ORDER_FILE))
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let glued = read_sidecar(&dir.join(GLUED_FILE));
    let drafts = read_sidecar(&dir.join(DRAFT_FILE));
    // Listed entries in sidecar order, then the rest by name.
    entries.sort_by_key(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let rank = order.iter().position(|o| *o == name).unwrap_or(usize::MAX);
        (rank, name)
    });

    let mut children = Vec::new();
    for entry in entries {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();

        // Skip dotfiles/dot-directories: sheet.rs's atomic-write temp siblings (and any other
        // hidden filesystem noise) are not tree entries.
        if file_name.starts_with('.') {
            continue;
        }

        if let Some(title) = file_name
            .strip_suffix(SHEET_SUFFIX)
            .or_else(|| file_name.strip_suffix(LEGACY_SHEET_SUFFIX))
        {
            let (content, attachments) = sheet::open(&path)?;
            children.push(Node::Sheet(Sheet {
                title: title.to_string(),
                content,
                // T-10-02/T-10-04: Goal/Keywords aren't persisted to disk yet — see project.rs's
                // `open()` note. Attachments (T-10-11) are, via sheet::open above.
                goal: None,
                keywords: Vec::new(),
                attachments,
                // Only a sheet right after another sheet can be glued to it.
                glued: glued.contains(&file_name)
                    && matches!(children.last(), Some(Node::Sheet(_))),
                draft: drafts.contains(&file_name),
            }));
        } else if let Some(title) = file_name.strip_suffix(GROUP_SUFFIX) {
            let grandchildren = open_children(&path)?;
            children.push(Node::Group(Group {
                title: title.to_string(),
                children: grandchildren,
                goal: None,
            }));
        } else if let Some(title) = file_name.strip_suffix(PROJECT_SUFFIX) {
            let grandchildren = open_children(&path)?;
            children.push(Node::Project(Project {
                title: title.to_string(),
                children: grandchildren,
                goal: None,
                language: read_project_meta(&path),
                section: read_project_section(&path),
            }));
        }
        // Anything else (Thumbs.db, desktop.ini, a stray note) isn't a node: skip it rather
        // than refuse to open the whole library.
    }
    Ok(children)
}

#[cfg(test)]
mod tests {
    #[test]
    fn node_disk_path_matches_what_save_writes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = |title: &str| {
            Node::Project(Project {
                title: title.to_string(),
                children: Vec::new(),
                goal: None,
                language: None,
                section: Section::Local,
            })
        };
        let library = Library {
            children: vec![project("Essays"), project("essays"), project("A/B")],
        };
        save_library(dir.path(), &library).expect("save");
        for (i, expected) in ["Essays.ulproject", "essays 2.ulproject", "A-B.ulproject"]
            .iter()
            .enumerate()
        {
            let path = node_disk_path(dir.path(), &library, &[i]).expect("path");
            assert_eq!(path, dir.path().join(expected));
            assert!(path.is_dir(), "{expected} exists on disk");
        }
        assert_eq!(node_disk_path(dir.path(), &library, &[9]), None);
    }

    use super::*;
    use parrot_core::{Inline, Paragraph, SheetContent, TagKind, TagMark};

    fn sheet_node(title: &str, heading: &str) -> Node {
        Node::Sheet(Sheet {
            title: title.to_string(),
            content: SheetContent::new(vec![Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text(heading.to_string())],
            }]),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        })
    }

    #[test]
    fn two_level_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");

        let library = Library {
            children: vec![Node::Group(Group {
                title: "Drafts".to_string(),
                goal: None,
                children: vec![
                    sheet_node("Sheet A", "First sheet"),
                    sheet_node("Sheet B", "Second sheet"),
                ],
            })],
        };

        save_library(dir.path(), &library).expect("save_library must succeed");
        let reopened = open_library(dir.path()).expect("open_library must succeed");

        assert_eq!(library, reopened);

        // Structure sanity: the group is a real directory, each sheet a real sub-directory.
        let group_path = dir.path().join("Drafts.ulgroup");
        assert!(group_path.is_dir());
        assert!(group_path.join("Sheet A.parrot").is_dir());
        assert!(group_path.join("Sheet B.parrot").is_dir());
    }

    fn titles(library: &Library) -> Vec<String> {
        library
            .children
            .iter()
            .map(|n| match n {
                Node::Sheet(s) => s.title.clone(),
                Node::Group(g) => g.title.clone(),
                Node::Project(p) => p.title.clone(),
                Node::Filter(f) => f.title.clone(),
            })
            .collect()
    }

    #[test]
    fn same_titled_sheets_do_not_overwrite_each_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library {
            children: vec![
                sheet_node("Untitled", "one"),
                sheet_node("Untitled", "two"),
                sheet_node("untitled", "three"),
            ],
        };
        save_library(dir.path(), &library).expect("save");
        let reopened = open_library(dir.path()).expect("open");
        assert_eq!(titles(&reopened), ["Untitled", "Untitled 2", "untitled 3"]);
    }

    #[test]
    fn sibling_order_survives_a_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let group = Node::Group(Group {
            title: "G".to_string(),
            children: vec![sheet_node("Zeta", "z"), sheet_node("Alpha", "a")],
            goal: None,
        });
        let library = Library {
            children: vec![sheet_node("b", "1"), group, sheet_node("a", "2")],
        };
        save_library(dir.path(), &library).expect("save");
        // A file copied in by hand, unknown to the sidecar, comes last.
        save_library(
            &dir.path().join("x"),
            &Library {
                children: vec![sheet_node("Extra", "e")],
            },
        )
        .expect("save extra");
        fs::rename(
            dir.path().join("x").join("Extra.parrot"),
            dir.path().join("Extra.parrot"),
        )
        .expect("move extra");
        let reopened = open_library(dir.path()).expect("open");
        assert_eq!(titles(&reopened), ["b", "G", "a", "Extra"]);
        let Node::Group(g) = &reopened.children[1] else {
            panic!("expected the group second");
        };
        let inner: Vec<_> = g
            .children
            .iter()
            .map(|n| match n {
                Node::Sheet(s) => s.title.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(inner, ["Zeta", "Alpha"]);
    }

    #[test]
    fn draft_sheets_survive_a_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut library = Library {
            children: vec![
                sheet_node("A", "a"),
                match sheet_node("B", "b") {
                    Node::Sheet(s) => Node::Sheet(Sheet { draft: true, ..s }),
                    other => other,
                },
            ],
        };
        save_library(dir.path(), &library).expect("save");
        assert!(dir.path().join(DRAFT_FILE).is_file());
        assert_eq!(open_library(dir.path()).expect("open"), library);

        library.children[1] = sheet_node("B", "b");
        save_library(dir.path(), &library).expect("save");
        assert!(!dir.path().join(DRAFT_FILE).exists());
        assert_eq!(open_library(dir.path()).expect("open"), library);
    }

    #[test]
    fn glued_sheets_survive_a_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let glue = |node: Node| match node {
            Node::Sheet(s) => Node::Sheet(Sheet { glued: true, ..s }),
            other => other,
        };
        let mut library = Library {
            children: vec![
                sheet_node("A", "a"),
                glue(sheet_node("B", "b")),
                sheet_node("C", "c"),
            ],
        };
        save_library(dir.path(), &library).expect("save");
        assert!(dir.path().join(GLUED_FILE).is_file());
        assert_eq!(open_library(dir.path()).expect("open"), library);

        // Unglued again: the sidecar goes away.
        library.children[1] = sheet_node("B", "b");
        save_library(dir.path(), &library).expect("save");
        assert!(!dir.path().join(GLUED_FILE).exists());
        assert_eq!(open_library(dir.path()).expect("open"), library);
    }

    #[test]
    fn renamed_and_removed_nodes_do_not_reappear_but_are_kept_aside() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut library = Library {
            children: vec![sheet_node("Old", "a"), sheet_node("Gone", "b")],
        };
        save_library(dir.path(), &library).expect("save");
        library.children.pop();
        if let Node::Sheet(s) = &mut library.children[0] {
            s.title = "New".to_string();
        }
        save_library(dir.path(), &library).expect("save again");
        let reopened = open_library(dir.path()).expect("open");
        assert_eq!(titles(&reopened), ["New"]);
        let removed = dir.path().join(".removed");
        assert!(removed.join("Old.parrot").is_dir());
        assert!(removed.join("Gone.parrot").is_dir());
    }

    #[test]
    fn unsafe_titles_stay_inside_the_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library {
            children: vec![
                sheet_node("../escape", "a"),
                sheet_node("a/b:c", "b"),
                sheet_node(".hidden", "c"),
                sheet_node("CON", "d"),
                sheet_node("", "e"),
            ],
        };
        save_library(dir.path(), &library).expect("save");
        assert!(!dir
            .path()
            .parent()
            .expect("parent")
            .join("escape.parrot")
            .exists());
        let reopened = open_library(dir.path()).expect("open");
        assert_eq!(
            titles(&reopened),
            ["-escape", "a-b-c", "hidden", "CON_", "Untitled"]
        );
    }

    #[test]
    fn filters_are_skipped_and_foreign_files_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library {
            children: vec![
                sheet_node("Kept", "a"),
                Node::Filter(parrot_core::Filter {
                    title: "Recent".to_string(),
                    keyword: None,
                    keyword_tag: None,
                    node_kind: parrot_core::NodeKindMatch::Any,
                    date_range: None,
                }),
            ],
        };
        save_library(dir.path(), &library).expect("a filter must not fail the save");
        fs::write(dir.path().join("Thumbs.db"), b"x").expect("write");
        fs::write(dir.path().join("desktop.ini"), b"x").expect("write");
        let reopened = open_library(dir.path()).expect("foreign files must not block open");
        assert_eq!(titles(&reopened), ["Kept"]);
        assert!(has_library_entries(dir.path()));
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(!has_library_entries(empty.path()));
    }

    #[test]
    fn project_section_survives_a_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = Library {
            children: vec![
                Node::Project(Project {
                    title: "Local".to_string(),
                    children: Vec::new(),
                    goal: None,
                    language: None,
                    section: Section::Local,
                }),
                Node::Project(Project {
                    title: "Cloud".to_string(),
                    children: Vec::new(),
                    goal: None,
                    language: Some("en-US".to_string()),
                    section: Section::ICloud,
                }),
            ],
        };
        save_library(dir.path(), &library).expect("save");
        // Local writes no section line (old libraries stay byte-identical);
        // iCloud writes `section=icloud` alongside the language.
        let local_meta =
            fs::read_to_string(dir.path().join("Local.ulproject").join(".project-meta"))
                .unwrap_or_default();
        assert!(!local_meta.contains("section"), "local is the default");
        let cloud_meta =
            fs::read_to_string(dir.path().join("Cloud.ulproject").join(".project-meta"))
                .expect("icloud meta");
        assert!(cloud_meta.contains("section=icloud"));
        assert!(cloud_meta.contains("language=en-US"));
        assert_eq!(open_library(dir.path()).expect("open"), library);
    }
}
