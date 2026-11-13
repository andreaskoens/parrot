//! T-2-04: the real rendering layer — AST -> styled layout (bold/italic/heading sizes/list
//! bullets/blockquote indent/code monospace) via `cosmic-text`, painted through `egui::Painter`
//! (ADR-2, confirmed viable by T-2-01's decision-gate spike).
//!
//! // DEVIATION (documented in DECISIONS.md): `paint_paragraph` paints each glyph's *layout box*
//! // (position/size as computed by cosmic-text), not its *rasterized image* — full glyph
//! // rasterization via `SwashCache` is deferred until this build can actually be viewed (no
//! // display in this sandbox; see the same deferral pattern already accepted for T-0-09/T-2-01).
//! // The boxes use cosmic-text's real layout positions, so adding rasterization later is a
//! // drop-in change to *what* gets painted at each box, not a relayout.

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, Weight};
use parrot_core::{Inline, Paragraph, TagKind, TagMark};
use parrot_style::resolve::Stylesheet;

/// A flat (text, style) span — the unit cosmic-text's rich-text API consumes. Produced by
/// flattening one paragraph's `Inline` tree (nesting composes: bold-inside-italic carries both)
/// — except codeblock paragraphs (T-3-04), whose spans come from `parrot_syntax::highlight`
/// instead, carrying a syntax-highlight `color`.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
    pub color: Option<(u8, u8, u8)>,
}

/// One paragraph's layout inputs: its styled spans, the font size to shape at (heading levels
/// get larger sizes), and a left indent (blockquote/list).
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphLayout {
    pub spans: Vec<StyledSpan>,
    pub font_size: f32,
    pub indent: f32,
}

/// Headless-measurable shaping output for `layout_metrics_for_fixed_ast`: how many visual lines
/// the paragraph wrapped to, and the glyph index of each distinct styled-span boundary (tracked
/// via cosmic-text's per-glyph `metadata`, which carries the originating span's index).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutMetrics {
    pub line_count: usize,
    pub run_boundaries: Vec<usize>,
}

/// Builds the styled-span layout for one paragraph: block-level size/indent/bullet from its
/// tag, then flattened inline spans. Codeblock paragraphs (T-3-04) take a different path
/// entirely: their spans come from `parrot_syntax::highlight`, not `flatten_inline`.
///
/// `stylesheet` (T-4-07) is the active ULSS stylesheet, if one is loaded: when present, its
/// cascaded `font-size`/`margin-left` for this paragraph's tag-derived selector override the
/// hardcoded defaults from `block_style`, property by property (a stylesheet that sets one but
/// not the other still gets the hardcoded fallback for whichever it omits). `None` reproduces
/// T-2-04's original hardcoded-only behavior exactly — loading a stylesheet is additive, not
/// mandatory for basic typing. Picking/loading the active stylesheet is T-4-09's UI.
pub fn layout_paragraph(p: &Paragraph, stylesheet: Option<&Stylesheet>) -> ParagraphLayout {
    if let Some(TagMark {
        kind: TagKind::CodeBlock(language),
        ..
    }) = p.tag.as_ref()
    {
        return layout_codeblock_paragraph(p, language.as_deref());
    }

    let (mut font_size, mut indent, bullet) = block_style(p.tag.as_ref());
    if let Some(sheet) = stylesheet {
        apply_stylesheet_overrides(p.tag.as_ref(), sheet, &mut font_size, &mut indent);
    }

    let mut spans = Vec::new();
    if let Some(b) = bullet {
        spans.push(StyledSpan {
            text: b.to_string(),
            bold: false,
            italic: false,
            monospace: false,
            color: None,
        });
    }
    for inline in &p.inlines {
        flatten_inline(inline, false, false, false, &mut spans);
    }
    ParagraphLayout {
        spans,
        font_size,
        indent,
    }
}

/// Codeblock content is always a single raw `Inline::Text` (per `parrot_markup::parse` — code
/// is never re-parsed as inline markup). With a known language, `parrot_syntax::highlight`
/// supplies real per-token colors; without one, or if the language isn't recognized, it's a
/// single plain monospace span — "defaults to plain-text highlighting when absent" (T-3-03).
fn layout_codeblock_paragraph(p: &Paragraph, language: Option<&str>) -> ParagraphLayout {
    let code: String = p
        .inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Text(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let highlighted = language.and_then(|lang| parrot_syntax::highlight(&code, lang).ok());
    let spans = match highlighted {
        Some(syntax_spans) => syntax_spans
            .into_iter()
            .map(|s| StyledSpan {
                text: s.text,
                bold: false,
                italic: false,
                monospace: true,
                color: Some(s.color),
            })
            .collect(),
        None => vec![StyledSpan {
            text: code,
            bold: false,
            italic: false,
            monospace: true,
            color: None,
        }],
    };

    ParagraphLayout {
        spans,
        font_size: BODY_SIZE_FLOOR,
        indent: 0.0,
    }
}

fn block_style(tag: Option<&TagMark>) -> (f32, f32, Option<&'static str>) {
    const BODY_SIZE: f32 = 15.0;
    const INDENT: f32 = 24.0;
    match tag.map(|t| &t.kind) {
        Some(TagKind::Heading(n)) => (heading_size(*n), 0.0, None),
        Some(TagKind::BlockQuote) => (BODY_SIZE, INDENT, None),
        Some(TagKind::UnorderedList) => (BODY_SIZE, INDENT, Some("• ")),
        Some(TagKind::OrderedList) => (BODY_SIZE, INDENT, None),
        Some(TagKind::CodeBlock(_) | TagKind::NativeBlock) => (BODY_SIZE, 0.0, None),
        _ => (BODY_SIZE, 0.0, None),
    }
}

fn heading_size(level: u8) -> f32 {
    (30.0 - (f32::from(level) - 1.0) * 2.5).max(BODY_SIZE_FLOOR)
}
const BODY_SIZE_FLOOR: f32 = 15.0;

/// Maps a paragraph's tag to the confirmed ULSS selector name (inventory §5.2) that governs its
/// block-level styling — `None` (a plain paragraph) maps to `"paragraph"`, the confirmed
/// selector for exactly that case.
fn selector_for_tag(tag: Option<&TagMark>) -> &'static str {
    match tag.map(|t| &t.kind) {
        Some(TagKind::Heading(1)) => "heading-1",
        Some(TagKind::Heading(2)) => "heading-2",
        Some(TagKind::Heading(3)) => "heading-3",
        Some(TagKind::Heading(4)) => "heading-4",
        Some(TagKind::Heading(5)) => "heading-5",
        Some(TagKind::Heading(_)) => "heading-6",
        Some(TagKind::BlockQuote) => "block-quote",
        Some(TagKind::UnorderedList) => "list-unordered",
        Some(TagKind::OrderedList) => "list-ordered",
        Some(TagKind::CodeBlock(_)) => "block-code",
        Some(TagKind::NativeBlock) => "block-raw",
        Some(TagKind::Comment) => "block-comment",
        Some(TagKind::Divider) => "paragraph-divider",
        None => "paragraph",
    }
}

/// Overrides `font_size`/`indent` from the stylesheet's cascaded `font-size`/`margin-left` for
/// this paragraph's selector, leaving either one at its hardcoded default if the stylesheet
/// doesn't set it. Unit conversion is deliberately a no-op (the raw numeric magnitude is used
/// regardless of `pt`/`mm`/etc.) — exact CSS-unit fidelity isn't this task's scope, just proving
/// the active stylesheet's values actually reach the rendering layer's input.
fn apply_stylesheet_overrides(
    tag: Option<&TagMark>,
    sheet: &Stylesheet,
    font_size: &mut f32,
    indent: &mut f32,
) {
    use parrot_style::resolve::{cascade, Value};
    use parrot_style::select::PathNode;

    let selector_name = selector_for_tag(tag);
    let computed = cascade(sheet, &[PathNode::new(selector_name)], None);

    if let Some(Value::Number { value, .. }) = computed.get("font-size") {
        *font_size = *value as f32;
    }
    if let Some(Value::Number { value, .. }) = computed.get("margin-left") {
        *indent = *value as f32;
    }
}

fn flatten_inline(
    inline: &Inline,
    bold: bool,
    italic: bool,
    mono: bool,
    out: &mut Vec<StyledSpan>,
) {
    let push = |text: String, bold: bool, italic: bool, mono: bool, out: &mut Vec<StyledSpan>| {
        if !text.is_empty() {
            out.push(StyledSpan {
                text,
                bold,
                italic,
                monospace: mono,
                color: None,
            });
        }
    };

    match inline {
        Inline::Text(s) => push(s.clone(), bold, italic, mono, out),
        Inline::Strong(children) => {
            for c in children {
                flatten_inline(c, true, italic, mono, out);
            }
        }
        Inline::Emph(children) => {
            for c in children {
                flatten_inline(c, bold, true, mono, out);
            }
        }
        Inline::Code(s) | Inline::InlineNative(s) => push(s.clone(), bold, italic, true, out),
        Inline::Delete(children) | Inline::InlineComment(children) | Inline::Mark(children) => {
            for c in children {
                flatten_inline(c, bold, italic, mono, out);
            }
        }
        Inline::Link { content, .. }
        | Inline::Footnote { content }
        | Inline::Annotation { content, .. } => {
            for c in content {
                flatten_inline(c, bold, italic, mono, out);
            }
        }
        Inline::Image { alt, .. } => push(alt.clone().unwrap_or_default(), bold, italic, mono, out),
        Inline::Video { .. } => {}
    }
}

/// `index` is stamped onto the span's `Attrs` as cosmic-text glyph `metadata` — our only way to
/// recover which original span each shaped glyph came from (used by `run_boundaries`).
fn rich_text_attrs(span: &StyledSpan, index: usize) -> Attrs<'static> {
    let mut attrs = Attrs::new().metadata(index);
    if span.bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if span.italic {
        attrs = attrs.style(Style::Italic);
    }
    if span.monospace {
        attrs = attrs.family(Family::Monospace);
    }
    if let Some((r, g, b)) = span.color {
        attrs = attrs.color(cosmic_text::Color::rgb(r, g, b));
    }
    attrs
}

/// Shapes one paragraph with `cosmic-text` and reports headless-measurable layout metrics.
pub fn shape_paragraph(
    p: &Paragraph,
    available_width: f32,
    stylesheet: Option<&Stylesheet>,
) -> LayoutMetrics {
    let layout = layout_paragraph(p, stylesheet);
    let mut font_system = FontSystem::new();
    let metrics = Metrics::new(layout.font_size, layout.font_size * 1.4);
    let mut buffer = Buffer::new(&mut font_system, metrics);
    buffer.set_size(
        &mut font_system,
        Some(available_width - layout.indent),
        None,
    );

    let rich_spans: Vec<(&str, Attrs)> = layout
        .spans
        .iter()
        .enumerate()
        .map(|(i, s)| (s.text.as_str(), rich_text_attrs(s, i)))
        .collect();
    buffer.set_rich_text(
        &mut font_system,
        rich_spans,
        Attrs::new(),
        Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut font_system, false);

    let mut line_count = 0;
    let mut run_boundaries = Vec::new();
    let mut last_metadata: Option<usize> = None;
    let mut glyph_index = 0;
    for run in buffer.layout_runs() {
        line_count += 1;
        for glyph in run.glyphs.iter() {
            if last_metadata != Some(glyph.metadata) {
                run_boundaries.push(glyph_index);
                last_metadata = Some(glyph.metadata);
            }
            glyph_index += 1;
        }
    }

    LayoutMetrics {
        line_count,
        run_boundaries,
    }
}

/// Paints one paragraph's glyph layout boxes through `egui::Painter`, starting at `origin`.
/// Returns the vertical space consumed, so callers can stack paragraphs. See the module-level
/// `DEVIATION` note: these are layout boxes, not rasterized glyph images.
pub fn paint_paragraph(
    ui: &egui::Ui,
    origin: egui::Pos2,
    p: &Paragraph,
    available_width: f32,
    stylesheet: Option<&Stylesheet>,
) -> f32 {
    let layout = layout_paragraph(p, stylesheet);
    let mut font_system = FontSystem::new();
    let metrics = Metrics::new(layout.font_size, layout.font_size * 1.4);
    let mut buffer = Buffer::new(&mut font_system, metrics);
    buffer.set_size(
        &mut font_system,
        Some(available_width - layout.indent),
        None,
    );

    let rich_spans: Vec<(&str, Attrs)> = layout
        .spans
        .iter()
        .enumerate()
        .map(|(i, s)| (s.text.as_str(), rich_text_attrs(s, i)))
        .collect();
    buffer.set_rich_text(
        &mut font_system,
        rich_spans,
        Attrs::new(),
        Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut font_system, false);

    let painter = ui.painter();
    let mut max_line_bottom = 0.0f32;
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    origin.x + layout.indent + glyph.x,
                    origin.y + run.line_top + glyph.y,
                ),
                egui::vec2(glyph.w.max(1.0), metrics.line_height * 0.6),
            );
            let color = match layout.spans.get(glyph.metadata).and_then(|s| s.color) {
                Some((r, g, b)) => egui::Color32::from_rgb(r, g, b),
                None => egui::Color32::from_white_alpha(140),
            };
            painter.rect_filled(rect, 0.0, color);
        }
        max_line_bottom = run.line_top + run.line_height;
    }
    max_line_bottom
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, TagKind, TagMark};

    fn fixed_ast_paragraph() -> Paragraph {
        Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Heading(2),
                literal: "## ".to_string(),
            }),
            inlines: vec![
                Inline::Text("Intro ".to_string()),
                Inline::Strong(vec![Inline::Text("bold".to_string())]),
                Inline::Text(" and ".to_string()),
                Inline::Emph(vec![Inline::Text("italic".to_string())]),
                Inline::Text(" end.".to_string()),
            ],
        }
    }

    #[test]
    fn layout_metrics_for_fixed_ast() {
        let metrics = shape_paragraph(&fixed_ast_paragraph(), 600.0, None);

        assert_eq!(
            metrics.line_count, 1,
            "this short paragraph must fit on one line at 600px width"
        );
        assert_eq!(
            metrics.run_boundaries.len(),
            5,
            "expected one run boundary per distinct styled span (plain/bold/plain/italic/plain), got {:?}",
            metrics.run_boundaries
        );
        // Boundaries must be strictly increasing (each new span starts after the last).
        for (a, b) in metrics
            .run_boundaries
            .iter()
            .zip(metrics.run_boundaries.iter().skip(1))
        {
            assert!(
                a < b,
                "run boundaries must be strictly increasing: {metrics:?}"
            );
        }
    }

    #[test]
    fn layout_paragraph_captures_block_style() {
        let heading = layout_paragraph(&fixed_ast_paragraph(), None);
        assert!(
            heading.font_size > BODY_SIZE_FLOOR,
            "heading2 must be larger than body text"
        );
        assert_eq!(heading.indent, 0.0);

        let list_item = Paragraph {
            tag: Some(TagMark {
                kind: TagKind::UnorderedList,
                literal: "* ".to_string(),
            }),
            inlines: vec![Inline::Text("an item".to_string())],
        };
        let list_layout = layout_paragraph(&list_item, None);
        assert!(list_layout.indent > 0.0);
        assert_eq!(list_layout.spans[0].text, "• ");
    }

    #[test]
    fn flatten_inline_composes_nested_styles() {
        let p = Paragraph {
            tag: None,
            inlines: vec![Inline::Strong(vec![Inline::Emph(vec![Inline::Text(
                "both".to_string(),
            )])])],
        };
        let layout = layout_paragraph(&p, None);
        assert_eq!(layout.spans.len(), 1);
        assert!(layout.spans[0].bold && layout.spans[0].italic);
    }

    #[test]
    fn codeblock_spans_match_syntect_output() {
        let code = "fn main() {}";
        let p = Paragraph {
            tag: Some(TagMark {
                kind: TagKind::CodeBlock(Some("rust".to_string())),
                literal: "```rust ".to_string(),
            }),
            inlines: vec![Inline::Text(code.to_string())],
        };

        let layout = layout_paragraph(&p, None);
        let expected =
            parrot_syntax::highlight(code, "rust").expect("rust must be a known language");

        assert_eq!(layout.spans.len(), expected.len());
        for (span, exp) in layout.spans.iter().zip(expected.iter()) {
            assert_eq!(
                span.text, exp.text,
                "span text must match syntect's token boundaries"
            );
            assert_eq!(span.color, Some(exp.color));
            assert!(span.monospace, "codeblock spans must be monospace");
        }
    }

    #[test]
    fn codeblock_without_language_is_plain_monospace() {
        let code = "no highlighting here";
        let p = Paragraph {
            tag: Some(TagMark {
                kind: TagKind::CodeBlock(None),
                literal: "``` ".to_string(),
            }),
            inlines: vec![Inline::Text(code.to_string())],
        };

        let layout = layout_paragraph(&p, None);
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].text, code);
        assert_eq!(layout.spans[0].color, None);
        assert!(layout.spans[0].monospace);
    }

    /// T-4-07: switching the active stylesheet changes the rendering layer's computed input
    /// (font_size here) for the same AST — proves the wiring actually threads through, not
    /// just that `Option<&Stylesheet>` compiles.
    #[test]
    fn style_switch_changes_computed_style_input() {
        let heading = Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Heading(1),
                literal: "# ".to_string(),
            }),
            inlines: vec![Inline::Text("Title".to_string())],
        };

        let small = parrot_style::resolve::resolve("heading-1 { font-size: 20pt }")
            .expect("stylesheet must resolve");
        let large = parrot_style::resolve::resolve("heading-1 { font-size: 40pt }")
            .expect("stylesheet must resolve");

        let no_sheet_layout = layout_paragraph(&heading, None);
        let small_layout = layout_paragraph(&heading, Some(&small));
        let large_layout = layout_paragraph(&heading, Some(&large));

        assert_eq!(small_layout.font_size, 20.0);
        assert_eq!(large_layout.font_size, 40.0);
        assert_ne!(
            small_layout.font_size, large_layout.font_size,
            "switching the active stylesheet must change the computed font size"
        );
        // And both differ from the no-stylesheet hardcoded default, proving the override path
        // (not just a coincidental match) is what's firing.
        assert_ne!(no_sheet_layout.font_size, small_layout.font_size);
        assert_ne!(no_sheet_layout.font_size, large_layout.font_size);
    }
}
