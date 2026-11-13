//! Create/open/save for a `.parrot` sheet directory package: `Content.xml` + `Text.txt`
//! siblings (feature inventory §2/§3.3), plus `Attachments.txt` (T-10-11, see below). Writes
//! are atomic per ADR-6: the new package is fully assembled under a sibling temp directory,
//! then swapped into place with a single `rename` — a crash before that rename leaves whatever
//! was already at `path` untouched.
//!
//! **`Attachments.txt` (T-10-11 — `// ASSUMPTION`, same flagging convention as D2):** no
//! Attachments field exists in the confirmed `Content.xml` schema (inventory §3.3's sample
//! predates this feature's discovery), so rather than extend that confirmed schema with an
//! unconfirmed addition, Attachments get their own sidecar file in the sheet's own directory —
//! additive, and easy to drop/ignore on a version of this app that doesn't know about it yet.
//! One attachment per line, tab-separated `KIND\tvalue` with `\`/newline/tab escaped in `value`;
//! omitted entirely when there are no attachments (so a sheet with none round-trips to exactly
//! the same two files as before this task).

use crate::xml::{self, XmlError};
use parrot_core::{Attachment, SheetContent};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

const CONTENT_XML: &str = "Content.xml";
const TEXT_TXT: &str = "Text.txt";
const ATTACHMENTS_TXT: &str = "Attachments.txt";

#[derive(Debug, Error)]
pub enum SheetIoError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("xml error: {0}")]
    Xml(#[from] XmlError),
}

/// Reads a `.parrot` sheet package back: its `Content.xml` AST, plus any `Attachments.txt`
/// (an empty `Vec` if that file doesn't exist — most sheets have none).
pub fn open(path: &Path) -> Result<(SheetContent, Vec<Attachment>), SheetIoError> {
    let xml_str = fs::read_to_string(path.join(CONTENT_XML))?;
    let content = xml::parse(&xml_str)?;

    let attachments = match fs::read_to_string(path.join(ATTACHMENTS_TXT)) {
        Ok(s) => parse_attachments(&s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };

    Ok((content, attachments))
}

/// Creates or overwrites a `.parrot` sheet package at `path` with `content` and
/// `attachments`, atomically.
pub fn save(
    path: &Path,
    content: &SheetContent,
    attachments: &[Attachment],
) -> Result<(), SheetIoError> {
    let tmp_path = tmp_sibling_path(path);
    write_tmp(&tmp_path, content, attachments)?;
    commit(&tmp_path, path)
}

fn serialize_attachments(attachments: &[Attachment]) -> String {
    attachments
        .iter()
        .map(|a| match a {
            Attachment::Note { text } => format!("NOTE\t{}\n", escape(text)),
            Attachment::File { file_name } => format!("FILE\t{}\n", escape(file_name)),
        })
        .collect()
}

fn parse_attachments(s: &str) -> Vec<Attachment> {
    s.lines()
        .filter_map(|line| {
            let (kind, value) = line.split_once('\t')?;
            let value = unescape(value);
            match kind {
                "NOTE" => Some(Attachment::Note { text: value }),
                "FILE" => Some(Attachment::File { file_name: value }),
                _ => None,
            }
        })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn tmp_sibling_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("sheet");
    parent.join(format!(".{name}.tmp-{}", std::process::id()))
}

/// Writes a complete, self-contained package to `tmp_path` — never touches the real `path`.
/// Split out from `save` so the atomic-write test can simulate a crash between this step and
/// `commit` without needing to actually kill the process.
fn write_tmp(
    tmp_path: &Path,
    content: &SheetContent,
    attachments: &[Attachment],
) -> Result<(), SheetIoError> {
    if tmp_path.exists() {
        fs::remove_dir_all(tmp_path)?;
    }
    fs::create_dir_all(tmp_path)?;

    let xml_str = xml::serialize(content)?;
    fs::write(tmp_path.join(CONTENT_XML), xml_str)?;

    let text = parrot_markup::render::render(content);
    fs::write(tmp_path.join(TEXT_TXT), text)?;

    if !attachments.is_empty() {
        fs::write(
            tmp_path.join(ATTACHMENTS_TXT),
            serialize_attachments(attachments),
        )?;
    }

    Ok(())
}

/// Atomically swaps the fully-written `tmp_path` into `path`, replacing any prior version.
fn commit(tmp_path: &Path, path: &Path) -> Result<(), SheetIoError> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Inline, Paragraph, TagKind, TagMark};

    fn sample(heading: &str) -> SheetContent {
        SheetContent::new(vec![Paragraph {
            tag: Some(TagMark {
                kind: TagKind::Heading(1),
                literal: "# ".to_string(),
            }),
            inlines: vec![Inline::Text(heading.to_string())],
        }])
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("Example.parrot");
        let original = sample("Hello, Parrot");

        save(&path, &original, &[]).expect("save must succeed");
        let (reopened, attachments) = open(&path).expect("open must succeed");

        assert_eq!(original, reopened);
        assert!(attachments.is_empty());
        assert!(path.join(CONTENT_XML).exists());
        assert!(path.join(TEXT_TXT).exists());
        assert!(
            !path.join(ATTACHMENTS_TXT).exists(),
            "no Attachments.txt when there are no attachments to write"
        );
    }

    #[test]
    fn roundtrip_with_attachments() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("Example.parrot");
        let original = sample("Hello, Parrot");
        let attachments = vec![
            Attachment::Note {
                text: "line one\nline two\twith a tab".to_string(),
            },
            Attachment::File {
                file_name: "reference.pdf".to_string(),
            },
        ];

        save(&path, &original, &attachments).expect("save must succeed");
        let (reopened_content, reopened_attachments) = open(&path).expect("open must succeed");

        assert_eq!(original, reopened_content);
        assert_eq!(reopened_attachments, attachments);
        assert!(path.join(ATTACHMENTS_TXT).exists());
    }

    #[test]
    fn atomic_write_survives_interrupted_save() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("Example.parrot");
        let original = sample("Version A");
        save(&path, &original, &[]).expect("initial save must succeed");

        // Simulate a crash between write_tmp and commit: write the replacement's contents to
        // the temp sibling, then stop — never call commit.
        let replacement = sample("Version B");
        let tmp_path = tmp_sibling_path(&path);
        write_tmp(&tmp_path, &replacement, &[]).expect("write_tmp must succeed");

        // The real path must be exactly what it was before the "interrupted" save.
        let (still_there, _) = open(&path).expect("open must succeed after interrupted save");
        assert_eq!(
            still_there, original,
            "interrupted save must not touch the prior version"
        );

        // Completing the commit now must produce the replacement, proving the split is faithful.
        commit(&tmp_path, &path).expect("commit must succeed");
        let (committed, _) = open(&path).expect("open must succeed after commit");
        assert_eq!(committed, replacement);
    }
}
