//! Writing Goals (T-10-02): word/character-count targets with deadlines, attached to a
//! Sheet/Group/Project node. Confirmed to exist via inventory §12 — `nm`-demangled symbols in
//! `ParrotCoreMac.framework` name `ULCountingGoal` and four tuning constants
//! (`ULGoalControllerMinimumProgress`, `ULGoalControllerAlmostReached`,
//! `ULGoalControllerAtMostTolerance`, `ULGoalControllerProgressPrecision`) confirming that
//! progress is a tolerance-banded percentage, not a raw `current / target` ratio.
//!
//! **Only the constants' names and roles were confirmed, not their numeric values** — nothing in
//! the inventory's evidence (no bundled plist/resource) exposed what those four constants are
//! actually set to; they're compiled into the framework. The values below (`MINIMUM_PROGRESS_PERCENT`
//! = 2, `ALMOST_REACHED_PERCENT` = 95, `AT_MOST_TOLERANCE_PERCENT` = 110, one-whole-percent
//! rounding for `PROGRESS_PRECISION`) are this project's own choice of a reasonable tolerance
//! band, not a reverse-engineered port — same disposition as ADR-7/ADR-8's other "confirmed
//! mechanism, chosen exact numbers" decisions. Logged in `DECISIONS.md`.
//!
//! Progress is represented in whole percentage points (`u32`), not `f64`, specifically so
//! hand-computed expected values in tests are exact integers with no floating-point rounding
//! ambiguity at the band boundaries.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalKind {
    Simple,
    Daily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalCounter {
    WordCount,
    CharCount,
}

/// `deadline` is an ISO 8601 date string (e.g. `"2026-12-31"`), the same string convention
/// `filter::DateRange` already uses — this workspace has no `chrono` dependency anywhere else,
/// so this follows that existing precedent rather than introducing one for a single field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    pub kind: GoalKind,
    pub counter: GoalCounter,
    pub target: u32,
    pub deadline: Option<String>,
    pub write_on_deadline: bool,
}

/// This project's own tolerance-band thresholds (see module doc) — named after the four
/// confirmed-to-exist `ULGoalController*` constants; the values are chosen here, not ported.
pub const MINIMUM_PROGRESS_PERCENT: u32 = 2;
pub const ALMOST_REACHED_PERCENT: u32 = 95;
pub const AT_MOST_TOLERANCE_PERCENT: u32 = 110;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalProgress {
    /// Banded percent-of-target, clamped to `[MINIMUM_PROGRESS_PERCENT, AT_MOST_TOLERANCE_PERCENT]`
    /// and rounded to the nearest whole percent.
    pub percent: u32,
    /// At least `ALMOST_REACHED_PERCENT` of target, but not yet reached.
    pub almost_reached: bool,
    /// `current >= target`.
    pub reached: bool,
}

/// Computes tolerance-banded progress for `current` units against `target` — never a raw
/// `current / target` ratio (see module doc): never below `MINIMUM_PROGRESS_PERCENT` (so the
/// indicator isn't literally empty at zero words), never above `AT_MOST_TOLERANCE_PERCENT` (so
/// writing far past target doesn't blow out the display), rounded to the nearest whole percent.
/// `target == 0` (no target set) is treated as trivially reached, at the tolerance ceiling.
pub fn progress(current: u32, target: u32) -> GoalProgress {
    if target == 0 {
        return GoalProgress {
            percent: AT_MOST_TOLERANCE_PERCENT,
            almost_reached: false,
            reached: true,
        };
    }
    // Integer round-to-nearest-percent: (current * 100 + target / 2) / target.
    let raw_percent = ((u64::from(current) * 100) + u64::from(target) / 2) / u64::from(target);
    let raw_percent = u32::try_from(raw_percent).unwrap_or(u32::MAX);
    let reached = current >= target;
    GoalProgress {
        percent: raw_percent.clamp(MINIMUM_PROGRESS_PERCENT, AT_MOST_TOLERANCE_PERCENT),
        reached,
        almost_reached: raw_percent >= ALMOST_REACHED_PERCENT && !reached,
    }
}

/// A Daily goal tracks *this session's* delta against the running lifetime total (inventory
/// §12's confirmed `ULDailyGoalValueProviderCurrentSessionContext`), not the lifetime total
/// itself — it resets each session/day rather than accumulating forever like a Simple goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailySession {
    /// The running lifetime total (e.g. word count) at the moment this session started.
    started_at_count: u32,
}

impl DailySession {
    /// Starts a session anchored at the current lifetime total.
    pub fn start(current_total: u32) -> Self {
        Self {
            started_at_count: current_total,
        }
    }

    /// Units written since this session started.
    pub fn delta(&self, current_total: u32) -> u32 {
        current_total.saturating_sub(self.started_at_count)
    }

    /// Starts a fresh session anchored at the current total (e.g. a new day beginning) —
    /// the previous session's delta is gone, by design: that's what "resets daily" means.
    pub fn reset(&mut self, current_total: u32) {
        self.started_at_count = current_total;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_goal_progress_bands() {
        // Zero progress still shows the minimum-progress floor, not a literal 0%.
        assert_eq!(
            progress(0, 100),
            GoalProgress {
                percent: MINIMUM_PROGRESS_PERCENT,
                almost_reached: false,
                reached: false,
            }
        );
        // Just below the almost-reached threshold.
        assert_eq!(
            progress(94, 100),
            GoalProgress {
                percent: 94,
                almost_reached: false,
                reached: false,
            }
        );
        // Exactly at the almost-reached threshold, not yet reached.
        assert_eq!(
            progress(ALMOST_REACHED_PERCENT, 100),
            GoalProgress {
                percent: ALMOST_REACHED_PERCENT,
                almost_reached: true,
                reached: false,
            }
        );
        // Exactly at target: reached, no longer "almost."
        assert_eq!(
            progress(100, 100),
            GoalProgress {
                percent: 100,
                almost_reached: false,
                reached: true,
            }
        );
        // Far past target: capped at the tolerance ceiling, still reached.
        assert_eq!(
            progress(1_000, 100),
            GoalProgress {
                percent: AT_MOST_TOLERANCE_PERCENT,
                almost_reached: false,
                reached: true,
            }
        );
        // No target set: trivially reached at the tolerance ceiling.
        assert_eq!(
            progress(0, 0),
            GoalProgress {
                percent: AT_MOST_TOLERANCE_PERCENT,
                almost_reached: false,
                reached: true,
            }
        );
    }

    #[test]
    fn daily_goal_resets_session_delta() {
        let mut session = DailySession::start(500);
        assert_eq!(session.delta(500), 0, "no words written yet this session");
        assert_eq!(
            session.delta(650),
            150,
            "150 words written since session start"
        );

        // A new day begins: reset anchors the session at today's starting total.
        session.reset(650);
        assert_eq!(
            session.delta(650),
            0,
            "freshly reset session has zero delta"
        );
        assert_eq!(
            session.delta(700),
            50,
            "delta only counts writing after the reset"
        );
    }
}
