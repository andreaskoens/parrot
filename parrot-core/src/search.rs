//! Library-wide search (T-1-07): a simple linear scan over every Sheet's plain-text rendition.
//!
//! This is a known future-optimization point, not a current requirement — a real library could
//! have thousands of sheets, at which point an index (inverted index, trigram, whatever) would
//! replace this. Correctness first; `parrot-export`'s backlog has no scheduled task for that
//! yet, so don't build it speculatively.

use crate::node::{Library, Node, Sheet};

/// Searches every Sheet in `library` for `query` (case-insensitive substring match against the
/// sheet's flattened plain text), returning every match in tree order.
pub fn search<'a>(library: &'a Library, query: &str) -> Vec<&'a Sheet> {
    search_with_paths(library, query)
        .into_iter()
        .map(|(_, sheet)| sheet)
        .collect()
}

/// Same match as [`search`], but also returns each result's tree path (same shape as
/// `AppState::node`'s `&[usize]`) — added for T-9-03 so a search result can actually be opened,
/// not just displayed.
pub fn search_with_paths<'a>(library: &'a Library, query: &str) -> Vec<(Vec<usize>, &'a Sheet)> {
    let mut matches = Vec::new();
    let mut path = Vec::new();
    collect_matches(&library.children, query, &mut path, &mut matches);
    matches
}

fn collect_matches<'a>(
    nodes: &'a [Node],
    query: &str,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, &'a Sheet)>,
) {
    let needle = query.to_lowercase();
    for (i, node) in nodes.iter().enumerate() {
        path.push(i);
        match node {
            Node::Sheet(s) => {
                if s.content.plain_text().to_lowercase().contains(&needle) {
                    out.push((path.clone(), s));
                }
            }
            Node::Group(g) => collect_matches(&g.children, query, path, out),
            Node::Project(p) => collect_matches(&p.children, query, path, out),
            Node::Filter(_) => {}
        }
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Inline, Paragraph, SheetContent};
    use crate::node::Group;

    fn sheet(title: &str, text: &str) -> Node {
        Node::Sheet(Sheet {
            title: title.to_string(),
            content: SheetContent::new(vec![Paragraph {
                tag: None,
                inlines: vec![Inline::Text(text.to_string())],
            }]),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        })
    }

    /// 50 sheets across a handful of groups; exactly the ones at indices 3, 17, 41 (0-based,
    /// across the flattened tree-order walk) mention "needle".
    fn fixture_tree_of_50() -> Library {
        let mut children = Vec::new();
        for i in 0..50 {
            let text = if matches!(i, 3 | 17 | 41) {
                format!("sheet {i} contains the needle word")
            } else {
                format!("sheet {i} has nothing special in it")
            };
            if i % 10 == 0 {
                children.push(Node::Group(Group {
                    title: format!("Group {}", i / 10),
                    goal: None,
                    children: vec![sheet(&format!("Sheet {i}"), &text)],
                }));
            } else {
                children.push(sheet(&format!("Sheet {i}"), &text));
            }
        }
        Library { children }
    }

    #[test]
    fn finds_matches_across_tree() {
        let library = fixture_tree_of_50();
        let matches = search(&library, "needle");

        let titles: Vec<&str> = matches.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Sheet 3", "Sheet 17", "Sheet 41"]);
    }

    #[test]
    fn search_with_paths_returns_matching_paths() {
        let library = fixture_tree_of_50();
        let matches = search_with_paths(&library, "needle");

        let paths: Vec<Vec<usize>> = matches.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(paths, vec![vec![3], vec![17], vec![41]]);
    }

    #[test]
    fn search_with_paths_resolves_through_nested_groups() {
        let library = Library {
            children: vec![Node::Group(Group {
                title: "Outer".to_string(),
                goal: None,
                children: vec![
                    sheet("Decoy", "nothing here"),
                    sheet("Target", "contains the needle"),
                ],
            })],
        };

        let matches = search_with_paths(&library, "needle");
        let paths: Vec<Vec<usize>> = matches.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(paths, vec![vec![0, 1]]);
    }

    #[test]
    fn no_match_returns_empty() {
        let library = fixture_tree_of_50();
        assert!(search(&library, "nonexistent-term").is_empty());
    }
}
