//! Visual design tokens (T-12-02): the dark-theme palette (ui-description §10.1), typography
//! scale (§10.2) and 4px spacing grid (§10.3), as the one place in `parrot-app` allowed to name
//! a literal color — `scripts/check_no_hardcoded_colors.sh` enforces that.

use egui::{Color32, FontFamily, FontId};

// --- Color palette (§10.1) ---

pub const BG_DEEP: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
pub const BG_MID: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x2e);
pub const SURFACE_INSET: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
pub const ACCENT: Color32 = Color32::from_rgb(0x28, 0x5e, 0x51);
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0x5d, 0xb5, 0x9a);
/// A library row that holds the selection without being it (the group of the open sheet):
/// a dim accent, distinct from both the full `ACCENT` fill and the gray hover.
pub const ACCENT_MUTED: Color32 = Color32::from_rgb(0x20, 0x3a, 0x33);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xf0, 0xf0, 0xf0);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xd0, 0xd0, 0xd0);
/// §5.1's "subtitle/first paragraph after a heading" body color.
pub const TEXT_SUBTITLE: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xc0);
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);
pub const TEXT_QUATERNARY: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
/// §5.1's code-block text color.
pub const TEXT_CODE_BLOCK: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);
pub const SPELLING: Color32 = Color32::from_rgb(0xf5, 0xa6, 0x23);
pub const GRAMMAR: Color32 = Color32::from_rgb(0x5a, 0xb0, 0xf6);
pub const DIVIDER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
pub const GOAL_INCOMPLETE: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
pub const GOAL_ALMOST: Color32 = Color32::from_rgb(0xf0, 0xa5, 0x00);
pub const GOAL_COMPLETE: Color32 = Color32::from_rgb(0x4c, 0xaf, 0x50);
/// §4.2's text-checker error dot. Not in §10.1's table; picked to read as "error" against
/// `BG_MID` without colliding with the spelling orange.
pub const ERROR: Color32 = Color32::from_rgb(0xe0, 0x4b, 0x4b);
/// §8's export preview "page" surfaces (light/dark preview theme).
pub const PAGE_LIGHT: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const PAGE_LIGHT_TEXT: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);

// --- Typography scale (§10.2) ---

/// Bold font family registered by `ui::install_fonts`: a system bold face when one is found,
/// otherwise an alias of the default proportional face, so it's always valid. Italics use
/// egui's synthetic slant (`TextFormat::italics`) instead of a separate face.
pub const FAMILY_BOLD: &str = "parrot-bold";

/// One §10.2 token: size in px plus whether it uses the bold face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeToken {
    pub size: f32,
    pub bold: bool,
    pub mono: bool,
}

impl TypeToken {
    const fn new(size: f32, bold: bool, mono: bool) -> Self {
        Self { size, bold, mono }
    }

    pub fn font_id(&self) -> FontId {
        let family = if self.mono {
            FontFamily::Monospace
        } else if self.bold {
            FontFamily::Name(FAMILY_BOLD.into())
        } else {
            FontFamily::Proportional
        };
        FontId::new(self.size, family)
    }
}

pub const HEADING_1: TypeToken = TypeToken::new(28.0, true, false);
pub const HEADING_2: TypeToken = TypeToken::new(22.0, true, false);
pub const HEADING_3: TypeToken = TypeToken::new(18.0, true, false);
pub const HEADING_4: TypeToken = TypeToken::new(16.0, true, false);
pub const HEADING_5: TypeToken = TypeToken::new(15.0, true, false);
pub const HEADING_6: TypeToken = TypeToken::new(14.0, true, false);
pub const BODY: TypeToken = TypeToken::new(15.0, false, false);
pub const UI_LABEL: TypeToken = TypeToken::new(13.0, false, false);
/// §4.2 sheet-row title: `ui-label` size, semibold.
pub const UI_LABEL_STRONG: TypeToken = TypeToken::new(13.0, true, false);
pub const UI_CAPTION: TypeToken = TypeToken::new(11.0, false, false);
pub const UI_SECTION: TypeToken = TypeToken::new(11.0, true, false);
/// §4.1 container title header above the sheet list.
pub const UI_HEADER: TypeToken = TypeToken::new(14.0, true, false);
pub const MONO: TypeToken = TypeToken::new(13.0, false, true);
/// §5.1's code-block size (one step below inline `mono`).
pub const MONO_BLOCK: TypeToken = TypeToken::new(12.0, false, true);

/// The heading token for Markdown-XL heading level `level` (1–6; out of range clamps).
pub fn heading(level: u8) -> TypeToken {
    match level {
        0 | 1 => HEADING_1,
        2 => HEADING_2,
        3 => HEADING_3,
        4 => HEADING_4,
        5 => HEADING_5,
        _ => HEADING_6,
    }
}

// --- Spacing grid (§10.3) ---

pub const GRID: f32 = 4.0;
pub const PANEL_PADDING_X: f32 = 3.0 * GRID;
pub const PANEL_PADDING_Y: f32 = 2.0 * GRID;
pub const TREE_ROW_HEIGHT: f32 = 22.0;
/// Within §10.3's 80–100px band; 88 = 22 grid units.
pub const SHEET_ROW_HEIGHT: f32 = 22.0 * GRID;
pub const INDENT: f32 = 4.0 * GRID;
pub const INSPECTOR_ICON_SIZE: f32 = 4.0 * GRID;
pub const SELECTION_RADIUS: f32 = GRID;
pub const BADGE_RADIUS: f32 = 10.0;
/// §3.3 goal ring diameter.
pub const GOAL_RING_DIAMETER: f32 = 14.0;

/// Applies the palette to egui's own widget visuals so stock widgets (buttons, text edits,
/// scrollbars, windows) match the custom-painted surfaces.
pub fn apply_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_PANEL;
    visuals.window_fill = BG_ELEVATED;
    visuals.extreme_bg_color = SURFACE_INSET;
    visuals.faint_bg_color = BG_MID;
    visuals.override_text_color = Some(TEXT_SECONDARY);
    visuals.hyperlink_color = ACCENT_LIGHT;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, DIVIDER);
    visuals.widgets.hovered.weak_bg_fill = BG_ELEVATED;
    visuals.widgets.inactive.weak_bg_fill = BG_MID;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, DIVIDER);
    let radius = egui::Rounding::same(SELECTION_RADIUS);
    visuals.widgets.inactive.rounding = radius;
    visuals.widgets.hovered.rounding = radius;
    visuals.widgets.active.rounding = radius;
    ctx.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_heights_match_spacing_grid() {
        assert_eq!(TREE_ROW_HEIGHT, 22.0);
        assert!((80.0..=100.0).contains(&SHEET_ROW_HEIGHT));
        assert_eq!(SHEET_ROW_HEIGHT % GRID, 0.0);
        for value in [
            PANEL_PADDING_X,
            PANEL_PADDING_Y,
            INDENT,
            INSPECTOR_ICON_SIZE,
            SELECTION_RADIUS,
        ] {
            assert_eq!(value % GRID, 0.0, "{value} must sit on the 4px grid");
        }
        assert_eq!((PANEL_PADDING_X, PANEL_PADDING_Y), (12.0, 8.0));
    }

    #[test]
    fn heading_scale_descends() {
        let sizes: Vec<f32> = (1..=6).map(|l| heading(l).size).collect();
        assert!(sizes.windows(2).all(|w| w[0] > w[1]), "{sizes:?}");
        assert_eq!(heading(1).size, 28.0);
        assert_eq!(BODY.size, 15.0);
    }
}
