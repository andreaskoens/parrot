# Manual Smoke Test

A hands-on checklist for the real GUI, on top of the automated suite. Rewritten at T-12-12 for the
M12 four-panel layout (ui-description §2); organized by surface. Every item is already
logic-verified by the automated suite, and `ui::tests::full_frames_render_headlessly` renders every
surface without panicking. This list covers what only a person looking at the running app can
confirm: layout, proportions, live feedback, real files on disk.

**Not yet executed.** No one has clicked through the app yet. Run this on real Windows hardware
(roadmap §0) and tick each box as it's confirmed.

## 1. First run & library lifecycle
- [ ] On first launch the editor area shows "Welcome to Parrot" with a folder field, a Browse…
      button (opens the Windows folder picker and fills the field) and New Library / Open Library
      buttons (disabled while the field is empty).
- [ ] New Library creates the folder; the status bar confirms it and shows the path on the right.
- [ ] File → New Library… / Open Library… open the folder picker and act on the chosen folder;
      cancelling does nothing. File → Library by Path… still accepts a typed path. File → Save and
      Ctrl+S save it. New Library on a folder that already holds a library is refused.
- [ ] Close and relaunch, then reopen the same folder: groups, sheets and content are intact.

## 2. Layout & toolbar
- [ ] Four panels left to right: Library (~160px, #1e1e1e), Sheet List (~280px, #252525), Editor
      (fills, #1a1a1a), Inspector (~220px, #1e1e1e). Panel edges can be dragged.
- [ ] ◧ hides/shows Library + Sheet List together; ◨ hides/shows only the Inspector; ▭ shows the
      editor alone and toggles back to the previous layout.
- [ ] The window toolbar holds only ◧ ▭ (left) and ◨ (right). ⬆ Export and ⚡ Quick Export sit at
      the top-left of the sheet area; ✎ Show Markdown, B, I, 🔗, ¶ sit centered in the editor's
      bottom bar.
      Hovering shows tooltips. B/I/🔗/¶ are disabled until you've clicked into a sheet.
- [ ] Glyphs render (no tofu boxes) with Segoe UI as the UI font.

## 3. Library sidebar
- [ ] Top rows: Library, All, Last 7 Days, Trash, then PROJECTS (and FILTERS when a root filter
      exists) as dimmed, unclickable headings. No NOTES heading.
- [ ] Opening the app or a library shows the shelf: one card per project (title, sheets, drafts,
      words, goal ring) plus a New Project card, full width. Clicking a card opens the project;
      the Library row returns to the shelf.
- [ ] New Project (shelf card, "+" on PROJECTS, File menu) creates a project and starts renaming it.
      Right-clicking a card offers Rename, Duplicate, Move to Trash, Add Goal, Get Info.
- [ ] Sheets and sections can't be created, dragged or restored outside a project; the status bar
      says why. An older library with sheets at its root opens with them in an "Unfiled" project.
- [ ] Right-click a sheet → Mark as Draft: a dashed-file badge appears on its row and card;
      Unmark as Draft removes it; it survives save and reopen.
- [ ] Rows are ~22px; selection is a blue rounded fill with white text; hover is a subtle gray.
- [ ] ▸/▾ collapses and expands groups; long titles end in "…".
- [ ] The "+" right of the PROJECTS heading creates a project, or a section/sheet/filter in the
      current project (disabled with no project selected).
- [ ] Right-click a node: New Section, New Sheet, New Filter, Rename, Duplicate, Move to Trash,
      Export…, Add Goal…, Add/Edit Keywords…, Get Info. Each one does what it says.
- [ ] Double-clicking a node renames it inline; Enter, ✓ or clicking away commits, Esc cancels; a
      blank name keeps the old one.
- [ ] Move to Trash removes the node; Trash shows it with a count; right-click → Put Back restores
      it at the library root; Empty Trash… asks for confirmation in the status bar.
- [ ] Known deviation: Last 7 Days lists the same sheets as All (no per-sheet dates yet)
      (DECISIONS.md, T-12-03).
- [ ] Ctrl-click adds/removes nodes, Shift-click selects the visible range from the last click,
      Ctrl+Shift-click adds that range. With two or more groups selected the header reads "N items
      selected" and the editor shows every sheet inside them (nested groups included), in order.
- [ ] Drag a node: a blue line above/below a row means "before/after", a blue box around a group
      means "into it". Dropping moves it (a whole selection moves together); dropping a group into
      itself does nothing. Save, reopen the library: the new order is kept.

## 4. Sheet list
- [ ] Click opens one sheet; Ctrl-click, Shift-click and Ctrl+Shift-click select several (as in the
      sidebar); the editor then shows the selected sheets one after another. Clicking the header
      returns to the whole container.
- [ ] Opening a sheet that lives in a group (from All, search, or the list) expands that group in
      the sidebar, scrolls to the sheet and highlights it there; the group or filter being listed
      keeps a dimmer blue highlight.
- [ ] Drag sheets (one, or the selection) above/below another row to reorder, or onto a group in
      the sidebar to move them there.
- [ ] Bold container name at the top; clicking it from an open sheet returns to the sheet flow.
- [ ] Each row: semibold title, up to two gray preview lines ending in "…", badges bottom-right.
- [ ] An empty group shows "Empty Sheet" at the bottom of the list.
- [ ] The search box lists matching sheets across the library; clearing it restores the list.

## 5. Editor (single sheet)
- [ ] Clicking a sheet row opens it: a centered column (≤640px) on a slightly lighter band, with
      wide side margins on a wide window.
- [ ] Headings are large and bold; `**bold**`, `*italic*`, `` `code` ``, `[links](url)` show as
      formatted text with no Markdown markers visible (list bullets stay).
- [ ] Paragraph numbers sit in the left margin and stay sequential as you add/remove paragraphs
      (paragraphs are separated by a blank line).
- [ ] The "N Words" pill bottom-right updates live; clicking it opens Statistics & Goals. A ⚠ beside
      it appears with spelling/grammar findings and opens Spelling & Grammar.
- [ ] Spelling findings are orange, grammar findings blue: underlined in the text and listed in the
      Inspector's Spelling & Grammar tab (per sheet when several are shown), not below the text.
      Clicking a finding scrolls the editor to it and selects it; with checking off the tab offers
      the "Check Spelling and Grammar" toggle.
- [ ] ✎ Show Markdown on reveals the markers (dimmed); off hides them again.
- [ ] Select a word and press B / I / Ctrl+B / Ctrl+I: it's wrapped in ** / *; pressing again unwraps.
- [ ] 🔗 asks for an address and wraps the selection as `[text](url)`; ¶ switches the paragraph
      under the cursor between heading levels, quote, lists, code, comment, body.
- [ ] Style menu → Compact visibly changes heading sizes; Set as Default persists across restarts.
- [ ] View → Focus Mode dims every paragraph except the one with the cursor.
- [ ] Ctrl+↓ / Ctrl+↑ moves to the next/previous sheet; Ctrl+N creates a sheet at the root.

## 6. Sheet flow
- [ ] Selecting a group shows all its sheets stacked in one scrolling column, each preceded by
      its title, separated by thin #333 rules (none before the first or after the last).
- [ ] The pill shows the combined word count; typing in any sheet updates it and saves to that sheet.
- [ ] With Focus Mode on, every sheet except the one being edited is dimmed.

## 7. Inspector
- [ ] Statistics & Goals (one tab): characters, words, reading time ("1 min 29 sec" style); GOAL
      with a large ring and Edit Goal, else "Add Goal…"; SESSIONS with the library word total,
      per-session bars and list, Start New Session.
- [ ] Keywords: chips (click ✕ to remove), field + "+" (or Enter) to add, Keyword Manager… opens the
      rename/delete window.
- [ ] Outline: headings indented by level; clicking one scrolls the editor to it.
- [ ] Links: each link's label and address; clicking copies the address (status bar confirms).
- [ ] Media: images/videos in the text and attachments; add a note or file attachment; 🗑 removes
      it; the 📎 badge appears/disappears on the sheet's rows accordingly.

## 8. Goals
- [ ] Add Goal… opens the goal form inside Statistics & Goals (no dialog); typing in its fields
      works while a sidebar rename is pending.
- [ ] Set a word target → Save → a ring appears next to the node in the sidebar and sheet list,
      filling clockwise as you write: gray, then orange near the target, green when reached.
- [ ] Remove Goal removes the ring.

## 9. Export
- [ ] ⬆ Export opens a dialog: options left (Markdown | PDF, page size for PDF, light/dark preview
      page, "Save to" + Browse…, blue Export), live preview right.
- [ ] PDF preview shows the PDF's own pages to scale (wrapped lines, larger bold headings); A5 makes
      more pages than A4. Markdown shows the exact file text. Both update as you type.
- [ ] The exported PDF opens with all text visible (regression: it used to be blank).
- [ ] Export writes the file at the path; open it in its native app (PDF reader, Word, browser,
      e-book reader) and check the content.
- [ ] ⚡ Quick Export after one export writes `<same folder>/<sheet title>.<same format>` without a
      modal; before any export it opens the modal instead.
- [ ] Browse… next to "Save to" opens the Windows Save As dialog in the destination's folder with
      its file name; the chosen path keeps the format's extension.
- [ ] After an export the status bar offers "Show in Folder", and File → Show Last Export in
      Folder does the same: Explorer opens with the file selected (also for paths with spaces).

## 10. Filters
- [ ] Sheet → New Filter… (or +/context menu): title, text-contains, keyword. Saved filters show in
      the sidebar with 🔍; selecting one lists matching sheets and shows them as a sheet flow.

## 11. Import & attachments
- [ ] File → Import… opens a file picker filtered to md/txt/html/rtf/docx/enex; each file becomes a
      sheet titled after the file, beside/inside the selected library node (an `.enex` with several
      notes becomes "<name> 1", "<name> 2", …). File → Import TextBundle… takes a `.textbundle` folder.
- [ ] Inspector → Media → Add File… opens a file picker and attaches the chosen file; the row's
      open button launches it in its default app; a deleted file reports an error instead.

## 11b. Character Counter
- [ ] The Inspector's last tab (Aa) is the Character Counter: a "Counting …" line, live "N
      characters, M words.", Counts / Syllables & Length / Readability / Time groups, platform limits
      (rows turn red past the limit), and the Top Frequency table (Character / 1 / 2 / 3 Words tabs,
      Top 10–100, Exclude grammar words).
- [ ] With nothing selected in the editor it counts every sheet the editor shows ("Counting 3 open
      sheets."); select some text and it counts only that ("Counting the selected text.").

## 12. Windows packaging
- [ ] `scripts/package_windows.sh` on a machine that can link `x86_64-pc-windows-msvc` produces a
      portable zip whose `parrot-app.exe` launches with the app icon and behaves like the dev build.
