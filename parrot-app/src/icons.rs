//! Every UI icon, by concept (README "Icons": one glyph per concept). All come from the
//! Phosphor icon font (`egui-phosphor`, regular weight, MIT) registered by `ui::setup` — never
//! an emoji or a stray Unicode symbol, which egui's bundled fonts and Segoe UI only partly cover
//! (ten of the previous glyphs rendered as missing-glyph boxes).

use egui_phosphor::regular as ph;

// Library tree nodes.
pub const SHEET: &str = ph::FILE_TEXT;
pub const GROUP: &str = ph::FOLDER;
pub const PROJECT: &str = ph::BOOKS;
/// The one search/filter glyph: saved filters, the search field, search results.
pub const SEARCH: &str = ph::MAGNIFYING_GLASS;
pub const EXPANDED: &str = ph::CARET_DOWN;
pub const COLLAPSED: &str = ph::CARET_RIGHT;

// Smart filters.
pub const ALL_SHEETS: &str = ph::STACK;
pub const RECENT: &str = ph::CLOCK_COUNTER_CLOCKWISE;
pub const TRASH: &str = ph::TRASH;

// Toolbar.
pub const TOGGLE_SIDEBAR: &str = ph::SIDEBAR_SIMPLE;
pub const EDITOR_ONLY: &str = ph::CORNERS_OUT;
pub const MARKUP: &str = ph::MARKDOWN_LOGO;
pub const BOLD: &str = ph::TEXT_B;
pub const ITALIC: &str = ph::TEXT_ITALIC;
pub const LINK: &str = ph::LINK;
pub const PARAGRAPH: &str = ph::PARAGRAPH;
pub const EXPORT: &str = ph::EXPORT;
pub const QUICK_EXPORT: &str = ph::LIGHTNING;
pub const TOGGLE_INSPECTOR: &str = ph::INFO;

// Inspector tabs and media entries.
pub const STATISTICS: &str = ph::CHART_LINE_UP;
pub const KEYWORDS: &str = ph::TAG;
pub const OUTLINE: &str = ph::LIST_BULLETS;
pub const IMAGE: &str = ph::IMAGE;
pub const VIDEO: &str = ph::FILM_STRIP;
pub const NOTE: &str = ph::NOTE;
pub const FILE: &str = ph::FILE;
/// The row badge meaning "this sheet has attachments".
pub const ATTACHMENT: &str = ph::PAPERCLIP;

// Actions and misc.
pub const ADD: &str = ph::PLUS;
pub const REMOVE: &str = ph::X;
pub const DELETE: &str = ph::TRASH;
pub const RENAME: &str = ph::PENCIL_SIMPLE;
pub const STYLE: &str = ph::GEAR;
pub const LIGHT: &str = ph::SUN;
pub const DARK: &str = ph::MOON;
pub const WARNING: &str = ph::WARNING_CIRCLE;
pub const BROWSE: &str = ph::FOLDER_NOTCH_OPEN;
pub const OPEN_EXTERNAL: &str = ph::ARROW_SQUARE_OUT;
pub const CHARACTER_COUNTER: &str = ph::TEXT_AA;
/// The Inspector's Spelling & Grammar tab.
pub const PROOFING: &str = ph::SEAL_CHECK;
/// A Draft Sheet's badge.
pub const DRAFT: &str = ph::FILE_DASHED;
/// The Library shelf (home).
pub const SHELF: &str = ph::BOOKS;
/// Accepts an inline edit (rename).
pub const CHECK: &str = ph::CHECK;

/// Every icon above, for the glyph-coverage test.
pub const ALL: &[&str] = &[
    SHEET,
    GROUP,
    PROJECT,
    SEARCH,
    EXPANDED,
    COLLAPSED,
    ALL_SHEETS,
    RECENT,
    TRASH,
    TOGGLE_SIDEBAR,
    EDITOR_ONLY,
    MARKUP,
    BOLD,
    ITALIC,
    LINK,
    PARAGRAPH,
    EXPORT,
    QUICK_EXPORT,
    TOGGLE_INSPECTOR,
    STATISTICS,
    KEYWORDS,
    OUTLINE,
    IMAGE,
    VIDEO,
    NOTE,
    FILE,
    ATTACHMENT,
    ADD,
    REMOVE,
    DELETE,
    RENAME,
    STYLE,
    LIGHT,
    DARK,
    WARNING,
    BROWSE,
    OPEN_EXTERNAL,
    CHARACTER_COUNTER,
    PROOFING,
    CHECK,
    DRAFT,
    SHELF,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each icon is one Phosphor codepoint (Private Use Area), so it can only come from the
    /// Phosphor font — not from an emoji fallback that might be missing.
    #[test]
    fn every_icon_is_a_phosphor_glyph() {
        for icon in ALL {
            let mut chars = icon.chars();
            let c = chars.next().expect("non-empty icon");
            assert!(chars.next().is_none(), "{icon:?} is more than one char");
            assert!(
                ('\u{E000}'..='\u{F8FF}').contains(&c),
                "{icon:?} (U+{:04X}) is not a Phosphor private-use glyph",
                c as u32
            );
        }
    }

    /// Every icon resolves to a real glyph in the fonts the app installs, in both the regular
    /// and bold families (the bold one is used for strong labels).
    #[test]
    fn every_icon_renders_with_installed_fonts() {
        let ctx = egui::Context::default();
        crate::ui::setup(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let families = [
            egui::FontFamily::Proportional,
            egui::FontFamily::Name(crate::tokens::FAMILY_BOLD.into()),
        ];
        for family in families {
            let font = egui::FontId::new(14.0, family.clone());
            let missing: Vec<&str> = ALL
                .iter()
                .copied()
                .filter(|icon| !ctx.fonts(|f| f.has_glyphs(&font, icon)))
                .collect();
            assert!(missing.is_empty(), "{family:?} is missing {missing:?}");
        }
    }
}
