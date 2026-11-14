//! EPUB backend (T-5-07): manual zip+OPF, not `epub-builder` — this project's own minimal-valid
//! EPUB 3 container, built on the same `zip` crate already vetted for Windows cross-compile by
//! T-5-06 (deflate-only feature set; see DECISIONS.md's zstd-sys note), rather than pulling in
//! another dependency with its own unknown cross-compile risk. Reuses `html::export_html`'s
//! already well-formed-XML output for the body content (one small adapter adds the XHTML
//! namespace) instead of re-deriving an AST walker.

use crate::css::ExportTheme;
use crate::html::export_html_with;
use parrot_core::SheetContent;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

const NAV_XHTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>Navigation</title></head>
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="content.xhtml">Start</a></li>
    </ol>
  </nav>
</body>
</html>"#;

const CONTENT_OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:parrot-export</dc:identifier>
    <dc:title>Export</dc:title>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2026-10-30T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="content" href="content.xhtml" media-type="application/xhtml+xml"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  </manifest>
  <spine>
    <itemref idref="content"/>
  </spine>
</package>"#;

fn to_xhtml_with(sheet: &SheetContent, theme: &ExportTheme) -> String {
    export_html_with(sheet, theme).replacen(
        "<html>",
        "<html xmlns=\"http://www.w3.org/1999/xhtml\">",
        1,
    )
}

pub fn export_epub(sheet: &SheetContent) -> Result<Vec<u8>, zip::result::ZipError> {
    export_epub_with(sheet, &ExportTheme::builtin())
}

/// [`export_epub`], with a stylesheet applied: the theme's raw CSS is embedded in the
/// chapter (the same inline `<style>` element the HTML backend emits — no separate file
/// or manifest entry, since the book stays single-chapter).
pub fn export_epub_with(
    sheet: &SheetContent,
    theme: &ExportTheme,
) -> Result<Vec<u8>, zip::result::ZipError> {
    let xhtml = to_xhtml_with(sheet, theme);
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));

        // The mimetype entry must be first and stored uncompressed — the EPUB spec's "magic
        // number" readers sniff before even parsing the zip's central directory.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored)?;
        zip.write_all(b"application/epub+zip")?;

        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        zip.start_file("META-INF/container.xml", deflated)?;
        zip.write_all(CONTAINER_XML.as_bytes())?;

        zip.start_file("OEBPS/content.opf", deflated)?;
        zip.write_all(CONTENT_OPF.as_bytes())?;

        zip.start_file("OEBPS/nav.xhtml", deflated)?;
        zip.write_all(NAV_XHTML.as_bytes())?;

        zip.start_file("OEBPS/content.xhtml", deflated)?;
        zip.write_all(xhtml.as_bytes())?;

        zip.finish()?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::full_tag_document;
    use std::io::Read as _;

    fn assert_well_formed_xml(label: &str, xml: &str) {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;
        let mut reader = Reader::from_str(xml);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("{label} is not well-formed XML: {e}\n{xml}"),
            }
        }
    }

    #[test]
    fn valid_container_structure() {
        let bytes = export_epub(&full_tag_document()).expect("export must succeed");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .expect("output must be a well-formed zip");

        assert_eq!(
            archive.by_index(0).unwrap().name(),
            "mimetype",
            "mimetype must be the first zip entry"
        );
        let mut mimetype = String::new();
        archive
            .by_name("mimetype")
            .unwrap()
            .read_to_string(&mut mimetype)
            .unwrap();
        assert_eq!(mimetype, "application/epub+zip");

        let mut container = String::new();
        archive
            .by_name("META-INF/container.xml")
            .unwrap()
            .read_to_string(&mut container)
            .unwrap();
        assert_well_formed_xml("META-INF/container.xml", &container);
        assert!(container.contains("OEBPS/content.opf"));

        let mut opf = String::new();
        archive
            .by_name("OEBPS/content.opf")
            .unwrap()
            .read_to_string(&mut opf)
            .unwrap();
        assert_well_formed_xml("content.opf", &opf);
        assert!(
            opf.contains("<manifest>") && opf.contains("<spine>"),
            "content.opf must declare a manifest and a spine"
        );

        let mut content_xhtml = String::new();
        archive
            .by_name("OEBPS/content.xhtml")
            .unwrap()
            .read_to_string(&mut content_xhtml)
            .unwrap();
        assert_well_formed_xml("content.xhtml", &content_xhtml);
        assert!(content_xhtml.contains("xmlns=\"http://www.w3.org/1999/xhtml\""));
    }

    #[test]
    fn styled_chapter_embeds_the_stylesheet() {
        use std::io::Read as _;
        let css = "body { font-family: Georgia, serif; }";
        let bytes = export_epub_with(&full_tag_document(), &ExportTheme::from_css(css))
            .expect("export must succeed");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
        let mut xhtml = String::new();
        archive
            .by_name("OEBPS/content.xhtml")
            .expect("content.xhtml")
            .read_to_string(&mut xhtml)
            .expect("readable");
        assert!(xhtml.contains("<style>"), "styled EPUB keeps the CSS");
        assert!(xhtml.contains(css), "stylesheet embedded verbatim");
        assert_well_formed_xml("styled content.xhtml", &xhtml);
        // Unstyled books carry no style element.
        let plain = export_epub(&full_tag_document()).expect("export must succeed");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(plain)).expect("valid zip");
        let mut plain_xhtml = String::new();
        archive
            .by_name("OEBPS/content.xhtml")
            .expect("content.xhtml")
            .read_to_string(&mut plain_xhtml)
            .expect("readable");
        assert!(!plain_xhtml.contains("<style>"));
    }
}
