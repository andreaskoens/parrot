//! `Filter` (`ULFilter`, feature inventory §2): a saved/smart query over the library tree.
//! `.ulfilter` is a confirmed **non-package** (flat file, not a directory) per inventory §2's
//! table — `Filter` derives `Serialize`/`Deserialize` so it's ready for that, but the actual
//! `.ulfilter` file I/O (format choice, read/write) isn't gated by this task's Done-when and has
//! no named test; deferred to whichever future task actually wires persistent filters in (the
//! Filters UI, T-7-01). See DECISIONS.md.

use crate::node::{Library, Node, Sheet};
use serde::{Deserialize, Serialize};

/// Which node kinds a filter's keyword match applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKindMatch {
    Any,
    Sheet,
    Group,
    Project,
}

/// Date-range criterion — a stub per T-1-06's scope: the fields exist for a future filter UI to
/// populate, but no node in the current AST carries a date yet, so `evaluate` always treats a
/// `DateRange` as satisfied. Revisit once sheets/groups gain real timestamp metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub after: Option<String>,
    pub before: Option<String>,
}

/// A saved/smart query over the library tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub title: String,
    pub keyword: Option<String>,
    /// T-10-04: matches sheets carrying this exact Keyword tag (`crate::keyword`) — distinct
    /// from `keyword` above, which is a substring search over plain-text content, not a tag.
    pub keyword_tag: Option<String>,
    pub node_kind: NodeKindMatch,
    pub date_range: Option<DateRange>,
}

/// Evaluates `filter` over `library`, returning every matching Sheet.
///
/// `node_kind` gates whether sheets are considered at all (`Group`/`Project`-only filters match
/// no sheets, since this evaluator is scoped to sheet results); `keyword`, when present, is a
/// case-insensitive substring match against the sheet's flattened plain-text content;
/// `keyword_tag` (T-10-04), when present, requires an exact match against the sheet's `keywords`
/// tag set — both clauses apply together (AND) when both are set; `date_range` is always
/// satisfied (see its doc comment).
pub fn evaluate<'a>(filter: &Filter, library: &'a Library) -> Vec<&'a Sheet> {
    evaluate_with_paths(filter, library)
        .into_iter()
        .map(|(_, sheet)| sheet)
        .collect()
}

/// Same matches as [`evaluate`], but with each result's tree path included (T-10-06, for the
/// sheet-flow container view — a Filter's live results need an absolute path into the library
/// to be editable, the same reason `search::search_with_paths` exists alongside `search`).
pub fn evaluate_with_paths<'a>(
    filter: &Filter,
    library: &'a Library,
) -> Vec<(Vec<usize>, &'a Sheet)> {
    if matches!(
        filter.node_kind,
        NodeKindMatch::Group | NodeKindMatch::Project
    ) {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut path = Vec::new();
    collect_matching_sheets(&library.children, filter, &mut path, &mut matches);
    matches
}

fn collect_matching_sheets<'a>(
    nodes: &'a [Node],
    filter: &Filter,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, &'a Sheet)>,
) {
    for (i, node) in nodes.iter().enumerate() {
        path.push(i);
        match node {
            Node::Sheet(s) => {
                if sheet_matches(s, filter) {
                    out.push((path.clone(), s));
                }
            }
            Node::Group(g) => collect_matching_sheets(&g.children, filter, path, out),
            Node::Project(p) => collect_matching_sheets(&p.children, filter, path, out),
            Node::Filter(_) => {}
        }
        path.pop();
    }
}

fn sheet_matches(sheet: &Sheet, filter: &Filter) -> bool {
    let keyword_ok = match &filter.keyword {
        None => true,
        Some(keyword) => {
            let haystack = sheet.content.plain_text().to_lowercase();
            haystack.contains(&keyword.to_lowercase())
        }
    };
    let tag_ok = match &filter.keyword_tag {
        None => true,
        Some(tag) => crate::keyword::has(&sheet.keywords, tag),
    };
    keyword_ok && tag_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Inline, SheetContent};
    use crate::node::Group;

    fn sheet(title: &str, text: &str) -> Node {
        tagged_sheet(title, text, &[])
    }

    fn tagged_sheet(title: &str, text: &str, tags: &[&str]) -> Node {
        Node::Sheet(Sheet {
            title: title.to_string(),
            content: SheetContent::new(vec![crate::ast::Paragraph {
                tag: None,
                inlines: vec![Inline::Text(text.to_string())],
            }]),
            goal: None,
            keywords: tags.iter().map(|t| t.to_string()).collect(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        })
    }

    fn fixture_tree() -> Library {
        Library {
            children: vec![
                sheet("Alpha", "the quick brown fox"),
                Node::Group(Group {
                    title: "Drafts".to_string(),
                    goal: None,
                    children: vec![
                        sheet("Beta", "jumps over the lazy dog"),
                        sheet("Gamma", "a quick note about nothing"),
                    ],
                }),
                sheet("Delta", "unrelated content entirely"),
            ],
        }
    }

    #[test]
    fn keyword_match_on_fixture_tree() {
        let library = fixture_tree();
        let filter = Filter {
            title: "contains quick".to_string(),
            keyword: Some("quick".to_string()),
            keyword_tag: None,
            node_kind: NodeKindMatch::Any,
            date_range: None,
        };

        let matches = evaluate(&filter, &library);
        let titles: Vec<&str> = matches.iter().map(|s| s.title.as_str()).collect();

        assert_eq!(titles, vec!["Alpha", "Gamma"]);
    }

    #[test]
    fn keyword_match_is_case_insensitive() {
        let library = fixture_tree();
        let filter = Filter {
            title: "contains FOX".to_string(),
            keyword: Some("FOX".to_string()),
            keyword_tag: None,
            node_kind: NodeKindMatch::Any,
            date_range: None,
        };

        let matches = evaluate(&filter, &library);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].title, "Alpha");
    }

    #[test]
    fn no_keyword_matches_every_sheet() {
        let library = fixture_tree();
        let filter = Filter {
            title: "all sheets".to_string(),
            keyword: None,
            keyword_tag: None,
            node_kind: NodeKindMatch::Any,
            date_range: None,
        };

        assert_eq!(evaluate(&filter, &library).len(), 4);
    }

    #[test]
    fn keyword_tag_match_on_fixture_tree() {
        let library = Library {
            children: vec![
                tagged_sheet("Alpha", "the quick brown fox", &["todo"]),
                Node::Group(Group {
                    title: "Drafts".to_string(),
                    goal: None,
                    children: vec![
                        tagged_sheet("Beta", "jumps over the lazy dog", &["todo", "draft"]),
                        tagged_sheet("Gamma", "a quick note about nothing", &["draft"]),
                    ],
                }),
                tagged_sheet("Delta", "unrelated content entirely", &[]),
            ],
        };

        let filter = Filter {
            title: "tagged todo".to_string(),
            keyword: None,
            keyword_tag: Some("todo".to_string()),
            node_kind: NodeKindMatch::Any,
            date_range: None,
        };

        let matches = evaluate(&filter, &library);
        let titles: Vec<&str> = matches.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn evaluate_with_paths_resolves_back_to_the_matching_sheets() {
        fn resolve_sheet<'a>(nodes: &'a [Node], path: &[usize]) -> &'a Sheet {
            let (&idx, rest) = path.split_first().expect("path must be non-empty");
            match &nodes[idx] {
                Node::Sheet(s) if rest.is_empty() => s,
                Node::Group(g) => resolve_sheet(&g.children, rest),
                Node::Project(p) => resolve_sheet(&p.children, rest),
                other => panic!("path did not resolve to a Sheet: {other:?}"),
            }
        }

        let library = fixture_tree();
        let filter = Filter {
            title: "contains quick".to_string(),
            keyword: Some("quick".to_string()),
            keyword_tag: None,
            node_kind: NodeKindMatch::Any,
            date_range: None,
        };

        let matches = evaluate_with_paths(&filter, &library);
        for (path, sheet) in &matches {
            assert_eq!(resolve_sheet(&library.children, path), *sheet);
        }
        let titles: Vec<&str> = matches.iter().map(|(_, s)| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Alpha", "Gamma"]);
    }
}
