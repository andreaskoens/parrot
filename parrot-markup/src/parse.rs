//! Text -> AST, typing-time parser: the full confirmed 19-tag table (inventory §3.2).
//! T-0-08 covered heading1-6/strong/emph/unorderedList/orderedList/blockquote; T-1-02 adds the
//! rest. codeblock/comment/divider/nativeblock are block-level (line-prefix) like blockquote;
//! delete/inlineComment/inlineNative/mark generalize the existing doubled/single-marker inline
//! scanner; annotation/image/video are this project's own typed-markup conventions — the
//! inventory confirms their marker *tokens* exist (`{...}`, `(img)`, `(vid)`) but not an exact
//! typing syntax, so these mirror `parrot_markup::render`'s own rendering choices for them
//! (`{content}`, `![alt](url)`, `(vid: url)`) so typing and rendering round-trip consistently
//! within this project, even though neither is confirmed Parrot behavior (see DECISIONS.md).

use parrot_core::{Inline, Paragraph, TagKind, TagMark};

/// A marker character paired with the `Inline` variant constructor it wraps content in.
type WrapMarker = (char, fn(Vec<Inline>) -> Inline);

/// Doubled-marker inline wrappers sharing the `Variant(Vec<Inline>)` shape: `**`/`__` for
/// Strong, `||` for Delete, `++` for InlineComment, `::` for Mark.
const DOUBLE_WRAP_MARKERS: &[WrapMarker] = &[
    ('*', Inline::Strong),
    ('_', Inline::Strong),
    ('|', Inline::Delete),
    ('+', Inline::InlineComment),
    (':', Inline::Mark),
];

/// Single-marker inline wrappers sharing the `Variant(Vec<Inline>)` shape: `*`/`_` for Emph.
/// (`~` for InlineNative is handled separately below — it wraps a raw `String`, not nested
/// `Inline`s.)
const SINGLE_WRAP_MARKERS: &[WrapMarker] = &[('*', Inline::Emph), ('_', Inline::Emph)];

/// Parses one line of raw Markdown-XL source into a `Paragraph`: an optional leading block tag,
/// followed by inline content (strong/emph spans and plain text).
pub fn parse_paragraph(line: &str) -> Paragraph {
    let (tag, rest) = parse_block_tag(line);
    // codeblock/nativeblock/comment content is raw, not re-parsed as inline markup — a `{` in a
    // code snippet must not be mistaken for an annotation, the way Code/InlineNative already
    // hold raw strings rather than parsed Inline trees.
    let inlines = match &tag {
        Some(TagMark {
            kind: TagKind::CodeBlock(_) | TagKind::NativeBlock | TagKind::Comment,
            ..
        }) => {
            if rest.is_empty() {
                Vec::new()
            } else {
                vec![Inline::Text(rest.to_string())]
            }
        }
        _ => parse_inlines(rest),
    };
    Paragraph { tag, inlines }
}

fn parse_block_tag(line: &str) -> (Option<TagMark>, &str) {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
        let literal_len = hashes + 1;
        return (
            Some(TagMark {
                kind: TagKind::Heading(hashes as u8),
                literal: line[..literal_len].to_string(),
            }),
            &line[literal_len..],
        );
    }

    if let Some(rest) = line.strip_prefix("> ") {
        return (
            Some(TagMark {
                kind: TagKind::BlockQuote,
                literal: "> ".to_string(),
            }),
            rest,
        );
    }

    // Lists: an optional indent (two spaces per nesting level) before the marker stays part of
    // the tag literal, so the source round-trips exactly.
    let indent = line.chars().take_while(|&c| c == ' ').count();
    let body = &line[indent..];
    for marker in ["* ", "+ ", "- "] {
        if let Some(rest) = body.strip_prefix(marker) {
            return (
                Some(TagMark {
                    kind: TagKind::UnorderedList,
                    literal: line[..indent + marker.len()].to_string(),
                }),
                rest,
            );
        }
    }

    let digits = body.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && body.as_bytes().get(digits) == Some(&b'.')
        && body.as_bytes().get(digits + 1) == Some(&b' ')
    {
        let literal_len = indent + digits + 2;
        return (
            Some(TagMark {
                kind: TagKind::OrderedList,
                literal: line[..literal_len].to_string(),
            }),
            &line[literal_len..],
        );
    }

    // divider: the whole line is "----" (a standalone horizontal rule, no trailing content).
    if line.trim_end() == "----" {
        return (
            Some(TagMark {
                kind: TagKind::Divider,
                literal: line.to_string(),
            }),
            "",
        );
    }

    // codeblock (backtick fence): T-3-03's own convention — ```lang <code>, language tag
    // optional. The confirmed pattern (inventory §3.2) doesn't specify a language-hint syntax;
    // this is a deliberate addition, not observed Parrot behavior (see DECISIONS.md).
    if let Some(after_fence) = line.strip_prefix("```") {
        let lang_len = after_fence
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .count();
        let (lang_token, after_lang) = after_fence.split_at(lang_len);
        if let Some(rest) = after_lang.strip_prefix(' ') {
            let language = (!lang_token.is_empty()).then(|| lang_token.to_string());
            return (
                Some(TagMark {
                    kind: TagKind::CodeBlock(language),
                    literal: format!("```{lang_token} "),
                }),
                rest,
            );
        }
    }

    for (marker, kind) in [
        ("%% ", TagKind::Comment),
        ("~~ ", TagKind::NativeBlock),
        ("'' ", TagKind::CodeBlock(None)),
    ] {
        if let Some(rest) = line.strip_prefix(marker) {
            return (
                Some(TagMark {
                    kind,
                    literal: marker.to_string(),
                }),
                rest,
            );
        }
    }

    (None, line)
}

/// Parses inline content: doubled/single wrapper markers (strong/delete/inlineComment/mark,
/// emph), `~raw~` (inlineNative), `` `raw` `` (code), `[label](url)` (link), `(fn: …)`
/// (footnote), `{content: note}` (annotation), `![alt](url)` (image), `(vid: url)` (video) —
/// recursing inside wrapper spans so nesting composes; everything else is plain text. Every
/// form here is one `crate::render` writes, so render → parse round-trips.
fn parse_inlines(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut inlines = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '!' && matches_at(&chars, i, "![") {
            if let Some((end, inline)) = try_parse_image(&chars, i) {
                flush_text(&mut inlines, &mut buf);
                inlines.push(inline);
                i = end;
                continue;
            }
        }

        if c == '`' {
            if let Some(close) = find_closing(&chars, i + 1, '`', false) {
                flush_text(&mut inlines, &mut buf);
                inlines.push(Inline::Code(chars[i + 1..close].iter().collect()));
                i = close + 1;
                continue;
            }
        }

        if c == '[' {
            if let Some((end, inline)) = try_parse_link(&chars, i) {
                flush_text(&mut inlines, &mut buf);
                inlines.push(inline);
                i = end;
                continue;
            }
        }

        if c == '(' && matches_at(&chars, i, "(fn: ") {
            let content_start = i + "(fn: ".chars().count();
            if let Some(close) = find_closing(&chars, content_start, ')', false) {
                flush_text(&mut inlines, &mut buf);
                let inner: String = chars[content_start..close].iter().collect();
                inlines.push(Inline::Footnote {
                    content: parse_inlines(&inner),
                });
                i = close + 1;
                continue;
            }
        }

        if c == '(' && matches_at(&chars, i, "(vid: ") {
            let content_start = i + "(vid: ".chars().count();
            if let Some(close) = find_closing(&chars, content_start, ')', false) {
                flush_text(&mut inlines, &mut buf);
                let url: String = chars[content_start..close].iter().collect();
                inlines.push(Inline::Video { url });
                i = close + 1;
                continue;
            }
        }

        if c == '{' {
            if let Some(close) = find_closing(&chars, i + 1, '}', false) {
                flush_text(&mut inlines, &mut buf);
                let inner: String = chars[i + 1..close].iter().collect();
                // `render` writes `{content: note}`; the last ": " splits the two (a ": " inside
                // the content itself is ambiguous in this syntax — last-wins is the convention).
                let (content, note) = match inner.rfind(": ") {
                    Some(at) => (&inner[..at], &inner[at + 2..]),
                    None => (inner.as_str(), ""),
                };
                inlines.push(Inline::Annotation {
                    note: note.to_string(),
                    content: parse_inlines(content),
                });
                i = close + 1;
                continue;
            }
        }

        if chars.get(i + 1) == Some(&c) {
            if let Some(&(_, ctor)) = DOUBLE_WRAP_MARKERS.iter().find(|(m, _)| *m == c) {
                if let Some(end) = find_closing(&chars, i + 2, c, true) {
                    flush_text(&mut inlines, &mut buf);
                    let inner: String = chars[i + 2..end].iter().collect();
                    inlines.push(ctor(parse_inlines(&inner)));
                    i = end + 2;
                    continue;
                }
            }
        }

        if c == '~' {
            if let Some(end) = find_closing(&chars, i + 1, '~', false) {
                flush_text(&mut inlines, &mut buf);
                let inner: String = chars[i + 1..end].iter().collect();
                inlines.push(Inline::InlineNative(inner));
                i = end + 1;
                continue;
            }
        } else if let Some(&(_, ctor)) = SINGLE_WRAP_MARKERS.iter().find(|(m, _)| *m == c) {
            if let Some(end) = find_closing(&chars, i + 1, c, false) {
                flush_text(&mut inlines, &mut buf);
                let inner: String = chars[i + 1..end].iter().collect();
                inlines.push(ctor(parse_inlines(&inner)));
                i = end + 1;
                continue;
            }
        }

        buf.push(c);
        i += 1;
    }
    flush_text(&mut inlines, &mut buf);
    inlines
}

/// `true` if `chars[i..]` starts with the literal string `s`.
fn matches_at(chars: &[char], i: usize, s: &str) -> bool {
    s.chars()
        .enumerate()
        .all(|(k, c)| chars.get(i + k) == Some(&c))
}

/// Tries to parse `![alt](url)` starting at `i`. Returns the index just past the closing `)`
/// and the resulting `Inline::Image` on success.
fn try_parse_image(chars: &[char], i: usize) -> Option<(usize, Inline)> {
    let alt_start = i + 2; // past "!["
    let close_bracket = find_closing(chars, alt_start, ']', false)?;
    if !matches_at(chars, close_bracket + 1, "(") {
        return None;
    }
    let url_start = close_bracket + 2;
    let close_paren = find_closing(chars, url_start, ')', false)?;

    let alt: String = chars[alt_start..close_bracket].iter().collect();
    let url: String = chars[url_start..close_paren].iter().collect();
    Some((
        close_paren + 1,
        Inline::Image {
            url,
            alt: if alt.is_empty() { None } else { Some(alt) },
        },
    ))
}

/// Tries to parse `[label](url)` starting at `i` (the `[`).
fn try_parse_link(chars: &[char], i: usize) -> Option<(usize, Inline)> {
    let close_bracket = find_closing(chars, i + 1, ']', false)?;
    if !matches_at(chars, close_bracket + 1, "(") {
        return None;
    }
    let url_start = close_bracket + 2;
    let close_paren = find_closing(chars, url_start, ')', false)?;
    let label: String = chars[i + 1..close_bracket].iter().collect();
    Some((
        close_paren + 1,
        Inline::Link {
            url: chars[url_start..close_paren].iter().collect(),
            content: parse_inlines(&label),
        },
    ))
}

fn flush_text(inlines: &mut Vec<Inline>, buf: &mut String) {
    if !buf.is_empty() {
        inlines.push(Inline::Text(std::mem::take(buf)));
    }
}

/// Finds the index of the closing marker (a doubled pair if `double`, else a single char).
fn find_closing(chars: &[char], start: usize, marker: char, double: bool) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == marker && (!double || chars.get(i + 1) == Some(&marker)) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_string())
    }

    #[test]
    fn link_code_footnote_annotation() {
        let p =
            parse_paragraph("see [the **docs**](https://x.y/z) and `a*b` (fn: note) {word: why}");
        assert_eq!(
            p.inlines,
            vec![
                text("see "),
                Inline::Link {
                    url: "https://x.y/z".to_string(),
                    content: vec![text("the "), Inline::Strong(vec![text("docs")])],
                },
                text(" and "),
                Inline::Code("a*b".to_string()),
                text(" "),
                Inline::Footnote {
                    content: vec![text("note")]
                },
                text(" "),
                Inline::Annotation {
                    note: "why".to_string(),
                    content: vec![text("word")]
                },
            ]
        );
    }

    #[test]
    fn render_then_parse_round_trips_every_inline() {
        let inlines = vec![
            text("a "),
            Inline::Strong(vec![text("b")]),
            text(" "),
            Inline::Emph(vec![text("c")]),
            text(" "),
            Inline::Code("d".to_string()),
            text(" "),
            Inline::Link {
                url: "u".to_string(),
                content: vec![text("e")],
            },
            text(" "),
            Inline::Footnote {
                content: vec![text("f")],
            },
            text(" "),
            Inline::Delete(vec![text("g")]),
            text(" "),
            Inline::InlineComment(vec![text("h")]),
            text(" "),
            Inline::InlineNative("i".to_string()),
            text(" "),
            Inline::Mark(vec![text("j")]),
            text(" "),
            Inline::Annotation {
                note: "n".to_string(),
                content: vec![text("k")],
            },
            text(" "),
            Inline::Image {
                url: "p.png".to_string(),
                alt: Some("l".to_string()),
            },
            text(" "),
            Inline::Video {
                url: "v.mp4".to_string(),
            },
        ];
        let p = Paragraph { tag: None, inlines };
        let rendered = crate::render::render(&parrot_core::SheetContent::new(vec![p.clone()]));
        assert_eq!(parse_paragraph(rendered.trim_end_matches('\n')), p);
    }

    #[test]
    fn heading() {
        let p = parse_paragraph("### Heading text");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::Heading(3),
                literal: "### ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("Heading text".to_string())]);
    }

    #[test]
    fn strong() {
        let p = parse_paragraph("**bold**");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Strong(vec![Inline::Text("bold".to_string())])]
        );
    }

    #[test]
    fn emph() {
        let p = parse_paragraph("*italic*");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Emph(vec![Inline::Text("italic".to_string())])]
        );
    }

    #[test]
    fn unordered_list() {
        let p = parse_paragraph("* an item");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::UnorderedList,
                literal: "* ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("an item".to_string())]);
    }

    #[test]
    fn ordered_list() {
        let p = parse_paragraph("1. an item");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::OrderedList,
                literal: "1. ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("an item".to_string())]);
    }

    #[test]
    fn blockquote() {
        let p = parse_paragraph("> quoted text");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::BlockQuote,
                literal: "> ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("quoted text".to_string())]);
    }

    #[test]
    fn codeblock() {
        let p = parse_paragraph("``` fn main() {}");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::CodeBlock(None),
                literal: "``` ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("fn main() {}".to_string())]);
    }

    #[test]
    fn codeblock_with_language_tag() {
        let p = parse_paragraph("```rust fn main() {}");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::CodeBlock(Some("rust".to_string())),
                literal: "```rust ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("fn main() {}".to_string())]);
    }

    #[test]
    fn codeblock_without_language_tag() {
        let p = parse_paragraph("``` fn main() {}");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::CodeBlock(None),
                literal: "``` ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("fn main() {}".to_string())]);
    }

    #[test]
    fn comment() {
        let p = parse_paragraph("%% a comment line");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::Comment,
                literal: "%% ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("a comment line".to_string())]);
    }

    #[test]
    fn divider() {
        let p = parse_paragraph("----");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::Divider,
                literal: "----".to_string()
            })
        );
        assert_eq!(p.inlines, Vec::new());
    }

    #[test]
    fn nativeblock() {
        let p = parse_paragraph("~~ <raw html>");
        assert_eq!(
            p.tag,
            Some(TagMark {
                kind: TagKind::NativeBlock,
                literal: "~~ ".to_string()
            })
        );
        assert_eq!(p.inlines, vec![Inline::Text("<raw html>".to_string())]);
    }

    #[test]
    fn delete() {
        let p = parse_paragraph("||struck||");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Delete(vec![Inline::Text("struck".to_string())])]
        );
    }

    #[test]
    fn inline_comment() {
        let p = parse_paragraph("++a note++");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::InlineComment(vec![Inline::Text(
                "a note".to_string()
            )])]
        );
    }

    #[test]
    fn inline_native() {
        let p = parse_paragraph("~raw~");
        assert_eq!(p.tag, None);
        assert_eq!(p.inlines, vec![Inline::InlineNative("raw".to_string())]);
    }

    #[test]
    fn mark() {
        let p = parse_paragraph("::highlighted::");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Mark(vec![Inline::Text("highlighted".to_string())])]
        );
    }

    #[test]
    fn annotation() {
        let p = parse_paragraph("{annotated text}");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Annotation {
                note: String::new(),
                content: vec![Inline::Text("annotated text".to_string())],
            }]
        );
    }

    #[test]
    fn image() {
        let p = parse_paragraph("![alt text](image.png)");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Image {
                url: "image.png".to_string(),
                alt: Some("alt text".to_string()),
            }]
        );
    }

    #[test]
    fn video() {
        let p = parse_paragraph("(vid: video.mp4)");
        assert_eq!(p.tag, None);
        assert_eq!(
            p.inlines,
            vec![Inline::Video {
                url: "video.mp4".to_string()
            }]
        );
    }

    #[test]
    fn indented_list_markers_are_nested_items_that_keep_their_indent() {
        for (line, kind, literal) in [
            ("  - item", TagKind::UnorderedList, "  - "),
            ("    * deep", TagKind::UnorderedList, "    * "),
            ("  2. item", TagKind::OrderedList, "  2. "),
        ] {
            let tag = parse_paragraph(line).tag.expect("a list item");
            assert_eq!((tag.kind, tag.literal.as_str()), (kind, literal));
        }
        assert_eq!(parse_paragraph("  plain").tag, None);
    }
}
