//! Toolbar actions (T-12-09, ui-description §7.1/§7.2), in three places: the window toolbar
//! (sidebar and layout toggles left, inspector toggle right), the sheet area's top-left
//! (Export, Quick Export), and the editor's bottom bar (Show Markdown, **B**/*I*/Link/¶).
//! (The macOS app's "Subscribe Now" button is excluded with the rest of licensing, roadmap §2.)
//!
//! Formatting acts on an [`EditorState`] holding the active editor's text and selection, so
//! each button produces exactly the Markdown-XL a writer would type by hand.

use crate::editor_state::EditorState;
use crate::icons;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    ToggleSidebar,
    ToggleEditorOnly,
    ToggleMarkup,
    Bold,
    Italic,
    Link,
    ParagraphStyle,
    Export,
    QuickExport,
    ToggleInspector,
}

impl ToolbarAction {
    /// Window toolbar, left-aligned, left to right.
    pub const WINDOW_LEFT: [ToolbarAction; 2] = [
        ToolbarAction::ToggleSidebar,
        ToolbarAction::ToggleEditorOnly,
    ];
    /// Window toolbar, right-aligned.
    pub const WINDOW_RIGHT: [ToolbarAction; 1] = [ToolbarAction::ToggleInspector];
    /// Top-left of the sheet viewing area.
    pub const SHEET_AREA: [ToolbarAction; 2] = [ToolbarAction::Export, ToolbarAction::QuickExport];
    /// The editor's bottom bar, left to right.
    pub const FORMAT_BAR: [ToolbarAction; 5] = [
        ToolbarAction::ToggleMarkup,
        ToolbarAction::Bold,
        ToolbarAction::Italic,
        ToolbarAction::Link,
        ToolbarAction::ParagraphStyle,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            ToolbarAction::ToggleSidebar => icons::TOGGLE_SIDEBAR,
            ToolbarAction::ToggleEditorOnly => icons::EDITOR_ONLY,
            ToolbarAction::ToggleMarkup => icons::MARKUP,
            ToolbarAction::Bold => icons::BOLD,
            ToolbarAction::Italic => icons::ITALIC,
            ToolbarAction::Link => icons::LINK,
            ToolbarAction::ParagraphStyle => icons::PARAGRAPH,
            ToolbarAction::Export => icons::EXPORT,
            ToolbarAction::QuickExport => icons::QUICK_EXPORT,
            ToolbarAction::ToggleInspector => icons::TOGGLE_INSPECTOR,
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            ToolbarAction::ToggleSidebar => "Show/hide library and sheet list",
            ToolbarAction::ToggleEditorOnly => "Editor only / all panels",
            ToolbarAction::ToggleMarkup => "Show Markdown (off: formatted text only)",
            ToolbarAction::Bold => "Bold (**…**)",
            ToolbarAction::Italic => "Italic (*…*)",
            ToolbarAction::Link => "Link ([…](url))",
            ToolbarAction::ParagraphStyle => "Paragraph style",
            ToolbarAction::Export => "Export…",
            ToolbarAction::QuickExport => "Quick Export (repeat last export)",
            ToolbarAction::ToggleInspector => "Show/hide inspector",
        }
    }

    /// Whether the action edits the active editor's text (disabled with no editor focused).
    pub fn needs_editor(self) -> bool {
        matches!(
            self,
            ToolbarAction::Bold
                | ToolbarAction::Italic
                | ToolbarAction::Link
                | ToolbarAction::ParagraphStyle
        )
    }
}

/// Bold button: same wrap/unwrap as the Ctrl+B shortcut (T-2-06).
pub fn bold(editor: &mut EditorState) {
    crate::shortcuts::toggle_bold(editor);
}

/// Italic button: same as Ctrl+I.
pub fn italic(editor: &mut EditorState) {
    crate::shortcuts::toggle_italic(editor);
}

/// Link button, after the URL prompt: wraps the selection as `[selection](url)` and leaves the
/// label selected. An empty selection inserts `[url](url)`.
pub fn link(editor: &mut EditorState, url: &str) {
    let (start, end) = editor.selection().range();
    let selected: String = editor
        .text()
        .chars()
        .skip(start)
        .take(end - start)
        .collect();
    let label = if selected.is_empty() {
        url.to_string()
    } else {
        selected
    };
    let label_len = label.chars().count();
    editor.replace_selection(&format!("[{label}]({url})"));
    editor.set_selection(start + 1, start + 1 + label_len);
}

/// ¶ picker entries: the block-level styles a paragraph can be switched to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphStyle {
    Body,
    Heading(u8),
    BlockQuote,
    UnorderedList,
    OrderedList,
    CodeBlock,
    Comment,
}

impl ParagraphStyle {
    pub const ALL: [ParagraphStyle; 12] = [
        ParagraphStyle::Body,
        ParagraphStyle::Heading(1),
        ParagraphStyle::Heading(2),
        ParagraphStyle::Heading(3),
        ParagraphStyle::Heading(4),
        ParagraphStyle::Heading(5),
        ParagraphStyle::Heading(6),
        ParagraphStyle::BlockQuote,
        ParagraphStyle::UnorderedList,
        ParagraphStyle::OrderedList,
        ParagraphStyle::CodeBlock,
        ParagraphStyle::Comment,
    ];

    pub fn label(self) -> String {
        match self {
            ParagraphStyle::Body => "Body".to_string(),
            ParagraphStyle::Heading(n) => format!("Heading {n}"),
            ParagraphStyle::BlockQuote => "Block Quote".to_string(),
            ParagraphStyle::UnorderedList => "Unordered List".to_string(),
            ParagraphStyle::OrderedList => "Ordered List".to_string(),
            ParagraphStyle::CodeBlock => "Code Block".to_string(),
            ParagraphStyle::Comment => "Comment".to_string(),
        }
    }

    /// The Markdown-XL line prefix `parrot_markup::parse` reads as this style.
    pub fn prefix(self) -> String {
        match self {
            ParagraphStyle::Body => String::new(),
            ParagraphStyle::Heading(n) => format!("{} ", "#".repeat(usize::from(n.clamp(1, 6)))),
            ParagraphStyle::BlockQuote => "> ".to_string(),
            ParagraphStyle::UnorderedList => "* ".to_string(),
            ParagraphStyle::OrderedList => "1. ".to_string(),
            ParagraphStyle::CodeBlock => "``` ".to_string(),
            ParagraphStyle::Comment => "%% ".to_string(),
        }
    }
}

/// ¶ picker: replaces the block prefix of the paragraph holding the cursor with `style`'s,
/// keeping the cursor on the same text.
pub fn set_paragraph_style(editor: &mut EditorState, style: ParagraphStyle) {
    let text = editor.text().to_string();
    let cursor_byte = text
        .char_indices()
        .nth(editor.selection().head)
        .map_or(text.len(), |(b, _)| b);
    let start = text[..cursor_byte].rfind("\n\n").map_or(0, |i| i + 2);
    let end = text[start..].find("\n\n").map_or(text.len(), |i| start + i);
    let paragraph = &text[start..end];
    let old_prefix = parrot_markup::parse::parse_paragraph(paragraph)
        .tag
        .filter(|t| paragraph.starts_with(t.literal.as_str()))
        .map_or(0, |t| t.literal.len());

    let start_char = text[..start].chars().count();
    let old_prefix_chars = paragraph[..old_prefix].chars().count();
    let new_prefix = style.prefix();
    let new_prefix_chars = new_prefix.chars().count();
    let (sel_start, sel_end) = editor.selection().range();
    let shift = |pos: usize| {
        if pos < start_char + old_prefix_chars {
            start_char + new_prefix_chars
        } else {
            pos + new_prefix_chars - old_prefix_chars
        }
    };
    let (new_start, new_end) = (shift(sel_start), shift(sel_end));
    editor.set_selection(start_char, start_char + old_prefix_chars);
    editor.replace_selection(&new_prefix);
    editor.set_selection(new_start, new_end);
}

#[cfg(test)]
mod tests {
    #[test]
    fn formatting_lives_in_the_editor_bar_and_export_in_the_sheet_area() {
        let window: Vec<ToolbarAction> = ToolbarAction::WINDOW_LEFT
            .into_iter()
            .chain(ToolbarAction::WINDOW_RIGHT)
            .collect();
        for action in ToolbarAction::FORMAT_BAR {
            assert!(!window.contains(&action), "{action:?} left the top rail");
        }
        assert!(ToolbarAction::FORMAT_BAR.contains(&ToolbarAction::Bold));
        assert!(ToolbarAction::FORMAT_BAR.contains(&ToolbarAction::ParagraphStyle));
        assert_eq!(ToolbarAction::SHEET_AREA[0], ToolbarAction::Export);
        assert!(!window.contains(&ToolbarAction::Export));
    }
    use super::*;
    use crate::export_modal::ExportModal;
    use parrot_markup::parse::parse_paragraph;

    fn select(text: &str, needle: &str) -> EditorState {
        let mut editor = EditorState::new(text);
        let byte = text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} not in {text}"));
        let start = text[..byte].chars().count();
        editor.set_selection(start, start + needle.chars().count());
        editor
    }

    #[test]
    fn bold_button_matches_markup_strong() {
        let mut editor = select("make this strong", "this");
        bold(&mut editor);
        assert_eq!(editor.text(), "make **this** strong");
        assert_eq!(
            editor.to_ast().paragraphs[0],
            parse_paragraph("make **this** strong")
        );
    }

    #[test]
    fn italic_button_matches_markup_emph() {
        let mut editor = select("make this emphasized", "this");
        italic(&mut editor);
        assert_eq!(
            editor.to_ast().paragraphs[0],
            parse_paragraph("make *this* emphasized")
        );
    }

    #[test]
    fn link_button_wraps_selection() {
        let mut editor = select("read the docs now", "the docs");
        link(&mut editor, "https://example.org");
        assert_eq!(editor.text(), "read [the docs](https://example.org) now");
        assert_eq!(
            editor.to_ast().paragraphs[0],
            parse_paragraph("read [the docs](https://example.org) now")
        );
        let (s, e) = editor.selection().range();
        let selected: String = editor.text().chars().skip(s).take(e - s).collect();
        assert_eq!(selected, "the docs");
    }

    #[test]
    fn paragraph_style_replaces_prefix() {
        let mut editor = select("First\n\n## Second para\n\nThird", "Second");
        set_paragraph_style(&mut editor, ParagraphStyle::BlockQuote);
        assert_eq!(editor.text(), "First\n\n> Second para\n\nThird");
        set_paragraph_style(&mut editor, ParagraphStyle::Body);
        assert_eq!(editor.text(), "First\n\nSecond para\n\nThird");
        set_paragraph_style(&mut editor, ParagraphStyle::Heading(1));
        assert_eq!(
            editor.to_ast().paragraphs[1],
            parse_paragraph("# Second para")
        );
    }

    #[test]
    fn export_and_quick_export_are_distinct_actions() {
        assert_ne!(ToolbarAction::Export, ToolbarAction::QuickExport);
        let dir = tempfile::tempdir().expect("tempdir");
        let content = EditorState::new("# Hello").to_ast();

        // Export opens the modal and writes nothing.
        let mut modal = ExportModal::default();
        modal.open_for(vec![vec![0]], "Hello", Some(dir.path()));
        assert!(modal.open);
        assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 0);

        // Quick Export writes directly and never opens the modal.
        modal.export(&content).expect("first export");
        let before = std::fs::read_dir(dir.path()).expect("dir").count();
        let path = modal.quick_export(&content, "Again").expect("quick export");
        assert!(!modal.open);
        assert!(path.exists());
        assert_eq!(
            std::fs::read_dir(dir.path()).expect("dir").count(),
            before + 1
        );
    }
}
