//! Panel 4 Inspector (T-12-08, ui-description §6): tab model and each tab's data, derived
//! straight from the sheet model so the panel never holds a copy that could drift. The target
//! is the sheet open in Panel 3, or every sheet of the sheet-flow view (§6.1/§6.3: counts and
//! outline then span the whole container).

use crate::icons;
use crate::state::NodePath;
use parrot_core::{Attachment, Inline, Sheet, TagKind};

/// §6 tab bar, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    /// Progress figures, the goal (set up in place) and the session chart, in one tab.
    Statistics,
    Keywords,
    Outline,
    Links,
    Media,
    /// The Character Counter (`char_counter`), over the editor's selection or open sheets.
    Counter,
    /// Spelling and grammar findings (`annotations`) for the editor's sheets.
    Proofing,
}

impl InspectorTab {
    pub const ALL: [InspectorTab; 7] = [
        InspectorTab::Statistics,
        InspectorTab::Keywords,
        InspectorTab::Outline,
        InspectorTab::Links,
        InspectorTab::Media,
        InspectorTab::Counter,
        InspectorTab::Proofing,
    ];

    /// Tab-bar glyph (§6: target, chart, tag, list, link, image).
    pub fn icon(self) -> &'static str {
        match self {
            InspectorTab::Statistics => icons::STATISTICS,
            InspectorTab::Keywords => icons::KEYWORDS,
            InspectorTab::Outline => icons::OUTLINE,
            InspectorTab::Links => icons::LINK,
            InspectorTab::Media => icons::IMAGE,
            InspectorTab::Counter => icons::CHARACTER_COUNTER,
            InspectorTab::Proofing => icons::PROOFING,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            InspectorTab::Statistics => "Statistics & Goals",
            InspectorTab::Keywords => "Keywords",
            InspectorTab::Outline => "Outline",
            InspectorTab::Links => "Links",
            InspectorTab::Media => "Media",
            InspectorTab::Counter => "Character Counter",
            InspectorTab::Proofing => "Spelling & Grammar",
        }
    }
}

/// §6.1 Progress figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub characters: u32,
    pub words: u32,
    pub reading_seconds: u32,
}

pub fn progress(sheets: &[&Sheet]) -> Progress {
    let characters = sheets
        .iter()
        .map(|s| parrot_core::statistics::char_count(&s.content))
        .fold(0u32, u32::saturating_add);
    let words = sheets
        .iter()
        .map(|s| parrot_core::statistics::word_count(&s.content))
        .fold(0u32, u32::saturating_add);
    Progress {
        characters,
        words,
        reading_seconds: parrot_core::statistics::reading_time_seconds(words),
    }
}

/// §6.1 "1 min 29 sec" reading-time format.
pub fn format_reading_time(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (0, s) => format!("{s} sec"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} sec"),
    }
}

/// Thousands-separated count, as §6.1 shows ("2,198").
pub fn format_count(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// §6.2: the sheet's assigned keyword set, as stored (T-10-05 keeps it deduplicated).
pub fn keywords(sheet: &Sheet) -> Vec<String> {
    sheet.keywords.clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub sheet: NodePath,
    /// Index into that sheet's `paragraphs` (what click-to-jump scrolls to).
    pub paragraph: usize,
    pub level: u8,
    pub text: String,
}

/// §6.3: every heading paragraph, in order, across `sheets`.
pub fn outline(sheets: &[(NodePath, &Sheet)]) -> Vec<OutlineEntry> {
    sheets
        .iter()
        .flat_map(|(path, sheet)| {
            sheet
                .content
                .paragraphs
                .iter()
                .enumerate()
                .filter_map(move |(i, p)| match p.tag.as_ref().map(|t| &t.kind) {
                    Some(TagKind::Heading(level)) => Some(OutlineEntry {
                        sheet: path.clone(),
                        paragraph: i,
                        level: *level,
                        text: p.plain_text(),
                    }),
                    _ => None,
                })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEntry {
    pub label: String,
    pub url: String,
}

/// §6.4: every `Link` inline (at any nesting depth), in source order.
pub fn links(sheets: &[&Sheet]) -> Vec<LinkEntry> {
    fn walk(inlines: &[Inline], out: &mut Vec<LinkEntry>) {
        for inline in inlines {
            match inline {
                Inline::Link { url, content } => {
                    let label: String = content.iter().map(inline_text).collect();
                    out.push(LinkEntry {
                        label: if label.is_empty() { url.clone() } else { label },
                        url: url.clone(),
                    });
                    walk(content, out);
                }
                Inline::Strong(c)
                | Inline::Emph(c)
                | Inline::Delete(c)
                | Inline::InlineComment(c)
                | Inline::Mark(c)
                | Inline::Footnote { content: c }
                | Inline::Annotation { content: c, .. } => walk(c, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for sheet in sheets {
        for p in &sheet.content.paragraphs {
            walk(&p.inlines, &mut out);
        }
    }
    out
}

fn inline_text(inline: &Inline) -> String {
    parrot_core::Paragraph {
        tag: None,
        inlines: vec![inline.clone()],
    }
    .plain_text()
}

/// §6.5 media entry: inline images/videos and sheet attachments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaEntry {
    Image {
        url: String,
        alt: Option<String>,
    },
    Video {
        url: String,
    },
    /// Index into the sheet's `attachments`, for removal.
    Attachment {
        index: usize,
        attachment: Attachment,
    },
}

impl MediaEntry {
    pub fn icon(&self) -> &'static str {
        match self {
            MediaEntry::Image { .. } => icons::IMAGE,
            MediaEntry::Video { .. } => icons::VIDEO,
            MediaEntry::Attachment {
                attachment: Attachment::Note { .. },
                ..
            } => icons::NOTE,
            MediaEntry::Attachment {
                attachment: Attachment::File { .. },
                ..
            } => icons::FILE,
        }
    }

    pub fn label(&self) -> String {
        match self {
            MediaEntry::Image { url, alt } => alt
                .clone()
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| file_name(url)),
            MediaEntry::Video { url } => file_name(url),
            MediaEntry::Attachment { attachment, .. } => match attachment {
                Attachment::Note { text } => text.clone(),
                Attachment::File { file_name } => file_name.clone(),
            },
        }
    }
}

fn file_name(url: &str) -> String {
    url.rsplit(['/', '\\']).next().unwrap_or(url).to_string()
}

/// Every image/video inline in `sheet`, then its attachments.
pub fn media(sheet: &Sheet) -> Vec<MediaEntry> {
    fn walk(inlines: &[Inline], out: &mut Vec<MediaEntry>) {
        for inline in inlines {
            match inline {
                Inline::Image { url, alt } => out.push(MediaEntry::Image {
                    url: url.clone(),
                    alt: alt.clone(),
                }),
                Inline::Video { url } => out.push(MediaEntry::Video { url: url.clone() }),
                Inline::Strong(c)
                | Inline::Emph(c)
                | Inline::Delete(c)
                | Inline::InlineComment(c)
                | Inline::Mark(c)
                | Inline::Link { content: c, .. }
                | Inline::Footnote { content: c }
                | Inline::Annotation { content: c, .. } => walk(c, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for p in &sheet.content.paragraphs {
        walk(&p.inlines, &mut out);
    }
    out.extend(
        sheet
            .attachments
            .iter()
            .enumerate()
            .map(|(index, a)| MediaEntry::Attachment {
                index,
                attachment: a.clone(),
            }),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Paragraph, SheetContent, TagMark};

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_string())
    }

    fn fixture() -> Sheet {
        Sheet {
            title: "Notes".to_string(),
            content: SheetContent::new(vec![
                Paragraph {
                    tag: Some(TagMark {
                        kind: TagKind::Heading(1),
                        literal: "# ".to_string(),
                    }),
                    inlines: vec![text("Intro")],
                },
                Paragraph {
                    tag: None,
                    inlines: vec![
                        text("Read "),
                        Inline::Link {
                            url: "https://example.org/a".to_string(),
                            content: vec![text("SPOILERS!")],
                        },
                        text(" and "),
                        Inline::Strong(vec![Inline::Link {
                            url: "file:///b".to_string(),
                            content: vec![],
                        }]),
                        Inline::Image {
                            url: "media/cover.png".to_string(),
                            alt: Some("Capa".to_string()),
                        },
                    ],
                },
                Paragraph {
                    tag: Some(TagMark {
                        kind: TagKind::Heading(2),
                        literal: "## ".to_string(),
                    }),
                    inlines: vec![text("Details")],
                },
            ]),
            goal: None,
            keywords: vec!["draft".to_string(), "essay".to_string()],
            attachments: vec![Attachment::File {
                file_name: "source.pdf".to_string(),
            }],
            glued: false,
            draft: false,
        }
    }

    #[test]
    fn progress_tab_matches_core_counts() {
        let sheet = fixture();
        let p = progress(&[&sheet]);
        assert_eq!(p.words, parrot_core::statistics::word_count(&sheet.content));
        assert_eq!(
            p.characters,
            parrot_core::statistics::char_count(&sheet.content)
        );
        assert_eq!(
            p.reading_seconds,
            parrot_core::statistics::reading_time_seconds(p.words)
        );
        let doubled = progress(&[&sheet, &sheet]);
        assert_eq!(doubled.words, 2 * p.words);
        assert_eq!(format_reading_time(89), "1 min 29 sec");
        assert_eq!(format_reading_time(30), "30 sec");
        assert_eq!(format_count(2198), "2,198");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }

    #[test]
    fn keywords_tab_matches_assigned_set() {
        let mut sheet = fixture();
        assert_eq!(keywords(&sheet), sheet.keywords);
        parrot_core::keyword::assign(&mut sheet.keywords, "final");
        assert_eq!(keywords(&sheet), sheet.keywords);
        assert!(keywords(&sheet).contains(&"final".to_string()));
    }

    #[test]
    fn outline_tab_matches_heading_extraction() {
        let sheet = fixture();
        let entries = outline(&[(vec![3], &sheet)]);
        let expected: Vec<(usize, u8)> = sheet
            .content
            .paragraphs
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.tag.as_ref().map(|t| &t.kind) {
                Some(TagKind::Heading(l)) => Some((i, *l)),
                _ => None,
            })
            .collect();
        let got: Vec<(usize, u8)> = entries.iter().map(|e| (e.paragraph, e.level)).collect();
        assert_eq!(got, expected);
        assert_eq!(entries[0].text, "Intro");
        assert_eq!(entries[1].sheet, vec![3]);
    }

    #[test]
    fn links_tab_matches_link_inlines() {
        let sheet = fixture();
        assert_eq!(
            links(&[&sheet]),
            vec![
                LinkEntry {
                    label: "SPOILERS!".to_string(),
                    url: "https://example.org/a".to_string()
                },
                LinkEntry {
                    label: "file:///b".to_string(),
                    url: "file:///b".to_string()
                },
            ]
        );
    }

    #[test]
    fn media_tab_matches_attachments() {
        let sheet = fixture();
        let entries = media(&sheet);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label(), "Capa");
        assert_eq!(
            entries[1],
            MediaEntry::Attachment {
                index: 0,
                attachment: sheet.attachments[0].clone()
            }
        );
        assert_eq!(entries[1].label(), "source.pdf");
    }
}
