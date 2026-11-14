//! Built-in CSS export styles: four manuscript-style themes adapted from Ariadne
//! (MIT, same author — see `THIRD_PARTY_LICENSES.md`) plus two originals matching this
//! app's own Default/Compact editor styles.
//!
//! These are plain CSS text, resolved per export by `parrot_export::css`. Supported
//! properties are the engine's subset (see `parrot-export`'s README): `body`, `p`,
//! `li`, `ul`, `ol`, `h1`–`h6`, `blockquote`, `pre`, `code`, `a` selectors plus
//! `@page` size/margins. Anything else passes through to HTML/EPUB verbatim and is
//! ignored by the fixed-layout backends.

/// `name` → CSS source for every built-in export style.
pub const BUILTIN_EXPORT_STYLES: &[(&str, &str)] = &[
    ("Built-in", ""),
    ("Manuscript", MANUSCRIPT_CSS),
    ("Student", STUDENT_CSS),
    ("Paperback Novel", PAPERBACK_CSS),
    ("Professional", PROFESSIONAL_CSS),
    ("Parrot Default", PARROT_DEFAULT_CSS),
    ("Parrot Compact", PARROT_COMPACT_CSS),
];

/// The default look: no CSS at all (the unstyled export each backend has always made).
pub const BUILT_IN_NAME: &str = "Built-in";

/// CSS text for a built-in style name, or `None` for an unknown name.
pub fn css_for(name: &str) -> Option<&'static str> {
    BUILTIN_EXPORT_STYLES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, css)| *css)
}

pub fn available_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_EXPORT_STYLES.iter().map(|(n, _)| *n)
}

// Adapted from Ariadne's built-in export styles (MIT, same author): standard
// submission, school paper, trade paperback and neutral professional looks.
// Deliberate divergence: Manuscript/Student/Professional drop Ariadne's `@page size`
// (Letter/Letter/A4) so the dialog's PAGE picker governs; their `@page margin` rules
// still apply. Paperback keeps its 5.5 x 8.5in trim size — it is the style.
const MANUSCRIPT_CSS: &str = "/* Manuscript — standard submission format: 11pt Courier, */\n/* double-spaced with indented paragraphs and 1-inch margins. */\nbody {\n  font-family: \"Courier New\", Courier, monospace;\n  color: #000000;\n  font-size: 11pt;\n  line-height: 2;\n  text-indent: 36pt;\n  margin-bottom: 0;\n}\nh1 {\n  font-size: 11pt;\n  font-weight: normal;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 12pt;\n  break-before: always;\n}\nh2 {\n  font-size: 11pt;\n  font-weight: normal;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\nh3 {\n  font-size: 11pt;\n  font-weight: normal;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\na {\n  color: #000000;\n}\nblockquote {\n  margin-left: 36pt;\n  margin-right: 36pt;\n  text-indent: 0;\n}\npre {\n  font-size: 11pt;\n  text-indent: 0;\n}\n@page {\n  margin: 1in;\n}\n";

const STUDENT_CSS: &str = "/* Student — MLA-style paper: 12pt Times New Roman, */\n/* double-spaced with indented paragraphs and 1-inch margins. */\nbody {\n  font-family: \"Times New Roman\", Times, serif;\n  color: #000000;\n  font-size: 12pt;\n  line-height: 2;\n  text-indent: 36pt;\n  margin-bottom: 0;\n}\nh1 {\n  font-size: 12pt;\n  font-weight: normal;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\nh2 {\n  font-size: 12pt;\n  font-weight: bold;\n  text-align: left;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\nh3 {\n  font-size: 12pt;\n  font-weight: bold;\n  text-align: left;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\na {\n  color: #000000;\n}\nblockquote {\n  margin-left: 36pt;\n  text-indent: 0;\n  line-height: 2;\n}\npre {\n  font-size: 12pt;\n  text-indent: 0;\n}\n@page {\n  margin: 1in;\n}\n";

const PAPERBACK_CSS: &str = "/* Paperback Novel — justified serif pages with first-line */\n/* indents on a 5.5 x 8.5 inch trade paperback sheet. */\nbody {\n  font-family: Georgia, \"Times New Roman\", serif;\n  color: #1a1a1a;\n  font-size: 11pt;\n  line-height: 1.5;\n  text-align: justify;\n  text-indent: 18pt;\n  margin-bottom: 0;\n}\nh1 {\n  font-size: 16pt;\n  font-weight: normal;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 18pt;\n  break-before: always;\n}\nh2 {\n  font-size: 13pt;\n  font-weight: bold;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\nh3 {\n  font-size: 11pt;\n  font-weight: bold;\n  text-align: center;\n  text-indent: 0;\n  margin-bottom: 12pt;\n}\na {\n  color: #1a1a1a;\n}\nblockquote {\n  font-style: italic;\n  margin-left: 18pt;\n  margin-right: 18pt;\n  text-indent: 0;\n}\npre {\n  font-family: \"Courier New\", Courier, monospace;\n  font-size: 9.5pt;\n  text-indent: 0;\n}\n@page {\n  size: 5.5in 8.5in;\n  margin: 0.75in;\n}\n";

const PROFESSIONAL_CSS: &str = "/* Professional — a clean, neutral theme for exports. */\n/* Only color, font size/weight/style, alignment, line height, */\n/* indents, page breaks, @page size and margins map onto PDF, Word */\n/* and RTF; everything else applies to HTML and ePub. */\nbody {\n  font-family: \"Helvetica Neue\", Helvetica, Arial, sans-serif;\n  color: #1f2933;\n  font-size: 11pt;\n  line-height: 1.65;\n  margin-bottom: 6pt;\n}\nh1 {\n  font-size: 24pt;\n  font-weight: bold;\n  color: #0f2a44;\n  margin-bottom: 12pt;\n}\nh2 {\n  font-size: 16pt;\n  font-weight: bold;\n  color: #1b3a5c;\n  margin-bottom: 8pt;\n}\nh3 {\n  font-size: 12.5pt;\n  font-weight: bold;\n  color: #1b3a5c;\n}\na {\n  color: #0066cc;\n}\nblockquote {\n  color: #52616f;\n  font-style: italic;\n  margin-left: 18pt;\n}\npre {\n  font-family: \"SF Mono\", Menlo, Consolas, monospace;\n  font-size: 9.5pt;\n}\n@page {\n  margin: 2.5cm 2cm;\n}\n";

// Original to this project: the Default/Compact editor styles from `style_picker.rs`
// (h1 28/22, h2 22/18, h3 18/16, body 15/13pt) expressed as export CSS.
const PARROT_DEFAULT_CSS: &str = "/* Parrot Default — this app's own default export look. */\nbody {\n  font-size: 15pt;\n  line-height: 1.4;\n  margin-bottom: 6pt;\n}\nh1 {\n  font-size: 28pt;\n  font-weight: bold;\n  margin-bottom: 12pt;\n}\nh2 {\n  font-size: 22pt;\n  font-weight: bold;\n  margin-bottom: 10pt;\n}\nh3 {\n  font-size: 18pt;\n  font-weight: bold;\n  margin-bottom: 8pt;\n}\nblockquote {\n  font-size: 15pt;\n  margin-left: 24pt;\n}\nul, ol {\n  margin-left: 24pt;\n}\npre {\n  font-family: Consolas, monospace;\n  font-size: 13pt;\n}\n";

const PARROT_COMPACT_CSS: &str = "/* Parrot Compact — this app's own tighter export look. */\nbody {\n  font-size: 13pt;\n  line-height: 1.3;\n  margin-bottom: 4pt;\n}\nh1 {\n  font-size: 22pt;\n  font-weight: bold;\n  margin-bottom: 10pt;\n}\nh2 {\n  font-size: 18pt;\n  font-weight: bold;\n  margin-bottom: 8pt;\n}\nh3 {\n  font-size: 16pt;\n  font-weight: bold;\n  margin-bottom: 6pt;\n}\nblockquote {\n  font-size: 13pt;\n  margin-left: 16pt;\n}\nul, ol {\n  margin-left: 16pt;\n}\npre {\n  font-family: Consolas, monospace;\n  font-size: 11pt;\n}\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_resolves_to_its_key_rules() {
        for (name, css) in BUILTIN_EXPORT_STYLES.iter().skip(1) {
            let theme = parrot_export::css::ExportTheme::from_css(css);
            assert!(!theme.raw_css.is_empty(), "{name} must carry CSS");
        }
        // Spot-check the values the backends actually read.
        let manuscript = parrot_export::css::ExportTheme::from_css(
            css_for("Manuscript").expect("Manuscript must exist"),
        );
        assert_eq!(manuscript.body.size_pt, 11.0);
        assert!(manuscript.h1.break_before);
        // Manuscript sets no page size (the dialog's picker governs) but keeps
        // its 1in margins; Paperback keeps its trim size.
        assert!(manuscript.page.as_ref().and_then(|p| p.size).is_none());
        assert_eq!(manuscript.page_dims_mm((1.0, 1.0)), (1.0, 1.0));
        assert_eq!(manuscript.page_margin_mm(0, 0.0), 25.4);
        let paperback = parrot_export::css::ExportTheme::from_css(
            css_for("Paperback Novel").expect("Paperback must exist"),
        );
        assert!(paperback.page.as_ref().and_then(|p| p.size).is_some());
        let compact =
            parrot_export::css::ExportTheme::from_css(css_for("Parrot Compact").expect("exists"));
        assert_eq!(compact.h1.size_pt, 22.0);
        assert_eq!(compact.body.size_pt, 13.0);
        assert_eq!(css_for("Built-in"), Some(""));
        assert_eq!(css_for("No Such Style"), None);
    }
}
