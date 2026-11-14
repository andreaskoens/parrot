//! Separate iCloud library on macOS (Ariadne reference:
//! `~/Library/Mobile Documents/com~apple~CloudDocs`).
//!
//! Parrot's on-disk tree is already Finder-readable, so a separate iCloud
//! library is just a normal library rooted inside iCloud Drive — Apple syncs
//! the files, no network code or merge protocol needed for v1.

use std::path::PathBuf;

/// Folder name inside iCloud Drive.
pub const ICLOUD_FOLDER_NAME: &str = "Parrot";

/// Env override for tests (mirrors Ariadne's `ARIADNE_ICLOUD_DIR`).
pub const ENV_OVERRIDE: &str = "PARROT_ICLOUD_DIR";

/// The user's iCloud Drive folder, if iCloud Drive is enabled.
/// macOS only; everywhere else this is `None` and all iCloud UI stays hidden.
#[cfg(target_os = "macos")]
pub fn icloud_drive_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(ENV_OVERRIDE) {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home).join("Library/Mobile Documents/com~apple~CloudDocs");
    p.is_dir().then_some(p)
}

/// Non-macOS builds never have iCloud Drive.
#[cfg(not(target_os = "macos"))]
pub fn icloud_drive_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(ENV_OVERRIDE) {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    None
}

/// Default location of the separate iCloud library.
pub fn default_icloud_library_dir() -> Option<PathBuf> {
    icloud_drive_dir().map(|d| d.join(ICLOUD_FOLDER_NAME))
}

/// Whether iCloud support is available on this machine.
pub fn is_available() -> bool {
    icloud_drive_dir().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_points_at_custom_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: tests run single-threaded here; guard against parallel use.
        unsafe {
            std::env::set_var(ENV_OVERRIDE, dir.path().join("CloudDocs"));
        }
        std::fs::create_dir_all(dir.path().join("CloudDocs")).expect("mkdir");
        let drive = icloud_drive_dir().expect("override");
        assert!(drive.ends_with("CloudDocs"));
        assert_eq!(
            default_icloud_library_dir().expect("default"),
            drive.join(ICLOUD_FOLDER_NAME)
        );
        unsafe {
            std::env::remove_var(ENV_OVERRIDE);
        }
    }
}
