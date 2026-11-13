//! `quick-xml`-based read/write for `Content.xml`, matching the confirmed schema (feature
//! inventory §3.3): `<sheet><markup/><string><p>...</p>...</string></sheet>`.

use parrot_core::{Inline, MarkupInfo, Paragraph, SheetContent, TagKind, TagMark};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XmlError {
    #[error("xml error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("malformed Content.xml: {0}")]
    Malformed(String),
}

/// Serializes a `SheetContent` AST to the confirmed `Content.xml` text form.
pub fn serialize(content: &SheetContent) -> Result<String, XmlError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    let mut sheet = BytesStart::new("sheet");
    sheet.push_attribute(("version", content.version.to_string().as_str()));
    writer.write_event(Event::Start(sheet))?;

    let mut markup = BytesStart::new("markup");
    markup.push_attribute(("version", content.markup.version.to_string().as_str()));
    markup.push_attribute(("identifier", content.markup.identifier.as_str()));
    markup.push_attribute(("displayName", content.markup.display_name.as_str()));
    writer.write_event(Event::Empty(markup))?;

    let mut string_el = BytesStart::new("string");
    string_el.push_attribute(("xml:space", "preserve"));
    writer.write_event(Event::Start(string_el))?;

    for p in &content.paragraphs {
        write_paragraph(&mut writer, p)?;
    }

    writer.write_event(Event::End(BytesEnd::new("string")))?;
    writer.write_event(Event::End(BytesEnd::new("sheet")))?;

    let bytes = writer.into_inner().into_inner();
    String::from_utf8(bytes).map_err(|e| XmlError::Malformed(e.to_string()))
}

fn write_paragraph(writer: &mut Writer<Cursor<Vec<u8>>>, p: &Paragraph) -> Result<(), XmlError> {
    writer.write_event(Event::Start(BytesStart::new("p")))?;
    if let Some(tag) = &p.tag {
        writer.write_event(Event::Start(BytesStart::new("tags")))?;
        let mut tag_el = BytesStart::new("tag");
        tag_el.push_attribute(("kind", tag.kind.identifier().as_str()));
        if let TagKind::CodeBlock(Some(lang)) = &tag.kind {
            tag_el.push_attribute(("language", lang.as_str()));
        }
        writer.write_event(Event::Start(tag_el))?;
        writer.write_event(Event::Text(BytesText::new(&tag.literal)))?;
        writer.write_event(Event::End(BytesEnd::new("tag")))?;
        writer.write_event(Event::End(BytesEnd::new("tags")))?;
    }
    for inline in &p.inlines {
        write_inline(writer, inline)?;
    }
    writer.write_event(Event::End(BytesEnd::new("p")))?;
    Ok(())
}

fn write_inline(writer: &mut Writer<Cursor<Vec<u8>>>, inline: &Inline) -> Result<(), XmlError> {
    match inline {
        Inline::Text(s) => {
            writer.write_event(Event::Text(BytesText::new(s)))?;
        }
        Inline::Strong(children) => {
            write_wrapped_element(writer, "strong", Some("**"), Some("**"), children)?;
        }
        Inline::Emph(children) => {
            write_wrapped_element(writer, "emph", Some("*"), Some("*"), children)?;
        }
        Inline::Code(s) => {
            let mut el = BytesStart::new("element");
            el.push_attribute(("kind", "code"));
            el.push_attribute(("startTag", "`"));
            el.push_attribute(("endTag", "`"));
            writer.write_event(Event::Start(el))?;
            writer.write_event(Event::Text(BytesText::new(s)))?;
            writer.write_event(Event::End(BytesEnd::new("element")))?;
        }
        Inline::Link { url, content } => {
            write_element_with_attrs(writer, "link", &[("URL", url)], content)?;
        }
        Inline::Footnote { content } => {
            write_wrapped_element(writer, "footnote", None, None, content)?;
        }
        Inline::Delete(children) => {
            write_wrapped_element(writer, "delete", Some("||"), Some("||"), children)?;
        }
        Inline::InlineComment(children) => {
            write_wrapped_element(writer, "inlineComment", Some("++"), Some("++"), children)?;
        }
        Inline::InlineNative(s) => {
            let mut el = BytesStart::new("element");
            el.push_attribute(("kind", "inlineNative"));
            el.push_attribute(("startTag", "~"));
            el.push_attribute(("endTag", "~"));
            writer.write_event(Event::Start(el))?;
            writer.write_event(Event::Text(BytesText::new(s)))?;
            writer.write_event(Event::End(BytesEnd::new("element")))?;
        }
        Inline::Mark(children) => {
            write_wrapped_element(writer, "mark", Some("::"), Some("::"), children)?;
        }
        Inline::Annotation { note, content } => {
            write_element_with_attrs(writer, "annotation", &[("text", note)], content)?;
        }
        Inline::Image { url, alt } => {
            let mut attrs = vec![("URL", url.as_str())];
            if let Some(a) = alt {
                attrs.push(("text", a.as_str()));
            }
            write_element_with_attrs(writer, "image", &attrs, &[])?;
        }
        Inline::Video { url } => {
            write_element_with_attrs(writer, "video", &[("URL", url)], &[])?;
        }
    }
    Ok(())
}

/// Writes `<element kind="..."><attribute identifier="...">value</attribute>...content</element>`
/// — the confirmed shape for inline elements that carry typed attributes (link, and by analogy
/// annotation/image/video — inventory §3.3; see ASSUMPTION note on `Inline` in parrot-core).
fn write_element_with_attrs(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    kind: &str,
    attrs: &[(&str, &str)],
    content: &[Inline],
) -> Result<(), XmlError> {
    let mut el = BytesStart::new("element");
    el.push_attribute(("kind", kind));
    writer.write_event(Event::Start(el))?;

    for (identifier, value) in attrs {
        let mut attr_el = BytesStart::new("attribute");
        attr_el.push_attribute(("identifier", *identifier));
        writer.write_event(Event::Start(attr_el))?;
        writer.write_event(Event::Text(BytesText::new(value)))?;
        writer.write_event(Event::End(BytesEnd::new("attribute")))?;
    }

    for c in content {
        write_inline(writer, c)?;
    }

    writer.write_event(Event::End(BytesEnd::new("element")))?;
    Ok(())
}

fn write_wrapped_element(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    kind: &str,
    start_tag: Option<&str>,
    end_tag: Option<&str>,
    children: &[Inline],
) -> Result<(), XmlError> {
    let mut el = BytesStart::new("element");
    el.push_attribute(("kind", kind));
    if let Some(s) = start_tag {
        el.push_attribute(("startTag", s));
    }
    if let Some(e) = end_tag {
        el.push_attribute(("endTag", e));
    }
    writer.write_event(Event::Start(el))?;
    for c in children {
        write_inline(writer, c)?;
    }
    writer.write_event(Event::End(BytesEnd::new("element")))?;
    Ok(())
}

fn parse_tag_kind(s: &str) -> Result<TagKind, XmlError> {
    TagKind::from_identifier(s).ok_or_else(|| XmlError::Malformed(format!("unknown tag kind: {s}")))
}

/// Parses the confirmed `Content.xml` text form back into a `SheetContent` AST.
pub fn parse(xml: &str) -> Result<SheetContent, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(false);

    let mut version = 1u32;
    let mut markup = MarkupInfo::default();
    let mut paragraphs = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"sheet" => {
                if let Some(v) = attr_value(&e, b"version")? {
                    version = v.parse().unwrap_or(1);
                }
            }
            Event::Empty(e) if e.local_name().as_ref() == b"markup" => {
                markup = parse_markup_attrs(&e)?;
            }
            Event::Start(e) if e.local_name().as_ref() == b"markup" => {
                markup = parse_markup_attrs(&e)?;
                skip_to_end(&mut reader, b"markup")?;
            }
            Event::Start(e) if e.local_name().as_ref() == b"string" => {
                let _ = e;
                loop {
                    match reader.read_event()? {
                        Event::Start(pe) if pe.local_name().as_ref() == b"p" => {
                            paragraphs.push(parse_paragraph(&mut reader)?);
                        }
                        Event::End(pe) if pe.local_name().as_ref() == b"string" => break,
                        Event::Eof => {
                            return Err(XmlError::Malformed("unexpected EOF in <string>".into()))
                        }
                        _ => {}
                    }
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"sheet" => break,
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(SheetContent {
        version,
        markup,
        paragraphs,
    })
}

fn attr_value(e: &BytesStart, name: &[u8]) -> Result<Option<String>, XmlError> {
    for attr in e.attributes() {
        let attr = attr.map_err(|err| XmlError::Malformed(err.to_string()))?;
        if attr.key.local_name().as_ref() == name {
            let value = attr
                .unescape_value()
                .map_err(|err| XmlError::Malformed(err.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn parse_markup_attrs(e: &BytesStart) -> Result<MarkupInfo, XmlError> {
    let mut info = MarkupInfo::default();
    if let Some(v) = attr_value(e, b"version")? {
        info.version = v.parse().unwrap_or(1);
    }
    if let Some(v) = attr_value(e, b"identifier")? {
        info.identifier = v;
    }
    if let Some(v) = attr_value(e, b"displayName")? {
        info.display_name = v;
    }
    Ok(info)
}

fn skip_to_end(reader: &mut Reader<&[u8]>, tag: &[u8]) -> Result<(), XmlError> {
    loop {
        match reader.read_event()? {
            Event::End(e) if e.local_name().as_ref() == tag => return Ok(()),
            Event::Eof => {
                return Err(XmlError::Malformed(format!(
                    "unexpected EOF while skipping <{}>",
                    String::from_utf8_lossy(tag)
                )))
            }
            _ => {}
        }
    }
}

fn parse_paragraph(reader: &mut Reader<&[u8]>) -> Result<Paragraph, XmlError> {
    let mut tag = None;
    let mut inlines = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"tags" => {
                tag = Some(parse_tags(reader)?);
            }
            Event::Start(e) if e.local_name().as_ref() == b"element" => {
                inlines.push(parse_element(reader, &e)?);
            }
            Event::Text(t) => {
                let text = t.unescape()?.into_owned();
                if !text.is_empty() {
                    inlines.push(Inline::Text(text));
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"p" => break,
            Event::Eof => return Err(XmlError::Malformed("unexpected EOF in <p>".into())),
            _ => {}
        }
    }

    Ok(Paragraph { tag, inlines })
}

fn parse_tags(reader: &mut Reader<&[u8]>) -> Result<TagMark, XmlError> {
    let mut kind = None;
    let mut literal = String::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"tag" => {
                if let Some(v) = attr_value(&e, b"kind")? {
                    let mut parsed = parse_tag_kind(&v)?;
                    if let TagKind::CodeBlock(language) = &mut parsed {
                        *language = attr_value(&e, b"language")?;
                    }
                    kind = Some(parsed);
                }
            }
            Event::Text(t) => {
                literal.push_str(&t.unescape()?);
            }
            Event::End(e) if e.local_name().as_ref() == b"tag" => {}
            Event::End(e) if e.local_name().as_ref() == b"tags" => break,
            Event::Eof => return Err(XmlError::Malformed("unexpected EOF in <tags>".into())),
            _ => {}
        }
    }

    let kind = kind.ok_or_else(|| XmlError::Malformed("<tag> missing kind attribute".into()))?;
    Ok(TagMark { kind, literal })
}

fn parse_element(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<Inline, XmlError> {
    let kind = attr_value(start, b"kind")?
        .ok_or_else(|| XmlError::Malformed("<element> missing kind attribute".into()))?;

    match kind.as_str() {
        "strong" => Ok(Inline::Strong(parse_inline_children(reader, b"element")?)),
        "emph" => Ok(Inline::Emph(parse_inline_children(reader, b"element")?)),
        "code" => Ok(Inline::Code(parse_text_only(reader, b"element")?)),
        "footnote" => Ok(Inline::Footnote {
            content: parse_inline_children(reader, b"element")?,
        }),
        "delete" => Ok(Inline::Delete(parse_inline_children(reader, b"element")?)),
        "inlineComment" => Ok(Inline::InlineComment(parse_inline_children(
            reader, b"element",
        )?)),
        "inlineNative" => Ok(Inline::InlineNative(parse_text_only(reader, b"element")?)),
        "mark" => Ok(Inline::Mark(parse_inline_children(reader, b"element")?)),
        "link" => {
            let (attrs, content) = parse_element_body(reader)?;
            Ok(Inline::Link {
                url: find_attr(&attrs, "URL"),
                content,
            })
        }
        "annotation" => {
            let (attrs, content) = parse_element_body(reader)?;
            Ok(Inline::Annotation {
                note: find_attr(&attrs, "text"),
                content,
            })
        }
        "image" => {
            let (attrs, _content) = parse_element_body(reader)?;
            Ok(Inline::Image {
                url: find_attr(&attrs, "URL"),
                alt: attrs
                    .iter()
                    .find(|(k, _)| k == "text")
                    .map(|(_, v)| v.clone()),
            })
        }
        "video" => {
            let (attrs, _content) = parse_element_body(reader)?;
            Ok(Inline::Video {
                url: find_attr(&attrs, "URL"),
            })
        }
        other => Err(XmlError::Malformed(format!(
            "unknown element kind: {other}"
        ))),
    }
}

fn find_attr(attrs: &[(String, String)], identifier: &str) -> String {
    attrs
        .iter()
        .find(|(k, _)| k == identifier)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn parse_inline_children(
    reader: &mut Reader<&[u8]>,
    end_tag: &[u8],
) -> Result<Vec<Inline>, XmlError> {
    let mut children = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"element" => {
                children.push(parse_element(reader, &e)?);
            }
            Event::Text(t) => {
                let text = t.unescape()?.into_owned();
                if !text.is_empty() {
                    children.push(Inline::Text(text));
                }
            }
            Event::End(e) if e.local_name().as_ref() == end_tag => break,
            Event::Eof => {
                return Err(XmlError::Malformed(
                    "unexpected EOF while parsing inline children".into(),
                ))
            }
            _ => {}
        }
    }
    Ok(children)
}

fn parse_text_only(reader: &mut Reader<&[u8]>, end_tag: &[u8]) -> Result<String, XmlError> {
    let mut text = String::new();
    loop {
        match reader.read_event()? {
            Event::Text(t) => text.push_str(&t.unescape()?),
            Event::End(e) if e.local_name().as_ref() == end_tag => break,
            Event::Eof => {
                return Err(XmlError::Malformed(
                    "unexpected EOF while parsing text-only element".into(),
                ))
            }
            _ => {}
        }
    }
    Ok(text)
}

/// `(identifier, value)` pairs parsed from an element's `<attribute>` children.
type ElementAttrs = Vec<(String, String)>;

/// Parses an `<element>` body made of zero or more `<attribute identifier="...">value</attribute>`
/// children followed by inline content, until the matching `</element>` — the confirmed shape
/// for link, and by analogy annotation/image/video (inventory §3.3).
fn parse_element_body(reader: &mut Reader<&[u8]>) -> Result<(ElementAttrs, Vec<Inline>), XmlError> {
    let mut attrs = Vec::new();
    let mut content = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"attribute" => {
                let identifier = attr_value(&e, b"identifier")?.unwrap_or_default();
                let value = parse_text_only(reader, b"attribute")?;
                attrs.push((identifier, value));
            }
            Event::Start(e) if e.local_name().as_ref() == b"element" => {
                content.push(parse_element(reader, &e)?);
            }
            Event::Text(t) => {
                let text = t.unescape()?.into_owned();
                if !text.is_empty() {
                    content.push(Inline::Text(text));
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"element" => break,
            Event::Eof => {
                return Err(XmlError::Malformed(
                    "unexpected EOF while parsing <element>".into(),
                ))
            }
            _ => {}
        }
    }

    Ok((attrs, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::Paragraph;

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
    fn roundtrip_confirmed_sample() {
        let ast = confirmed_example();
        let xml = serialize(&ast).expect("serialize must succeed for a valid AST");
        let parsed = parse(&xml).expect("parse must succeed for our own serializer's output");
        assert_eq!(
            ast, parsed,
            "parse(serialize(ast)) must equal ast\nxml was:\n{xml}"
        );
    }

    #[test]
    fn roundtrip_full_inline_variety() {
        let ast = SheetContent::new(vec![Paragraph {
            tag: Some(TagMark {
                kind: TagKind::BlockQuote,
                literal: "> ".to_string(),
            }),
            inlines: vec![
                Inline::Strong(vec![Inline::Text("bold".to_string())]),
                Inline::Text(" ".to_string()),
                Inline::Code("let x = 1;".to_string()),
                Inline::Text(" ".to_string()),
                Inline::Footnote {
                    content: vec![Inline::Text("a footnote".to_string())],
                },
            ],
        }]);
        let xml = serialize(&ast).expect("serialize must succeed");
        let parsed = parse(&xml).expect("parse must succeed");
        assert_eq!(ast, parsed);
    }

    /// T-1-03: a document using every one of the 19 confirmed tags (inventory §3.2) at least
    /// once — one paragraph per block-level tag, plus one paragraph exercising every inline
    /// variant — round-trips exactly through `parse(serialize(ast))`.
    #[test]
    fn roundtrip_full_tag_coverage() {
        fn text(s: &str) -> Inline {
            Inline::Text(s.to_string())
        }

        let ast = SheetContent::new(vec![
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Heading(2),
                    literal: "## ".to_string(),
                }),
                inlines: vec![text("A heading")],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::UnorderedList,
                    literal: "* ".to_string(),
                }),
                inlines: vec![text("an unordered item")],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::OrderedList,
                    literal: "1. ".to_string(),
                }),
                inlines: vec![text("an ordered item")],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::BlockQuote,
                    literal: "> ".to_string(),
                }),
                inlines: vec![text("a quote")],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::CodeBlock(Some("rust".to_string())),
                    literal: "```rust ".to_string(),
                }),
                inlines: vec![text("let x = 1;")],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Comment,
                    literal: "%% ".to_string(),
                }),
                inlines: vec![text("a comment")],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::Divider,
                    literal: "----".to_string(),
                }),
                inlines: vec![],
            },
            Paragraph {
                tag: Some(TagMark {
                    kind: TagKind::NativeBlock,
                    literal: "~~ ".to_string(),
                }),
                inlines: vec![text("<raw/>")],
            },
            Paragraph {
                tag: None,
                inlines: vec![
                    Inline::Strong(vec![text("strong")]),
                    text(" "),
                    Inline::Emph(vec![text("emph")]),
                    text(" "),
                    Inline::Code("code".to_string()),
                    text(" "),
                    Inline::Link {
                        url: "example.com".to_string(),
                        content: vec![text("link")],
                    },
                    text(" "),
                    Inline::Footnote {
                        content: vec![text("footnote")],
                    },
                    text(" "),
                    Inline::Delete(vec![text("deleted")]),
                    text(" "),
                    Inline::InlineComment(vec![text("aside")]),
                    text(" "),
                    Inline::InlineNative("<b>raw</b>".to_string()),
                    text(" "),
                    Inline::Mark(vec![text("marked")]),
                    text(" "),
                    Inline::Annotation {
                        note: "a note".to_string(),
                        content: vec![text("annotated")],
                    },
                    text(" "),
                    Inline::Image {
                        url: "image.png".to_string(),
                        alt: Some("an image".to_string()),
                    },
                    text(" "),
                    Inline::Video {
                        url: "video.mp4".to_string(),
                    },
                ],
            },
        ]);

        let xml = serialize(&ast).expect("serialize must succeed");
        let parsed = parse(&xml).expect("parse must succeed");
        assert_eq!(
            ast, parsed,
            "parse(serialize(ast)) must equal ast\nxml was:\n{xml}"
        );
    }

    /// T-1-04: `parse(serialize(x)) == x` for every fixture under `tests/fixtures/content_xml/`,
    /// where `x` is the AST obtained by parsing each real fixture file (not a hardcoded literal)
    /// — so this test automatically covers any fixture added to that directory later, without
    /// code changes here.
    #[test]
    fn corpus_roundtrip() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/content_xml");
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("fixtures directory must exist")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "xml"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        assert!(
            entries.len() >= 10,
            "expected at least 10 fixtures, found {}",
            entries.len()
        );

        for entry in entries {
            let path = entry.path();
            let xml = std::fs::read_to_string(&path).expect("fixture must be readable");
            let ast1 =
                parse(&xml).unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
            let xml2 = serialize(&ast1)
                .unwrap_or_else(|e| panic!("failed to serialize {}: {e}", path.display()));
            let ast2 = parse(&xml2).unwrap_or_else(|e| {
                panic!(
                    "failed to re-parse serialized output of {}: {e}",
                    path.display()
                )
            });
            assert_eq!(
                ast1,
                ast2,
                "parse(serialize(parse(x))) != parse(x) for {}",
                path.display()
            );
        }
    }
}
