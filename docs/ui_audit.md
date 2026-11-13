# UI deviation list (T-9-02)

Every screen from `docs/ui_structural_audit.md` (T-9-01), checked against the spacing/typography
convention now documented in `parrot-app/README.md`. Each item below is **open** until T-9-03
fixes it and marks it resolved (`- [x]`, with the fix's commit noted). This file is append-only
history, not a living doc rewritten each milestone — see T-11-02/T-11-03 for the next audit pass
on top of this one.

## Library tree panel

- [x] **Search result rows use the wrong magnifying-glass glyph and aren't clickable.** Fixed:
  `SearchUiState::execute_with_paths` (new, `parrot_core::search::search_with_paths` underneath)
  returns each match's tree path; `ui.rs` now renders results as `selectable_label`s reading
  `🔍 {title}` that call `self.select(path)` on click. T-9-03.
- [x] **Three different separator outcomes for the same search box.** Fixed: the inner
  conditional separator is gone; a non-empty, zero-result query now shows a "No results." label
  instead of an unexplained gap, and exactly two separators (before/after the whole section)
  render in every state. T-9-03.
- [x] **Group row's delete button is first; Sheet row's is last.** Fixed: Group's `small_button`
  moved to the end of its `ui.horizontal`, matching Sheet's ordering. T-9-03.
- [x] **Filter builder's section title is a plain label, not a heading.** Fixed:
  `ui.label("New filter")` → `ui.heading("New filter")`. T-9-03.

## Top bar / style picker

- [ ] No deviations found against the documented convention — the top bar is explicitly the one
  heading-less region (by design, per README), and all its actions are plain `ui.button`s
  (correctly "primary/infrequent," per the Buttons rule). Nothing to fix here in T-9-03.

## Status bar

- [x] **Status text has no success/error color coding.** Fixed: status text wrapped in
  `egui::RichText`, tinted red when `self.status.starts_with("Error")` (relies on this file's
  existing "Error ...: {e}" message convention rather than adding new state). T-9-03.

## Single-sheet editor / preview

- [x] **The one stray spacing constant in the file.** Fixed: `ui.add_space(4.0)` →
  `ui.add_space(8.0)`. T-9-03.
- [x] **"Preview:" is a plain label, not a heading**, while "Editor" one line above is. Fixed:
  `ui.label("Preview:")` → `ui.heading("Preview")`. T-9-03.
- [ ] **Empty-selection state has no heading at all** (`ui.rs:352`, just a label) — the one
  central-panel state with neither "Editor" nor "Preview" framing. Low-priority (there's nothing
  to put a heading *on* when nothing's selected), but note it so it isn't mistaken for an
  oversight later — no fix planned, this is a deliberate exception.

## First-run flow

- [ ] No spacing/heading deviation (`ui.rs:334`, `ui.heading("Welcome to Parrot")` already
  matches the convention). The structural note from T-9-01 — that the top bar and tree panel
  still render during first-run instead of a focused flow — is a product/behavior question, not a
  spacing one; out of scope for T-9-03's layout-fix mandate, left as a T-9-01 observation only.

## Preferences

- Not applicable — T-9-01 already confirmed no preferences screen exists yet (only the top bar's
  "Set as Default" button). Nothing to audit against a spacing scale until that UI exists, which
  is feature work, not a T-9-03 fix. Carried forward as context for M10 if a preferences surface
  is ever added there.

---

**Open count at T-9-02 close: 7.** **All 7 fixed in T-9-03** (2026-10-30) — see each item above
and `DECISIONS.md`'s T-9-03 entry. The 2 explicitly no-fix items (empty-selection heading-less
state, first-run's shared chrome) remain logged as deliberate exceptions, not gaps. Open count
after T-9-03: **0**.

---

# T-11-02: M10 surfaces + cross-surface pass

Every new M10 screen (`docs/ui_structural_audit.md` §8-14, from T-11-01) checked against the
same `parrot-app/README.md` convention, plus the two specific cross-surface checks the roadmap
calls out by name: the sheet-flow view's integration with existing chrome, and Focus Mode's
interaction with multi-sheet scroll.

## Export dialog

- [x] **No separators between the dialog's sections.** Fixed: added a `ui.separator()` before
  the Destination-path row and before the Export/Cancel buttons, matching the other dialogs'
  section-per-separator structure. T-11-03.

## Goal setup/edit dialog

- [ ] No fix-worthy deviation. The three-button bottom row (Save/Remove Goal/Cancel) is more
  buttons than any other dialog has in one row, but it's not confusing in context (one primary
  action, one destructive, one dismiss) — noted as a one-off, not required to change.

## Keyword Manager

- [x] **"Assign" is disabled when nothing is selected; the Attachments dialog's "Add" buttons
  used to no-op silently instead.** Fixed: both of Attachments' "Add" buttons now use
  `add_enabled`, matching Keyword Manager's pattern (and renamed to "Add Note"/"Add File" at
  the same time — see that dialog's own entry below). T-11-03.

## Container/sheet-flow view — integration with existing chrome (ADR-7)

- [x] **The sheet-flow view's per-sheet editors had none of the single-sheet editor's writing
  aids.** Fixed: `show_sheet_flow` now runs `spellcheck::check`/`grammar::check` on every
  sheet's buffer and shows the same "Possibly misspelled"/grammar-message labels under each
  one, same colors as the single-sheet editor. T-11-03.

## Focus Mode — interaction with multi-sheet scroll (ADR-7/T-10-09)

- [x] **Focus Mode didn't exist in the sheet-flow view at all.** Design decision made in
  T-11-03: in this view, Focus Mode's unit is a whole *sheet*, not a paragraph (there's no
  separate preview per sheet to dim paragraph-by-paragraph the way the single-sheet view does)
  — checking the same "Focus Mode" toggle now fades every sheet's region except whichever one
  currently has keyboard focus (via each `TextEdit`'s own `response.has_focus()`), or dims
  nothing if none is focused yet (first frame, nothing clicked). Same overlay technique
  (`rect_filled` at `FADED_ALPHA`) as the single-sheet view, applied per-sheet-region instead of
  per-paragraph. T-11-03.

## Statistics panel

- [ ] No fix-worthy deviation. Its explanatory body-text line is the only one of its kind among
  the dialogs, but it's there because "session" needing an explanation is genuinely different
  from every other dialog's self-explanatory controls — not an inconsistency to resolve.

## Attachments dialog

- [x] **Icon collision: `📎` meant two different things in the same dialog.** Fixed: the File
  attachment list icon changed to `💾` (`📝` stays Note's icon); `📎` now means only "this row
  has no attachments yet" (the badge), everywhere. T-11-03.
- [x] **Both "Add" buttons shared one label.** Fixed: renamed to "Add Note" / "Add File" (and
  given the same `add_enabled` guard as the Keyword Manager's "Assign," see that dialog's own
  entry above). T-11-03.

---

**Open count after T-11-02: 5.** **All 5 fixed in T-11-03** (2026-10-30) — see each item above
and `DECISIONS.md`'s T-11-03 entry. The 2 explicitly no-fix items (Goal dialog's button count,
Statistics' body text) remain logged as deliberate exceptions, not gaps. Open count after
T-11-03: **0**.

# T-12-12: ui-description §11.1 gap table

Each row of the screenshot-grounded gap table, with its resolution. Structural evidence:
`docs/ui_structural_audit.md` §15–25.

- [x] **Panel 1 (Library)** — disclosure triangles (▸/▾, click toggles; no animation — egui panels
  animate, rows don't), PROJECTS/NOTES section headers, All/Last 7 Days/Open Files/Trash smart
  filters, goal rings, attachment badges, context menu. iCloud sync badges: **deferred
  permanently** — CloudKit sync is excluded (roadmap §2), so there is no sync state to show.
  T-12-03/T-12-04.
- [x] **Panel 2 (Sheet List)** — 2-line preview snippets, status indicators (goal ring, attachment,
  error dot), container title header. T-12-05.
- [x] **Panel 3 (Editor)** — centered 640px column with margins, paragraph numbering, word-count
  badge, styled typography (heading sizes, bold, italic, links, code; ULSS sizes when a style is
  active) instead of monospace. T-12-06.
- [x] **Panel 3 (Sheet-flow)** — #333 divider between sheets; aggregate word count. T-12-07.
- [x] **Panel 4 (Inspector)** — added, with Progress/Statistics/Keywords/Outline/Links/Media tabs.
  T-12-08.
- [x] **Spelling/Grammar** — colors were (200,140,20)/(140,100,200), now exactly #f5a623/#5ab0f6;
  inline underlines added in the same colors. T-12-11.
- [x] **Toolbar** — sidebar + layout toggles left; Markup, B/I/Link/¶, Export, Quick Export,
  inspector toggle right. Library/Save/Style actions moved to the menu bar (File/Style). T-12-09.
- [x] **Export modal** — format-picker sheet with preview replaces direct dispatch. T-12-10.
- [x] **Focus Mode** — reachable from View → Focus Mode; fades non-current paragraphs inside the
  styled editor (single sheet) or non-active sheets (flow), per T-10-09/T-11-03.
- [x] **Statistics** — the word-count badge and View → Statistics open the Inspector's Statistics tab
  (activity bars + per-session list).
- [x] **Goal UI** — rings in tree and sheet list; large ring in Inspector Progress; Add Goal… from the
  context menu, Sheet menu or Progress tab.
- [x] **Keywords UI** — chips + add field in the Inspector Keywords tab; Keyword Manager under Sheet.
- [x] **Theming** — the style picker (Style menu) now visibly changes editor typography through ULSS
  font sizes; the dark palette is applied to every egui widget via `tokens::apply_visuals`.

Open items against §11.1: **0** (one sub-item, iCloud badges, permanently deferred by the §2
exclusion).
