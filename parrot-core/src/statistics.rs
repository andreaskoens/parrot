//! Writing activity statistics (T-10-10): per inventory §15's confirmed
//! `ULActivityTrackerStatistic`, tracks writing activity *over time* — a session-by-session
//! series of how much was written each session — not just a point-in-time word count.
//!
//! `day` is an opaque, caller-supplied ordinal (a session/day counter), not a real calendar
//! date — this workspace has no `chrono` dependency anywhere else (same precedent as
//! `Goal::deadline`'s plain `Option<String>`), and a real calendar is `parrot-app`'s concern
//! (deciding when a "new day" has actually begun), not this module's.

use crate::ast::SheetContent;

/// One session's writing activity: how many units (words, or whatever the caller is counting)
/// were written during that session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityPoint {
    pub day: u32,
    pub written: u32,
}

/// Builds an [`ActivityPoint`] series across sessions. Each session starts at some running
/// total (e.g. a sheet's current word count) and [`record`](Self::record) is called as that
/// total changes during the session; the point for `day` always reflects the *delta* since
/// that session's `start_session` call, not the raw running total.
#[derive(Debug, Clone, Default)]
pub struct ActivityTracker {
    points: Vec<ActivityPoint>,
    session_start_count: Option<u32>,
    current_day: u32,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new session for `day`, anchored at `current_total`. Calling this again for the
    /// same `day` (e.g. the app restarting mid-session) re-anchors that day's point rather than
    /// starting a duplicate one — `record` below already merges same-day points, but this keeps
    /// the anchor itself correct too.
    pub fn start_session(&mut self, day: u32, current_total: u32) {
        self.current_day = day;
        self.session_start_count = Some(current_total);
    }

    /// Records the current running total for the active session, updating (not appending to)
    /// that session's point if one already exists for `current_day`.
    pub fn record(&mut self, current_total: u32) {
        let Some(start) = self.session_start_count else {
            return;
        };
        let written = current_total.saturating_sub(start);
        if let Some(last) = self.points.last_mut() {
            if last.day == self.current_day {
                last.written = written;
                return;
            }
        }
        self.points.push(ActivityPoint {
            day: self.current_day,
            written,
        });
    }

    /// The activity series so far, in session order.
    pub fn series(&self) -> &[ActivityPoint] {
        &self.points
    }
}

/// Words in `content`'s plain-text projection (whitespace-delimited, ui-description §6.1) —
/// the one count every word-count surface (editor badge, inspector, library total) shares.
pub fn word_count(content: &SheetContent) -> u32 {
    u32::try_from(content.plain_text().split_whitespace().count()).unwrap_or(u32::MAX)
}

/// Characters (Unicode scalar values) in `content`'s plain-text projection, §6.1.
pub fn char_count(content: &SheetContent) -> u32 {
    u32::try_from(content.plain_text().chars().count()).unwrap_or(u32::MAX)
}

/// Reading-speed assumption for [`reading_time_seconds`], per §6.1's "standard ~200 wpm".
pub const READING_WORDS_PER_MINUTE: u32 = 200;

/// Estimated reading time for `words` words, rounded to the nearest second.
pub fn reading_time_seconds(words: u32) -> u32 {
    (words * 60 + READING_WORDS_PER_MINUTE / 2) / READING_WORDS_PER_MINUTE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Inline, Paragraph};

    #[test]
    fn counts_and_reading_time() {
        let content = SheetContent::new(vec![
            Paragraph {
                tag: None,
                inlines: vec![Inline::Text("two words".to_string())],
            },
            Paragraph {
                tag: None,
                inlines: vec![Inline::Strong(vec![Inline::Text("three".to_string())])],
            },
        ]);
        assert_eq!(word_count(&content), 3);
        assert_eq!(
            char_count(&content),
            "two words three".chars().count() as u32
        );
        assert_eq!(reading_time_seconds(200), 60);
        assert_eq!(reading_time_seconds(357), 107);
    }

    #[test]
    fn activity_series_across_sessions() {
        let mut tracker = ActivityTracker::new();

        // Day 1: starts at 0 words, writes up to 120 over the session.
        tracker.start_session(1, 0);
        tracker.record(50);
        tracker.record(120);

        // Day 2: a new session anchored at yesterday's running total (120), writes up to 180.
        tracker.start_session(2, 120);
        tracker.record(180);

        // Day 3: anchored at 180, no further writing that session.
        tracker.start_session(3, 180);
        tracker.record(180);

        assert_eq!(
            tracker.series(),
            &[
                ActivityPoint {
                    day: 1,
                    written: 120
                },
                ActivityPoint {
                    day: 2,
                    written: 60
                },
                ActivityPoint { day: 3, written: 0 },
            ],
            "the series must reflect each session's own delta, not the raw running total"
        );
    }

    #[test]
    fn record_without_a_started_session_is_a_no_op() {
        let mut tracker = ActivityTracker::new();
        tracker.record(100);
        assert_eq!(tracker.series(), &[]);
    }

    #[test]
    fn re_recording_the_same_day_updates_in_place_not_appends() {
        let mut tracker = ActivityTracker::new();
        tracker.start_session(1, 0);
        tracker.record(10);
        tracker.record(20);
        tracker.record(30);
        assert_eq!(tracker.series().len(), 1);
        assert_eq!(tracker.series()[0].written, 30);
    }
}
