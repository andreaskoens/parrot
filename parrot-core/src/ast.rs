//! Sheet content AST, matching the confirmed `Content.xml` shape (feature inventory §3.3).
//!
//! T-1-01 extends the MVP (T-0-03) tag/inline subset to the full confirmed 19-tag table
//! (inventory §3.2). `identifier()`/`from_identifier()` on `TagKind` are the single source of
//! truth for the `kind="..."` strings used by the Content.xml serializer (T-0-04/T-1-03) and the
//! typing-time parser (T-0-08/T-1-02) — previously `parrot-container::xml` duplicated this
//! mapping locally; centralizing it here avoids the two drifting as more tags are added.

/// A block-level markup prefix recognized at the start of a paragraph (inventory §3.2).
///
/// `CodeBlock` carries an optional language tag — this project's own addition (T-3-03): the
/// confirmed `codeblock` pattern doesn't specify a language-hint syntax, so this is a deliberate
/// extension beyond the confirmed spec, not observed Parrot behavior (see DECISIONS.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagKind {
    Heading(u8),
    UnorderedList,
    OrderedList,
    BlockQuote,
    CodeBlock(Option<String>),
    Comment,
    Divider,
    NativeBlock,
}

impl TagKind {
    /// The confirmed `kind="..."` identifier string for this tag (inventory §3.2/§3.3). A
    /// `CodeBlock`'s language tag is a separate `<tag language="...">` XML attribute (see
    /// `parrot_container::xml`), not part of this identifier.
    pub fn identifier(&self) -> String {
        match self {
            TagKind::Heading(n) => format!("heading{n}"),
            TagKind::UnorderedList => "unorderedList".to_string(),
            TagKind::OrderedList => "orderedList".to_string(),
            TagKind::BlockQuote => "blockquote".to_string(),
            TagKind::CodeBlock(_) => "codeblock".to_string(),
            TagKind::Comment => "comment".to_string(),
            TagKind::Divider => "divider".to_string(),
            TagKind::NativeBlock => "nativeblock".to_string(),
        }
    }

    /// Parses a confirmed `kind="..."` identifier string back into a `TagKind`. `codeblock`
    /// comes back with no language — callers that also have a `language` attribute to hand
    /// (`parrot_container::xml::parse_tags`) fill it in afterward.
    pub fn from_identifier(id: &str) -> Option<TagKind> {
        match id {
            "heading1" => Some(TagKind::Heading(1)),
            "heading2" => Some(TagKind::Heading(2)),
            "heading3" => Some(TagKind::Heading(3)),
            "heading4" => Some(TagKind::Heading(4)),
            "heading5" => Some(TagKind::Heading(5)),
            "heading6" => Some(TagKind::Heading(6)),
            "unorderedList" => Some(TagKind::UnorderedList),
            "orderedList" => Some(TagKind::OrderedList),
            "blockquote" => Some(TagKind::BlockQuote),
            "codeblock" => Some(TagKind::CodeBlock(None)),
            "comment" => Some(TagKind::Comment),
            "divider" => Some(TagKind::Divider),
            "nativeblock" => Some(TagKind::NativeBlock),
            _ => None,
        }
    }
}

/// A paragraph's leading tag, keeping both its semantic kind and its literal markup text so the
/// plain-text renderer (T-0-05) can reconstruct exactly what was typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagMark {
    pub kind: TagKind,
    pub literal: String,
}

/// Inline content within a paragraph (full confirmed set, inventory §3.2).
///
/// // ASSUMPTION: the inventory confirms the `delete`/`inlineComment`/`inlineNative`/`mark`
/// // marker *patterns* exist, and that annotation/image/video carry typed attributes like link
/// // (inventory §3.3), but no real sample file showed their exact Content.xml shape (only
/// // link's was directly observed). Delete/InlineComment/Mark follow link's "wrapper with
/// // startTag/endTag" shape by analogy; Annotation/Image/Video follow its "typed attribute"
/// // shape by analogy. Revisit if real samples using these tags ever surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Emph(Vec<Inline>),
    Code(String),
    Link { url: String, content: Vec<Inline> },
    Footnote { content: Vec<Inline> },
    Delete(Vec<Inline>),
    InlineComment(Vec<Inline>),
    InlineNative(String),
    Mark(Vec<Inline>),
    Annotation { note: String, content: Vec<Inline> },
    Image { url: String, alt: Option<String> },
    Video { url: String },
}

impl Inline {
    /// The confirmed `kind="..."` identifier string for this inline (inventory §3.2/§3.3).
    /// `Text` has no `kind` — it's raw text content, not an `<element>`.
    pub fn kind_identifier(&self) -> Option<&'static str> {
        match self {
            Inline::Text(_) => None,
            Inline::Strong(_) => Some("strong"),
            Inline::Emph(_) => Some("emph"),
            Inline::Code(_) => Some("code"),
            Inline::Link { .. } => Some("link"),
            Inline::Footnote { .. } => Some("footnote"),
            Inline::Delete(_) => Some("delete"),
            Inline::InlineComment(_) => Some("inlineComment"),
            Inline::InlineNative(_) => Some("inlineNative"),
            Inline::Mark(_) => Some("mark"),
            Inline::Annotation { .. } => Some("annotation"),
            Inline::Image { .. } => Some("image"),
            Inline::Video { .. } => Some("video"),
        }
    }
}

/// A single paragraph: an optional leading block tag, followed by inline content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub tag: Option<TagMark>,
    pub inlines: Vec<Inline>,
}

impl Paragraph {
    /// This paragraph's flattened plain text — the per-paragraph unit of
    /// [`SheetContent::plain_text`] (sheet-list previews and the inspector's outline need one
    /// line per paragraph, not the whole sheet joined).
    pub fn plain_text(&self) -> String {
        self.inlines.iter().map(inline_plain_text).collect()
    }
}

/// The `<markup>` element: identifies which tag dialect the sheet uses. Every confirmed sample
/// uses `markdownxl`/"Markdown XL" (inventory §3.2) — no other dialect has been observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupInfo {
    pub version: u32,
    pub identifier: String,
    pub display_name: String,
}

impl Default for MarkupInfo {
    fn default() -> Self {
        Self {
            version: 1,
            identifier: "markdownxl".to_string(),
            display_name: "Markdown XL".to_string(),
        }
    }
}

/// The full parsed content of one `.parrot` sheet's `Content.xml` (inventory §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetContent {
    pub version: u32,
    pub markup: MarkupInfo,
    pub paragraphs: Vec<Paragraph>,
}

impl SheetContent {
    pub fn new(paragraphs: Vec<Paragraph>) -> Self {
        Self {
            version: 1,
            markup: MarkupInfo::default(),
            paragraphs,
        }
    }

    /// Flattens this sheet's content to plain text for keyword/search matching — deliberately
    /// minimal (concatenation of text-bearing spans, markup stripped), not a general-purpose
    /// renderer; that's `parrot_markup::render`, which this crate can't depend on (it depends
    /// on `parrot-core`, not the other way). Used by `crate::filter` and `crate::search`.
    pub fn plain_text(&self) -> String {
        self.paragraphs
            .iter()
            .flat_map(|p| p.inlines.iter())
            .map(inline_plain_text)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn inline_plain_text(inline: &Inline) -> String {
    match inline {
        Inline::Text(s) | Inline::InlineNative(s) | Inline::Code(s) => s.clone(),
        Inline::Strong(c)
        | Inline::Emph(c)
        | Inline::Delete(c)
        | Inline::InlineComment(c)
        | Inline::Mark(c) => c.iter().map(inline_plain_text).collect(),
        Inline::Link { content, .. }
        | Inline::Footnote { content }
        | Inline::Annotation { content, .. } => content.iter().map(inline_plain_text).collect(),
        Inline::Image { alt, .. } => alt.clone().unwrap_or_default(),
        Inline::Video { .. } => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the literal confirmed example from feature inventory §3.3:
    /// a heading3 paragraph, then a paragraph with emph and a link carrying a URL attribute.
    fn confirmed_example() -> SheetContent {
        SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(3),
                    literal: "### ".to_string(),
                }),
                inlines: vec![Inline::Text("Heading text".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Text("Body with ".to_string()),
                    Inline::Emph(vec![Inline::Text("italic".to_string())]),
                    Inline::Text(" and ".to_string()),
                    Inline::Link {
                        url: "example.com".to_string(),
                        content: vec![Inline::Text("label".to_string())],
                    },
                    Inline::Text(".".to_string()),
                ],
            },
        ])
    }

    #[test]
    fn builds_confirmed_example_shape() {
        let a = confirmed_example();
        let b = confirmed_example();
        assert_eq!(
            a, b,
            "builder must deterministically reproduce the confirmed §3.3 shape"
        );

        assert_eq!(a.markup.identifier, "markdownxl");
        assert_eq!(a.paragraphs.len(), 2);
        assert_eq!(
            a.paragraphs[0].tag,
            Some(TagMark {
                kind: TagKind::Heading(3),
                literal: "### ".to_string(),
            })
        );
    }

    /// The literal 19 confirmed tags from feature inventory §3.2 (heading1-6 counted once, as
    /// the single parameterized `TagKind::Heading(u8)` variant) — this hardcoded list IS the
    /// spec-coverage check for T-1-01: a tag confirmed in the inventory with no AST variant
    /// fails this test, not silently.
    #[test]
    fn enum_covers_confirmed_tag_table() {
        let confirmed = [
            "codeblock",
            "comment",
            "divider",
            "nativeblock",
            "blockquote",
            "orderedList",
            "unorderedList",
            "code",
            "delete",
            "emph",
            "strong",
            "inlineComment",
            "inlineNative",
            "mark",
            "annotation",
            "link",
            "footnote",
            "image",
            "video",
        ];
        assert_eq!(
            confirmed.len(),
            19,
            "sanity check on the hardcoded list itself"
        );

        let tag_kind_identifiers: Vec<String> = [
            TagKind::CodeBlock(None),
            TagKind::Comment,
            TagKind::Divider,
            TagKind::NativeBlock,
            TagKind::BlockQuote,
            TagKind::OrderedList,
            TagKind::UnorderedList,
        ]
        .iter()
        .map(TagKind::identifier)
        .collect();

        let inline_kind_identifiers: Vec<&'static str> = [
            Inline::Code(String::new()),
            Inline::Delete(vec![]),
            Inline::Emph(vec![]),
            Inline::Strong(vec![]),
            Inline::InlineComment(vec![]),
            Inline::InlineNative(String::new()),
            Inline::Mark(vec![]),
            Inline::Annotation {
                note: String::new(),
                content: vec![],
            },
            Inline::Link {
                url: String::new(),
                content: vec![],
            },
            Inline::Footnote { content: vec![] },
            Inline::Image {
                url: String::new(),
                alt: None,
            },
            Inline::Video { url: String::new() },
        ]
        .iter()
        .filter_map(Inline::kind_identifier)
        .collect();

        for id in confirmed {
            let covered = tag_kind_identifiers.iter().any(|s| s == id)
                || inline_kind_identifiers.contains(&id);
            assert!(covered, "missing AST coverage for confirmed tag: {id}");
        }
    }
}
