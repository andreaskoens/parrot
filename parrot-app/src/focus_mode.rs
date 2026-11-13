//! Focus Mode (T-10-09): per inventory §15's confirmed `ULFocusModeRenderingModifier`/
//! `ULFocusModeFadedAlpha`, dims every paragraph except the one containing the cursor.
//!
//! Only the *existence* of `ULFocusModeFadedAlpha` was confirmed (via `nm`-demangled symbols) —
//! not its numeric value, so `FADED_ALPHA` below is this project's own chosen value, not a
//! reverse-engineered one, same disposition as T-10-02's Goal tolerance-band constants.

/// Alpha applied to every paragraph except the cursor's (as the strength of a darkening overlay
/// painted on top of it — see `ui.rs::show_preview` — not a per-glyph color change).
pub const FADED_ALPHA: f32 = 0.55;
/// Alpha for the paragraph containing the cursor — fully visible, i.e. no overlay at all.
pub const FOCUSED_ALPHA: f32 = 0.0;

/// One alpha value per paragraph (parallel to `paragraph_starts`): `FOCUSED_ALPHA` for the
/// paragraph containing `cursor` (a byte offset into the whole sheet text), `FADED_ALPHA` for
/// every other paragraph. `paragraph_starts` must be each paragraph's byte start, in order.
pub fn paragraph_alphas(paragraph_starts: &[usize], cursor: usize) -> Vec<f32> {
    let active = active_paragraph_index(paragraph_starts, cursor);
    (0..paragraph_starts.len())
        .map(|i| {
            if Some(i) == active {
                FOCUSED_ALPHA
            } else {
                FADED_ALPHA
            }
        })
        .collect()
}

/// The index of the last paragraph whose start is at or before `cursor` — i.e. the paragraph
/// the cursor is currently inside (or just past the end of).
fn active_paragraph_index(paragraph_starts: &[usize], cursor: usize) -> Option<usize> {
    paragraph_starts.iter().rposition(|&start| start <= cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_follows_cursor_paragraph() {
        // Three paragraphs starting at these byte offsets in a fixed fixture.
        let starts = [0, 20, 50];

        assert_eq!(
            paragraph_alphas(&starts, 5),
            vec![FOCUSED_ALPHA, FADED_ALPHA, FADED_ALPHA],
            "cursor in the first paragraph"
        );
        assert_eq!(
            paragraph_alphas(&starts, 25),
            vec![FADED_ALPHA, FOCUSED_ALPHA, FADED_ALPHA],
            "cursor moved into the second paragraph"
        );
        assert_eq!(
            paragraph_alphas(&starts, 99),
            vec![FADED_ALPHA, FADED_ALPHA, FOCUSED_ALPHA],
            "cursor moved into the third (last) paragraph"
        );
    }

    #[test]
    fn cursor_before_first_paragraph_focuses_none() {
        // Degenerate case: no paragraph starts at or before the cursor (shouldn't happen with a
        // real document, since the first paragraph always starts at 0, but the function must
        // not panic and must not silently mark something as focused either).
        assert_eq!(
            paragraph_alphas(&[5, 10], 0),
            vec![FADED_ALPHA, FADED_ALPHA]
        );
    }

    #[test]
    fn empty_document_has_no_alphas() {
        assert_eq!(paragraph_alphas(&[], 0), Vec::<f32>::new());
    }
}
