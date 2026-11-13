//! Reopening the last library on launch: the path of the most recently opened or created
//! library is kept in a one-line file in the user's config directory (`%APPDATA%\Parrot` on
//! Windows, `~/Library/Application Support/Parrot` on macOS, `$XDG_CONFIG_HOME/parrot` or
//! `~/.config/parrot` elsewhere).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "last-library.txt";

/// Where the last library's path is kept, if the platform's config directory is known.
pub fn default_file() -> Option<PathBuf> {
    let env = |key: &str| {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = if cfg!(windows) {
        env("APPDATA")?.join("Parrot")
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/Parrot")
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))?
            .join("parrot")
    };
    Some(dir.join(FILE_NAME))
}

/// Records `library` in `file` as the library to reopen next launch.
pub fn remember(file: &Path, library: &Path) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(file, library.display().to_string())
}

/// The library recorded in `file`, when it still holds a library.
pub fn recall(file: &Path) -> Option<PathBuf> {
    // Fall back to the file kept under the pre-rename app name.
    let legacy = file.parent().and_then(|dir| {
        let parent = dir.parent()?;
        [("Parrot", "Ulysses"), ("parrot", "ulysses")]
            .iter()
            .find(|(new, _)| dir.file_name().is_some_and(|n| n == *new))
            .map(|(_, old)| parent.join(old).join(FILE_NAME))
    });
    let text = fs::read_to_string(file)
        .or_else(|e| legacy.map_or(Err(e), fs::read_to_string))
        .ok()?;
    let path = PathBuf::from(text.trim());
    parrot_container::tree::has_library_entries(&path).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_a_library_until_it_disappears() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("config").join(FILE_NAME);
        assert_eq!(recall(&file), None, "nothing recorded yet");

        let library = dir.path().join("lib");
        let mut state = crate::state::AppState::new();
        state.create_library(&library);
        state.create_project("P");
        state.create_sheet(&[0], "A").expect("sheet");
        state.save().expect("save");
        remember(&file, &library).expect("remember");
        assert_eq!(recall(&file), Some(library.clone()));

        std::fs::remove_dir_all(&library).expect("remove");
        assert_eq!(recall(&file), None, "a deleted library isn't reopened");
    }
}
