//! Panel 3 editor surface (T-12-06, ui-description §5.1/§12.4/§12.10): the centered text
//! column's metrics, per-paragraph numbering, the word-count badge, and the styled
//! [`LayoutJob`] the editor's `TextEdit` lays its buffer out with: the text gets the §5.1
//! typography and Markdown-XL markers are concealed, so the writer sees formatted text. With
//! Show Markdown on, the markers stay visible but dimmed.
//!
//! The editor buffer is the same `"\n\n"`-separated Markdown-XL text every other editor path
//! uses (`EditorState::to_ast`); [`buffer_to_content`] is that one parse.

use crate::annotations::{annotation_color, Annotation};
use crate::tokens::{self, TypeToken};
use egui::text::{LayoutJob, TextFormat};
use egui::Color32;
use parrot_core::{SheetContent, TagKind};
use std::ops::Range;

/// §5.1 text column: at most 640px wide, ~80px side margins.
pub const TEXT_COLUMN_MAX_WIDTH: f32 = 640.0;
pub const SIDE_MARGIN: f32 = 80.0;
/// Vertical padding inside the inset column (§12.4).
pub const COLUMN_PADDING_Y: f32 = 40.0;
/// §12.10: paragraph labels end this far left of the text column's left edge (right-aligned,
/// so a block-type tag beside the number has room to grow leftwards).
pub const PARAGRAPH_NUMBER_OFFSET: f32 = 12.0;
/// Narrowest the column gets before margins stop shrinking it further.
const MIN_COLUMN_WIDTH: f32 = 200.0;

/// The text column's width for a Panel 3 `available` px wide: margins on both sides, clamped
/// to [`TEXT_COLUMN_MAX_WIDTH`] however wide the window gets.
pub fn text_column_width(available: f32) -> f32 {
    (available - 2.0 * SIDE_MARGIN)
        .min(TEXT_COLUMN_MAX_WIDTH)
        .max(MIN_COLUMN_WIDTH.min(available))
}

/// Parses an editor buffer into sheet content — the same split/parse as `EditorState::to_ast`.
pub fn buffer_to_content(text: &str) -> SheetContent {
    crate::editor_state::EditorState::new(text).to_ast()
}

/// Renders sheet content into an editor buffer (`EditorState::from_ast`'s text), after making
/// it match what the editor itself would type: see [`viewport_content`].
pub fn content_to_buffer(content: &SheetContent) -> String {
    let text = crate::editor_state::EditorState::from_ast(&viewport_content(content))
        .text()
        .to_string();
    // Markdown written line by line (a heading, bullets and a quote on consecutive lines)
    // becomes one block per line, so each is formatted rather than shown raw.
    crate::markdown_blocks::normalize_blocks(&text)
}

/// Sheet content as the viewport shows it. Sheets written elsewhere (older imports, other
/// tools) can hold what the editor's own text never does, which would show as stray blank
/// lines or as raw markers: empty untagged paragraphs are dropped, a divider is `----` and a
/// code fence is followed by a space, as `parrot_markup::parse` requires.
pub fn viewport_content(content: &SheetContent) -> SheetContent {
    use parrot_core::{Inline, TagKind};
    let visible = |inlines: &[Inline]| {
        inlines
            .iter()
            .any(|i| !matches!(i, Inline::Text(t) if t.trim().is_empty()))
    };
    let paragraphs = content
        .paragraphs
        .iter()
        .filter(|p| p.tag.is_some() || visible(&p.inlines))
        .cloned()
        .map(|mut p| {
            if let Some(tag) = &mut p.tag {
                match tag.kind {
                    TagKind::Divider => tag.literal = "----".to_string(),
                    TagKind::CodeBlock(_)
                        if tag.literal.starts_with("```") && !tag.literal.ends_with(' ') =>
                    {
                        tag.literal.push(' ');
                    }
                    _ => {}
                }
            }
            p
        })
        .collect();
    SheetContent::new(paragraphs)
}

/// §5.1 badge text, e.g. "357 Words".
pub fn word_count_label(words: u32) -> String {
    if words == 1 {
        "1 Word".to_string()
    } else {
        format!("{words} Words")
    }
}

/// The badge's count for a live buffer: `parrot_core::statistics` over the parsed content.
pub fn buffer_word_count(text: &str) -> u32 {
    parrot_core::statistics::word_count(&buffer_to_content(text))
}

/// Byte range of every non-empty `"\n\n"`-separated paragraph in `text`, in order — the same
/// units [`buffer_to_content`] parses into `Paragraph`s.
pub fn paragraph_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for part in text.split("\n\n") {
        if !part.is_empty() {
            spans.push(offset..offset + part.len());
        }
        offset += part.len() + 2;
    }
    spans
}

/// The paragraph (byte range) holding byte `caret`, the caret at its very end included; `None`
/// between paragraphs, where there's nothing to edit.
pub fn paragraph_at(text: &str, caret: usize) -> Option<Range<usize>> {
    paragraph_spans(text)
        .into_iter()
        .find(|span| span.start <= caret && caret <= span.end)
}

/// One §12.10 label per paragraph: its byte start (for positioning against the laid-out
/// galley) and its 1-based number.
pub fn paragraph_numbers(text: &str) -> Vec<(usize, String)> {
    paragraph_spans(text)
        .into_iter()
        .enumerate()
        .map(|(i, span)| (span.start, (i + 1).to_string()))
        .collect()
}

/// The short gutter tag of the block type `paragraph` is written in — `H1`…`H6`, `Quote`, `UL`,
/// `OL`, `Code`, `Note` (comment), `Rule` (divider), `Native` — or `None` for body text.
pub fn block_tag_label(paragraph: &str) -> Option<String> {
    use parrot_core::TagKind;
    let tag = parrot_markup::parse::parse_paragraph(paragraph).tag?;
    if !paragraph.starts_with(tag.literal.as_str()) {
        return None;
    }
    Some(match tag.kind {
        TagKind::Heading(n) => format!("H{n}"),
        TagKind::UnorderedList => "UL".to_string(),
        TagKind::OrderedList => "OL".to_string(),
        TagKind::BlockQuote => "Quote".to_string(),
        TagKind::CodeBlock(_) => "Code".to_string(),
        TagKind::Comment => "Note".to_string(),
        TagKind::Divider => "Rule".to_string(),
        TagKind::NativeBlock => "Native".to_string(),
    })
}

/// The gutter label of every paragraph: its number, led by its block-type tag when it has one
/// (`H1 3`). Same order and byte starts as [`paragraph_numbers`].
pub fn paragraph_gutter_labels(text: &str) -> Vec<(usize, String)> {
    paragraph_spans(text)
        .into_iter()
        .enumerate()
        .map(|(i, span)| {
            let n = i + 1;
            let label = match block_tag_label(&text[span.clone()]) {
                Some(tag) => format!("{tag} {n}"),
                None => n.to_string(),
            };
            (span.start, label)
        })
        .collect()
}

/// What the editor paints over a block's (invisible) Markdown marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockDecoration {
    /// A bullet; the nesting level picks its shape (`•`, `◦`, `▪`).
    Bullet(usize),
    /// The item's number among its siblings at its level, e.g. `"2."`.
    Number(String),
    /// A divider: a horizontal rule across the column.
    Rule,
}

impl BlockDecoration {
    /// The bullet glyph for nesting `level`.
    pub fn bullet_glyph(level: usize) -> &'static str {
        ["•", "◦", "▪"][level % 3]
    }
}

/// The nesting level of a list marker literal: two leading spaces per level.
fn list_level(literal: &str) -> usize {
    literal.chars().take_while(|&c| c == ' ').count() / 2
}

/// Decorations for the list items and dividers in `text`: the byte where the item's text
/// starts (after its marker; a divider's start) and what to draw there. Ordered items are
/// numbered per level within their run of list items, restarting after a bullet or after the
/// list steps out and back in.
pub fn block_decorations(text: &str) -> Vec<(usize, BlockDecoration)> {
    use parrot_core::TagKind;
    let mut out = Vec::new();
    let mut counts: Vec<u32> = Vec::new();
    let mut previous_end: Option<usize> = None;
    for span in paragraph_spans(text) {
        let source = &text[span.clone()];
        let tag = parrot_markup::parse::parse_paragraph(source)
            .tag
            .filter(|t| source.starts_with(t.literal.as_str()));
        if !previous_end.is_some_and(|end| end + 2 == span.start) {
            counts.clear();
        }
        previous_end = Some(span.end);
        match tag {
            Some(t) if matches!(t.kind, TagKind::OrderedList) => {
                let level = list_level(&t.literal);
                counts.truncate(level + 1);
                counts.resize(level + 1, 0);
                counts[level] += 1;
                let number = format!("{}.", counts[level]);
                out.push((
                    span.start + t.literal.len(),
                    BlockDecoration::Number(number),
                ));
            }
            Some(t) if matches!(t.kind, TagKind::UnorderedList) => {
                let level = list_level(&t.literal);
                counts.truncate(level);
                out.push((span.start + t.literal.len(), BlockDecoration::Bullet(level)));
            }
            Some(t) if matches!(t.kind, TagKind::Divider) => {
                counts.clear();
                out.push((span.start, BlockDecoration::Rule));
            }
            _ => counts.clear(),
        }
    }
    out
}

/// Whether `paragraph` is a list item.
fn is_list_item(paragraph: &str) -> bool {
    use parrot_core::TagKind;
    parrot_markup::parse::parse_paragraph(paragraph)
        .tag
        .is_some_and(|t| {
            paragraph.starts_with(t.literal.as_str())
                && matches!(t.kind, TagKind::UnorderedList | TagKind::OrderedList)
        })
}

/// Splits the sheet text at the selection `[start, end)` (char indices) into the text before,
/// the selection and the text after, dropping the blank separators around them. Returns the
/// non-empty parts in order and the index of the selection among them — `None` when nothing
/// is selected or the selection is everything (then there's nothing to split).
pub fn split_around_selection(
    text: &str,
    start: usize,
    end: usize,
) -> Option<(Vec<String>, usize)> {
    let (start, end) = (start.min(end), start.max(end));
    let byte = |c: usize| text.char_indices().nth(c).map_or(text.len(), |(b, _)| b);
    let (before, selected, after) = (
        text[..byte(start)].trim_end(),
        text[byte(start)..byte(end)].trim(),
        text[byte(end)..].trim_start(),
    );
    if selected.is_empty() || (before.is_empty() && after.is_empty()) {
        return None;
    }
    let mut parts = Vec::new();
    if !before.is_empty() {
        parts.push(before.to_string());
    }
    let selection = parts.len();
    parts.push(selected.to_string());
    if !after.is_empty() {
        parts.push(after.to_string());
    }
    Some((parts, selection))
}

/// Inputs to [`layout_job`] beyond the text itself.
#[derive(Debug, Clone, Default)]
pub struct EditorStyle<'a> {
    /// Show Markdown: markers visible (dimmed) when on; concealed when off, the default.
    pub show_markdown: bool,
    /// Byte range of the paragraph the caret is in: its markers stay visible (dimmed) for
    /// editing while every other paragraph shows formatted. See [`paragraph_at`].
    pub reveal: Option<Range<usize>>,
    /// Focus Mode (T-10-09): per-paragraph fade amounts from `focus_mode::paragraph_alphas`.
    pub paragraph_alphas: Option<Vec<f32>>,
    /// Text-checker findings to underline in their own colors (§9.1).
    pub annotations: &'a [Annotation],
    /// The style picker's active ULSS stylesheet (T-4-09): when set, its cascaded font size
    /// for each paragraph (via `render::layout_paragraph`) replaces the token default.
    pub stylesheet: Option<&'a parrot_style::resolve::Stylesheet>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RunStyle {
    token: TypeToken,
    italic: bool,
    color: Color32,
    underline: Option<Color32>,
    /// A Markdown-XL marker (`**`, `# `, `](url)`, …) rather than text.
    marker: bool,
    /// Source text drawn over by a block decoration (list bullet/number, divider line): when
    /// markers are concealed it keeps its width but turns transparent.
    ghost: bool,
}

/// Concealed markers stay in the buffer (the `TextEdit` edits the real text) but are laid out
/// at this size in a transparent color, so they take no visible space.
const CONCEALED_SIZE: f32 = 0.01;

impl RunStyle {
    fn body() -> Self {
        Self {
            token: tokens::BODY,
            italic: false,
            color: tokens::TEXT_SECONDARY,
            underline: None,
            marker: false,
            ghost: false,
        }
    }

    fn format(&self, conceal_markers: bool) -> TextFormat {
        if self.ghost && conceal_markers {
            return TextFormat {
                font_id: self.token.font_id(),
                color: Color32::TRANSPARENT,
                ..Default::default()
            };
        }
        if self.marker && conceal_markers {
            return TextFormat {
                font_id: egui::FontId::new(CONCEALED_SIZE, self.token.font_id().family),
                color: Color32::TRANSPARENT,
                ..Default::default()
            };
        }
        TextFormat {
            font_id: self.token.font_id(),
            color: self.color,
            italics: self.italic,
            underline: self
                .underline
                .map(|c| egui::Stroke::new(1.0_f32, c))
                .unwrap_or(egui::Stroke::NONE),
            ..Default::default()
        }
    }
}

/// Builds the editor's styled layout for `text`. Every byte of `text` lands in exactly one
/// section, as `TextEdit`'s layouter requires.
pub fn layout_job(text: &str, style: &EditorStyle, wrap_width: f32) -> LayoutJob {
    let mut runs = vec![RunStyle::body(); text.len()];
    let mut after_heading = false;
    for span in paragraph_spans(text) {
        after_heading = style_paragraph(text, span, after_heading, style.stylesheet, &mut runs);
    }

    // The paragraph being edited shows its Markdown.
    if let Some(reveal) = &style.reveal {
        for run in &mut runs[reveal.start.min(text.len())..reveal.end.min(text.len())] {
            run.marker = false;
            run.ghost = false;
        }
    }

    // List items sit tight: the empty row between two items collapses (the blank line is only
    // the paragraph separator).
    let spans = paragraph_spans(text);
    for pair in spans.windows(2) {
        if pair[1].start == pair[0].end + 2
            && is_list_item(&text[pair[0].clone()])
            && is_list_item(&text[pair[1].clone()])
        {
            runs[pair[0].end + 1].marker = true;
        }
    }

    if let Some(alphas) = &style.paragraph_alphas {
        for (span, alpha) in paragraph_spans(text).into_iter().zip(alphas) {
            if *alpha > 0.0 {
                for run in &mut runs[span] {
                    run.color = run.color.gamma_multiply(1.0 - alpha);
                }
            }
        }
    }

    for a in style.annotations {
        let range = a.byte_range.start.min(text.len())..a.byte_range.end.min(text.len());
        for run in &mut runs[range] {
            run.underline = Some(annotation_color(a.kind));
        }
    }

    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let mut start = 0;
    while start < text.len() {
        let mut end = start + 1;
        while end < text.len() && runs[end] == runs[start] {
            end += 1;
        }
        // Run boundaries only ever fall on char boundaries (every range styled above is
        // char-aligned), but stay safe for multi-byte chars split by a stray range.
        while !text.is_char_boundary(end) {
            end += 1;
        }
        job.append(
            &text[start..end],
            0.0,
            runs[start].format(!style.show_markdown),
        );
        start = end;
    }
    job
}

/// Styles one paragraph in place; returns whether it was a heading (for the next paragraph's
/// §5.1 "subtitle" color).
fn style_paragraph(
    text: &str,
    span: Range<usize>,
    after_heading: bool,
    stylesheet: Option<&parrot_style::resolve::Stylesheet>,
    runs: &mut [RunStyle],
) -> bool {
    let source = &text[span.clone()];
    let paragraph = parrot_markup::parse::parse_paragraph(source);
    let (mut base, inline_markup) = match paragraph.tag.as_ref().map(|t| &t.kind) {
        Some(TagKind::Heading(level)) => (
            RunStyle {
                token: tokens::heading(*level),
                color: tokens::TEXT_PRIMARY,
                ..RunStyle::body()
            },
            true,
        ),
        Some(TagKind::CodeBlock(_)) => (
            RunStyle {
                token: tokens::MONO_BLOCK,
                color: tokens::TEXT_CODE_BLOCK,
                ..RunStyle::body()
            },
            false,
        ),
        Some(TagKind::Comment) | Some(TagKind::NativeBlock) => (
            RunStyle {
                color: tokens::TEXT_TERTIARY,
                ..RunStyle::body()
            },
            false,
        ),
        Some(TagKind::BlockQuote) => (
            RunStyle {
                italic: true,
                ..RunStyle::body()
            },
            true,
        ),
        _ if after_heading => (
            RunStyle {
                color: tokens::TEXT_SUBTITLE,
                ..RunStyle::body()
            },
            true,
        ),
        _ => (RunStyle::body(), true),
    };
    if let Some(sheet) = stylesheet {
        base.token.size = crate::render::layout_paragraph(&paragraph, Some(sheet)).font_size;
    }
    for run in &mut runs[span.clone()] {
        *run = base;
    }

    let prefix_len = paragraph
        .tag
        .as_ref()
        .filter(|t| source.starts_with(t.literal.as_str()))
        .map_or(0, |t| t.literal.len());
    let prefix = span.start..span.start + prefix_len;
    match paragraph.tag.as_ref().map(|t| &t.kind) {
        // List markers and dividers keep their width but are drawn over by the editor
        // ([`block_decorations`]) as a bullet, a number or a rule; Show Markdown dims them.
        Some(TagKind::UnorderedList | TagKind::OrderedList) => {
            for run in &mut runs[prefix] {
                run.color = tokens::TEXT_TERTIARY;
                run.ghost = true;
            }
        }
        Some(TagKind::Divider) => {
            for run in &mut runs[span.clone()] {
                run.color = tokens::TEXT_TERTIARY;
                run.ghost = true;
            }
        }
        _ => mark(runs, prefix),
    }
    if inline_markup {
        scan_inlines(text, span.start + prefix_len..span.end, base, runs);
    }
    matches!(
        paragraph.tag.as_ref().map(|t| &t.kind),
        Some(TagKind::Heading(_))
    )
}

/// Flags a Markdown-XL marker run (`**`, `# `, `](url)`, …): concealed, or dimmed to the
/// §10.1 quaternary color when Markdown is shown.
fn mark(runs: &mut [RunStyle], range: Range<usize>) {
    for run in &mut runs[range] {
        run.color = tokens::TEXT_QUATERNARY;
        run.marker = true;
    }
}

/// Inline styling over `range`: `**`/`__` strong, `*`/`_` emph, `` ` `` code, `[label](url)`
/// links and `![alt](url)` images — the same markers `parrot_markup::parse` reads.
fn scan_inlines(text: &str, range: Range<usize>, inherited: RunStyle, runs: &mut [RunStyle]) {
    let bytes = text.as_bytes();
    let mut i = range.start;
    while i < range.end {
        let rest = &text[i..range.end];
        let doubled = rest.starts_with("**") || rest.starts_with("__");
        if doubled {
            let marker = &rest[..2];
            if let Some(close) = rest[2..].find(marker).filter(|&c| c > 0) {
                let inner = i + 2..i + 2 + close;
                let bold = RunStyle {
                    token: TypeToken {
                        bold: true,
                        ..inherited.token
                    },
                    color: tokens::TEXT_PRIMARY,
                    ..inherited
                };
                apply(runs, inner.clone(), bold);
                scan_inlines(text, inner.clone(), bold, runs);
                mark(runs, i..i + 2);
                mark(runs, inner.end..inner.end + 2);
                i = inner.end + 2;
                continue;
            }
        }
        match bytes[i] {
            b'*' | b'_' if !doubled => {
                let marker = bytes[i];
                if let Some(close) = text[i + 1..range.end]
                    .bytes()
                    .position(|b| b == marker)
                    .filter(|&c| c > 0)
                {
                    let inner = i + 1..i + 1 + close;
                    let emph = RunStyle {
                        italic: true,
                        ..inherited
                    };
                    apply(runs, inner.clone(), emph);
                    scan_inlines(text, inner.clone(), emph, runs);
                    mark(runs, i..i + 1);
                    mark(runs, inner.end..inner.end + 1);
                    i = inner.end + 1;
                    continue;
                }
            }
            b'`' => {
                if let Some(close) = text[i + 1..range.end].find('`') {
                    let inner = i + 1..i + 1 + close;
                    apply(
                        runs,
                        inner.clone(),
                        RunStyle {
                            token: tokens::MONO,
                            ..inherited
                        },
                    );
                    mark(runs, i..i + 1);
                    mark(runs, inner.end..inner.end + 1);
                    i = inner.end + 1;
                    continue;
                }
            }
            b'[' | b'!' => {
                let open = if bytes[i] == b'!' { i + 1 } else { i };
                if bytes.get(open) == Some(&b'[') {
                    if let Some(end) = link_end(text, open, range.end) {
                        let (label, url_end) = end;
                        let link = RunStyle {
                            color: tokens::ACCENT_LIGHT,
                            underline: Some(tokens::ACCENT_LIGHT),
                            ..inherited
                        };
                        apply(runs, open + 1..label, link);
                        mark(runs, i..open + 1);
                        mark(runs, label..url_end);
                        i = url_end;
                        continue;
                    }
                }
            }
            _ => {}
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
}

/// For a `[` at `open`, the label's end (the `]`) and the byte after the closing `)` of a
/// `[label](url)` within `limit`.
fn link_end(text: &str, open: usize, limit: usize) -> Option<(usize, usize)> {
    let label_end = open + 1 + text[open + 1..limit].find("](")?;
    let url_close = label_end + 2 + text[label_end + 2..limit].find(')')?;
    Some((label_end, url_close + 1))
}

fn apply(runs: &mut [RunStyle], range: Range<usize>, style: RunStyle) {
    for run in &mut runs[range] {
        *run = style;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, TagMark};

    fn fixed_ast() -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(1),
                    literal: "# ".to_string(),
                }),
                inlines: vec![Inline::Text("A title".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Some ".to_string()),
                    Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    Inline::Text(" words here.".to_string()),
                ],
            },
            Paragraph {
                tag: None,
                inlines: vec![Inline::Text("Third paragraph.".to_string())],
            },
        ])
    }

    const OPENED_MARKDOWN: &str = "# Title\n\nIntro with **bold** and *it*.\n\n- a\n- b\n  - nested\n    - deeper\n- c\n\n1. one\n2. two\n\n> quoted\n\n```rust\nfn main() {}\n```\n\n---\n\nAfter.\n";

    /// What opening an imported Markdown file must put in the editor buffer.
    const OPENED_BUFFER: &str = "# Title\n\nIntro with **bold** and *it*.\n\n- a\n\n- b\n\n  - nested\n\n    - deeper\n\n- c\n\n1. one\n\n2. two\n\n> quoted\n\n```rust fn main() {}\n\n----\n\nAfter.";

    #[test]
    fn an_opened_markdown_file_reads_as_the_editor_would_type_it() {
        let content = parrot_import::markdown::import_markdown(OPENED_MARKDOWN);
        let buffer = content_to_buffer(&content);
        assert_eq!(buffer, OPENED_BUFFER);
        // The buffer parses back to exactly what was opened: nothing is lost or reshaped.
        assert_eq!(buffer_to_content(&buffer), viewport_content(&content));
    }

    #[test]
    fn an_opened_file_is_laid_out_formatted_not_as_raw_markdown() {
        let buffer = content_to_buffer(&parrot_import::markdown::import_markdown(OPENED_MARKDOWN));
        let job = layout_job(&buffer, &EditorStyle::default(), 640.0);
        let format = |needle: &str| section_for(&job, needle).format.clone();
        // Heading and bold are styled and their markers hidden.
        assert_eq!(format("Title").font_id.size, 28.0);
        assert!(format("# ").font_id.size < 0.1);
        assert_eq!(
            format("bold").font_id.family,
            egui::FontFamily::Name(tokens::FAMILY_BOLD.into())
        );
        // List markers (nested ones too) and the divider are invisible, replaced by drawn
        // bullets, numbers and a rule; the code block is monospace.
        for marker in ["- a", "  - nested", "    - deeper", "1. one", "----"] {
            assert_eq!(format(marker).color, Color32::TRANSPARENT, "{marker}");
        }
        assert_eq!(
            format("fn main").font_id.family,
            tokens::MONO_BLOCK.font_id().family
        );
        let decorations: Vec<BlockDecoration> = block_decorations(&buffer)
            .into_iter()
            .map(|(_, d)| d)
            .collect();
        assert_eq!(
            decorations,
            vec![
                BlockDecoration::Bullet(0),
                BlockDecoration::Bullet(0),
                BlockDecoration::Bullet(1),
                BlockDecoration::Bullet(2),
                BlockDecoration::Bullet(0),
                BlockDecoration::Number("1.".into()),
                BlockDecoration::Number("2.".into()),
                BlockDecoration::Rule,
            ]
        );
        // Gutter labels name the block types, nested items included.
        let labels: Vec<String> = paragraph_gutter_labels(&buffer)
            .into_iter()
            .map(|(_, l)| l)
            .collect();
        assert_eq!(labels[0], "H1 1");
        assert!(labels.contains(&"UL 6".to_string()));
        assert!(labels.contains(&"Quote 10".to_string()));
    }

    #[test]
    fn markdown_opened_line_by_line_is_formatted_not_shown_raw() {
        // A .txt/.md note with no blank lines between its blocks: one paragraph as imported.
        let note = "### Chapter\nSome intro\n- first\n- second\n  - nested\n## Next\n1. one\n2. two\n---\nend";
        let content = parrot_import::plaintext::import_plaintext(note);
        assert_eq!(content.paragraphs.len(), 1);
        let buffer = content_to_buffer(&content);
        assert_eq!(
            buffer,
            "### Chapter\n\nSome intro\n\n- first\n\n- second\n\n  - nested\n\n## Next\n\n1. one\n\n2. two\n\n----\n\nend"
        );
        let job = layout_job(&buffer, &EditorStyle::default(), 640.0);
        let section = |needle: &str| section_for(&job, needle).format.clone();
        // Headings are styled (their `###` hidden), and every item's marker is replaced.
        assert!(section("### ").font_id.size < 0.1);
        assert_eq!(section("Chapter").font_id.size, tokens::heading(3).size);
        assert!(section("## ").font_id.size < 0.1);
        assert_eq!(section("Next").font_id.size, tokens::heading(2).size);
        for marker in [
            "- first",
            "- second",
            "  - nested",
            "1. one",
            "2. two",
            "----",
        ] {
            assert_eq!(section(marker).color, Color32::TRANSPARENT, "{marker}");
        }
        let kinds: Vec<BlockDecoration> = block_decorations(&buffer)
            .into_iter()
            .map(|(_, d)| d)
            .collect();
        assert_eq!(
            kinds,
            vec![
                BlockDecoration::Bullet(0),
                BlockDecoration::Bullet(0),
                BlockDecoration::Bullet(1),
                BlockDecoration::Number("1.".into()),
                BlockDecoration::Number("2.".into()),
                BlockDecoration::Rule,
            ]
        );
    }

    #[test]
    fn legacy_content_is_cleaned_up_for_the_viewport() {
        let tagged = |kind, literal: &str, text: &str| Paragraph {
            tag: Some(TagMark {
                kind,
                literal: literal.to_string(),
            }),
            inlines: if text.is_empty() {
                vec![]
            } else {
                vec![Inline::Text(text.to_string())]
            },
        };
        let blank = Paragraph {
            tag: None,
            inlines: vec![Inline::Text("  ".to_string())],
        };
        let content = SheetContent::new(vec![
            blank.clone(),
            tagged(TagKind::CodeBlock(None), "```", "code"),
            tagged(TagKind::Divider, "---", ""),
            blank,
            tagged(TagKind::UnorderedList, "- ", ""),
            Paragraph {
                tag: None,
                inlines: vec![],
            },
        ]);
        assert_eq!(content_to_buffer(&content), "``` code\n\n----\n\n- ");
    }

    #[test]
    fn text_column_width_clamped() {
        for window in [1000.0, 1920.0, 3840.0, 10_000.0] {
            assert_eq!(text_column_width(window), TEXT_COLUMN_MAX_WIDTH, "{window}");
        }
        assert_eq!(text_column_width(600.0), 600.0 - 2.0 * SIDE_MARGIN);
        assert!(text_column_width(150.0) <= 150.0);
    }

    #[test]
    fn paragraph_numbers_sequential() {
        let buffer = content_to_buffer(&fixed_ast());
        let numbers = paragraph_numbers(&buffer);
        assert_eq!(numbers.len(), fixed_ast().paragraphs.len());
        let labels: Vec<&str> = numbers.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(labels, vec!["1", "2", "3"]);
        assert!(numbers.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(buffer_to_content(&buffer), fixed_ast());
    }

    #[test]
    fn gutter_labels_name_the_block_type() {
        let labels: Vec<String> = paragraph_gutter_labels("# Title\n\nBody\n\n> quoted\n\n* item")
            .into_iter()
            .map(|(_, l)| l)
            .collect();
        assert_eq!(labels, vec!["H1 1", "2", "Quote 3", "UL 4"]);
    }

    #[test]
    fn lists_and_dividers_get_decorations() {
        let text = "1. a\n\n1. b\n\ntext\n\n* c\n\n----\n\n1. d";
        let kinds: Vec<BlockDecoration> = block_decorations(text)
            .into_iter()
            .map(|(_, d)| d)
            .collect();
        assert_eq!(
            kinds,
            vec![
                BlockDecoration::Number("1.".into()),
                BlockDecoration::Number("2.".into()),
                BlockDecoration::Bullet(0),
                BlockDecoration::Rule,
                BlockDecoration::Number("1.".into()),
            ]
        );
    }

    #[test]
    fn the_paragraph_being_edited_shows_its_markdown_and_the_rest_stays_formatted() {
        let text = "# Title\n\nSome **bold** text\n\n- item";
        let editing = paragraph_at(text, text.find("bold").unwrap()).expect("paragraph");
        assert_eq!(&text[editing.clone()], "Some **bold** text");
        let job = layout_job(
            text,
            &EditorStyle {
                reveal: Some(editing),
                ..Default::default()
            },
            640.0,
        );
        let section = |needle: &str| section_for(&job, needle).format.clone();
        // The caret's paragraph: markers visible, dimmed — the text still formatted.
        assert!(section("**").font_id.size > 1.0);
        assert_eq!(section("**").color, tokens::TEXT_QUATERNARY);
        assert_eq!(
            section("bold").font_id.family,
            egui::FontFamily::Name(tokens::FAMILY_BOLD.into())
        );
        // Every other paragraph: markers hidden.
        assert!(section("# ").font_id.size < 0.1);
        assert_eq!(section("- ").color, Color32::TRANSPARENT);
        // Moving the caret to the list item reveals its marker and hides the others again.
        let editing = paragraph_at(text, text.len()).expect("paragraph");
        assert_eq!(&text[editing.clone()], "- item");
        let job = layout_job(
            text,
            &EditorStyle {
                reveal: Some(editing),
                ..Default::default()
            },
            640.0,
        );
        assert_ne!(section_for(&job, "- ").format.color, Color32::TRANSPARENT);
        assert!(section_for(&job, "**").format.font_id.size < 0.1);
        // Between paragraphs there is nothing to reveal.
        assert_eq!(paragraph_at("a\n\n\n\nb", 3), None);
    }

    #[test]
    fn nested_items_are_numbered_per_level() {
        let text = "1. a\n\n  1. b\n\n  1. c\n\n1. d\n\n  - e";
        let kinds: Vec<BlockDecoration> = block_decorations(text)
            .into_iter()
            .map(|(_, d)| d)
            .collect();
        assert_eq!(
            kinds,
            vec![
                BlockDecoration::Number("1.".into()),
                BlockDecoration::Number("1.".into()),
                BlockDecoration::Number("2.".into()),
                BlockDecoration::Number("2.".into()),
                BlockDecoration::Bullet(1),
            ]
        );
    }

    #[test]
    fn list_items_sit_tight_but_other_paragraphs_keep_their_gap() {
        let text = "- a\n\n- b\n\ntext";
        let job = layout_job(text, &EditorStyle::default(), 640.0);
        let size_at = |byte: usize| {
            job.sections
                .iter()
                .find(|s| s.byte_range.contains(&byte))
                .expect("section")
                .format
                .font_id
                .size
        };
        // The second newline between the two items collapses; the one before "text" doesn't.
        assert!(size_at(4) < 0.1);
        assert!(size_at(text.find("b\n\n").unwrap() + 2) > 1.0);
    }

    #[test]
    fn split_keeps_before_selection_and_after() {
        let (parts, sel) = split_around_selection("one\n\ntwo\n\nthree", 5, 8).expect("split");
        assert_eq!(parts, vec!["one", "two", "three"]);
        assert_eq!(sel, 1);
        let (parts, sel) = split_around_selection("one\n\ntwo", 5, 8).expect("split");
        assert_eq!(
            (parts, sel),
            (vec!["one".to_string(), "two".to_string()], 1)
        );
        let (parts, sel) = split_around_selection("one\n\ntwo", 0, 3).expect("split");
        assert_eq!(
            (parts, sel),
            (vec!["one".to_string(), "two".to_string()], 0)
        );
        assert_eq!(split_around_selection("one\n\ntwo", 0, 8), None);
        assert_eq!(split_around_selection("one\n\ntwo", 2, 2), None);
    }

    #[test]
    fn word_count_badge_matches_core_count() {
        let ast = fixed_ast();
        let buffer = content_to_buffer(&ast);
        let core = parrot_core::statistics::word_count(&ast);
        assert_eq!(core, 8);
        assert_eq!(buffer_word_count(&buffer), core);
        assert_eq!(word_count_label(buffer_word_count(&buffer)), "8 Words");
        assert_eq!(word_count_label(1), "1 Word");
    }

    fn section_for<'a>(job: &'a LayoutJob, needle: &str) -> &'a egui::text::LayoutSection {
        let at = job
            .text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} not in job"));
        job.sections
            .iter()
            .find(|s| s.byte_range.contains(&at))
            .unwrap_or_else(|| panic!("no section covers {needle}"))
    }

    #[test]
    fn layout_job_applies_typography() {
        let buffer = content_to_buffer(&fixed_ast());
        let style = EditorStyle {
            show_markdown: true,
            ..Default::default()
        };
        let job = layout_job(&buffer, &style, 640.0);
        assert_eq!(job.text, buffer);
        let covered: usize = job.sections.iter().map(|s| s.byte_range.len()).sum();
        assert_eq!(covered, buffer.len());

        assert_eq!(section_for(&job, "A title").format.font_id.size, 28.0);
        assert_eq!(
            section_for(&job, "# ").format.color,
            tokens::TEXT_QUATERNARY
        );
        assert_eq!(
            section_for(&job, "**").format.color,
            tokens::TEXT_QUATERNARY
        );
        let bold = &section_for(&job, "bold").format;
        assert_eq!(
            bold.font_id.family,
            egui::FontFamily::Name(tokens::FAMILY_BOLD.into())
        );
        assert_eq!(
            section_for(&job, "Some").format.color,
            tokens::TEXT_SUBTITLE
        );
        assert_eq!(
            section_for(&job, "Third").format.color,
            tokens::TEXT_SECONDARY
        );
    }

    #[test]
    fn layout_job_links_and_underlines() {
        let buffer = "See [docs](https://x.y) and *it*.";

        let notes = [Annotation {
            kind: crate::annotations::AnnotationKind::Spelling,
            title: String::new(),
            message: String::new(),
            byte_range: 0..3,
            fix: None,
        }];
        let styled = layout_job(
            buffer,
            &EditorStyle {
                show_markdown: true,
                annotations: &notes,
                ..Default::default()
            },
            640.0,
        );
        assert_eq!(
            section_for(&styled, "docs").format.color,
            tokens::ACCENT_LIGHT
        );
        assert_eq!(
            section_for(&styled, "https").format.color,
            tokens::TEXT_QUATERNARY
        );
        assert!(section_for(&styled, "it*").format.italics);

        let compact =
            crate::style_picker::load_builtin_stylesheet("Compact").expect("built-in style");
        let with_sheet = layout_job(
            "# Big",
            &EditorStyle {
                show_markdown: true,
                stylesheet: Some(&compact),
                ..Default::default()
            },
            640.0,
        );
        let expected = crate::render::layout_paragraph(
            &parrot_markup::parse::parse_paragraph("# Big"),
            Some(&compact),
        )
        .font_size;
        assert_eq!(
            section_for(&with_sheet, "Big").format.font_id.size,
            expected
        );
        assert_eq!(
            section_for(&styled, "See").format.underline.color,
            tokens::SPELLING
        );
    }

    /// By default the editor shows formatted text: every marker is laid out invisible and
    /// near-zero size, while the text it marks keeps its formatting.
    #[test]
    fn markers_are_concealed_unless_markdown_is_shown() {
        let buffer = "# Title\n\nSome **bold** and [docs](https://x.y) and *it*.\n\n- item";
        let job = layout_job(buffer, &EditorStyle::default(), 640.0);
        // Each needle starts on a marker: `# `, `**`, `](url)`, `[`, `*`.
        for marker in ["# ", "**", "](https://x.y)", "[docs", "*it"] {
            let format = &section_for(&job, marker).format;
            assert_eq!(format.color, Color32::TRANSPARENT, "{marker} hidden");
            assert!(format.font_id.size < 0.1, "{marker} takes no space");
        }
        // The marked text itself stays formatted and visible.
        assert_eq!(section_for(&job, "Title").format.font_id.size, 28.0);
        assert_eq!(
            section_for(&job, "bold").format.font_id.family,
            egui::FontFamily::Name(tokens::FAMILY_BOLD.into())
        );
        assert_eq!(section_for(&job, "docs").format.color, tokens::ACCENT_LIGHT);
        assert!(section_for(&job, "it*").format.italics);
        // A list marker keeps its width (the editor paints the bullet over it).
        let marker = &section_for(&job, "- ").format;
        assert_eq!(marker.color, Color32::TRANSPARENT);
        assert!(marker.font_id.size > 1.0);
        // Every byte is still laid out, as the `TextEdit` requires.
        assert_eq!(job.text, buffer);

        let shown = layout_job(
            buffer,
            &EditorStyle {
                show_markdown: true,
                ..Default::default()
            },
            640.0,
        );
        assert_eq!(
            section_for(&shown, "**").format.color,
            tokens::TEXT_QUATERNARY
        );
    }
}
