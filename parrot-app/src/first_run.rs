//! First-run flow (T-7-05): minimal "create your first library" UX. Explicitly **not** the
//! excluded `CustomerLifecycle` onboarding/marketing system (feature inventory Tier 4) — just
//! the bare functional prompt-or-skip decision a freshly started app needs to make, with no
//! tours, telemetry, or marketing content.

use crate::state::AppState;

/// Whether the first-run prompt should be shown: exactly when no library has been opened or
/// created yet (`AppState::library_path` is `None`).
pub fn should_prompt(state: &AppState) -> bool {
    state.library_path.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn prompts_when_no_library_exists() {
        let state = AppState::new();
        assert!(
            should_prompt(&state),
            "a brand-new AppState has no library yet"
        );
    }

    #[test]
    fn skips_once_a_library_is_open() {
        let mut state = AppState::new();
        state.create_library(Path::new("/tmp/does-not-matter-for-this-test"));
        assert!(
            !should_prompt(&state),
            "a library is already open — don't prompt again"
        );
    }
}
