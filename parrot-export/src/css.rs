//! CSS export styles (Ariadne-inspired): a hand-rolled stylesheet engine that turns a
//! user-supplied (or built-in) CSS text into per-element styles for every backend.
//!
//! The design mirrors the reference: `ExportTheme::from_css` resolves a fixed set of
//! selectors (`body`, `p`, `li`, `ul`, `ol`, `h1`-`h6`, `blockquote`, `pre`, `code`,
//! `a`, comma-separated) and `@page` size/margins into plain structs, keeping
//! the raw text for verbatim embedding. HTML and EPUB embed the raw stylesheet, so the
//! full language works there; the fixed formats (PDF, DOCX, RTF) apply the resolved
//! properties they have a native analogue for and ignore the rest.
//!
//! Every field that a backend default already covers is `Option`: `None` keeps the
//! backend's built-in behavior, so an empty stylesheet exports byte-identical output
//! to the unstyled path. Heading levels 4+ share the `h4` style.
//!
//! Supported properties: `color`, `font-size` (`pt`/`px`/`in`/`cm`/`mm`/`em`/`rem`/`%`,
//! bare numbers are points), `font-weight`, `font-style`, `text-align`, `line-height`,
//! `margin-bottom`, `text-indent`, `margin-left`, `margin-right`, `break-before` /
//! `break-after` (plus the `page-break-*` aliases), `font-family` (first stack entry).
//! `@page` supports `size` (named sizes, optional `portrait`/`landscape`, one or two
//! lengths) and `margin` (1–4 lengths) plus the four `margin-*` sides; named pages and
//! margin boxes are ignored. Lengths accept `pt`, `px` (96px = 72pt), `in`, `cm`, `mm`,
//! and bare numbers (points); `em`/`rem` resolve against the element's own size.

use parrot_core::TagKind;

/// Page sizes for `@page size`, in millimetres: (name, width, height).
const NAMED_SIZES_MM: &[(&str, f32, f32)] = &[
    ("a5", 148.0, 210.0),
    ("a4", 210.0, 297.0),
    ("a3", 297.0, 420.0),
    ("b5", 176.0, 250.0),
    ("b4", 250.0, 353.0),
    ("letter", 215.9, 279.4),
    ("legal", 215.9, 355.6),
    ("ledger", 279.4, 431.8),
];

const PT_PER_MM: f32 = 72.0 / 25.4;
const PX_PER_PT: f32 = 96.0 / 72.0;

/// Text alignment for a block element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// Which built-in PDF font family a CSS family name maps to. PDFs here only use the
/// always-available base-14 fonts (nothing to embed, no font files to license), so
/// names map to the closest of Helvetica, Times or Courier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfFamily {
    Sans,
    Serif,
    Mono,
}

/// Maps a CSS font name to the closest built-in PDF font family. `sans-serif` contains
/// `serif`, so sans is tested first. Unknown names (and cursive/fantasy) stay on the
/// format default.
pub fn pdf_family_for(name: &str) -> PdfFamily {
    let n = name.trim().to_lowercase();
    if n.contains("mono")
        || n.contains("courier")
        || n.contains("consola")
        || n.contains("menlo")
        || n == "monospace"
    {
        PdfFamily::Mono
    } else if n.contains("sans")
        || n.contains("arial")
        || n.contains("helvetica")
        || n.contains("verdana")
        || n.contains("trebuchet")
        || n.contains("tahoma")
        || n.contains("system")
    {
        PdfFamily::Sans
    } else if n.contains("serif")
        || n.contains("georgia")
        || n.contains("times")
        || n.contains("garamond")
        || n.contains("palatino")
        || n.contains("cambria")
    {
        PdfFamily::Serif
    } else {
        PdfFamily::Sans
    }
}

/// Resolved style for one export element. Optional fields mean "no opinion": the
/// backend keeps its own default, so empty stylesheets change nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct ElStyle {
    pub size_pt: f32,
    pub color: Option<(u8, u8, u8)>,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    pub line_height: Option<f32>,
    pub space_after_pt: Option<f32>,
    pub family: Option<String>,
    pub text_indent: Option<f32>,
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub break_before: bool,
    pub break_after: bool,
}

impl ElStyle {
    fn plain(size_pt: f32) -> Self {
        Self {
            size_pt,
            color: None,
            bold: false,
            italic: false,
            align: Align::Left,
            line_height: None,
            space_after_pt: None,
            family: None,
            text_indent: None,
            margin_left: None,
            margin_right: None,
            break_before: false,
            break_after: false,
        }
    }
}

/// Page setup from an `@page` rule, in millimetres. Every field is optional: `None`
/// keeps the backend's default page. Margins are top/right/bottom/left.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageStyle {
    pub size: Option<(f32, f32)>,
    pub margin: [Option<f32>; 4],
}

/// A stylesheet for exports: resolved element styles plus the raw CSS, which HTML and
/// EPUB embed verbatim (full fidelity there).
#[derive(Debug, Clone)]
pub struct ExportTheme {
    pub h1: ElStyle,
    pub h2: ElStyle,
    pub h3: ElStyle,
    pub h4: ElStyle,
    pub body: ElStyle,
    pub quote: ElStyle,
    pub code: ElStyle,
    pub link_color: Option<(u8, u8, u8)>,
    pub page: Option<PageStyle>,
    pub raw_css: String,
}

impl ExportTheme {
    /// The unstyled look: exactly what the backends do without a stylesheet (11pt body,
    /// 20/16/14pt bold headings), so themed exports with an empty stylesheet are
    /// byte-identical to unthemed ones.
    pub fn builtin() -> Self {
        let mut h1 = ElStyle::plain(20.0);
        h1.bold = true;
        let mut h2 = ElStyle::plain(16.0);
        h2.bold = true;
        let mut h3 = ElStyle::plain(14.0);
        h3.bold = true;
        let mut h4 = ElStyle::plain(12.0);
        h4.bold = true;
        Self {
            h1,
            h2,
            h3,
            h4,
            body: ElStyle::plain(11.0),
            quote: ElStyle::plain(11.0),
            code: ElStyle::plain(11.0),
            link_color: None,
            page: None,
            raw_css: String::new(),
        }
    }

    /// Resolves `css` into element styles. Unknown selectors, properties and values are
    /// skipped, so a stylesheet never fails to parse — at worst it changes nothing.
    pub fn from_css(css: &str) -> Self {
        let mut theme = Self::builtin();
        theme.raw_css = css.to_string();
        for (selector, decls) in Self::parse_rules(css) {
            for part in selector.split(',') {
                // `em` lengths resolve against the body's size, captured fresh so an
                // earlier `body` rule in the same stylesheet already applies.
                let base = theme.body.size_pt;
                let element: Option<&mut ElStyle> = match part.trim() {
                    "h1" => Some(&mut theme.h1),
                    "h2" => Some(&mut theme.h2),
                    "h3" => Some(&mut theme.h3),
                    "h4" | "h5" | "h6" => Some(&mut theme.h4),
                    "blockquote" => Some(&mut theme.quote),
                    "pre" | "code" => Some(&mut theme.code),
                    "body" | "p" | "li" | "ul" | "ol" => Some(&mut theme.body),
                    "a" => {
                        for (prop, val) in &decls {
                            if prop == "color" {
                                if let Some(c) = Self::parse_color(val) {
                                    theme.link_color = Some(c);
                                }
                            }
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(el) = element {
                    Self::apply_decls(el, &decls, base);
                }
            }
        }
        // Elements without their own color or side indents inherit the body's; every
        // element except code inherits the body's family (code keeps monospace).
        let body = theme.body.clone();
        for el in [
            &mut theme.h1,
            &mut theme.h2,
            &mut theme.h3,
            &mut theme.h4,
            &mut theme.quote,
            &mut theme.code,
        ] {
            if el.color.is_none() {
                el.color = body.color;
            }
            if el.text_indent.is_none() {
                el.text_indent = body.text_indent;
            }
            if el.margin_left.is_none() {
                el.margin_left = body.margin_left;
            }
            if el.margin_right.is_none() {
                el.margin_right = body.margin_right;
            }
        }
        for el in [
            &mut theme.h1,
            &mut theme.h2,
            &mut theme.h3,
            &mut theme.h4,
            &mut theme.quote,
        ] {
            if el.family.is_none() {
                el.family = body.family.clone();
            }
        }
        // Only the plain `@page` rule applies; named pages are ignored for now.
        for (selector, decls) in Self::parse_page_blocks(css) {
            if selector.is_empty() {
                let mut page = theme.page.unwrap_or_default();
                Self::apply_page_decls(&mut page, &decls);
                theme.page = Some(page);
            }
        }
        theme
    }

    /// The element style for a block-level tag, or the body style for plain paragraphs.
    /// Heading levels 4 and up share the h4 style.
    pub fn element_for(&self, kind: Option<&TagKind>) -> &ElStyle {
        match kind {
            Some(TagKind::Heading(1)) => &self.h1,
            Some(TagKind::Heading(2)) => &self.h2,
            Some(TagKind::Heading(3)) => &self.h3,
            Some(TagKind::Heading(_)) => &self.h4,
            Some(TagKind::BlockQuote) => &self.quote,
            Some(TagKind::CodeBlock(_)) => &self.code,
            _ => &self.body,
        }
    }

    /// Page dimensions in millimetres: the `@page` size, else the caller's default.
    pub fn page_dims_mm(&self, default_mm: (f32, f32)) -> (f32, f32) {
        self.page
            .as_ref()
            .and_then(|p| p.size)
            .unwrap_or(default_mm)
    }

    /// One page margin in millimetres (0 = top, 1 = right, 2 = bottom, 3 = left),
    /// else the caller's default.
    pub fn page_margin_mm(&self, side: usize, default_mm: f32) -> f32 {
        self.page
            .as_ref()
            .and_then(|p| p.margin.get(side).copied().flatten())
            .unwrap_or(default_mm)
    }

    fn apply_decls(el: &mut ElStyle, decls: &[(String, String)], base_pt: f32) {
        for (prop, val) in decls {
            match prop.as_str() {
                "color" => {
                    if let Some(c) = Self::parse_color(val) {
                        el.color = Some(c);
                    }
                }
                "font-size" => {
                    if let Some(pt) = Self::parse_size_pt(val, base_pt.max(el.size_pt)) {
                        if pt > 0.0 {
                            el.size_pt = pt;
                        }
                    }
                }
                "font-weight" => {
                    let lowered = val.to_lowercase();
                    let v: &str = &lowered;
                    el.bold = matches!(v, "bold" | "bolder" | "700" | "800" | "900")
                        || v.parse::<u32>().is_ok_and(|n| n >= 700);
                    if v == "normal" {
                        el.bold = false;
                    }
                }
                "font-style" => {
                    let v = val.to_lowercase();
                    el.italic = v == "italic" || v == "oblique";
                }
                "text-align" => {
                    el.align = match val.to_lowercase().as_str() {
                        "center" => Align::Center,
                        "right" => Align::Right,
                        "justify" => Align::Justify,
                        _ => Align::Left,
                    };
                }
                "line-height" => {
                    if let Some(m) = Self::parse_line_height(val, el.size_pt) {
                        el.line_height = Some(m);
                    }
                }
                "margin-bottom" => {
                    if let Some(pt) = Self::parse_size_pt(val, el.size_pt) {
                        el.space_after_pt = Some(pt);
                    }
                }
                "text-indent" => {
                    if let Some(pt) = Self::parse_element_length(val, el.size_pt) {
                        el.text_indent = Some(pt);
                    }
                }
                "margin-left" => {
                    if let Some(pt) = Self::parse_element_length(val, el.size_pt) {
                        el.margin_left = Some(pt);
                    }
                }
                "margin-right" => {
                    if let Some(pt) = Self::parse_element_length(val, el.size_pt) {
                        el.margin_right = Some(pt);
                    }
                }
                "break-before" | "page-break-before" => {
                    el.break_before =
                        matches!(val.trim().to_lowercase().as_str(), "always" | "page");
                }
                "break-after" | "page-break-after" => {
                    el.break_after =
                        matches!(val.trim().to_lowercase().as_str(), "always" | "page");
                }
                "font-family" => {
                    // First family in the stack; `inherit` etc. mean "no opinion".
                    let first = val.split(',').next().unwrap_or("").trim();
                    let name = first
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .or_else(|| first.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                        .unwrap_or(first)
                        .trim();
                    el.family = match name.to_lowercase().as_str() {
                        "" | "inherit" | "initial" | "unset" => None,
                        _ => Some(name.to_string()),
                    };
                }
                _ => {}
            }
        }
    }

    fn apply_page_decls(style: &mut PageStyle, decls: &[(String, String)]) {
        for (prop, val) in decls {
            match prop.as_str() {
                "size" => {
                    if let Some(size) = Self::parse_page_size(val) {
                        style.size = Some(size);
                    }
                }
                "margin" => {
                    if let Some(m) = Self::parse_margin_shorthand(val, Self::parse_page_length) {
                        style.margin = m.map(Some);
                    }
                }
                "margin-top" => {
                    if let Some(mm) = Self::parse_page_length(val) {
                        style.margin[0] = Some(mm);
                    }
                }
                "margin-right" => {
                    if let Some(mm) = Self::parse_page_length(val) {
                        style.margin[1] = Some(mm);
                    }
                }
                "margin-bottom" => {
                    if let Some(mm) = Self::parse_page_length(val) {
                        style.margin[2] = Some(mm);
                    }
                }
                "margin-left" => {
                    if let Some(mm) = Self::parse_page_length(val) {
                        style.margin[3] = Some(mm);
                    }
                }
                _ => {}
            }
        }
    }

    /// Splits a stylesheet into `(selector, declarations)` pairs, dropping
    /// `/* comments */`. At-rules other than `@page` (handled separately) are skipped.
    fn parse_rules(css: &str) -> Vec<(String, Vec<(String, String)>)> {
        let mut clean = String::with_capacity(css.len());
        let mut chars = css.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                let mut prev_star = false;
                for c2 in chars.by_ref() {
                    if prev_star && c2 == '/' {
                        break;
                    }
                    prev_star = c2 == '*';
                }
            } else {
                clean.push(c);
            }
        }
        let mut rules = Vec::new();
        for chunk in clean.split('}') {
            let Some((sel, decls)) = chunk.split_once('{') else {
                continue;
            };
            let selector = sel.trim().to_lowercase();
            if selector.is_empty() || selector.starts_with('@') {
                continue;
            }
            let parsed = Self::parse_decls(decls);
            if !parsed.is_empty() {
                rules.push((selector, parsed));
            }
        }
        rules
    }

    /// Splits one declaration block (`prop: value; …`) into pairs. Property names are
    /// lowercased; values keep their case (font names are case-sensitive).
    fn parse_decls(block: &str) -> Vec<(String, String)> {
        let mut parsed = Vec::new();
        for decl in block.split(';') {
            let Some((prop, val)) = decl.split_once(':') else {
                continue;
            };
            let prop = prop.trim().to_lowercase();
            let val = val.trim().to_string();
            if !prop.is_empty() && !val.is_empty() {
                parsed.push((prop, val));
            }
        }
        parsed
    }

    /// Finds `@page` blocks: `(selector after @page, declarations)`. Nested margin
    /// boxes (`@top-center` etc.) are stripped; only the plain `@page` rule is applied.
    fn parse_page_blocks(css: &str) -> Vec<(String, Vec<(String, String)>)> {
        let b = css.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let is_at = b[i] == b'@'
                && css
                    .get(i..i + 5)
                    .is_some_and(|s| s.eq_ignore_ascii_case("@page"))
                && b.get(i + 5)
                    .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'-' || *c == b'_'));
            if !is_at {
                i += 1;
                continue;
            }
            let mut j = i + 5;
            while j < b.len() && (b[j] as char).is_whitespace() {
                j += 1;
            }
            let sel_start = j;
            while j < b.len() && b[j] != b'{' {
                j += 1;
            }
            if j >= b.len() {
                break;
            }
            let selector = css.get(sel_start..j).unwrap_or("").trim().to_lowercase();
            let mut depth = 0usize;
            let mut k = j;
            while k < b.len() {
                if b[k] == b'{' {
                    depth += 1;
                } else if b[k] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                k += 1;
            }
            if k >= b.len() {
                break;
            }
            let body = css.get(j + 1..k).unwrap_or("");
            let decls = Self::parse_decls(&Self::strip_nested_at_blocks(body));
            out.push((selector, decls));
            i = k + 1;
        }
        out
    }

    /// Removes nested `@name { … }` blocks (margin boxes) from an @page body.
    fn strip_nested_at_blocks(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'@' {
                let mut j = i;
                while j < b.len() && b[j] != b'{' && b[j] != b';' {
                    j += 1;
                }
                if j < b.len() && b[j] == b'{' {
                    let mut depth = 0usize;
                    let mut k = j;
                    while k < b.len() {
                        if b[k] == b'{' {
                            depth += 1;
                        } else if b[k] == b'}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        k += 1;
                    }
                    i = (k + 1).min(b.len());
                    continue;
                }
            }
            // `i` always sits on a char boundary: it starts at 0 and only
            // ever advances past ASCII bytes or whole chars.
            if let Some(ch) = s[i..].chars().next() {
                out.push(ch);
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        out
    }

    /// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (alpha is ignored), `rgb()`/`rgba()`
    /// (comma-separated, numbers or percentages; alpha is ignored), or a common color
    /// name. Anything else is not a color.
    fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
        let lowered = s.trim().to_lowercase();
        let v = lowered.as_str();
        if let Some(hex) = v.strip_prefix('#') {
            let digits: Vec<char> = hex.chars().collect();
            let expand = |c: char| c.to_digit(16).map(|d| (d * 17) as u8);
            return match digits.as_slice() {
                [r, g, b] => Some((expand(*r)?, expand(*g)?, expand(*b)?)),
                [r, g, b, _] => Some((expand(*r)?, expand(*g)?, expand(*b)?)),
                [r1, r2, g1, g2, b1, b2] => {
                    let n = |a: char, b: char| Some((a.to_digit(16)? * 16 + b.to_digit(16)?) as u8);
                    Some((n(*r1, *r2)?, n(*g1, *g2)?, n(*b1, *b2)?))
                }
                [r1, r2, g1, g2, b1, b2, _, _] => {
                    let n = |a: char, b: char| Some((a.to_digit(16)? * 16 + b.to_digit(16)?) as u8);
                    Some((n(*r1, *r2)?, n(*g1, *g2)?, n(*b1, *b2)?))
                }
                _ => None,
            };
        }
        if let Some(inner) = v
            .strip_prefix("rgba(")
            .or_else(|| v.strip_prefix("rgb("))
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() >= 3 {
                let num = |s: &str| {
                    let s = s.trim();
                    if let Some(pct) = s.strip_suffix('%') {
                        pct.trim().parse::<f32>().ok().map(|n| (n * 2.55) as u8)
                    } else {
                        s.parse::<u8>().ok()
                    }
                };
                return Some((num(parts[0])?, num(parts[1])?, num(parts[2])?));
            }
            return None;
        }
        Some(match v {
            "black" => (0, 0, 0),
            "white" => (255, 255, 255),
            "red" => (255, 0, 0),
            "green" => (0, 128, 0),
            "blue" => (0, 0, 255),
            "yellow" => (255, 255, 0),
            "cyan" | "aqua" => (0, 255, 255),
            "magenta" | "fuchsia" => (255, 0, 255),
            "gray" | "grey" => (128, 128, 128),
            "darkgray" | "darkgrey" => (169, 169, 169),
            "lightgray" | "lightgrey" => (211, 211, 211),
            "orange" => (255, 165, 0),
            "purple" => (128, 0, 128),
            "brown" => (165, 42, 42),
            "navy" => (0, 0, 128),
            "teal" => (0, 128, 128),
            "olive" => (128, 128, 0),
            "maroon" => (128, 0, 0),
            _ => return None,
        })
    }

    /// Splits a number from its unit: `("12", "pt")`, `("1.5", "")`, or `None`.
    fn split_number_unit(s: &str) -> Option<(f32, &str)> {
        let s = s.trim();
        let end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '+' || c == '-'))
            .unwrap_or(s.len());
        if end == 0 {
            return None;
        }
        let value: f32 = s[..end].parse().ok()?;
        Some((value, s[end..].trim()))
    }

    /// A font size in points. `em`/`rem`/`%` resolve against `base_pt`; a bare number
    /// is points; anything else (or a negative) is not a size. Zero parses (margins
    /// need it); the `font-size` rule itself refuses a zero size.
    fn parse_size_pt(s: &str, base_pt: f32) -> Option<f32> {
        let (value, unit) = Self::split_number_unit(s)?;
        if value < 0.0 {
            return None;
        }
        Some(match unit.to_lowercase().as_str() {
            "" | "pt" => value,
            "px" => value / PX_PER_PT,
            "in" => value * 72.0,
            "cm" => value * 72.0 / 2.54,
            "mm" => value * 72.0 / 25.4,
            "em" | "rem" => value * base_pt,
            "%" => value * base_pt / 100.0,
            _ => return None,
        })
    }

    /// An element indent or side margin in points. Like [`Self::parse_size_pt`] but
    /// without `%` (relative to what would be ambiguous here).
    fn parse_element_length(s: &str, base_pt: f32) -> Option<f32> {
        let (value, unit) = Self::split_number_unit(s)?;
        if value < 0.0 {
            return None;
        }
        let pt = match unit.to_lowercase().as_str() {
            "" | "pt" => value,
            "px" => value / PX_PER_PT,
            "in" => value * 72.0,
            "cm" => value * 72.0 / 2.54,
            "mm" => value * 72.0 / 25.4,
            "em" | "rem" => value * base_pt,
            _ => return None,
        };
        Some(pt)
    }

    /// A page length in millimetres. No `em`/`rem`/`%` (no font context on a page).
    /// `q` is a quarter millimetre.
    fn parse_page_length(s: &str) -> Option<f32> {
        let (value, unit) = Self::split_number_unit(s)?;
        if value < 0.0 {
            return None;
        }
        let mm = match unit.to_lowercase().as_str() {
            "" | "pt" => value / PT_PER_MM,
            "px" => value / PX_PER_PT / PT_PER_MM,
            "in" => value * 25.4,
            "cm" => value * 10.0,
            "mm" => value,
            "q" => value * 0.25,
            _ => return None,
        };
        Some(mm)
    }

    /// A CSS line-height: a bare number or `%` is a multiple of the font size, a length
    /// is divided back down to one.
    fn parse_line_height(s: &str, size_pt: f32) -> Option<f32> {
        let (value, unit) = Self::split_number_unit(s)?;
        if value <= 0.0 {
            return None;
        }
        match unit.to_lowercase().as_str() {
            "" => Some(value),
            "%" => Some(value / 100.0),
            _ => Self::parse_size_pt(s, size_pt).map(|pt| pt / size_pt.max(0.01)),
        }
    }

    /// A CSS 1–4 value margin shorthand, returned as top/right/bottom/left. The
    /// `convert` function decides the unit system (points for elements, mm for pages).
    fn parse_margin_shorthand(s: &str, convert: fn(&str) -> Option<f32>) -> Option<[f32; 4]> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        let mut lengths = Vec::with_capacity(parts.len());
        for part in &parts {
            lengths.push(convert(part)?);
        }
        match lengths.as_slice() {
            [all] => Some([*all, *all, *all, *all]),
            [vertical, horizontal] => Some([*vertical, *horizontal, *vertical, *horizontal]),
            [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
            [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
            _ => None,
        }
    }

    /// An `@page size`: a named size, optionally followed by `portrait`/`landscape`,
    /// or one or two lengths (width, then optional height; one length is square).
    /// Returns millimetres.
    fn parse_page_size(s: &str) -> Option<(f32, f32)> {
        let mut words: Vec<&str> = s.split_whitespace().collect();
        if words.is_empty() {
            return None;
        }
        let mut landscape = false;
        words.retain(|w| {
            let lower = w.to_lowercase();
            if lower == "landscape" {
                landscape = true;
                false
            } else {
                lower != "portrait"
            }
        });
        if words.is_empty() {
            return None;
        }
        let named = NAMED_SIZES_MM
            .iter()
            .find(|(name, _, _)| *name == words[0].to_lowercase())
            .map(|(_, w, h)| (*w, *h));
        let dims = if let Some((w, h)) = named {
            if words.len() > 1 {
                return None;
            }
            (w, h)
        } else {
            let mut lengths = Vec::with_capacity(words.len());
            for word in &words {
                lengths.push(Self::parse_page_length(word)?);
            }
            match lengths.as_slice() {
                [side] => (*side, *side),
                [w, h] => (*w, *h),
                _ => return None,
            }
        };
        if dims.0 <= 0.0 || dims.1 <= 0.0 {
            return None;
        }
        Some(if landscape { (dims.1, dims.0) } else { dims })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(css: &str) -> ExportTheme {
        ExportTheme::from_css(css)
    }

    #[test]
    fn empty_stylesheet_is_the_builtin_look() {
        let t = theme("");
        assert_eq!(t.body.size_pt, 11.0);
        assert_eq!(t.h1.size_pt, 20.0);
        assert!(t.h1.bold);
        assert_eq!(t.raw_css, "");
        assert!(t.page.is_none());
        let unstyled = ExportTheme::builtin();
        assert_eq!(t.h1.size_pt, unstyled.h1.size_pt);
        assert_eq!(t.body.size_pt, unstyled.body.size_pt);
    }

    #[test]
    fn comments_and_at_rules_are_skipped() {
        let t = theme("/* a comment */ body { font-size: 12pt; } /* trailing");
        assert_eq!(t.body.size_pt, 12.0);
        let t = theme("@media print { body { font-size: 99pt; } } body { font-size: 13pt; }");
        assert_eq!(t.body.size_pt, 13.0);
        // Unclosed comment swallows the rest: nothing applies, nothing panics.
        let t = theme("body { font-size: 12pt; } /* never ends");
        assert_eq!(t.body.size_pt, 12.0);
    }

    #[test]
    fn comma_selectors_share_declarations_and_unknown_ones_are_ignored() {
        let t = theme("h1, h2 { font-size: 30pt; } marquee { color: red; }");
        assert_eq!(t.h1.size_pt, 30.0);
        assert_eq!(t.h2.size_pt, 30.0);
        assert_eq!(t.h3.size_pt, 14.0);
        assert_eq!(t.body.size_pt, 11.0);
    }

    #[test]
    fn lengths_accept_every_supported_unit() {
        // 96px = 72pt; 1in = 72pt; 2.54cm = 72pt; 25.4mm = 72pt.
        assert_eq!(ExportTheme::parse_size_pt("72pt", 11.0), Some(72.0));
        assert_eq!(ExportTheme::parse_size_pt("96px", 11.0), Some(72.0));
        assert_eq!(ExportTheme::parse_size_pt("1in", 11.0), Some(72.0));
        assert_eq!(ExportTheme::parse_size_pt("2.54cm", 11.0), Some(72.0));
        assert_eq!(ExportTheme::parse_size_pt("25.4mm", 11.0), Some(72.0));
        assert_eq!(ExportTheme::parse_size_pt("2em", 11.0), Some(22.0));
        assert_eq!(ExportTheme::parse_size_pt("200%", 11.0), Some(22.0));
        assert_eq!(ExportTheme::parse_size_pt("12", 11.0), Some(12.0));
        assert_eq!(ExportTheme::parse_size_pt("12vw", 11.0), None);
        assert_eq!(ExportTheme::parse_size_pt("-4pt", 11.0), None);
        assert_eq!(ExportTheme::parse_size_pt("huge", 11.0), None);
        // Page lengths come back in millimetres.
        assert_eq!(ExportTheme::parse_page_length("1in"), Some(25.4));
        assert_eq!(ExportTheme::parse_page_length("72pt"), Some(25.4));
        assert_eq!(ExportTheme::parse_page_length("2em"), None);
    }

    #[test]
    fn colors_cover_hex_functions_and_names() {
        assert_eq!(ExportTheme::parse_color("#fff"), Some((255, 255, 255)));
        assert_eq!(
            ExportTheme::parse_color("#1a2b3c"),
            Some((0x1a, 0x2b, 0x3c))
        );
        // 4- and 8-digit hex: the alpha nibble/byte is ignored.
        assert_eq!(ExportTheme::parse_color("#f008"), Some((255, 0, 0)));
        assert_eq!(
            ExportTheme::parse_color("#1a2b3c80"),
            Some((0x1a, 0x2b, 0x3c))
        );
        // Functional notation, including percentages; alpha is ignored.
        assert_eq!(
            ExportTheme::parse_color("rgb(255, 0, 0)"),
            Some((255, 0, 0))
        );
        assert_eq!(
            ExportTheme::parse_color("rgba(0, 128, 0, 0.5)"),
            Some((0, 128, 0))
        );
        assert_eq!(
            ExportTheme::parse_color("rgb(100%, 0%, 0%)"),
            Some((255, 0, 0))
        );
        assert_eq!(ExportTheme::parse_color("RED"), Some((255, 0, 0)));
        assert_eq!(ExportTheme::parse_color("grey"), Some((128, 128, 128)));
        assert_eq!(ExportTheme::parse_color("cyan"), Some((0, 255, 255)));
        assert_eq!(ExportTheme::parse_color("aqua"), Some((0, 255, 255)));
        assert_eq!(ExportTheme::parse_color("magenta"), Some((255, 0, 255)));
        assert_eq!(ExportTheme::parse_color("darkgrey"), Some((169, 169, 169)));
        assert_eq!(ExportTheme::parse_color("lightgray"), Some((211, 211, 211)));
        assert_eq!(ExportTheme::parse_color("#ff"), None);
        assert_eq!(ExportTheme::parse_color("chartreuse"), None);
        assert_eq!(ExportTheme::parse_color("silver"), None);
        assert_eq!(ExportTheme::parse_color(""), None);
    }

    #[test]
    fn font_weight_style_and_alignment_map() {
        let t = theme("p { font-weight: 700; font-style: italic; text-align: center; }");
        assert!(t.body.bold);
        assert!(t.body.italic);
        assert_eq!(t.body.align, Align::Center);
        let t = theme("p { font-weight: normal; text-align: justify; }");
        assert!(!t.body.bold);
        assert_eq!(t.body.align, Align::Justify);
        let t = theme("p { font-weight: bolder; }");
        assert!(t.body.bold);
    }

    #[test]
    fn family_takes_the_first_stack_entry_unquoted() {
        let t = theme("body { font-family: \"Helvetica Neue\", Helvetica, sans-serif; }");
        assert_eq!(t.body.family.as_deref(), Some("Helvetica Neue"));
        // `inherit` (and friends) mean "no opinion", not a family named inherit.
        let t = theme("body { font-family: inherit; }");
        assert_eq!(t.body.family, None);
        // Headings inherit the body's family; code keeps its own.
        let t =
            theme("body { font-family: Georgia, serif; } pre { font-family: Menlo, monospace; }");
        assert_eq!(t.h1.family.as_deref(), Some("Georgia"));
        assert_eq!(t.code.family.as_deref(), Some("Menlo"));
    }

    #[test]
    fn body_color_and_indents_cascade_to_elements() {
        let t = theme("body { color: #112233; margin-left: 24pt; } h1 { font-size: 30pt; }");
        assert_eq!(t.h1.color, Some((0x11, 0x22, 0x33)));
        assert_eq!(t.h1.size_pt, 30.0);
        assert_eq!(t.h1.margin_left, Some(24.0));
        assert_eq!(t.code.margin_left, Some(24.0));
        assert_eq!(
            t.code.family, None,
            "code keeps monospace, not the body family"
        );
    }

    #[test]
    fn margin_shorthand_expands_one_to_four_values() {
        let m = |s: &str| ExportTheme::parse_margin_shorthand(s, ExportTheme::parse_page_length);
        assert_eq!(m("1in"), Some([25.4, 25.4, 25.4, 25.4]));
        assert_eq!(
            m("1in 2cm"),
            Some([25.4, 20.0, 25.4, 20.0]),
            "vertical horizontal"
        );
        assert_eq!(
            m("1pt 2pt 3pt"),
            Some([
                1.0 / PT_PER_MM,
                2.0 / PT_PER_MM,
                3.0 / PT_PER_MM,
                2.0 / PT_PER_MM
            ])
        );
        assert_eq!(m("1pt 2pt 3pt 4pt 5pt"), None);
        assert_eq!(m("1pt bogus"), None);
    }

    #[test]
    fn page_rule_sets_size_and_margins() {
        let t = theme("@page { size: letter; margin: 1in; }");
        let page = t.page.as_ref().expect("page must be set");
        assert_eq!(page.size, Some((215.9, 279.4)));
        assert_eq!(
            page.margin,
            [Some(25.4), Some(25.4), Some(25.4), Some(25.4)]
        );
        assert_eq!(t.page_dims_mm((210.0, 297.0)), (215.9, 279.4));
        // Landscape swaps; individual sides override the shorthand.
        let t = theme("@page { size: A4 landscape; margin: 2cm; margin-top: 1in; }");
        assert_eq!(t.page_dims_mm((210.0, 297.0)), (297.0, 210.0));
        assert_eq!(t.page_margin_mm(0, 20.0), 25.4);
        assert_eq!(t.page_margin_mm(1, 20.0), 20.0);
        // Named @page rules and margin boxes are ignored, not errors.
        let t = theme(
            "@page :first { size: A3; } @page { @top-center { content: \"x\"; } margin: 0; }",
        );
        assert_eq!(t.page_dims_mm((1.0, 1.0)), (1.0, 1.0));
        // Unknown sizes leave the default alone.
        let t = theme("@page { size: poster; }");
        assert!(t.page.as_ref().and_then(|p| p.size).is_none());
    }

    #[test]
    fn element_for_maps_block_kinds() {
        let t = theme("h1 { font-size: 30pt; } blockquote { font-style: italic; }");
        assert_eq!(t.element_for(Some(&TagKind::Heading(1))).size_pt, 30.0);
        assert_eq!(t.element_for(Some(&TagKind::Heading(2))).size_pt, 16.0);
        assert!(t.element_for(Some(&TagKind::BlockQuote)).italic);
        assert_eq!(t.element_for(None).size_pt, 11.0);
    }

    #[test]
    fn line_height_accepts_factors_percents_and_lengths() {
        assert_eq!(ExportTheme::parse_line_height("1.5", 12.0), Some(1.5));
        assert_eq!(ExportTheme::parse_line_height("150%", 12.0), Some(1.5));
        assert_eq!(ExportTheme::parse_line_height("18pt", 12.0), Some(1.5));
        assert_eq!(ExportTheme::parse_line_height("0", 12.0), None);
        let t = theme("p { line-height: 2; margin-bottom: 0; space-after: 1px; }");
        assert_eq!(t.body.line_height, Some(2.0));
        assert_eq!(t.body.space_after_pt, Some(0.0));
    }

    #[test]
    fn family_maps_to_the_closest_pdf_builtin() {
        assert_eq!(pdf_family_for("Courier New"), PdfFamily::Mono);
        assert_eq!(pdf_family_for("Menlo"), PdfFamily::Mono);
        assert_eq!(pdf_family_for("Georgia"), PdfFamily::Serif);
        assert_eq!(pdf_family_for("Helvetica Neue"), PdfFamily::Sans);
        assert_eq!(pdf_family_for("Papyrus"), PdfFamily::Sans);
    }

    #[test]
    fn page_breaks_and_link_color_apply() {
        let t = theme(
            "h1 { break-before: always; } a { color: #0066cc; } h2 { page-break-after: page; }",
        );
        assert!(t.h1.break_before);
        assert!(t.h2.break_after);
        assert!(!t.h3.break_before);
        assert_eq!(t.link_color, Some((0x00, 0x66, 0xcc)));
    }
}
