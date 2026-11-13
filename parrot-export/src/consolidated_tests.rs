//! Format-validity test suite (T-5-11): consolidates T-5-04..T-5-09's own validity checks into
//! one suite run against the single shared `fixtures::full_tag_document`, so a future change
//! that breaks one backend on that shared document is caught here too, not just in that
//! backend's own narrower test. Each backend's primary validity test (named in its own module)
//! still owns its backend-specific assertions (well-formed XML, structurally-valid RTF,
//! docx-rs/printpdf round-trips, TextBundle's required files) — this suite only re-asserts
//! "doesn't fail and isn't empty" for every backend together, against the one document.

#[cfg(test)]
mod tests {
    use crate::docx::{export_docx, reopen as reopen_docx};
    use crate::epub::export_epub;
    use crate::fixtures::full_tag_document;
    use crate::html::export_html;
    use crate::pdf::export_pdf;
    use crate::rtf::{export_rtf, is_structurally_valid};
    use crate::textbundle::export_textbundle;

    #[test]
    fn all_backends_valid_for_shared_fixture() {
        let sheet = full_tag_document();

        let html = export_html(&sheet);
        assert!(!html.is_empty(), "html backend produced empty output");

        let rtf = export_rtf(&sheet);
        assert!(
            is_structurally_valid(&rtf),
            "rtf backend produced structurally invalid output"
        );

        let docx_bytes = export_docx(&sheet).expect("docx backend must succeed");
        reopen_docx(&docx_bytes).expect("docx backend output must be re-openable by docx-rs");

        let epub_bytes = export_epub(&sheet).expect("epub backend must succeed");
        zip::ZipArchive::new(std::io::Cursor::new(epub_bytes))
            .expect("epub backend output must be a well-formed zip");

        let pdf_bytes = export_pdf(&sheet);
        assert!(!pdf_bytes.is_empty(), "pdf backend produced empty output");

        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        export_textbundle(&dir.path().join("Export.textbundle"), &sheet)
            .expect("textbundle backend must succeed");
    }
}
