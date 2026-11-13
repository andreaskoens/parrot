//! Four-panel window layout (T-12-01, ui-description §2/§7.1): Library (P1) | Sheet List (P2) |
//! Editor (P3) | Inspector (P4). Plain data, no `egui` dependency — `ui.rs` reads
//! [`PanelLayout::is_visible`] and the width constants to build its `SidePanel`s, so the
//! collapse rules are testable headlessly.

/// Panel 1 (Library sidebar) default width, ui-description §2.
pub const LIBRARY_WIDTH: f32 = 160.0;
/// Panel 2 (Sheet List) default width, ui-description §2.
pub const SHEET_LIST_WIDTH: f32 = 280.0;
/// Panel 4 (Inspector) default width, ui-description §2.
pub const INSPECTOR_WIDTH: f32 = 220.0;

/// The four panels, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Library,
    SheetList,
    Editor,
    Inspector,
}

impl Panel {
    pub const ALL: [Panel; 4] = [
        Panel::Library,
        Panel::SheetList,
        Panel::Editor,
        Panel::Inspector,
    ];

    /// Default width, or `None` for the Editor, which fills whatever space is left.
    pub fn default_width(self) -> Option<f32> {
        match self {
            Panel::Library => Some(LIBRARY_WIDTH),
            Panel::SheetList => Some(SHEET_LIST_WIDTH),
            Panel::Editor => None,
            Panel::Inspector => Some(INSPECTOR_WIDTH),
        }
    }
}

/// Which panels are shown. P1+P2 share one flag because the spec has no way to collapse P2
/// independently of P1 (§2 "Panel collapse behavior").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelLayout {
    pub show_sidebar: bool,
    pub show_inspector: bool,
    /// The §7.1 layout toggle: editor-only ("single-pane") view. Overrides both flags above
    /// without clearing them, so switching back restores whatever was visible before.
    pub editor_only: bool,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            show_sidebar: true,
            show_inspector: true,
            editor_only: false,
        }
    }
}

impl PanelLayout {
    /// Toolbar sidebar toggle (§7.1): shows/hides P1 and P2 together. Leaving editor-only mode
    /// is part of the same click — otherwise the toggle would appear to do nothing.
    pub fn toggle_sidebar(&mut self) {
        if self.editor_only {
            self.editor_only = false;
            self.show_sidebar = true;
        } else {
            self.show_sidebar = !self.show_sidebar;
        }
    }

    /// Toolbar inspector toggle (§7.1): P4 only, independent of the sidebar.
    pub fn toggle_inspector(&mut self) {
        if self.editor_only {
            self.editor_only = false;
            self.show_inspector = true;
        } else {
            self.show_inspector = !self.show_inspector;
        }
    }

    /// Toolbar layout toggle (§7.1): single-pane (editor only) vs. the full layout.
    pub fn toggle_editor_only(&mut self) {
        self.editor_only = !self.editor_only;
    }

    pub fn is_visible(&self, panel: Panel) -> bool {
        match panel {
            Panel::Editor => true,
            _ if self.editor_only => false,
            Panel::Library | Panel::SheetList => self.show_sidebar,
            Panel::Inspector => self.show_inspector,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible(layout: &PanelLayout) -> Vec<Panel> {
        Panel::ALL
            .into_iter()
            .filter(|p| layout.is_visible(*p))
            .collect()
    }

    #[test]
    fn sidebar_toggle_hides_p1_and_p2_together() {
        let mut layout = PanelLayout::default();
        assert_eq!(visible(&layout), Panel::ALL.to_vec());

        layout.toggle_sidebar();
        assert_eq!(visible(&layout), vec![Panel::Editor, Panel::Inspector]);

        layout.toggle_sidebar();
        assert_eq!(visible(&layout), Panel::ALL.to_vec());
    }

    #[test]
    fn inspector_toggle_independent() {
        let mut layout = PanelLayout::default();
        layout.toggle_inspector();
        assert_eq!(
            visible(&layout),
            vec![Panel::Library, Panel::SheetList, Panel::Editor]
        );

        // Sidebar toggling leaves the inspector's own state alone, and vice versa.
        layout.toggle_sidebar();
        assert_eq!(visible(&layout), vec![Panel::Editor]);
        layout.toggle_inspector();
        assert_eq!(visible(&layout), vec![Panel::Editor, Panel::Inspector]);
    }

    #[test]
    fn default_panel_widths_match_spec() {
        assert_eq!(Panel::Library.default_width(), Some(160.0));
        assert_eq!(Panel::SheetList.default_width(), Some(280.0));
        assert_eq!(Panel::Editor.default_width(), None);
        assert_eq!(Panel::Inspector.default_width(), Some(220.0));
    }

    #[test]
    fn editor_only_restores_previous_layout() {
        let mut layout = PanelLayout::default();
        layout.toggle_inspector();
        layout.toggle_editor_only();
        assert_eq!(visible(&layout), vec![Panel::Editor]);
        layout.toggle_editor_only();
        assert_eq!(
            visible(&layout),
            vec![Panel::Library, Panel::SheetList, Panel::Editor]
        );
    }
}
