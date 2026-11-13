//! RTF / attributed-string importer (T-6-07, best-effort): hand-rolled reader covering the
//! confirmed representative subset — plain text, bold/italic runs, paragraph breaks (`\par`) —
//! not the full RTF spec (explicitly out of scope per this task's own fallback). See
//! `parrot-import/README.md` for the exact covered subset.

use parrot_core::{Inline, Paragraph, SheetContent};

/// Groups opened by one of these control words (with or without the `\*` "ignorable" marker)
/// are skipped wholesale — their content is metadata (fonts, colors, document info), not body
/// text. `*` itself covers any other extension group this reader doesn't recognize by name.
const SKIP_GROUP_WORDS: &[&str] = &[
    "*",
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "generator",
];

enum ControlAction {
    Bold(bool),
    Italic(bool),
    Par,
    Char(char),
    None,
}

pub fn import_rtf(rtf: &str) -> SheetContent {
    let chars: Vec<char> = rtf.chars().collect();
    let mut i = 0usize;
    let mut paragraphs = Vec::new();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut run_text = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut depth: i32 = 0;
    let mut skip_from_depth: Option<i32> = None;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => {
                depth += 1;
                if skip_from_depth.is_none() {
                    if let Some(word) = peek_group_keyword(&chars, i + 1) {
                        if SKIP_GROUP_WORDS.contains(&word.as_str()) {
                            skip_from_depth = Some(depth - 1);
                        }
                    }
                }
                i += 1;
            }
            '}' => {
                if skip_from_depth == Some(depth - 1) {
                    skip_from_depth = None;
                }
                depth -= 1;
                i += 1;
            }
            '\\' if skip_from_depth.is_none() => {
                let (consumed, action) = read_control(&chars, i + 1);
                i += 1 + consumed;
                match action {
                    ControlAction::Bold(on) => {
                        flush_run(&mut run_text, bold, italic, &mut inlines);
                        bold = on;
                    }
                    ControlAction::Italic(on) => {
                        flush_run(&mut run_text, bold, italic, &mut inlines);
                        italic = on;
                    }
                    ControlAction::Par => {
                        flush_run(&mut run_text, bold, italic, &mut inlines);
                        paragraphs.push(Paragraph {
                            tag: None,
                            inlines: std::mem::take(&mut inlines),
                        });
                    }
                    ControlAction::Char(c) => run_text.push(c),
                    ControlAction::None => {}
                }
            }
            '\r' | '\n' if skip_from_depth.is_none() => i += 1,
            _ if skip_from_depth.is_some() => i += 1,
            _ => {
                run_text.push(c);
                i += 1;
            }
        }
    }
    flush_run(&mut run_text, bold, italic, &mut inlines);
    if !inlines.is_empty() {
        paragraphs.push(Paragraph { tag: None, inlines });
    }
    SheetContent::new(paragraphs)
}

fn flush_run(run_text: &mut String, bold: bool, italic: bool, inlines: &mut Vec<Inline>) {
    if run_text.is_empty() {
        return;
    }
    let text = std::mem::take(run_text);
    let inline = match (bold, italic) {
        (true, true) => Inline::Strong(vec![Inline::Emph(vec![Inline::Text(text)])]),
        (true, false) => Inline::Strong(vec![Inline::Text(text)]),
        (false, true) => Inline::Emph(vec![Inline::Text(text)]),
        (false, false) => Inline::Text(text),
    };
    inlines.push(inline);
}

/// If the character at `i` (immediately after an opening `{`) is a backslash introducing a
/// control word (or the literal `\*` ignorable marker), returns that word — `"*"` for the
/// marker itself.
fn peek_group_keyword(chars: &[char], i: usize) -> Option<String> {
    if chars.get(i) != Some(&'\\') {
        return None;
    }
    if chars.get(i + 1) == Some(&'*') {
        return Some("*".to_string());
    }
    let mut j = i + 1;
    while chars.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
        j += 1;
    }
    (j > i + 1).then(|| chars[i + 1..j].iter().collect())
}

/// Reads one control word/symbol starting at `start` (just past the backslash that introduced
/// it), returning how many characters it consumed and the resulting action.
fn read_control(chars: &[char], start: usize) -> (usize, ControlAction) {
    match chars.get(start) {
        Some(&c @ ('{' | '}' | '\\')) => return (1, ControlAction::Char(c)),
        Some('\'') => {
            let hex: String = chars
                .get(start + 1..start + 3)
                .map(|s| s.iter().collect())
                .unwrap_or_default();
            return if hex.len() == 2 {
                match u8::from_str_radix(&hex, 16) {
                    Ok(byte) => (3, ControlAction::Char(byte as char)),
                    Err(_) => (1, ControlAction::None),
                }
            } else {
                (1, ControlAction::None)
            };
        }
        _ => {}
    }

    let mut i = start;
    while chars.get(i).is_some_and(|c| c.is_ascii_alphabetic()) {
        i += 1;
    }
    let word: String = chars[start..i].iter().collect();

    let num_start = i;
    if chars.get(i) == Some(&'-') {
        i += 1;
    }
    while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    let num: Option<i32> = chars[num_start..i].iter().collect::<String>().parse().ok();

    let mut consumed = i - start;
    if chars.get(i) == Some(&' ') {
        consumed += 1;
    }

    let action = match word.as_str() {
        "par" | "line" => ControlAction::Par,
        "b" => ControlAction::Bold(num != Some(0)),
        "i" => ControlAction::Italic(num != Some(0)),
        "tab" => ControlAction::Char('\t'),
        "u" => num
            .and_then(|n| char::from_u32(n as u32))
            .map(ControlAction::Char)
            .unwrap_or(ControlAction::None),
        _ => ControlAction::None,
    };
    (consumed, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_export::rtf::export_rtf;

    fn representative() -> SheetContent {
        SheetContent::new(vec![Paragraph {
            tag: None,
            inlines: vec![
                Inline::Text("Plain, ".to_string()),
                Inline::Strong(vec![Inline::Text("bold".to_string())]),
                Inline::Text(", and ".to_string()),
                Inline::Emph(vec![Inline::Text("italic".to_string())]),
                Inline::Text(".".to_string()),
            ],
        }])
    }

    #[test]
    fn basic_runs_import_correctly() {
        let rtf = export_rtf(&representative());
        let imported = import_rtf(&rtf);
        assert_eq!(imported, representative(), "round-trip through our own RTF exporter must be lossless for plain/bold/italic runs:\n{rtf}");
    }

    #[test]
    fn skips_font_table_metadata() {
        let rtf = r"{\rtf1\ansi\deff0{\fonttbl{\f0 Calibri;}}\pard plain text\par}";
        let imported = import_rtf(rtf);
        assert_eq!(
            imported,
            SheetContent::new(vec![Paragraph {
                tag: None,
                inlines: vec![Inline::Text("plain text".to_string())],
            }])
        );
    }
}
