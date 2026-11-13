//! Import-then-export golden suite (T-6-08): consolidates T-6-01..T-6-07's own round-trip
//! checks into one test run, so a regression that breaks one importer against its fixture is
//! caught here too, not just in that importer's own narrower test module. Each importer's own
//! test still owns the detailed assertions (exact golden-file byte comparisons, structural
//! checks); this suite re-runs the same fixtures and re-asserts the same pass/fail signal
//! together.

#[cfg(test)]
mod tests {
    use crate::docx::import_docx;
    use crate::enex::import_enex;
    use crate::html::import_html;
    use crate::markdown::import_markdown;
    use crate::plaintext::import_plaintext;
    use crate::rtf::import_rtf;
    use crate::textbundle::import_textbundle;

    #[test]
    fn all_formats_roundtrip() {
        let markdown = include_str!("../../tests/fixtures/import/representative.md");
        let exported = parrot_export::mvp::export_markdown(&import_markdown(markdown));
        let golden = include_str!("../../tests/fixtures/import/representative.exported.md");
        assert_eq!(exported, golden, "markdown importer regressed");

        let plain = include_str!("../../tests/fixtures/import/representative.plain.txt");
        assert_eq!(
            parrot_export::mvp::export_plain_text(&import_plaintext(plain)),
            plain,
            "plaintext importer regressed"
        );

        let html = include_str!("../../tests/fixtures/import/representative.html");
        assert_eq!(
            parrot_export::mvp::export_markdown(&import_html(html)),
            golden,
            "html importer regressed"
        );

        let enex = include_str!("../../tests/fixtures/import/representative.enex");
        let notes = import_enex(enex).expect("enex fixture must parse");
        let enex_golden =
            include_str!("../../tests/fixtures/import/representative.enex.exported.md");
        assert_eq!(
            parrot_export::mvp::export_markdown(&notes[0]),
            enex_golden,
            "enex importer regressed"
        );

        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let bundle_path = dir.path().join("Note.textbundle");
        parrot_export::textbundle::export_textbundle(&bundle_path, &notes[0])
            .expect("textbundle export must succeed");
        let reexported = parrot_export::mvp::export_markdown(
            &import_textbundle(&bundle_path).expect("textbundle import must succeed"),
        );
        let original = std::fs::read_to_string(bundle_path.join("text.markdown")).unwrap();
        assert_eq!(reexported, original, "textbundle importer regressed");

        // docx/rtf: reuse the enex-imported note rather than re-deriving a third representative
        // document. Their own module tests already validate the exact golden shape against a
        // purpose-built fixture (docx) / this project's own exporter's real output including its
        // font table (rtf); here we only need "doesn't panic, round-trips without error."
        let docx_bytes =
            parrot_export::docx::export_docx(&notes[0]).expect("docx export must succeed");
        import_docx(&docx_bytes).expect("docx import must succeed");

        let rtf = parrot_export::rtf::export_rtf(&notes[0]);
        import_rtf(&rtf);
    }
}
