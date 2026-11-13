# Fixtures Source

These are reference copies taken from the user's own legally-owned Parrot installation
(`/Applications/ParrotMac.app`) for local test purposes only — not redistributed with any
shipped build of this project. See `THIRD_PARTY_LICENSES.md` for the equivalent note covering
syntax-highlighting grammars.

## tests/fixtures/ulss/ (T-4-01)

**Correction to the feature inventory, made during this extraction (T-4-01 is a gate that
actually inspects the real files, not a restatement of the inventory):** the inventory's §5.2
claimed "15 real, complete, shipped style files" across RichText (7), HTML (5), and ePub (3)
categories, all `.ulss`. Direct inspection of the real bundle shows this is only true for
RichText — **HTML and ePub `._ulstyle` bundles contain plain `.css` files, not `.ulss`.** A
bundle-wide `find -iname "*.ulss"` turns up exactly **10** real `.ulss` files total, not 17:

| File in this directory | Original path | Category |
|---|---|---|
| `defaults.ulss` | `Contents/Frameworks/ParrotCoreMac.framework/Versions/A/Resources/Styles/defaults.ulss` | Base |
| `readingmode.ulss` | `Contents/Resources/Preview/readingmode.ulss` | Theme (Decision D3) |
| `rtf_default.ulss` | `Contents/Frameworks/ParrotCoreMac.framework/Versions/A/Resources/Export Formats/RTF/Default._ulstyle/default.ulss` | RTF export default |
| `swissknife.ulss` | `.../Styles/RichText/SwissKnife._ulstyle/swissknife.ulss` | RichText |
| `papers.ulss` | `.../Styles/RichText/Papers._ulstyle/papers.ulss` | RichText |
| `manuscript.ulss` | `.../Styles/RichText/Manuscript._ulstyle/manuscript.ulss` | RichText |
| `business.ulss` | `.../Styles/RichText/Business._ulstyle/business.ulss` | RichText |
| `academica.ulss` | `.../Styles/RichText/Academica._ulstyle/academica.ulss` | RichText |
| `columns.ulss` | `.../Styles/RichText/Columns._ulstyle/columns.ulss` | RichText |
| `revision.ulss` | `.../Styles/RichText/Revision._ulstyle/revision.ulss` | RichText |

This is the complete, verified set — not a sample. `scripts/check_fixtures.sh` asserts exactly
these 10 files are present (roadmap T-4-01's "17 files" Done-when is corrected to 10 here; later
M4 tasks citing "17 fixtures" are updated to 10 accordingly — see DECISIONS.md).

**Correction to a correction:** an earlier pass of this note claimed the roadmap's cited mixin
example didn't exist verbatim, because it was only checked against `manuscript.ulss`. It's
actually in **`defaults.ulss`**, verbatim, lines 130-135:

```
@inline-footnote-style {
	baseline-shift: superscript
}

inline-footnote :anchor : @inline-footnote-style {}
inline-annotation :anchor : @inline-footnote-style {}
```

So the roadmap's T-4-04 citation is accurate — it just didn't say which file. (`manuscript.ulss`
separately has its own real mixin, `@code { font-weight: normal; font-slant: normal; }`, applied
via `block-code : @code {}` / `inline-code : @code {}` at lines 79-86, 205, 334 — also usable as
a second worked example, just not the one originally cited.) The `table-cell :header {
font-weight: bold; ... }` example T-4-06 cites is verified verbatim in `manuscript.ulss`
(lines 169-174) and was never in question.
