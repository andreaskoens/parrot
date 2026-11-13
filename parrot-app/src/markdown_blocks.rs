//! Turns Markdown laid out line by line into the editor's block layout. The editor (and
//! `parrot_markup::parse`) starts a block only after a blank line, but Markdown files and pasted
//! notes put headings, list items, quotes and rules on consecutive lines — left as they are,
//! only the first line of such a run is formatted and the rest shows as raw `###` or `-`.
//! [`normalize_blocks`] gives every block-level line its own paragraph (`"\n\n"`-separated) in the
//! form the editor types: ATX headings without closing hashes, `-` / `1.` items nested two
//! spaces a level (`*`, `+`, `•` and `1)` accepted), `> ` quotes, `----` dividers and
//! ```` ```lang code ```` fences.

/// What a block-level line is.
enum Line {
    /// A heading or divider: one line, ends its block.
    Standalone(String),
    /// A list item; following non-marker lines continue it.
    Item(String),
    /// A quote line's text; following quote lines join the same quote.
    Quote(String),
}

/// `text` with every block-level line in its own paragraph. Plain lines stay together as a
/// paragraph with soft line breaks; code, comment and native blocks keep all their lines.
pub fn normalize_blocks(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let step = nesting_step(&lines);
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    // The current block holds code or a comment: its lines stick to it as they are.
    let mut raw = false;
    // The current block is a quote, so the next quote line extends it.
    let mut quote = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let flush = |current: &mut Option<String>, blocks: &mut Vec<String>| {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
        };
        if line.trim().is_empty() {
            flush(&mut current, &mut blocks);
            raw = false;
            quote = false;
            i += 1;
            continue;
        }
        if raw {
            if let Some(block) = &mut current {
                block.push('\n');
                block.push_str(line.trim_end());
            }
            i += 1;
            continue;
        }
        if let Some(language) = fence_language(line) {
            if let Some(close) = (i + 1..lines.len()).find(|&j| lines[j].trim() == "```") {
                flush(&mut current, &mut blocks);
                quote = false;
                // A blank line inside code would end the paragraph, so it becomes a lone space.
                let code: Vec<&str> = lines[i + 1..close]
                    .iter()
                    .map(|l| {
                        if l.trim().is_empty() {
                            " "
                        } else {
                            l.trim_end()
                        }
                    })
                    .collect();
                blocks.push(format!("```{language} {}", code.join("\n")));
                i = close + 1;
                continue;
            }
        }
        if is_raw_start(line) {
            flush(&mut current, &mut blocks);
            quote = false;
            current = Some(line.to_string());
            raw = true;
            i += 1;
            continue;
        }
        match block_line(line, step) {
            Some(Line::Standalone(block)) => {
                flush(&mut current, &mut blocks);
                quote = false;
                blocks.push(block);
            }
            Some(Line::Item(block)) => {
                flush(&mut current, &mut blocks);
                quote = false;
                current = Some(block);
            }
            Some(Line::Quote(text)) => match (&mut current, quote) {
                (Some(block), true) => {
                    block.push('\n');
                    block.push_str(&text);
                }
                _ => {
                    flush(&mut current, &mut blocks);
                    current = Some(format!("> {text}"));
                    quote = true;
                }
            },
            None => match &mut current {
                Some(block) => {
                    block.push('\n');
                    block.push_str(line.trim_end());
                }
                None => current = Some(line.trim_end().to_string()),
            },
        }
        i += 1;
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks.join("\n\n")
}

/// The language of a Markdown code fence line (```` ``` ```` or ```` ```rust ````), if that's
/// all the line is. The editor's own `` ```lang `` + space + code form is not a fence.
fn fence_language(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("```")?;
    rest.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '+' | '#' | '.'))
        .then_some(rest)
}

/// Whether `line` opens a block whose following lines are raw: the editor's code
/// (```` ```lang code ````), comment (`%% `) and native (`~~ `) blocks.
fn is_raw_start(line: &str) -> bool {
    line.starts_with("```") || ["%% ", "~~ ", "'' "].iter().any(|m| line.starts_with(m))
}

/// Leading whitespace of `line` in columns (a tab is four) and the rest.
fn split_indent(line: &str) -> (usize, &str) {
    let mut columns = 0;
    for (at, c) in line.char_indices() {
        match c {
            ' ' => columns += 1,
            '\t' => columns += 4,
            _ => return (columns, &line[at..]),
        }
    }
    (columns, "")
}

/// Columns one nesting level is indented by in `lines`: four when every indented list line
/// steps in fours (Markdown's usual), otherwise two (the editor's own).
fn nesting_step(lines: &[&str]) -> usize {
    let indents: Vec<usize> = lines
        .iter()
        .filter_map(|line| {
            let (indent, rest) = split_indent(line.trim_end());
            list_marker(rest).map(|_| indent)
        })
        .filter(|&indent| indent > 0)
        .collect();
    if !indents.is_empty() && indents.iter().all(|i| i % 4 == 0) {
        4
    } else {
        2
    }
}

/// The marker of a list item at the start of `rest` — `-`/`1.` normalized — and its text. A
/// marker alone is an empty item.
fn list_marker(rest: &str) -> Option<(String, &str)> {
    let first = rest.chars().next()?;
    if matches!(first, '-' | '*' | '+' | '•') {
        let after = &rest[first.len_utf8()..];
        if after.is_empty() {
            return Some(("-".to_string(), ""));
        }
        return after
            .strip_prefix(' ')
            .map(|text| ("-".to_string(), text.trim()));
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        if after == "." || after == ")" {
            return Some((format!("{}.", &rest[..digits]), ""));
        }
        if let Some(text) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            return Some((format!("{}.", &rest[..digits]), text.trim()));
        }
    }
    None
}

fn block_line(line: &str, step: usize) -> Option<Line> {
    let (indent, rest) = split_indent(line.trim_end());
    if rest.is_empty() {
        return None;
    }

    if indent <= 3 {
        let hashes = rest.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && (rest.len() == hashes || rest[hashes..].starts_with(' ')) {
            let mut title = rest[hashes..].trim();
            // `### Title ###`: closing hashes aren't part of the title.
            let without = title.trim_end_matches('#');
            if without.len() != title.len() && (without.is_empty() || without.ends_with(' ')) {
                title = without.trim_end();
            }
            return Some(Line::Standalone(format!("{} {title}", "#".repeat(hashes))));
        }
        let first = rest.chars().next()?;
        if rest.len() >= 3 && matches!(first, '-' | '*' | '_') && rest.chars().all(|c| c == first) {
            return Some(Line::Standalone("----".to_string()));
        }
        if rest == ">" || rest.starts_with("> ") {
            return Some(Line::Quote(rest[1..].trim_start().to_string()));
        }
    }

    let (marker, text) = list_marker(rest)?;
    let level = (indent / step).min(6);
    Some(Line::Item(format!("{}{marker} {text}", "  ".repeat(level))))
}

#[cfg(test)]
mod tests {
    use super::normalize_blocks;

    #[test]
    fn every_block_line_gets_its_own_paragraph() {
        let md = "# Title\nIntro line\ncontinues\n## Sub\n- a\n- b\n1. one\n2. two\n> quote\n> more\nafter";
        assert_eq!(
            normalize_blocks(md),
            "# Title\n\nIntro line\ncontinues\n\n## Sub\n\n- a\n\n- b\n\n1. one\n\n2. two\n\n> quote\nmore\nafter"
        );
    }

    #[test]
    fn headings_are_cleaned_up_to_the_editors_form() {
        assert_eq!(normalize_blocks("### Title ###"), "### Title");
        assert_eq!(normalize_blocks("   ## Indented"), "## Indented");
        assert_eq!(normalize_blocks("###### Six"), "###### Six");
        assert_eq!(normalize_blocks("C# is fine"), "C# is fine");
        assert_eq!(normalize_blocks("#NoSpace"), "#NoSpace");
        assert_eq!(normalize_blocks("####### seven"), "####### seven");
    }

    #[test]
    fn bullets_of_every_kind_become_dash_items_nested_by_indent() {
        let md = "* star\n+ plus\n• dot\n- dash\n  - two spaces\n    - four more\n1) paren\n  2. nested number";
        assert_eq!(
            normalize_blocks(md),
            "- star\n\n- plus\n\n- dot\n\n- dash\n\n  - two spaces\n\n    - four more\n\n1. paren\n\n  2. nested number"
        );
        // When every indent steps in fours (or tabs), each step is one level.
        assert_eq!(
            normalize_blocks("- a\n    - b\n\t\t- c\n- d"),
            "- a\n\n  - b\n\n    - c\n\n- d"
        );
        // Empty items and empty headings keep their marker.
        assert_eq!(normalize_blocks("- \n1. \n#"), "- \n\n1. \n\n# ");
    }

    #[test]
    fn dividers_and_emphasis_are_told_apart() {
        assert_eq!(normalize_blocks("a\n\n***\n\nb"), "a\n\n----\n\nb");
        assert_eq!(normalize_blocks("___"), "----");
        assert_eq!(normalize_blocks("*emphasis* text"), "*emphasis* text");
        assert_eq!(normalize_blocks("**bold** first"), "**bold** first");
        assert_eq!(normalize_blocks("-no space"), "-no space");
        assert_eq!(normalize_blocks("--"), "--");
    }

    #[test]
    fn a_list_item_keeps_its_continuation_lines() {
        assert_eq!(
            normalize_blocks("- item\n  continues here\n- next"),
            "- item\n  continues here\n\n- next"
        );
    }

    #[test]
    fn fenced_code_becomes_one_block_and_code_lines_are_never_reformatted() {
        let md = "before\n```rust\n# not a heading\n- not a list\n\nstill code\n```\nafter";
        assert_eq!(
            normalize_blocks(md),
            "before\n\n```rust # not a heading\n- not a list\n \nstill code\n\nafter"
        );
        // The editor's own forms are left alone, their extra lines kept.
        let own = "``` first\n# comment\n- x\n\n%% note\n- not a list";
        assert_eq!(normalize_blocks(own), own);
        // A fence that never closes is the editor's own (empty) code block: its lines stay.
        assert_eq!(normalize_blocks("```\n# T"), "```\n# T");
    }

    #[test]
    fn text_already_in_the_editors_form_is_unchanged() {
        let text =
            "# Title\n\nSome **bold** text\n\n- a\n\n  - b\n\n1. one\n\n> quote\n\n----\n\nend";
        assert_eq!(normalize_blocks(text), text);
        assert_eq!(normalize_blocks(&normalize_blocks(text)), text);
        assert_eq!(normalize_blocks(""), "");
        assert_eq!(normalize_blocks("a\r\n# b\r\n"), "a\n\n# b");
    }
}
