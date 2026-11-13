# Third-Party Licenses — Syntax Highlighting Grammars

T-3-01 (roadmap §M3, a gate — do not skip): per-language license resolution for every code-block
syntax highlighting grammar this project ships, before any grammar file or dependency bundling
one is added. Required when a `.tmLanguage`/`.tmTheme`/`.sublime-syntax` file is added; enforced
by `scripts/check_licenses.sh`.

## Decision: use `syntect`'s own bundled default syntax definitions, not Parrot' SyntaxKit files

The feature inventory (§3.1) found Parrot ships ~70 real TextMate `.tmLanguage` grammars under
`SyntaxKit.framework/.../Resources/Syntaxes/`, "each with its own `LICENSE`/`LICENSE.md` where
present." This build's sandbox has access to the real `/Applications/ParrotMac.app` bundle, so
this review inspected it directly rather than working from the inventory doc alone:

- `Syntaxes/{rust,python,markdown,json,javascript}/` — the five languages this project ships
  highlighting for (T-3-05) — **each has no accompanying `LICENSE`/`LICENSE.md` file**, and the
  `.tmLanguage` binary plists themselves carry no embedded author/copyright/license metadata
  (checked directly: `grep -i "author\|copyright\|license"` over the decoded plist found
  nothing but scope-name strings).
- A `LICENSE`/`LICENSE.md`/`LICENSE.txt` *is* present for several other bundled languages
  (`smarty`, `handlebars`, `swift`, `graphql`, `julia`, `j`, `vimscript`, `haxe`, `smalltalk`,
  `fsharp`, `mathematica`) — confirming the inventory's "where present" was accurate, and that
  its absence for our five target languages is a real gap, not an inspection miss.

No license confirmed for reuse ⇒ **resolution: replaced with an alternative open grammar**, per
the roadmap's own stated options for this task. None of Parrot' `.tmLanguage` files are reused,
redistributed, or shipped by this project for any language.

The alternative: `syntect` (the crate `parrot-syntax` wraps, T-3-02) bundles its own default
syntax definitions, built from the Sublime Text community `Packages` repository
(confirmed directly: `syntect`'s own `.gitmodules` declares `testdata/Packages` as a submodule
of `https://github.com/sublimehq/Packages`; its `Readme.md`/`DESIGN.md` describe dumping "all the
default syntax definitions" from that checkout into the binary `assets/*.packdump` files shipped
in the published crate). Verified for each of our five target languages, directly against that
repository:

| Language | Grammar file in `sublimehq/Packages` | License |
|---|---|---|
| Rust | `Rust/Rust.sublime-syntax` | **MIT** — `Rust/LICENSE.txt` present in the repo, full MIT text |
| Python | `Python/Python.sublime-syntax` | No per-package override — repo-wide license applies (permissive: "Permission to copy, use, modify, sell and distribute this software is granted," zlib-style) |
| Markdown | `Markdown/Markdown.sublime-syntax` | No per-package override — repo-wide license applies |
| JSON | `JSON/JSON.sublime-syntax` | No per-package override — repo-wide license applies |
| JavaScript | `JavaScript/JavaScript.sublime-syntax` | No per-package override — repo-wide license applies |

`syntect` itself is MIT-licensed (`LICENSE.txt` in the published crate, confirmed by direct
inspection of the local cargo registry cache). All five languages: **reused under permissive
license, via `syntect`'s bundled defaults — resolved.**

## Resolution table (per T-3-01's required format)

| Language | Resolution |
|---|---|
| Rust | Reused under MIT, via `syntect`'s bundled default syntax definitions (sourced from `sublimehq/Packages`, which licenses `Rust/` under MIT) |
| Python | Reused under a permissive (zlib-style) license, via `syntect`'s bundled defaults (sourced from `sublimehq/Packages`'s repo-wide license) |
| JavaScript | Reused under a permissive (zlib-style) license, via `syntect`'s bundled defaults |
| JSON | Reused under a permissive (zlib-style) license, via `syntect`'s bundled defaults |
| Markdown | Reused under a permissive (zlib-style) license, via `syntect`'s bundled defaults |

No Parrot/SyntaxKit grammar file is reused for any language — all five are **replaced** with
`syntect`'s bundled set, which happens to trace back to the same ultimate upstream
(`sublimehq/Packages`) by a route with confirmed, verifiable licensing.

## What this means going forward

- No `.tmLanguage`/`.tmTheme`/`.sublime-syntax` file from the Parrot bundle is copied into this
  repository, ever, for any language.
- Adding a new highlighted language later: check whether `syntect`'s bundled defaults already
  cover it (`SyntaxSet::load_defaults_newlines()` — see `parrot-syntax`'s wrapper) before
  reaching for any other grammar source; if a new grammar file is added directly to this repo
  for any reason, add a resolution line here first — `scripts/check_licenses.sh` enforces that.

## Icon font: Phosphor (via `egui-phosphor`)

The UI's icons come from the Phosphor icon font (regular weight), embedded by the
`egui-phosphor` 0.7 crate (MIT OR Apache-2.0). The Phosphor icon set itself is MIT-licensed
(© Phosphor Icons). No icon file is copied into this repository; the font ships inside the crate.
**Reused under MIT — resolved.**
