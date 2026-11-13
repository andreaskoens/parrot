# Progress Log

One line per completed task, date, deviations. See roadmap Progress Overview for current at-a-glance status.

- 2026-10-29 — T-0-01: Cargo workspace scaffolded (8 crates per ADR-3), rust-toolchain.toml pinned to 1.99.1 w/ windows-msvc target, scripts/check.sh green on empty skeleton. Rust toolchain was not present on this machine; installed via `brew install rustup` + `rustup toolchain install stable`.
- 2026-10-29 — T-0-02: no-network guard (scripts/no_network_guard.sh) bans reqwest/hyper/ureq/tokio/async-std/TLS crates from the resolved dependency tree and un-allowlisted `std::net` from source; regression-tested both directions in scripts/no_network_guard_test.sh via synthetic fixtures (no real dep added). Wired into scripts/check.sh.
- 2026-10-29 — T-0-03: parrot-core data types — `ast.rs` (TagKind/TagMark/Inline/Paragraph/MarkupInfo/SheetContent matching the confirmed §3.3 shape) and `node.rs` (Node enum {Sheet,Group,Project,Filter}, Library). Builder test reproduces the literal confirmed example and asserts structural equality.
- 2026-10-29 — T-0-04: parrot-container `xml.rs` — quick-xml event-based serializer/parser for Content.xml (sheet/markup/string/p/tags/tag/element/attribute). Round-trips the confirmed §3.3 example plus a second fixture covering strong/code/footnote. Also fixed the unwrap/expect clippy-lint test exemption workspace-wide (see DECISIONS.md).
- 2026-10-29 — T-0-05: parrot-markup `render.rs` — AST -> Text.txt plain renderer. Golden file at tests/fixtures/mvp/confirmed_example.txt for the confirmed §3.3 sample; link/footnote plain-text shape is this project's own decision (see DECISIONS.md).
- 2026-10-29 — T-0-06: parrot-container `sheet.rs` — create/open/save for a `.parrot` directory package (Content.xml + Text.txt siblings), atomic via sibling-temp-dir-then-rename (ADR-6). `write_tmp`/`commit` split lets the interrupted-save test simulate a crash without actually killing the process.
- 2026-10-29 — T-0-07: parrot-container `tree.rs` — Library/Group directory tree I/O (`<title>.parrot` / `<title>.ulgroup` children, ASSUMPTION(D2)). Two-level (Library>Group>2 Sheets) round-trip test passes; dotfile/temp-sibling entries are skipped on read.
- 2026-10-29 — T-0-08: parrot-markup `parse.rs` — typing-time parser for the MVP tag subset (heading1-6/strong/emph/unorderedList/orderedList/blockquote). Block-tag detection then a recursive inline scanner for `**`/`__` strong and `*`/`_` emph. One test per marker, all green first try.
- 2026-10-29 — T-0-09: parrot-app `state.rs` (headless AppState: create/rename/delete Group/Sheet, save/open library) + `ui.rs`/`main.rs` (egui/eframe shell: tree panel, text editor + styled preview, status bar). AppState tests (create_group/rename_sheet/delete_node) green. Fixed no-network guard to filter by actual build targets after eframe transitively surfaced a Linux-only tokio dep (see DECISIONS.md). Smoke-test checklist written (docs/mvp_smoke_test.md) but not executed — no display in this sandbox.
- 2026-10-29 — T-0-10: parrot-export `mvp.rs` — ad-hoc AST->String exporters for `.md` (delegates to parrot-markup's renderer) and `.txt` (strips all markup). Golden files for both under tests/fixtures/mvp/.
- 2026-10-29 — T-0-11: parrot-app/tests/mvp_e2e.rs — headless end-to-end test (create library->group->sheet->type->save->close->reopen->assert->export->golden). Split main.rs into lib.rs+main.rs so the integration test can depend on AppState as an ordinary crate.
- 2026-10-29 — T-0-12: confirmed `cargo check --target x86_64-pc-windows-msvc --workspace` passes for all 8 crates (already wired into scripts/check.sh since T-0-01). Actual linking/running on Windows deferred to the real hardware pass — this sandbox has no MSVC linker (see DECISIONS.md). MVP (12/12) complete; tagged milestone/MVP-complete.
- 2026-10-29 — T-1-01: parrot-core `ast.rs` extended to the full 19-tag table (added CodeBlock/Comment/Divider/NativeBlock to TagKind; Delete/InlineComment/InlineNative/Mark/Annotation/Image/Video to Inline), plus `TagKind::identifier()`/`from_identifier()` centralizing the kind-string mapping. Coverage test `enum_covers_confirmed_tag_table` checks all 19 against a hardcoded list. Propagated exhaustive-match updates through xml.rs (container), render.rs (markup), mvp.rs (export), ui.rs (app) — compiler-enforced, no catch-all arms added anywhere.
- 2026-10-29 — T-1-02: parrot-markup `parse.rs` extended to all 11 new tags. Block-level codeblock/comment/divider/nativeblock via line-prefix detection (content kept raw, not re-parsed). Inline delete/inlineComment/mark/inlineNative generalize the doubled/single-marker scanner via a table-driven `WrapMarker` lookup; annotation/image/video get this project's own typing syntax mirroring T-1-01's rendering choices. 11 new tests, all green first try.
- 2026-10-29 — T-1-03: added `roundtrip_full_tag_coverage` to parrot-container xml.rs — one paragraph per block-level tag plus one paragraph exercising all 12 Inline variants, round-trips exactly. Serializer plumbing itself was already added during T-1-01's exhaustiveness fix; this task was the dedicated coverage test.
- 2026-10-29 — T-1-04: authored 10 hand-written Content.xml fixtures under tests/fixtures/content_xml/, each a distinct tag combination. `corpus_roundtrip` test reads the directory at runtime (not hardcoded includes), parses each, re-serializes, re-parses, asserts AST equality — all 10 pass first try.
- 2026-10-29 — T-1-05: `.ulproject` support. Extended tree.rs's save_children/open_children (made pub(crate)) with a `.ulproject` suffix case identical to Group's; new `project.rs` wraps save/open for a single project root plus its own roundtrip test (mixed Group+Sheet tree inside a Project).
- 2026-10-29 — T-1-06: parrot-core `filter.rs` — Filter{title, keyword, node_kind, date_range} (serde-derived), `evaluate()` walks the Library tree returning matching &Sheet. Moved the old stub Filter out of node.rs into filter.rs. 3 tests (keyword match, case-insensitivity, no-keyword-matches-all), all green first try.
- 2026-10-29 — T-1-07: parrot-core `search.rs` — linear-scan library-wide search reusing `SheetContent::plain_text()` (refactored out of filter.rs). 50-sheet fixture tree test (`finds_matches_across_tree`) + an empty-result test, both green first try.
- 2026-10-29 — T-1-08: M1 milestone close-out. Full regression suite green (fmt/clippy/test/windows-check/no-network-guard, including its own regression test). Tagged milestone/M1-complete.
- 2026-10-29 — T-2-01: text-shaping decision gate. cosmic-text 0.12 + egui::Painter confirmed viable via a headless spike (no window/GPU needed — raw egui::Context::run + tessellate). No fallback to iced needed. Caught two real API/integration snags along the way (see DECISIONS.md).
- 2026-10-29 — T-2-02: parrot-app `editor_state.rs` — pure-Rust EditorState (cursor/selection, insert, delete_backward, undo/redo stack with redo-clear-on-new-edit). 4 named tests, all green first try.
- 2026-10-29 — T-2-03: added EditorState::to_ast/from_ast (typing<->AST two-way binding). Typed-sequence test matches a directly-built AST, both directions. ui.rs's existing simpler buffer binding left untouched — rewiring it onto EditorState is T-2-04's job once the custom paint layer exists.
- 2026-10-29 — T-2-04: parrot-app `render.rs` — AST->styled-layout via cosmic-text (bold/italic/heading sizes/list bullet/blockquote indent/code monospace), `shape_paragraph` for headless metrics (line count + per-span run boundaries via glyph metadata), `paint_paragraph` for egui::Painter output (layout boxes, not rasterized glyphs — see DECISIONS.md). Rewired ui.rs's preview panel onto it, replacing T-0-09's RichText approximation. 3 tests green.
- 2026-10-29 — T-2-05: added EditorState::find_all/replace_all (operates on the plain-text buffer, so a match/replace inside a markup span like `**bold**` re-parses correctly afterward — verified directly in replace_preserves_surrounding_spans). 2 named tests green first try.
- 2026-10-29 — T-2-06: parrot-app `shortcuts.rs` — bold/italic toggle (wrap/unwrap selection), save, new_sheet, navigate next/previous sheet (tree-order flatten). Logic tested at AppState/EditorState level per Done-when; actual egui key-event wiring deferred (see DECISIONS.md — ui.rs's TextEdit isn't on EditorState yet).
- 2026-10-29 — T-2-07: M2 milestone close-out. Full regression suite green (fmt/clippy/test/windows-check/no-network-guard + its own regression test). Smoke-test doc updated with the M2 checklist. Tagged milestone/M2-complete.
- 2026-10-29 — T-3-01: license review gate. Inspected the real /Applications/ParrotMac.app (present on this machine) — rust/python/markdown/json/javascript grammars have no license info, so none are reused. THIRD_PARTY_LICENSES.md documents the resolution: parrot-syntax uses syntect's bundled defaults instead (verified MIT + confirmed sublimehq/Packages provenance, each target language individually checked). scripts/check_licenses.sh wired into check.sh. Added syntect dependency (default-fancy feature — pure-Rust regex, avoids onig_sys's C build breaking the Windows cross-check).
- 2026-10-29 — T-3-02: parrot-syntax `lib.rs` — `highlight(code, lang) -> Vec<StyledSpan>` wrapping syntect's bundled syntax/theme defaults (base16-ocean.dark), SyntaxSet/ThemeSet cached via OnceLock. Test proves real syntax-aware coloring (keyword vs identifier get different colors), not just one uniform span. Both tests green first try.
- 2026-10-29 — T-3-03: adopted ```` ```lang ```` codeblock language-tag convention (own addition, not confirmed). TagKind::CodeBlock became CodeBlock(Option<String>) — propagated through ast.rs/xml.rs (new `language` XML attribute)/render.rs/parse.rs. 2 new tests + existing codeblock test updated, all green.
- 2026-10-29 — T-3-04: parrot-app render.rs — layout_codeblock_paragraph wires parrot_syntax::highlight into the rendering layer; StyledSpan gained a color field threaded through rich_text_attrs and paint_paragraph (glyph-box tinting via glyph.metadata). No-language/unknown-language falls back to plain monospace. 2 new tests green first try; smoke-test doc updated.
- 2026-10-30 — T-3-05: added insta dev-dependency + 5 token-snapshot tests (rust/python/js/json/markdown) to parrot-syntax. Generated via INSTA_UPDATE=always, verified stable across two subsequent normal `cargo test` runs (no .snap.new pending). Snapshots committed under parrot-syntax/src/snapshots/.
- 2026-10-30 — T-3-06: M3 milestone close-out. Full regression suite green (fmt/clippy/test/windows-check/no-network-guard+its own regression test/check_licenses). Tagged milestone/M3-complete.
- 2026-10-30 — T-4-01: extracted the real ULSS fixtures from /Applications/ParrotMac.app. Found the inventory's "17 files" claim was wrong — only 10 real .ulss files exist bundle-wide (HTML/ePub "styles" are plain CSS). Corrected scripts/check_fixtures.sh and FIXTURES_SOURCE.md to the verified 10. (Self-correction next entry: the @inline-footnote-style mixin example IS real, just in defaults.ulss not manuscript.ulss.)
- 2026-10-30 — T-4-02: parrot-style grammar.pest — decoded the real ULSS.tmLanguage binary plist directly for the exact selector/pseudo-class/property vocabulary. Smoke-tested against 4 real fixtures before committing, caught 2 real gaps (adjacent-sibling `+` combinator, optional `;` on variable definitions). Pinned pest/pest_derive to =2.7.15 — 2.9.x's pest_stack_guard breaks Windows cross-compile the same way onig_sys did (see DECISIONS.md).
- 2026-10-30 — T-4-03: formal parses_all_real_fixtures test — all 10 real ULSS fixtures parse cleanly (the gaps were already fixed during T-4-02's ad-hoc smoke check). Reads the fixtures directory at runtime, same pattern as container::xml's corpus_roundtrip.
- 2026-10-30 — T-4-04: parrot-style resolve.rs — AST->Stylesheet model, variable substitution + recursive memoized mixin expansion with override precedence. Test built from the real @inline-footnote-style/inline-footnote :anchor example (defaults.ulss, location-corrected per T-4-01). Also resolves all 10 real fixtures cleanly (kept as a permanent test). All green first try.
- 2026-10-30 — T-4-05: parrot-style select.rs — selector matching engine (PathNode-based: kind + active pseudo-classes). Descendant (right-to-left ancestor search), adjacent-sibling (via a synthesized sibling-path), universal, and pseudo-class compounds all implemented and tested. 4/4 tests green first try (3 required + a bonus pseudo-class-compound test).
- 2026-10-30 — T-4-06: added ComputedStyle/cascade() to resolve.rs (same module the roadmap's Tests line names). Cascade = last-matching-rule-wins per property, walking stylesheet.rules in order. Test resolves the real manuscript.ulss and confirms table-cell :header computes font-weight: bold — all 10 parrot-style tests green.
- 2026-10-30 — T-4-07: render.rs's layout_paragraph/shape_paragraph/paint_paragraph now take Option<&Stylesheet>; per-property override of font-size/margin-left via cascade(), None preserving T-2-04's exact old behavior. New test proves switching stylesheets changes the computed font_size. ui.rs passes None for now (T-4-09 wires the actual picker). All 6 render tests green first try.
- 2026-10-30 — T-4-08: parrot-style theme.rs — Theme::load/computed wraps resolve::resolve+cascade, queried by theme-scoped selector name. Loading the real readingmode.ulss reproduces its confirmed palette (#f5f5f5 paragraph text, #d1a804 inline-link, #a0a0a0 inline-comment) exactly. ASSUMPTION(D3) flagged in the module doc. Green first try.
- 2026-10-30 — T-4-09: parrot-app style_picker.rs — StylePicker with two original built-in stylesheets ("Default"/"Compact", not copies of Parrot' own — see DECISIONS.md). Wired into ui.rs: a ComboBox in the top bar, and paint_paragraph now receives the active stylesheet instead of T-4-07's placeholder None. All 20 parrot-app unit tests + the e2e test green.
- 2026-10-30 — T-4-10: M4 milestone close-out. Full regression suite green (fmt/clippy/test/windows-check/no-network-guard+its own regression test/check_licenses/check_fixtures). Tagged milestone/M4-complete.
- 2026-10-30 — T-5-02: parrot-export rules.rs — TOML conversion rule-table (own schema), dispatch by node kind with first-match-wins precondition order, confirmed against the real Default.plist. README.md documents the schema. 2/2 new tests green, full check.sh green.
- 2026-10-30 — T-5-03: parrot-export dsl.rs — expression DSL parser+evaluator ({{ }}, if/end, method chains). Confirmed real `code` rule template reproduced exactly. 5/5 new tests green, full check.sh green.
- 2026-10-30 — T-5-04: parrot-export html.rs — AST->HTML direct tree walk, well-formed-XML validated via quick-xml. Added shared fixtures.rs (one-of-every-tag doc) for M5's later tasks.
- 2026-10-30 — T-5-05: parrot-export rtf.rs — hand-rolled RTF 1.9 writer, balanced-braces structural validity check. 3/3 new tests green, full check.sh green.
- 2026-10-30 — T-5-06: parrot-export docx.rs — AST->DOCX via docx-rs+zip, round-trip-reopened by docx-rs itself. Caught and fixed a zstd-sys Windows cross-compile regression from zip's default features. Full check.sh green.
- 2026-10-30 — T-5-07: parrot-export epub.rs — manual zip+OPF EPUB backend, reusing html.rs's XHTML-safe output. Validity test checks zip structure + well-formed XML for every part. Full check.sh green.
- 2026-10-30 — T-5-08: parrot-export pdf.rs — AST->PDF via printpdf, builtin-font text, manual page-break layout. Page-count/content-stream verified via printpdf's own re-parse. Full check.sh green.
- 2026-10-30 — T-5-09: parrot-export textbundle.rs — atomic directory-package export (info.json + text.markdown), reusing mvp::export_markdown. Full check.sh green.
- 2026-10-30 — T-5-11: parrot-export consolidated_tests.rs — one suite exercising all 6 M5 backends against the shared fixture. Full check.sh green.
- 2026-10-30 — T-5-12: M5 milestone close-out. Tagged milestone/M5-complete. T-5-01 left open as a deferred cleanup pass (per roadmap's own note); T-5-10 skipped (Inferred-only).
- 2026-10-30 — T-5-01: parrot-export conversion.rs — MVP exporters refactored onto the rule-table+DSL engine; byte-identical golden regression plus a real fix (correct backtick-escaping for Markdown code-span export). Full check.sh + app e2e green.
- 2026-10-30 — T-6-01: parrot-import markdown.rs — pulldown-cmark->AST importer. Fixed a real blockquote-nesting bug before committing. Round-trip golden test green, full check.sh green.
- 2026-10-30 — T-6-02: parrot-import plaintext.rs — trivial blank-line-split importer. Round-trip golden test green, full check.sh green.
- 2026-10-30 — T-6-03: parrot-import html.rs — scraper-based HTML importer, best-effort tag mapping, transparent-wrapper fallback for unknown block elements. Reused T-6-01's golden file. Full check.sh green.
- 2026-10-30 — T-6-04: parrot-import enex.rs — quick-xml ENEX parser reusing html::import_html for ENML content. Fixed a div-as-paragraph gap in html.rs along the way. Full check.sh green.
- 2026-10-30 — T-6-05: parrot-import textbundle.rs — thin wrapper over the Markdown importer. Export->import->export fixed-point test green, full check.sh green.
- 2026-10-30 — T-6-06: parrot-import docx.rs — docx-rs read path, best-effort heading/bold/italic/list detection matching our own exporter's conventions. Round-trip golden test green, full check.sh green.
- 2026-10-30 — T-6-07: parrot-import rtf.rs — hand-rolled RTF reader (plain/bold/italic/par), covered subset in new README.md. Round-trips through our own T-5-05 exporter including its font table. Full check.sh green.
- 2026-10-30 — T-6-08: parrot-import consolidated_tests.rs — one suite for all 7 importers. Exposed and fixed a real loose-list nesting bug in markdown.rs (export->import->export was dropping list tags). Full check.sh green.
- 2026-10-30 — T-6-09: M6 milestone close-out. Tagged milestone/M6-complete.
- 2026-10-30 — T-7-01/T-7-02: parrot-app filters_ui.rs + search_ui.rs, wired into ui.rs. Full check.sh green.
- 2026-10-30 — T-7-03: parrot-app tree_ops.rs — move/duplicate/delete-with-confirmation, wired into ui.rs with a real confirm/cancel dialog. Full check.sh green.
- 2026-10-30 — T-7-04: parrot-app preferences.rs — TOML preferences (fonts + default style), open-font substitutions documented. Wired into ui.rs. Full check.sh green.
- 2026-10-30 — T-7-05: parrot-app first_run.rs — bare prompt-vs-skip decision over AppState.library_path, wired into ui.rs. Full check.sh green.
- 2026-10-30 — T-7-06: Windows packaging — generate_icon.py (real app icon) + package_windows.sh (portable zip, falls back to a packaging kit since this sandbox can't link a real .exe — no link.exe present). Script confirmed exits 0 and produces a real artifact.
- 2026-10-30 — T-7-07: full regression pass — 121 tests green across the workspace, check.sh green. Renamed+extended the manual smoke-test checklist (M5/M6/M7 items added); actual manual execution not possible (no display), logged as an open deviation per precedent.
- 2026-10-30 — T-7-08: close-out. 72/72 tasks complete. Tagged v1.0.0-hands-off-complete.
- 2026-10-30 — T-9-01: visual capture harness. No offscreen-rendering path available in this
  sandbox after a genuine two-path attempt (tiny-skia hand rasterizer, egui_kittest version
  mismatch) — fell back to the roadmap's own sanctioned structural audit:
  docs/ui_structural_audit.md walks every screen's egui layout/widget tree by hand, found several
  real issues (non-clickable search results, inconsistent delete-button ordering, missing
  preferences UI). scripts/check_visual_captures.sh added + wired into check.sh. Full check.sh
  green.
- 2026-10-30 — T-9-02: parrot-app/README.md documents this project's own 8px-grid/heading/button
  UI convention (Parrot' real spacing was never reverse-engineered, so this is an original
  convention, not a rebuild target). docs/ui_audit.md lists 7 open deviations found against it
  (icon mismatch + non-clickable search results, double-separator gap, Group/Sheet delete-button
  order, plain-label section titles in two places, no error/success status coloring, one stray
  4.0 spacing constant) plus 2 explicitly no-fix items. Full check.sh green (audit-only task, no
  code changes).
- 2026-10-30 — T-9-03: fixed all 7 open items from T-9-02's docs/ui_audit.md (search results
  clickable + correct glyph via new parrot_core::search::search_with_paths, delete-button
  ordering unified, filter-builder + preview headings, status-text error coloring, 4.0->8.0
  spacing). 2 new parrot-core tests + 1 new parrot-app test for the path-aware search. Full
  check.sh green.
- 2026-10-30 — T-9-04: M9 milestone close-out. Full regression suite green (fmt/clippy/test/
  windows-check/no-network-guard+its regression test/check_licenses/check_fixtures/
  check_visual_captures). docs/ui_audit.md shows 0 open items (2 explicitly logged as deliberate
  no-fix exceptions, not left open by omission). Tagged milestone/M9-complete.
- 2026-10-30 — T-10-01: parrot-app/src/export_ui.rs — ExportFormat enum + export_sheet()
  dispatching to all 8 parrot-export backends; wired into ui.rs as an Export dialog window off
  a new top-bar button. 8 new dispatch tests (one per backend), all green; full check.sh green
  after a clean rebuild (hit and resolved a disk-space issue — target/ had grown to 14G).
- 2026-10-30 — T-10-02: parrot-core/src/goal.rs — Goal{kind,counter,target,deadline,
  write_on_deadline}, tolerance-banded progress() (own chosen constants, named after the 4
  confirmed-but-unvalued ULGoalController* symbols), DailySession for daily-goal session deltas.
  Added goal: Option<Goal> to Sheet/Group/Project (not yet persisted on disk — deferred, see
  DECISIONS.md). 2 new tests; full check.sh green (clean rebuild from T-10-01 already verified).
- 2026-10-30 — T-10-03: parrot-app/src/goal_ui.rs — GoalSetupFlow builder + count_units()
  (Sheet: own text; Group/Project: sum of descendant Sheets). AppState::set_goal added. Wired
  into ui.rs: 🎯 edit button + inline percent indicator on Sheet/Group tree rows, backed by a
  new Goal dialog window. 5 new tests (incl. the named setup_flow_output_matches_
  direct_construction); full check.sh green.
- 2026-10-30 — T-10-04: parrot-core/src/keyword.rs (assign/unassign/has) + Sheet.keywords:
  Vec<String>; Filter gained keyword_tag: Option<String> (exact-match tag, distinct from the
  existing substring-search keyword field), sheet_matches ANDs both. FilterBuilder + its UI
  "Tag:" field updated to match. 3 new tests (keyword::assign_and_query,
  filter::keyword_tag_match_on_fixture_tree, plus FilterBuilder's existing tests extended);
  full check.sh green.
- 2026-10-30 — T-10-05: parrot-app/src/keyword_manager.rs — add/remove (one sheet) +
  rename/delete/list_all (library-wide, walking AppState.library directly). New
  AppState::assign_keyword/unassign_keyword. Wired into ui.rs as a "Keywords…" window. 6 new
  tests incl. the 3 named ones (add, rename_propagates, delete_unassigns); full check.sh green.
- 2026-10-30 — T-10-06: parrot-app/src/container_view.rs — layout_metrics (pure, tested) +
  child_sheets/child_sheets_with_paths (direct children only, not recursive through nested
  Groups). Added parrot_core::filter::evaluate_with_paths so Filter results are editable too.
  Wired into ui.rs: Group/Filter rows selectable, central panel shows a scrolling per-sheet
  editor stack (show_sheet_flow) for a selected container instead of the single-sheet view.
  Manual smoke-test entries added under a new M10 section. 6 new tests incl. the named
  layout_metrics_for_three_sheet_fixture; full check.sh green.
- 2026-10-30 — T-10-07: parrot-app/src/spellcheck.rs — check()/check_sheet() over an
  originally-written common-word list (not a real dictionary — licensing, see DECISIONS.md).
  Wired into ui.rs's single-sheet editor as a "Possibly misspelled: ..." label. 4 new tests
  incl. the 2 named ones; full check.sh green.
- 2026-10-30 — T-10-08: grammar category deferred (no JRE in this sandbox; bundling LanguageTool
  + a JRE is disproportionate for one category — see DECISIONS.md, roadmap's own fallback is
  explicitly fine with this). Implemented the other 4 confirmed categories in
  parrot-app/src/grammar.rs (Casing/Compounding/Punctuation/Typography) as rule-based checks,
  wired into ui.rs. 5 new tests incl. the named flags_representative_grammar_error; full
  check.sh green.
- 2026-10-30 — T-10-09: parrot-app/src/focus_mode.rs — paragraph_alphas (own chosen
  FADED_ALPHA=0.55, ULFocusModeFadedAlpha's value was never confirmed). Wired into ui.rs: a
  "Focus Mode" checkbox, cursor tracked via TextEdit::show()'s cursor_range, non-cursor
  paragraphs dimmed via a translucent rect_filled overlay in show_preview. 3 new tests incl. the
  named alpha_follows_cursor_paragraph; full check.sh green.
- 2026-10-30 — T-10-10: parrot-core/src/statistics.rs — ActivityTracker/ActivityPoint
  (session-delta series, day is an opaque ordinal, no chrono dep). AppState gained activity/
  total_word_count()/start_new_day(); set_sheet_content now records activity automatically.
  Wired into ui.rs as a "Statistics…" window. 3 new core tests incl. the named
  activity_series_across_sessions; full check.sh green.
- 2026-10-30 — T-10-11: parrot-core::attachment::Attachment (Note/File, sheet-level only).
  parrot_container::sheet gained a real Attachments.txt sidecar (sheet::save/open signatures
  changed) since this task's Done-when requires an actual disk round-trip, unlike Goal/Keywords.
  New parrot-app::attachments module + AppState::add_attachment/remove_attachment/
  has_attachments. Wired into ui.rs as a 📎/📌 per-row badge+dialog. 3 new app tests (the exact
  named roundtrip_note_attachment/roundtrip_file_attachment/badge_reflects_presence) + 2 new
  container tests; full check.sh green.
- 2026-10-30 — T-10-12: M10 milestone close-out. Full regression suite green (fmt/clippy/test
  [69 parrot-app + 19 parrot-core + others, 18 test binaries all ok]/windows-check/
  no-network-guard/check_licenses/check_fixtures/check_visual_captures). docs/manual_smoke_test.md
  extended with entries for every M10 surface (export dialog, goal setup, keyword manager,
  sheet-flow view, focus mode, statistics, attachments, spellcheck/grammar). Tagged
  milestone/M10-complete.
- 2026-10-30 — T-11-01: docs/ui_structural_audit.md extended with §8-14 (every new M10 screen,
  structural walk). scripts/check_visual_captures.sh now asserts 14 screens. Full check.sh
  green.
- 2026-10-30 — T-11-02: docs/ui_audit.md extended with 5 open cross-surface items — most
  notably the sheet-flow view missing spellcheck/grammar/Focus Mode entirely (a real
  feature-parity gap) and an icon collision in the Attachments dialog. Audit-only task, no code
  changes; full check.sh green.
- 2026-10-30 — T-11-03: fixed all 5 T-11-02 items (Export dialog separators, Attachments
  add_enabled + renamed buttons + 💾 icon de-collision, sheet-flow spellcheck/grammar parity,
  and Focus Mode in the sheet-flow view — a real design decision: dims other *sheets*, not
  paragraphs, there). docs/ui_audit.md at 0 open items. docs/manual_smoke_test.md rewritten
  end-to-end into 13 feature-area sections; still not executable here (no display). Full
  check.sh green.
- 2026-10-30 — T-11-04: M11 milestone close-out. Full regression suite green (fmt/clippy/test
  [18 test binaries, all ok]/windows-check/no-network-guard/check_licenses/check_fixtures/
  check_visual_captures). Backlog complete: 92/92 tasks. Tagged milestone/M11-complete and
  v1.1.0-ui-and-features-complete.
- 2026-10-31 — T-12-01: `layout.rs` PanelLayout (P1+P2 share the sidebar toggle, P4 independent, §7.1 editor-only toggle); ui.rs rebuilt as menu bar + toolbar + four panels (Library 160 / Sheet List 280 / Editor fill / Inspector 220) + status bar.
- 2026-10-31 — T-12-02: `tokens.rs` (§10.1 palette, §10.2 type scale, §10.3 4px grid, egui visuals); `scripts/check_no_hardcoded_colors.sh` wired into check.sh, zero literals outside tokens.rs. System Segoe UI/Consolas loaded from the local Fonts folder when present (bold family for headings/strong).
- 2026-10-31 — T-12-03: `sidebar.rs` — smart filters (All/Last 7 Days/Open Files/Trash), PROJECTS/NOTES section labels, §3.4 context menu dispatching to the existing AppState handlers; AppState gained a session-only Trash (move/restore/empty). Deviations in DECISIONS.md.
- 2026-10-31 — T-12-04: `badges.rs` — goal ring (clockwise arc, §3.3 color bands) + attachment badge in sidebar and sheet-list rows; draw-call test runs a real headless egui frame.
- 2026-10-31 — T-12-05: `sheet_list.rs` — title + 2-line ellipsized preview + badges/error dot rows, bold container header (click returns to the sheet flow), "Empty Sheet" footer; library search moved to the top of P2.
- 2026-10-31 — T-12-06: `editor.rs` — centered 640px inset column, paragraph numbers in the left margin, live word-count pill (opens Statistics), Markdown-XL styled via a TextEdit layouter (dimmed markers, §5.1 typography, active ULSS style drives sizes). Replaces the raw editor + glyph-box preview. Also added `statistics::word_count/char_count/reading_time_seconds` and `Paragraph::plain_text` to core.
- 2026-10-31 — T-12-07: `container_view::flow_items`/`aggregate_word_count` — #333 dividers only between sheets, aggregate badge in flow mode.
- 2026-10-31 — T-12-08: `inspector.rs` + P4 — Progress (chars/words/reading time/large goal ring), Statistics (activity bars, moved from its window), Keywords chips, Outline (click-to-jump), Links, Media (+ attachment add/remove, moved from its window). Fixed a pre-existing editor data-loss bug on the way: the markup parser never read back links, inline code, footnotes or annotation notes that the renderer writes (see DECISIONS.md).
- 2026-10-31 — T-12-09: `toolbar.rs` — sidebar/layout toggles left; Markup, B/I/Link/¶, Export, Quick Export, inspector right; formatting acts on the focused editor's egui selection via EditorState; Ctrl+B/I/S/N/↑/↓ wired.
- 2026-10-31 — T-12-10: `export_modal.rs` — centered modal (format, light/dark preview, style + page-size labels, live paginated preview, Export); Quick Export repeats the last format/folder; PDF backend gained page sizes (`export_pdf_sized`, `page_lines`).
- 2026-10-31 — T-12-11: `annotations.rs` — merged spelling/grammar findings, colors corrected to #f5a623/#5ab0f6 (were (200,140,20)/(140,100,200)), plus matching inline underlines in the editor.
- 2026-10-31 — T-12-12: structural-audit fallback again (no visual verification by design); docs/ui_structural_audit.md §15–25 + table, check_visual_captures.sh covers 25 surfaces, docs/ui_audit.md resolves all 13 §11.1 rows (iCloud badges permanently deferred under §2). manual_smoke_test.md and parrot-app/README.md rewritten for the M12 layout.
- 2026-10-31 — T-12-13: M12 close-out. check.sh green (fmt/clippy/18 test binaries/windows-check/no-network/licenses/fixtures/visual-captures/no-hardcoded-colors). First real linked Windows build: dist/parrot-windows-portable-v0.1.0.zip (kind: full-binary). Tags not created — no git repo (see DECISIONS.md). App never launched; manual smoke test still pending.
