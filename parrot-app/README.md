# `parrot-app` UI conventions

Parrot' real pixel-level spacing was never reverse-engineered (the source inventory's evidence
stops at structural/behavioral confirmation, not exact point values — see the roadmap's T-9-02),
so this project doesn't target it. These are **this rewrite's own** conventions, picked in T-9-02
so every screen converges on one scale instead of accreting ad-hoc numbers. T-9-03 brings existing
code into line with them; new UI work should follow them from the start.

## Tokens (M12)

As of M12 the visual target is ui-description §10 (screenshot-grounded), not this project's own
guesses. All colors, type sizes and spacing come from `src/tokens.rs`:

- **Colors:** use a `tokens::` constant; `scripts/check_no_hardcoded_colors.sh` (run by check.sh)
  rejects any literal `Color32::from_rgb*`/`from_gray` elsewhere.
- **Type:** `tokens::{HEADING_1..6, BODY, UI_LABEL, UI_LABEL_STRONG, UI_CAPTION, UI_SECTION,
  UI_HEADER, MONO, MONO_BLOCK}`, via `ui.rs`'s `text(..)` helper or `TypeToken::font_id()`.
- **Spacing:** 4px grid (`tokens::GRID`); panels pad 12×8; tree rows 22px; sheet rows 88px;
  selection radius 4; pills radius 10.
- **Logic stays out of `ui.rs`:** every rule with a spec value or a behavior to test lives in its
  own `egui`-light module (`layout`, `sidebar`, `sheet_list`, `editor`, `inspector`, `toolbar`,
  `export_modal`, `annotations`, `badges`); `ui.rs` paints and dispatches.

The sections below are the M9 conventions; they still apply where they don't conflict with the
tokens (the 8px base unit is superseded by the 4px grid, and the old top bar no longer exists).

## Spacing (M9)

- **Base unit: 8px.** Any explicit spacing constant in `ui.rs` (`add_space`, custom padding,
  etc.) should be a multiple of 8 — or exactly 4px for a deliberate "half-step" inside a tightly
  packed row, never an arbitrary value picked by eye.
- **Item spacing:** use `egui`'s default (`item_spacing = (8.0, 3.0)`) for ordinary
  `ui.horizontal`/`ui.vertical` rows — don't override it per-widget. Only `ui.add_space(_)` calls
  between *logically separate* blocks (e.g. one preview paragraph and the next) need an explicit
  value, and that value is 8px, not 4px or anything else.
- **Separators (`ui.separator()`):** used to divide a panel into named sub-sections. Each divider
  should be preceded by content and followed by content — never two in a row with nothing visible
  between them (a search box with zero results and an empty query produced exactly this before
  T-9-03; see `docs/ui_audit.md`).

## Headings vs. labels

- **Every named region gets a heading** (`ui.heading("…")`), not a plain `ui.label` standing in
  for one. "Named region" means: the library tree panel, the editor, the preview, the filter
  builder, and the welcome/first-run message. A plain `ui.label` is reserved for body text,
  status text, and inline row captions ("Title:", "Keyword:", "Search:") — never for something
  that's acting as a section title.
- The top bar is the one region with no heading, by design (it's a toolbar, not a named section)
  — don't add one there.

## Buttons

- **Primary, infrequent actions** (library-level: New Library, Open Library, Save, Set as
  Default) use `ui.button`, in the top bar.
- **Repeated, per-row or per-item actions** (tree-node add/duplicate/delete, filter/group
  add-child buttons) use `ui.small_button`, consistently — never a plain `ui.button` for a
  per-row action, which would make rows visually heavier than the list they're part of.
- **Button ordering within a row of the same kind must match across all node kinds.** Today a
  Sheet row's delete button is last; a Group row's is first — pick one order (delete last, after
  content, matching the Sheet row's convention since it's the more common node kind) and make
  Group match it.

## Icons

- Every icon is a [Phosphor](https://phosphoricons.com) glyph (regular weight, via the
  `egui-phosphor` crate, registered as a fallback font in `ui::setup`) and is referenced through
  a named constant in `src/icons.rs` — never an emoji or a raw Unicode symbol literal, which
  egui's bundled fonts and Segoe UI only partly cover. `scripts/check_icons.sh` enforces this;
  `icons::tests` asserts every constant renders in both the regular and bold families.
- One glyph per concept, used consistently everywhere that concept appears. In particular
  `icons::SEARCH` (magnifying glass) is the one "search/filter" glyph — saved filters, the
  search field and search results all use it.
- Add actions are the literal text `"+ <Noun>"` where they carry a label; a bare add button
  uses `icons::ADD`. Don't introduce a second add glyph.

## Interactive affordance

- If a list row represents something the user can open (a sheet, a search result pointing at
  one), it must be clickable/selectable (`ui.selectable_label` or a button), not a plain
  `ui.label` — a plain label on a row that looks like every other clickable row is a trap, not
  just a spacing nit.
