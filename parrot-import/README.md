# parrot-import

Format importers -> `parrot-core` AST. Each importer is best-effort unless noted otherwise —
see the module-level doc comment in each file for its specific known limitations, and
`DECISIONS.md` for the reasoning behind them.

## RTF importer (T-6-07): covered subset

`rtf.rs` is a hand-rolled reader, **not** a full RTF spec implementation (explicitly out of
scope per this task's own fallback). It covers:

- Plain text runs.
- Bold (`\b` / `\b0`) and italic (`\i` / `\i0`) runs, including combined bold+italic.
- Paragraph breaks (`\par`, `\line`).
- The `\{`, `\}`, `\\` literal-character escapes and `\'XX` hex-byte escapes (interpreted as a
  Latin-1 codepoint — no codepage/charset table is implemented).
- A simple `\uN` Unicode escape (decimal codepoint; does not implement the `\ucN` "skip N
  replacement characters" mechanism real RTF Unicode escapes pair it with).
- Skipping whole metadata groups by name — `fonttbl`, `colortbl`, `stylesheet`, `info`,
  `generator` — and any group marked ignorable with the `\*` prefix, so their content (font
  names, color definitions, etc.) never leaks into the imported body text.

**Not covered** (falls through as a no-op control word, or — for an unrecognized metadata group
without the `\*` marker and not in the named list above — may leak its text into the body):
underline/strikethrough/font-size/color runs, lists, tables, embedded objects/images, field
codes, any codepage other than the hex-escape's implicit Latin-1 assumption, and real Unicode
surrogate-pair handling.

The importer's own test (`rtf::tests::basic_runs_import_correctly`) validates it against this
project's own RTF exporter's (`parrot_export::rtf`) real output — including that output's font
table — not just a hand-authored minimal fixture, so the font-table-skip logic above is
exercised by an actual round-trip, not just a synthetic unit test.
