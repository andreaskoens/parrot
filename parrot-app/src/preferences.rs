//! Preferences: fonts & default style (T-7-04).
//!
//! **Font substitution (Decision, logged here and in DECISIONS.md):** the confirmed
//! `ULDefaultFonts.plist` names correspond to commercial Adobe fonts bundled with macOS/Creative
//! Cloud (Myriad Pro, Chaparral Pro, Minion Pro) — this project has no license to bundle or even
//! reference those as defaults. Each is substituted with an open-licensed (SIL OFL) Google Fonts
//! equivalent of a similar role/character:
//! - Myriad Pro (humanist sans, body text)      -> **Source Sans 3**
//! - Chaparral Pro (humanist serif, body text)   -> **Source Serif 4**
//! - Minion Pro (classic serif, alt body/print)  -> **Lora**
//! - (monospace, not in the confirmed plist but needed for code blocks) -> **Source Code Pro**
//!
//! These are *name* substitutions only — no font files are bundled in this repo; actual text
//! shaping (T-2-01/T-2-04, `cosmic-text`) resolves font names against whatever's installed on
//! the system. A user without these specific fonts installed falls back to their system's
//! default per `cosmic-text`'s own fallback behavior, unaffected by this change.
//!
//! **Default-style-per-sheet:** `default_style_name`, when set, is applied to a `StylePicker`
//! (T-4-09) via `apply_to` — the M4 wiring the task asks for.

use crate::style_picker::StylePicker;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PreferencesError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("serialization error: {0}")]
    Ser(#[from] toml::ser::Error),
    #[error("deserialization error: {0}")]
    De(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    pub body_font: String,
    pub serif_font: String,
    pub monospace_font: String,
    pub default_style_name: Option<String>,
    /// The export dialog's built-in style (`None` = the Built-in default look).
    /// `default` keeps older prefs files (written before this field existed) loading.
    #[serde(default)]
    pub export_style_name: Option<String>,
    /// A user-picked CSS file that wins over `export_style_name` while readable.
    #[serde(default)]
    pub export_css_path: Option<std::path::PathBuf>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            body_font: "Source Sans 3".to_string(),
            serif_font: "Source Serif 4".to_string(),
            monospace_font: "Source Code Pro".to_string(),
            default_style_name: None,
            export_style_name: None,
            export_css_path: None,
        }
    }
}

impl Preferences {
    /// Loads preferences from `path`, or this project's defaults if the file doesn't exist yet
    /// (first run, before any preferences have ever been saved).
    pub fn load(path: &Path) -> Result<Self, PreferencesError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(toml::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), PreferencesError> {
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Applies `default_style_name` (if set) to `picker` — the M4 wiring this task asks for.
    pub fn apply_to(&self, picker: &mut StylePicker) {
        if let Some(name) = &self.default_style_name {
            picker.select(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_across_restart() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("preferences.toml");

        let prefs = Preferences {
            body_font: "Source Serif 4".to_string(),
            default_style_name: Some("Compact".to_string()),
            ..Preferences::default()
        };
        prefs.save(&path).expect("save must succeed");

        // Simulate a restart: a fresh load from the same path, not the in-memory value.
        let reloaded = Preferences::load(&path).expect("load must succeed");
        assert_eq!(reloaded, prefs);
    }

    #[test]
    fn loading_a_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        let path = dir.path().join("does-not-exist.toml");
        assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
    }

    #[test]
    fn apply_to_selects_the_default_style() {
        let prefs = Preferences {
            default_style_name: Some("Compact".to_string()),
            ..Preferences::default()
        };
        let mut picker = StylePicker::new();
        prefs.apply_to(&mut picker);
        assert_eq!(picker.selected_name, Some("Compact".to_string()));
    }
}
