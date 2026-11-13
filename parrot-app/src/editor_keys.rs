//! Editing behaviour of the sheet editor's keys — Enter, Tab, Backspace and Space — as pure
//! functions over the buffer text and the selection (char indices), so they test headlessly.
//! They follow the reference note editor: Enter starts a new block and keeps a list going (an
//! empty item ends it), Tab / Shift+Tab nest and un-nest list items (two spaces a level) or
//! insert / remove two spaces elsewhere, Backspace right after a block marker turns the block
//! back into body text, and Space completes shortcuts (`---` + Space is a divider, `->` + Space
//! an arrow, …). The buffer is the `"\n\n"`-separated Markdown-XL text, one paragraph per block.

use parrot_core::TagKind;

/// The buffer after a key, with the selection as `(anchor, head)` char indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub anchor: usize,
    pub head: usize,
}

impl Edit {
    fn caret(text: String, at: usize) -> Self {
        Self {
            text,
            anchor: at,
            head: at,
        }
    }
}

/// Spaces one list nesting level is indented by.
const INDENT: usize = 2;

fn byte_of(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(b, _)| b)
}

fn char_of(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

/// A paragraph's byte range and its block marker, if the marker is really at its start.
struct Block {
    start: usize,
    end: usize,
    kind: Option<TagKind>,
    /// Byte length of the marker (indent included for list items).
    prefix: usize,
    literal: String,
}

impl Block {
    fn at(text: &str, byte: usize) -> Self {
        let byte = byte.min(text.len());
        let start = text[..byte].rfind("\n\n").map_or(0, |i| i + 2);
        let end = text[start..].find("\n\n").map_or(text.len(), |i| start + i);
        let source = &text[start..end];
        let tag = parrot_markup::parse::parse_paragraph(source)
            .tag
            .filter(|t| source.starts_with(t.literal.as_str()));
        Self {
            start,
            end,
            prefix: tag.as_ref().map_or(0, |t| t.literal.len()),
            literal: tag.as_ref().map(|t| t.literal.clone()).unwrap_or_default(),
            kind: tag.map(|t| t.kind),
        }
    }

    fn is_list(&self) -> bool {
        matches!(
            self.kind,
            Some(TagKind::UnorderedList | TagKind::OrderedList)
        )
    }

    /// Nesting level of a list item (0 for anything else).
    fn level(&self) -> usize {
        if self.is_list() {
            self.literal.chars().take_while(|&c| c == ' ').count() / INDENT
        } else {
            0
        }
    }

    /// The marker of the item that follows this one.
    fn next_marker(&self) -> String {
        match self.kind {
            Some(TagKind::UnorderedList | TagKind::BlockQuote) => self.literal.clone(),
            Some(TagKind::OrderedList) => {
                let indent = self.literal.len() - self.literal.trim_start().len();
                let number: usize = self
                    .literal
                    .trim()
                    .trim_end_matches('.')
                    .parse()
                    .unwrap_or(0);
                format!("{}{}. ", " ".repeat(indent), number + 1)
            }
            _ => String::new(),
        }
    }
}

/// Enter: a new paragraph (a `"\n\n"`), continuing a list or quote with the next marker. Enter on
/// an empty item leaves the list (one level out when nested). Code, comment and native blocks
/// take a plain line break instead — `None` — and Shift+Enter always does.
pub fn enter(text: &str, selection: (usize, usize)) -> Option<Edit> {
    let (start, end) = (selection.0.min(selection.1), selection.0.max(selection.1));
    let (s, e) = (byte_of(text, start), byte_of(text, end));
    let block = Block::at(text, s);
    if matches!(
        block.kind,
        Some(TagKind::CodeBlock(_) | TagKind::Comment | TagKind::NativeBlock)
    ) {
        return None;
    }
    let content_start = block.start + block.prefix;
    let caret = s.max(content_start).min(block.end);
    let continues = block.is_list() || matches!(block.kind, Some(TagKind::BlockQuote));
    let empty_item = continues && text[content_start..block.end].trim().is_empty();

    if empty_item && e <= block.end {
        // Leave the list: nested items step out a level, top-level ones become body text.
        let (cut, keep) = if block.level() > 0 {
            (block.start..block.start + INDENT, content_start - INDENT)
        } else {
            (block.start..content_start, block.start)
        };
        let mut out = text.to_string();
        out.replace_range(cut, "");
        return Some(Edit::caret(out.clone(), char_of(&out, keep)));
    }

    let marker = if continues {
        block.next_marker()
    } else {
        String::new()
    };
    let mut out = String::with_capacity(text.len() + 2 + marker.len());
    out.push_str(&text[..caret]);
    out.push_str("\n\n");
    out.push_str(&marker);
    let caret_at = out.chars().count();
    out.push_str(&text[e.max(caret)..]);
    Some(Edit::caret(out, caret_at))
}

/// Tab / Shift+Tab. In a list: nests (an item may go one level below the item above it) or
/// un-nests every item the selection touches. Elsewhere: two spaces in, or up to two spaces
/// before the caret out.
pub fn tab(text: &str, selection: (usize, usize), shift: bool) -> Edit {
    let (start, end) = (selection.0.min(selection.1), selection.0.max(selection.1));
    let (s, e) = (byte_of(text, start), byte_of(text, end));
    if !Block::at(text, s).is_list() {
        return tab_outside_list(text, start, end, shift);
    }

    // The touched paragraphs, first to last.
    let mut blocks = Vec::new();
    let mut at = Block::at(text, s).start;
    loop {
        let block = Block::at(text, at);
        let next = block.end + 2;
        let last = block.end >= e || next > text.len();
        blocks.push(block);
        if last {
            break;
        }
        at = next;
    }

    let mut out = text.to_string();
    // (char index of a paragraph's start in the original text, chars it gained or lost)
    let mut moves: Vec<(usize, isize)> = Vec::new();
    let mut shifted = 0isize;
    let mut previous_level = {
        let first = &blocks[0];
        if first.start >= 2 {
            let before = Block::at(text, first.start - 2);
            before.is_list().then(|| before.level())
        } else {
            None
        }
    };
    for block in &blocks {
        let level = block.level();
        let at = (block.start as isize + shifted) as usize;
        let delta: isize = if !block.is_list() {
            0
        } else if shift {
            if level > 0 {
                out.replace_range(at..at + INDENT, "");
                -(INDENT as isize)
            } else {
                0
            }
        } else if previous_level.is_some_and(|p| level <= p) {
            out.insert_str(at, &" ".repeat(INDENT));
            INDENT as isize
        } else {
            0
        };
        if delta != 0 {
            moves.push((char_of(text, block.start), delta));
        }
        previous_level = block
            .is_list()
            .then(|| (level as isize + delta / INDENT as isize) as usize);
        shifted += delta;
    }
    // Selection ends past a paragraph's start move with its text; ones inside removed
    // indentation land at the paragraph start.
    let moved = |p: usize| {
        moves.iter().fold(p as isize, |p_now, &(at, delta)| {
            if delta > 0 {
                if p >= at {
                    p_now + delta
                } else {
                    p_now
                }
            } else if p >= at + INDENT {
                p_now + delta
            } else if p > at {
                p_now - (p - at) as isize
            } else {
                p_now
            }
        }) as usize
    };
    Edit {
        text: out,
        anchor: moved(selection.0),
        head: moved(selection.1),
    }
}

fn tab_outside_list(text: &str, start: usize, end: usize, shift: bool) -> Edit {
    let (s, e) = (byte_of(text, start), byte_of(text, end));
    if !shift {
        let mut out = text.to_string();
        out.replace_range(s..e, &" ".repeat(INDENT));
        return Edit::caret(out, start + INDENT);
    }
    if start != end {
        return Edit {
            text: text.to_string(),
            anchor: start,
            head: end,
        };
    }
    let spaces = text[..s]
        .chars()
        .rev()
        .take(INDENT)
        .take_while(|&c| c == ' ')
        .count();
    let mut out = text.to_string();
    out.replace_range(s - spaces..s, "");
    Edit::caret(out, start - spaces)
}

/// Backspace. Right after a block's marker it turns the block back into body text (a nested
/// list item steps out a level instead); at the start of a plain paragraph it joins it to the
/// one above. `None` leaves the key to the text field.
pub fn backspace(text: &str, selection: (usize, usize)) -> Option<Edit> {
    if selection.0 != selection.1 {
        return None;
    }
    let caret = byte_of(text, selection.0);
    let block = Block::at(text, caret);
    if block.kind.is_some() && caret == block.start + block.prefix {
        let mut out = text.to_string();
        if block.level() > 0 {
            out.replace_range(block.start..block.start + INDENT, "");
            return Some(Edit::caret(out, selection.0 - INDENT));
        }
        out.replace_range(block.start..caret, "");
        return Some(Edit::caret(out, char_of(text, block.start)));
    }
    if block.kind.is_none() && caret == block.start && block.start >= 2 {
        let mut out = text.to_string();
        out.replace_range(block.start - 2..block.start, "");
        return Some(Edit::caret(out, selection.0 - 2));
    }
    None
}

/// Typography shortcuts completed by Space: `(pattern, replacement)`, longest first.
const TYPOGRAPHY: [(&str, &str); 23] = [
    ("<-->", "↔"),
    ("<==>", "⇔"),
    ("<<>>", "‹›"),
    ("(tm)", "™"),
    ("-->", "→"),
    ("<--", "←"),
    ("==>", "⇒"),
    ("<==", "⇐"),
    ("...", "…"),
    ("+/-", "±"),
    ("(c)", "©"),
    ("(r)", "®"),
    ("(p)", "℗"),
    ("->", "→"),
    ("<-", "←"),
    ("=>", "⇒"),
    ("<<", "«"),
    (">>", "»"),
    ("--", "—"),
    ("!=", "≠"),
    ("<=", "≤"),
    (">=", "≥"),
    ("+-", "±"),
];

/// Space typed at a collapsed caret: completes a shortcut — `---`, `***` or `___` alone in a
/// paragraph becomes a divider, and arrows, dashes, `(c)` and the like turn into their symbols
/// (not in code, comments or native blocks). `None` when the Space is just a space. (`~~` is
/// deliberately not a shortcut: it closes a strikethrough.)
pub fn space(text: &str, selection: (usize, usize)) -> Option<Edit> {
    if selection.0 != selection.1 {
        return None;
    }
    let caret = byte_of(text, selection.0);
    let block = Block::at(text, caret);
    if matches!(
        block.kind,
        Some(TagKind::CodeBlock(_) | TagKind::Comment | TagKind::NativeBlock)
    ) {
        return None;
    }
    let before = &text[block.start..caret.clamp(block.start, block.end)];

    if caret == block.end && matches!(before, "---" | "___" | "***") {
        let mut out = text[..block.start].to_string();
        out.push_str("----\n\n");
        let at = out.chars().count();
        out.push_str(&text[block.end..]);
        return Some(Edit::caret(out, at));
    }

    for (pattern, symbol) in TYPOGRAPHY {
        let Some(head) = before.strip_suffix(pattern) else {
            continue;
        };
        // `--` is an em dash only on its own, `<=` / `>=` only as the plain comparisons.
        let blocked = match pattern {
            "--" => head.ends_with('-'),
            "<=" => head.ends_with('<'),
            ">=" => head.ends_with('>'),
            _ => false,
        };
        if blocked {
            continue;
        }
        let at = caret - pattern.len();
        let mut out = text[..at].to_string();
        out.push_str(symbol);
        out.push(' ');
        let caret_at = out.chars().count();
        out.push_str(&text[caret..]);
        return Some(Edit::caret(out, caret_at));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(edit: Option<Edit>) -> (String, usize) {
        let edit = edit.expect("handled");
        assert_eq!(edit.anchor, edit.head);
        (edit.text, edit.head)
    }

    #[test]
    fn enter_continues_a_list_and_numbers_the_next_item() {
        assert_eq!(run(enter("- one", (5, 5))), ("- one\n\n- ".to_string(), 9));
        assert_eq!(
            run(enter("1. one\n\n2. two", (14, 14))),
            ("1. one\n\n2. two\n\n3. ".to_string(), 19)
        );
        // Mid-item: the rest of the line moves to the new item.
        assert_eq!(
            run(enter("- one two", (5, 5))),
            ("- one\n\n-  two".to_string(), 9)
        );
        assert_eq!(
            run(enter("> quote", (7, 7))),
            ("> quote\n\n> ".to_string(), 11)
        );
    }

    #[test]
    fn enter_on_an_empty_item_leaves_the_list() {
        assert_eq!(run(enter("a\n\n- ", (5, 5))), ("a\n\n".to_string(), 3));
        // A nested empty item steps out one level instead.
        assert_eq!(
            run(enter("- a\n\n  - ", (9, 9))),
            ("- a\n\n- ".to_string(), 7)
        );
    }

    #[test]
    fn enter_elsewhere_starts_a_body_paragraph_but_code_takes_a_line_break() {
        assert_eq!(
            run(enter("# Title", (7, 7))),
            ("# Title\n\n".to_string(), 9)
        );
        assert_eq!(run(enter("plain", (5, 5))), ("plain\n\n".to_string(), 7));
        assert_eq!(enter("``` code", (8, 8)), None);
        assert_eq!(enter("%% note", (7, 7)), None);
    }

    #[test]
    fn tab_nests_an_item_under_the_one_above_and_shift_tab_undoes_it() {
        let nested = tab("- a\n\n- b", (7, 7), false);
        assert_eq!(nested.text, "- a\n\n  - b");
        assert_eq!((nested.anchor, nested.head), (9, 9));
        // The first item has nothing above to nest under.
        assert_eq!(tab("- a\n\n- b", (2, 2), false).text, "- a\n\n- b");
        // Never more than one level below the item above.
        assert_eq!(
            tab("- a\n\n  - b\n\n  - c", (16, 16), false).text,
            "- a\n\n  - b\n\n    - c"
        );
        // An item below a nested one can still join the nested level.
        assert_eq!(
            tab("- a\n\n  - b\n\n- c", (15, 15), false).text,
            "- a\n\n  - b\n\n  - c"
        );
        let back = tab("- a\n\n  - b", (9, 9), true);
        assert_eq!(back.text, "- a\n\n- b");
        assert_eq!((back.anchor, back.head), (7, 7));
        assert_eq!(tab("- a", (2, 2), true).text, "- a");
    }

    #[test]
    fn tab_outside_a_list_moves_two_spaces() {
        let tabbed = tab("ab", (1, 1), false);
        assert_eq!((tabbed.text.as_str(), tabbed.head), ("a  b", 3));
        let back = tab("a   b", (4, 4), true);
        assert_eq!((back.text.as_str(), back.head), ("a b", 2));
        assert_eq!(tab("ab", (1, 1), true).text, "ab");
    }

    #[test]
    fn backspace_after_a_marker_turns_the_block_into_body_text() {
        assert_eq!(
            run(backspace("a\n\n- b", (5, 5))),
            ("a\n\nb".to_string(), 3)
        );
        assert_eq!(run(backspace("# T", (2, 2))), ("T".to_string(), 0));
        assert_eq!(
            run(backspace("- a\n\n  - b", (9, 9))),
            ("- a\n\n- b".to_string(), 7)
        );
        // Plain paragraphs join with the one above; elsewhere it's an ordinary backspace.
        assert_eq!(run(backspace("a\n\nb", (3, 3))), ("ab".to_string(), 1));
        assert_eq!(backspace("ab", (2, 2)), None);
        assert_eq!(backspace("ab", (0, 2)), None);
    }

    #[test]
    fn space_completes_dividers_and_typography() {
        assert_eq!(run(space("---", (3, 3))), ("----\n\n".to_string(), 6));
        assert_eq!(run(space("a ->", (4, 4))), ("a → ".to_string(), 4));
        assert_eq!(run(space("(c)", (3, 3))), ("© ".to_string(), 2));
        assert_eq!(
            run(space("a -- b ...", (10, 10))),
            ("a -- b … ".to_string(), 9)
        );
        assert_eq!(
            space("a ---", (5, 5)),
            None,
            "a divider is a paragraph of its own"
        );
        assert_eq!(space("x --- y", (7, 7)), None);
        assert_eq!(space("``` a -> b ->", (13, 13)), None, "not in code");
        assert_eq!(
            space("~~gone~~", (8, 8)),
            None,
            "strikethrough isn't a shortcut"
        );
        assert_eq!(space("plain", (5, 5)), None);
        assert_eq!(run(space("a x<=", (5, 5))), ("a x≤ ".to_string(), 5));
        assert_eq!(space("<<=", (3, 3)), None);
    }
}
