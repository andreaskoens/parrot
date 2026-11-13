//! TextBundle backend (T-5-09): directory + `info.json` + `text.markdown`, per the TextBundle
//! spec (<https://textbundle.org/spec/>). Atomic write follows the same "assemble under a
//! sibling temp directory, then `rename` into place" idiom as
//! `parrot_container::sheet::save` (ADR-6) — not shared code (this is a different package
//! shape, and parrot-export doesn't otherwise depend on parrot-container), but the same
//! principle: a crash before the rename leaves whatever was already at `path` untouched.
//!
//! Reuses `mvp::export_markdown` for `text.markdown`'s content instead of a third Markdown
//! renderer — TextBundle's text file is exactly the plain-Markdown content the MVP exporter
//! already produces.

use crate::mvp::export_markdown;
use parrot_core::SheetContent;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

const INFO_JSON: &str = "info.json";
const TEXT_MARKDOWN: &str = "text.markdown";
const CREATOR_IDENTIFIER: &str = "studio.parrot-rewrite";

#[derive(Debug, Error)]
pub enum TextBundleError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Creates or overwrites a `.textbundle` package at `path` with `content`, atomically.
pub fn export_textbundle(path: &Path, content: &SheetContent) -> Result<(), TextBundleError> {
    let tmp_path = tmp_sibling_path(path);
    write_tmp(&tmp_path, content)?;
    commit(&tmp_path, path)
}

fn tmp_sibling_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("export");
    parent.join(format!(".{name}.tmp-{}", std::process::id()))
}

fn write_tmp(tmp_path: &Path, content: &SheetContent) -> Result<(), TextBundleError> {
    if tmp_path.exists() {
        fs::remove_dir_all(tmp_path)?;
    }
    fs::create_dir_all(tmp_path)?;

    let info = format!(
        "{{\n  \"version\": 2,\n  \"type\": \"net.daringfireball.markdown\",\n  \"creatorIdentifier\": \"{CREATOR_IDENTIFIER}\"\n}}\n"
    );
    fs::write(tmp_path.join(INFO_JSON), info)?;
    fs::write(tmp_path.join(TEXT_MARKDOWN), export_markdown(content))?;

    Ok(())
}

fn commit(tmp_path: &Path, path: &Path) -> Result<(), TextBundleError> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::full_tag_document;

    #[test]
    fn matches_spec_structure() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let bundle_path = dir.path().join("Export.textbundle");

        export_textbundle(&bundle_path, &full_tag_document()).expect("export must succeed");

        assert!(
            bundle_path.is_dir(),
            "the bundle itself must be a directory"
        );
        assert!(bundle_path.join(INFO_JSON).is_file());
        assert!(bundle_path.join(TEXT_MARKDOWN).is_file());

        let info = fs::read_to_string(bundle_path.join(INFO_JSON)).unwrap();
        // No serde_json dependency for one test's sake — just the required spec fields present.
        assert!(info.contains("\"version\": 2"));
        assert!(info.contains("\"type\": \"net.daringfireball.markdown\""));
        assert!(info.contains("\"creatorIdentifier\""));

        let markdown = fs::read_to_string(bundle_path.join(TEXT_MARKDOWN)).unwrap();
        assert!(!markdown.is_empty());
    }

    #[test]
    fn overwrites_an_existing_bundle_atomically() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let bundle_path = dir.path().join("Export.textbundle");

        export_textbundle(&bundle_path, &full_tag_document()).expect("first export must succeed");
        export_textbundle(&bundle_path, &full_tag_document()).expect("second export must succeed");

        assert!(bundle_path.join(INFO_JSON).is_file());
        // No leftover temp sibling directories.
        let siblings: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(
            siblings.len(),
            1,
            "no stray temp directories should remain: {siblings:?}"
        );
    }
}
