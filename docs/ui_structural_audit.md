# UI Structural Audit (T-9-01)

> **M12 note (T-12-12):** sections 1–14 describe `ui.rs` as it was through M11. M12 replaced that
> layout; the current surfaces are audited in **M12 surfaces (T-12-12)** at the end of this file.

**Why this exists instead of PNGs:** T-9-01 asks for `tests/visual/*.png` captures of every
existing screen, rendered offscreen. Two paths were tried first:

1. **Rasterize `egui`'s tessellated output directly** (tessellate shapes to triangle meshes via
   `egui::Context::tessellate`, rasterize with a software renderer such as `tiny-skia`). `tiny-skia`
   is reachable from this sandbox's crate registry, but it only fills vector paths/solid
   triangles — it has no built-in support for `egui`'s *textured* meshes (font-atlas glyph
   sampling, UV-mapped icons), so a correct result would mean hand-writing a textured-triangle
   rasterizer that re-samples `egui`'s font atlas per pixel. That's reimplementing a chunk of
   `eframe`'s own GPU backend in software, and the result still wouldn't be "what the real
   `eframe`/`glow` backend draws" — it'd be a second, independent renderer, which undermines the
   point of a visual capture (catching real rendering bugs) rather than serving it.
2. **The purpose-built tool**, `egui_kittest` (the `egui` project's own headless-testing/snapshot
   crate), supports exactly this via a software backend — but the version compatible with this
   workspace's pinned `egui`/`eframe` 0.29 doesn't exist on crates.io; the lowest available version
   (0.30.0) requires `egui`/`eframe` 0.30, which would mean bumping the GUI framework version for
   every crate that touches rendering (`render.rs`, `style_picker.rs`, `ui.rs`, the `cosmic-text`
   painter integration) — a cross-cutting migration far outside a "capture harness" task, and
   risky to do blind in a sandbox that already can't visually verify the result either way.

Both paths also run into the standing limitation logged since T-0-09/T-7-07: this sandbox has no
display server, so there is no way to confirm either rendering path's output actually matches a
real screen even if it compiled. Per T-9-01's own fallback clause, this document is the
**structural audit** in place of images: a plain, honest walk of every screen's `egui` layout code
and widget tree, by hand, with file:line references so T-9-02 (spacing/typography scale) and T-9-03
(fixes) have real material to work from. See `DECISIONS.md` (T-9-01 entry) for the short version.

All UI construction lives in a single file, `parrot-app/src/ui.rs` — `parrot-app`'s other modules
(`filters_ui.rs`, `search_ui.rs`, `preferences.rs`, `first_run.rs`, `tree_ops.rs`, `style_picker.rs`)
are pure state/logic with no `egui` calls of their own (by design, per ADR-3's
headless-testability rule). So "one screen" below usually means "one region of `ParrotApp::update`
in `ui.rs`," not a separate file or window — this app has one continuously-visible layout, not
modal screens.

---

## 1. Top bar (covers "style picker")

`ui.rs::update`, `TopBottomPanel::top("menu")`, lines 205–259.

- One `ui.horizontal` row, default item spacing (`egui::Spacing::default().item_spacing` =
  `(8.0, 3.0)`, never overridden anywhere in this file).
- Order: label "Library folder:" → single-line text edit (uses default-constructed
  `desired_width`, i.e. whatever `TextEdit` defaults to — not sized to its content, not capped) →
  "New Library" button → "Open Library" button → "Save" button → `ui.separator()` → label
  "Style:" → `egui::ComboBox` (id `"style_picker"`, selected text defaults to `"(none)"`) →
  "Set as Default" button.
- No heading widget on this bar (contrast with the tree panel's `ui.heading("Library")` and the
  editor's `ui.heading("Editor")` below) — it's the only top-level region with no heading at all.
- The Style `ComboBox`'s dropdown (`show_ui`) lists `self.style_picker.available_names()` as
  `ui.selectable_label` rows — no scrolling container, so this list grows unbounded with however
  many stylesheets `StylePicker` ever registers (today: 2, "Default"/"Compact" — fine now, a
  future audit should check this if the style count ever grows).

## 2. Library tree panel (covers "library tree," "filters UI," "search UI")

`ui.rs::update`, `SidePanel::left("tree")`, lines 261–311. Default `SidePanel` width (no
`.default_width()`/`.min_width()`/`.max_width()` set), so its width is whatever `egui` picks on
first run and then whatever the user drags it to — no app-defined minimum, so it can be dragged
narrow enough to wrap/clip the tree-row buttons below.

- `ui.heading("Library")` (line 262) — the one heading this panel has.
- `ui.horizontal`: three `small_button`s — "+ Group", "+ Sheet", "+ Filter" (lines 264–273).
  `small_button` uses a tighter padding than the top bar's plain `button`s one section away —
  first inconsistency candidate for T-9-02.
- **Filter builder** (`show_filter_builder` toggle, lines 275–293): when open, wrapped in
  `ui.group()` (draws its own frame/border — the only bordered region in the whole app, besides
  whatever `ComboBox`'s own popup frame draws). Contents: label "New filter" (plain `ui.label`,
  *not* a heading, unlike every other named sub-section) → horizontal "Title:" + text edit →
  horizontal "Keyword:" + text edit → "Save filter" button. The two text-edit rows use
  `ui.horizontal` with a label prefix, same pattern as the top bar's "Library folder:" row.
- `ui.separator()` (line 295).
- **Search** (lines 296–305): horizontal "Search:" + text edit, same label+field pattern again. If
  the query is non-empty, results render as plain `ui.label(format!("🔎 {title}"))` rows — not
  selectable/clickable (every other list in this app — the tree nodes below — uses
  `selectable_label`), so search results can't be clicked to open the sheet. That's a real
  usability gap, not just a spacing deviation; flagged for T-9-03's "outright layout/behavior bugs"
  category, not just the spacing pass. Followed by a conditional `ui.separator()` only when there
  are results.
- `ui.separator()` again (line 307) — so an empty search still draws one separator (305 is
  skipped, 307 always runs); a non-empty search with no results draws one separator (307) with no
  blank gap acknowledging "no results," a non-empty search with results draws two (305 then 307).
  Three different visual outcomes for what is conceptually the same "search box" region.
- **Tree nodes** (`show_node`, lines 106–180), recursively, one call per root child
  (`for i in 0..self.state.library.children.len()`, line 308):
  - **Sheet** row: `ui.horizontal` → `selectable_label("📄")` → inline-editable title
    (`text_edit_singleline`, so every sheet title is *always* an editable field, never plain text)
    → `small_button("⧉")` duplicate → `small_button("🗑")` delete.
  - **Group** row: `ui.horizontal` → `small_button("🗑")` **first**, then label "📁", then the
    title text edit — note the delete button is the *first* element on a Group row but the *last*
    element on a Sheet row (lines 114–126 vs. 130–140) — an inconsistent ordering, not just a
    spacing one. Then `ui.indent(...)` containing its own `ui.horizontal` of "+ Group"/"+ Sheet"
    small buttons, then recurses into children.
  - **Filter** row: `ui.horizontal` → label "🔍" → a plain (non-editable, non-selectable) label
    showing title + live match count → `small_button("🗑")`.
  - **Project**: a single static label, "(Project UI lands in a later milestone)" — i.e. Project
    nodes render structurally differently from every other node kind (no icon, no buttons, no
    children walk). Confirmed gap, not a bug — the roadmap's own M1 note defers Project UI.
  - Icons used across rows: 📄 (sheet), 📁 (group), 🔍 (filter), 🔎 (search result), ⧉ (duplicate),
    🗑 (delete), + (add, as literal text in "+ Group"/"+ Sheet"/"+ Filter", not an icon glyph) — six
    different glyph/text conventions for "add/remove/act on an item," never unified.

## 3. Status bar (no dedicated name in the roadmap's list — bottom chrome shared by every screen)

`ui.rs::update`, `TopBottomPanel::bottom("status")`, lines 313–330.

- Delete confirmation (`pending_delete`, lines 314–328): `ui.horizontal` → label "Delete this
  item? This cannot be undone." → "Confirm delete" button → "Cancel" button. This whole row only
  exists while a delete is pending, so the status bar's height changes between "idle" and
  "confirming a delete" — a layout jump every other panel doesn't have (the top bar, tree panel,
  and central panel are all stable-height by comparison, modulo content length).
- `ui.label(&self.status)` (line 329) always renders last/below the (conditional) confirmation
  row — plain, unstyled text; no color coding for error vs. success status strings even though
  `self.status` is set to messages like `"Error saving: {e}"` vs. `"Saved."` (see e.g. lines
  230–236) — all rendered identically regardless of outcome.

## 4. Central panel — first-run flow

`ui.rs::update`, `CentralPanel::default()`, lines 332–339 (the early-return branch).

- Only renders when `crate::first_run::should_prompt` is true (no library open yet): a heading
  "Welcome to Parrot" + one explanatory label, then `return`s out of the whole `CentralPanel`
  closure — so nothing else in the central panel (editor, preview) is even attempted while this is
  showing. Note the top bar and tree panel are **still fully rendered** during first-run (their
  code isn't gated on `should_prompt` at all) — so a first-time user sees a "New Library"/"Open
  Library" bar, a populated-looking (but actually empty) tree panel, and the welcome message all
  at once, rather than a focused first-run screen. Whether that's desirable is a product question,
  but it's worth T-9-02/T-9-03 noting since it's the one "screen" that was explicitly designed as
  its own flow (T-7-05) yet shares chrome with every other state.

## 5. Central panel — single-sheet editor

`ui.rs::update`, `CentralPanel::default()`, lines 340–353 (the normal branch, `self.selected`
is `Some`).

- `ui.heading("Editor")` → `egui::TextEdit::multiline` with `desired_width(f32::INFINITY)` (the
  only widget in the whole app with an explicit width directive — every other text field uses the
  unset default) and no `desired_rows`/height directive, so its height is whatever egui's default
  multiline sizing picks, independent of how much vertical room the preview below it will also
  need → `ui.separator()` → label "Preview:" (plain label, not a heading — inconsistent with
  "Editor" one line above being a heading for what is arguably the same importance level) →
  `self.show_preview(ui)`.
- **Preview** (`show_preview`, lines 186–200): for each paragraph, paints via
  `crate::render::paint_paragraph` at the panel's current `available_width`, allocates a rect sized
  to the painted height, then `ui.add_space(4.0)` between paragraphs (line 198) — the **one**
  explicit numeric spacing constant anywhere in `ui.rs`, and it doesn't match any other spacing
  value in the file (the default item spacing used everywhere else is `3.0`–`8.0` depending on
  axis). First concrete candidate for T-9-02's "pick one scale" pass.
- When nothing is selected (lines 351–353): a single label, "Select or create a sheet from the
  library panel on the left." — no heading at all for this state, unlike every other named region.

## 6. Preferences — **does not exist as a distinct screen**

Confirmed gap, logged here rather than silently assumed away: `parrot-app/src/preferences.rs`
defines `body_font`, `serif_font`, `monospace_font`, and `default_style_name`, and `ui.rs` loads/
saves the file (`load_preferences`/`save_preferences`, lines 65–82) — but **no widget anywhere in
`ui.rs` lets a user view or edit `body_font`/`serif_font`/`monospace_font`**. The only
preferences-related UI is the top bar's "Set as Default" button (line 255), which persists the
*already-selected* style name — it never touches the font fields at all. T-7-04's own Definition
of Done only required an `AppState`-level test proving the struct round-trips and applies
correctly, which it does; a font-editing UI was never actually built. This is a missing-feature
gap, not a layout/spacing bug in an existing screen, so per T-9-03's own scope ("fix every
deviation... and any outright layout bugs") it is **not** fixed as part of M9 — building a new
preferences dialog is feature work, which belongs with M9's bracketing purpose (give M10's new
surfaces a sane foundation) rather than inside it. Logged in `DECISIONS.md` as an open, scoped-out
item.

## 7. First-run flow

See §4 above (same code path; listed separately here only because the roadmap's screen list names
it independently of "single-sheet editor").

---

# M10 surfaces (T-11-01)

Same fallback, same reasoning as the header above — still no offscreen-rendering path in this
sandbox, still no display server to confirm one against anyway (nothing changed since T-9-01).
This extends the structural walk to every new screen M10 added. T-11-02 does the
convention-deviation pass on these, the same way T-9-02 did for §1–7 above.

## 8. Export dialog

`ui.rs`, `egui::Window::new("Export")` (around line 484), opened by the top bar's "Export…"
button (line 451). Contents in order: `ui.heading("Export")` → a horizontal "Format:" row with
an `egui::ComboBox` listing all 8 `ExportFormat::ALL` entries as `selectable_label`s → a
horizontal "Destination path:" row with a `text_edit_singleline` → a horizontal row with
"Export" and "Cancel" buttons. No separators between these three rows (unlike the Filter
builder group in §2, which does separate its fields). Clicking "Export" calls
`export_selected` (which also applies any pending editor-buffer edits first, same as the main
"Save" button) and closes the dialog unconditionally — there is no inline validation/error
state in the dialog itself; a failure surfaces only as a status-bar message after the dialog
has already closed.

## 9. Goal setup/edit dialog

`ui.rs`, `egui::Window::new("Goal")` (around line 526), opened by a 🎯 `small_button` on every
Sheet/Group row (§2's tree rows). Contents: `ui.heading("Goal")` → "Type:" combo (Simple/Daily)
→ "Counts:" combo (Words/Characters) → "Target:" text field → "Deadline (YYYY-MM-DD,
optional):" text field → a `ui.checkbox` for "Lock sheet on deadline" (no horizontal wrapper,
unlike every text-field row above it, which are all `ui.horizontal` label+field pairs) → a row
of three buttons: "Save", "Remove Goal", "Cancel". Three buttons in one row is more than any
other dialog in this app has (Export/Keywords/Statistics/Attachments all cap at two per row).

## 10. Keyword Manager

`ui.rs`, `egui::Window::new("Keywords")` (around line 607), opened by the top bar's
"Keywords…" button (line 454). Contents: `ui.heading("Keywords")` → one row per keyword
(`ui.label(&keyword)` + ✎ rename + 🗑 delete `small_button`s) → a conditional rename row
(only while a rename is in progress) → a separator → "Assign to the selected sheet:" label →
a text field + an "Assign" button that's disabled (`add_enabled(has_selection, ...)`) when
nothing is selected — the only disabled-rather-than-hidden control in the whole app — → the
selected sheet's current tags as removable chips in a `ui.horizontal_wrapped` (label + "✕" per
chip, one `small_button` each, not a separate remove icon) → a separator → "Close".

## 11. Continuous sheet-flow / container view

Already documented in full in `DECISIONS.md`'s T-10-06 entry and exercised by
`parrot-app::container_view`'s own tests; summarized here for this audit's own completeness.
`ui.rs`'s central panel, when the selection is a Group/Project/Filter rather than a Sheet:
`ui.heading("Sheets")` → `show_sheet_flow`, an `egui::ScrollArea::vertical()` containing one
`ui.heading(&title)` + `TextEdit::multiline` pair per child sheet, each pair separated by
`ui.separator()` (no separator after the last one). An empty container shows a single label,
"This container has no sheets yet.", instead of an empty scroll region.

## 12. Focus Mode overlay

Not a separate window — a `ui.checkbox(&mut self.focus_mode, "Focus Mode")` next to the
"Editor" heading (§5's single-sheet editor, same horizontal row as the heading itself, the only
heading in this app sharing a row with another control). When on, `show_preview` paints a
`rect_filled` translucent overlay (`egui::Color32::from_black_alpha(...)`, see T-10-09's
`FADED_ALPHA`) on top of every already-painted paragraph except the one containing the text
cursor — the overlay is drawn after that paragraph's own content, in the same loop iteration,
not as a separate pass over the whole preview.

## 13. Statistics panel

`ui.rs`, `egui::Window::new("Statistics")` (around line 688), opened by the top bar's
"Statistics…" button (line 457). Contents: `ui.heading("Statistics")` → a "Total words in
library: N" label → a separator → an explanatory label (the only dialog in this app with
explanatory body text rather than just control labels) → one label per `ActivityPoint` in the
series → a separator → "Start New Session" and "Close" buttons in one row.

## 14. Attachments dialog

`ui.rs`, `egui::Window::new("Attachments")` (around line 729), opened by the 📎/📌
`small_button` on every Sheet row (§2's tree rows — note this reuses the same button-as-badge
pattern the Goal 🎯 button established, rather than a separate label + a different button).
Contents: `ui.heading("Attachments")` → one row per existing attachment (📝/📎 + its
text/file_name + a 🗑 `small_button`) → a separator → a "Note:" field + "Add" button → a
"File name:" field + "Add" button → a separator → "Close". The two "Add" buttons are both
labeled identically ("Add") rather than "Add Note"/"Add File" — distinguished only by the
preceding field's label, unlike the Goal dialog's differently-labeled buttons in its own
bottom row.

---

## Summary table (screen → artifact)

| Roadmap screen name | Artifact | Location |
|---|---|---|
| library tree | §2 above | this file |
| single-sheet editor | §5 above | this file |
| filters UI | §2 above (filter builder) | this file |
| search UI | §2 above (search) | this file |
| preferences | §6 above — **gap noted, no screen exists yet** | this file |
| style picker | §1 above | this file |
| first-run flow | §4 above | this file |
| export dialog | §8 above | this file |
| goal setup/edit flow | §9 above | this file |
| keyword manager | §10 above | this file |
| container/sheet-flow view | §11 above | this file |
| focus mode overlay | §12 above | this file |
| statistics panel | §13 above | this file |
| attachments UI | §14 above | this file |

No `tests/visual/*.png` files were produced (see the two attempted approaches above); this
document, together with `DECISIONS.md`'s T-9-01/T-11-01 entries, is the fallback artifact both
tasks' own Done-when criteria explicitly allow.

---

# M12 surfaces (T-12-12)

Same fallback as T-9-01 — no offscreen capture, and the owner's standing instruction is to verify
through lint/typecheck/tests only, never by looking at a running app. Two things are stronger than
in M9/M11: every surface below has a headless logic test against ui-description's spec values, and
`ui::tests::full_frames_render_headlessly` runs complete egui frames of the real panel functions
(every inspector tab, flow + single-sheet mode, every export format, every dialog, Trash) with real
system fonts, so a panic or a bad font-family reference on any surface fails the suite. Line
references are `parrot-app/src/ui.rs` unless noted.

## 15. Four-panel layout
`update` (2494): menu bar, toolbar, status bar, then `SidePanel::left("library")` 160px (BG_PANEL),
`SidePanel::left("sheet_list")` 280px (BG_MID), `SidePanel::right("inspector")` 220px (BG_PANEL),
`CentralPanel` (BG_DEEP). Visibility comes from `layout::PanelLayout::is_visible`; panels use
`show_animated`. Tests: `layout::tests::*`.

## 16. Library sidebar smart filters
`library_panel` (933) iterates `sidebar::root_rows`: All / Last 7 Days / Open Files / Trash, then
`PROJECTS` (only when a root Project exists) and `NOTES` section labels (UI_SECTION, quaternary, no
selection, 12px gap above), then the tree. `tree_row` (143): 22px rows, 16px indent, ▸/▾ triangle
only on containers, accent fill + white text when selected, #2e2e2e hover, truncated label.
Tests: `sidebar::tests::*`.

## 17. Sidebar context menu
`tree_node` (1006): right-click lists `ContextAction::for_node` (New Group, New Sheet, New Filter,
Rename, Duplicate, Move to Trash, Export…, Add Goal…, Add/Edit Keywords…, Get Info), dispatched via
`run_context_action` → `sidebar::apply_context_action`. Double-click renames inline. The Trash row
has "Empty Trash…"; trash rows have "Put Back". Sheet-list rows share the same menu.

## 18. Goal rings and badges
`badges::paint_badges`/`paint_goal_ring` (badges.rs): 14px ring, #555 base, clockwise arc from
12 o'clock in tertiary/#f0a500/#4caf50 by band; 📎 when attachments exist; red dot for text-checker
findings (sheet list only). Drawn right-aligned in `tree_row` and `sheet_row`. Tests: `badges::tests::*`.

## 19. Sheet list rows
`sheet_list_panel` (1095): bold 14px container header (click → back to sheet flow), search box,
then `sheet_row` (206): 88px rows, semibold 13px title, 2-row 11px #888 preview with ellipsis,
badges bottom-right, #333 bottom rule; "Empty Sheet" footer only for an empty container.
Tests: `sheet_list::tests::*`.

## 20. Editor text column
`single_sheet` (1357) → `text_column` (2406) + `sheet_editor` (2426): centered inset (#222) column,
640px max text width, 80px side margins, 40px vertical padding; paragraph numbers 40px left of the
text in quaternary 11px; word-count pill top-right (`word_count_strip`, 1229; ● when findings
exist; click → Statistics). Styled markup via `editor::layout_job`. Tests: `editor::tests::*`.

## 21. Sheet-flow dividers
`sheet_flow` (1406): one inset column holding every sheet, iterating `container_view::flow_items`;
`FlowItem::Divider` is a 1px #333 rule spanning the column including margins, 24px space either
side; sheet titles as quaternary captions; aggregate word-count pill.
Tests: `container_view::tests::{divider_count_matches_sheet_count_minus_one, aggregate_word_count_sums_all_sheets}`.

## 22. Inspector panel
`inspector_panel` (1516): 16px icon tab bar (Progress, Statistics, Keywords, Outline, Links,
Media), uppercase section label, scrolling tab body (`progress_tab` 1552 … `media_tab` 1797).
Tests: `inspector::tests::*`.

## 23. Toolbar
`toolbar` (879): left ◧ sidebar, ▭ editor-only; right ✎ Markup, **B**, *I*, 🔗, ¶, ⬆ Export,
⚡ Quick Export, ◨ inspector. Toggles render selected while active; formatting buttons disabled
until an editor has had focus. No "Subscribe Now". Tests: `toolbar::tests::*`,
`ui::tests::toolbar_format_round_trips_through_text_edit_state`.

## 24. Quick Export modal
`export_window` (1886): anchored centered `Window`; top row ☀/☾ theme, format combo (8 formats),
accent Export button; "⚙ Style · A4" clickable labels; "Save to" path; live preview on #252525 —
paged formats as page-proportioned sheets (white or dark), text formats as one long sheet.
Tests: `export_modal::tests::*`.

## 25. Annotation styling
`annotation_lines` (2475) below each sheet, colored by `annotations::annotation_color`
(#f5a623 spelling, #5ab0f6 grammar), capped at 12 lines; the same colors underline the flagged
ranges inline (`editor::layout_job`). Tests: `annotations::tests::*`.

## M12 summary table

| Surface | Artifact |
|---|---|
| four-panel layout | §15 + `layout::tests` |
| library sidebar smart filters | §16 + `sidebar::tests` |
| sidebar context menu | §17 + `sidebar::tests::context_menu_action_dispatches_to_existing_handler` |
| goal rings and badges | §18 + `badges::tests` |
| sheet list rows | §19 + `sheet_list::tests` |
| editor text column | §20 + `editor::tests` |
| sheet-flow dividers | §21 + `container_view::tests` |
| inspector panel | §22 + `inspector::tests` |
| toolbar | §23 + `toolbar::tests` |
| quick export modal | §24 + `export_modal::tests` |
| annotation styling | §25 + `annotations::tests` |
