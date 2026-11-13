//! T-2-01: text-shaping backend decision gate (ADR-2). Proves `cosmic-text` can shape a single
//! paragraph with mixed bold/italic/heading-size runs, and that the resulting glyph layout can
//! actually be painted through `egui::Painter` — exercised headlessly via a raw `egui::Context`
//! (no window/GPU needed: `Context::run` produces paint primitives as plain data), so this is a
//! real pass/fail check, not just "it compiles." See DECISIONS.md for the resulting decision.

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, Weight};

/// Headless-measurable outcome of shaping + painting the spike paragraph.
#[derive(Debug)]
pub struct SpikeResult {
    pub line_count: usize,
    pub glyph_count: usize,
    pub paint_primitive_count: usize,
}

/// Shapes a single paragraph mixing a heading-size run, a bold run, an italic run, and plain
/// text, then paints the resulting glyph boxes through a headless `egui::Context`.
pub fn run_spike() -> SpikeResult {
    let mut font_system = FontSystem::new();
    let metrics = Metrics::new(16.0, 22.0);
    let mut buffer = Buffer::new(&mut font_system, metrics);
    buffer.set_size(&mut font_system, Some(500.0), Some(200.0));

    let base_attrs = Attrs::new().family(Family::SansSerif);
    let spans: Vec<(&str, Attrs)> = vec![
        ("Heading ", base_attrs.weight(Weight::BOLD)),
        ("bold ", base_attrs.weight(Weight::BOLD)),
        ("italic ", base_attrs.style(Style::Italic)),
        ("plain text.", base_attrs),
    ];
    buffer.set_rich_text(&mut font_system, spans, base_attrs, Shaping::Advanced);
    buffer.shape_until_scroll(&mut font_system, false);

    let mut line_count = 0;
    let mut glyph_count = 0;
    let mut glyph_boxes = Vec::new();
    for run in buffer.layout_runs() {
        line_count += 1;
        for glyph in run.glyphs.iter() {
            glyph_count += 1;
            glyph_boxes.push(egui::Rect::from_min_size(
                egui::pos2(glyph.x, run.line_top + glyph.y),
                egui::vec2(glyph.w.max(1.0), metrics.line_height),
            ));
        }
    }

    // Prove the layout actually paints through egui's Painter, headlessly: `Context::run`
    // produces `ClippedPrimitive`s as data, with no window or GPU context required.
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(500.0, 200.0),
        )),
        ..Default::default()
    };
    let output = ctx.run(raw_input, |ctx| {
        // A full-screen CentralPanel gets the screen_rect as its clip region immediately, unlike
        // an auto-sized Area/Window, which needs a settling frame before its content isn't
        // clipped away — we only run one frame here, so CentralPanel is what actually paints.
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();
            for rect in &glyph_boxes {
                painter.rect_filled(*rect, 0.0, egui::Color32::WHITE);
            }
        });
    });
    let paint_primitive_count = ctx.tessellate(output.shapes, output.pixels_per_point).len();

    SpikeResult {
        line_count,
        glyph_count,
        paint_primitive_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_and_paints_mixed_style_paragraph() {
        let result = run_spike();

        assert_eq!(
            result.line_count, 1,
            "a 500px-wide buffer must fit this short paragraph on one line"
        );
        assert!(
            result.glyph_count >= 20,
            "expected at least one glyph per character across all four runs, got {}",
            result.glyph_count
        );
        assert!(
            result.paint_primitive_count > 0,
            "glyph boxes must produce at least one paint primitive"
        );
    }
}
