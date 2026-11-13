//! Expression DSL (T-5-03): `{{ }}`-delimited templates with `if/end` conditionals and
//! method-chain expressions, reimplementing — with Rust-native semantics, not a literal port —
//! the confirmed real operations from `Export Formats/Markdown/Conversion/Default.plist`:
//! `.plainText`, `.replace()`, `.unencodedContents`, `.anchoredReplace()`.
//!
//! ASSUMPTION(D-export-1): this pipeline has no separate "encoded" content representation —
//! HTML-escaping, where a backend needs it, happens in that backend's own serializer, not here
//! — so `.plainText` and `.unencodedContents` are both identity accessors on the string value
//! already bound to `content`, rather than two distinct underlying representations.
//! `array.new().selectUsingPatterns()` is intentionally not implemented: no confirmed real rule
//! in `Default.plist` exercises it, and the roadmap's own fallback for this task allows shipping
//! only the confirmed function set as hardcoded builtins rather than a speculative
//! general-purpose language.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Bool(bool),
}

#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    pub content_plain_text: String,
}

#[derive(Debug, PartialEq)]
pub enum DslError {
    Parse(String),
    Eval(String),
}

impl fmt::Display for DslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DslError::Parse(msg) => write!(f, "template parse error: {msg}"),
            DslError::Eval(msg) => write!(f, "template eval error: {msg}"),
        }
    }
}

impl std::error::Error for DslError {}

enum Expr {
    Ident(String),
    Str(String),
    Access(Box<Expr>, String),
    Call(Box<Expr>, String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Dot,
    LParen,
    RParen,
    Comma,
}

fn tokenize(s: &str) -> Result<Vec<Token>, DslError> {
    let chars: Vec<char> = s.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '"' => {
                i += 1;
                let mut out = String::new();
                loop {
                    let c = *chars
                        .get(i)
                        .ok_or_else(|| DslError::Parse("unterminated string literal".into()))?;
                    i += 1;
                    if c == '"' {
                        break;
                    }
                    if c != '\\' {
                        out.push(c);
                        continue;
                    }
                    let esc = *chars
                        .get(i)
                        .ok_or_else(|| DslError::Parse("unterminated escape sequence".into()))?;
                    i += 1;
                    match esc {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        'u' => {
                            let hex: String = chars
                                .get(i..i + 4)
                                .ok_or_else(|| DslError::Parse("incomplete \\u escape".into()))?
                                .iter()
                                .collect();
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                                DslError::Parse(format!("invalid \\u escape: {hex}"))
                            })?;
                            out.push(char::from_u32(code).ok_or_else(|| {
                                DslError::Parse(format!("invalid unicode scalar: {hex}"))
                            })?);
                            i += 4;
                        }
                        other => return Err(DslError::Parse(format!("unknown escape: \\{other}"))),
                    }
                }
                tokens.push(Token::Str(out));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(DslError::Parse(format!("unexpected character: {other}"))),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    /// Parses one `primary ( '.' member )*` chain. Does not assert end-of-input — call sites
    /// that parse a nested argument expression rely on that, since trailing tokens (a `)` or
    /// `,`) are expected and handled by the caller.
    fn parse_chain(&mut self) -> Result<Expr, DslError> {
        let mut expr = match self.next() {
            Some(Token::Ident(name)) => Expr::Ident(name),
            Some(Token::Str(s)) => Expr::Str(s),
            other => {
                return Err(DslError::Parse(format!(
                    "expected identifier or string, got {other:?}"
                )))
            }
        };
        while matches!(self.peek(), Some(Token::Dot)) {
            self.next();
            let name = match self.next() {
                Some(Token::Ident(name)) => name,
                other => {
                    return Err(DslError::Parse(format!(
                        "expected property/method name after '.', got {other:?}"
                    )))
                }
            };
            if matches!(self.peek(), Some(Token::LParen)) {
                self.next();
                let mut args = Vec::new();
                if !matches!(self.peek(), Some(Token::RParen)) {
                    loop {
                        args.push(self.parse_chain()?);
                        if matches!(self.peek(), Some(Token::Comma)) {
                            self.next();
                        } else {
                            break;
                        }
                    }
                }
                match self.next() {
                    Some(Token::RParen) => {}
                    other => return Err(DslError::Parse(format!("expected ')', got {other:?}"))),
                }
                expr = Expr::Call(Box::new(expr), name, args);
            } else {
                expr = Expr::Access(Box::new(expr), name);
            }
        }
        Ok(expr)
    }
}

fn parse_expr_str(s: &str) -> Result<Expr, DslError> {
    let tokens = tokenize(s)?;
    if tokens.is_empty() {
        return Err(DslError::Parse("empty expression".into()));
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_chain()?;
    if parser.pos != parser.tokens.len() {
        return Err(DslError::Parse("trailing tokens after expression".into()));
    }
    Ok(expr)
}

fn eval(expr: &Expr, ctx: &EvalContext) -> Result<Value, DslError> {
    match expr {
        Expr::Ident(name) => match name.as_str() {
            "content" => Ok(Value::Str(ctx.content_plain_text.clone())),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err(DslError::Eval(format!("unknown identifier: {other}"))),
        },
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Access(base, prop) => {
            let base_val = eval(base, ctx)?;
            match (&base_val, prop.as_str()) {
                (Value::Str(_), "plainText" | "unencodedContents") => Ok(base_val),
                (Value::Str(_), other) => Err(DslError::Eval(format!(
                    "unknown property on string: {other}"
                ))),
                (Value::Bool(_), other) => Err(DslError::Eval(format!(
                    "cannot access property '{other}' on a boolean"
                ))),
            }
        }
        Expr::Call(base, method, args) => {
            let base_val = eval(base, ctx)?;
            let arg_vals = args
                .iter()
                .map(|a| eval(a, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            match &base_val {
                Value::Str(s) => call_string_method(s, method, &arg_vals),
                Value::Bool(_) => Err(DslError::Eval(format!(
                    "cannot call method '{method}' on a boolean"
                ))),
            }
        }
    }
}

fn expect_str_arg<'a>(args: &'a [Value], index: usize, method: &str) -> Result<&'a str, DslError> {
    match args.get(index) {
        Some(Value::Str(s)) => Ok(s),
        Some(Value::Bool(_)) => Err(DslError::Eval(format!(
            "{method}: argument {index} must be a string"
        ))),
        None => Err(DslError::Eval(format!(
            "{method}: missing argument {index}"
        ))),
    }
}

fn call_string_method(s: &str, method: &str, args: &[Value]) -> Result<Value, DslError> {
    match method {
        "containsString" => {
            let needle = expect_str_arg(args, 0, method)?;
            Ok(Value::Bool(s.contains(needle)))
        }
        "replace" => {
            let from = expect_str_arg(args, 0, method)?;
            let to = expect_str_arg(args, 1, method)?;
            Ok(Value::Str(s.replace(from, to)))
        }
        "anchoredReplace" => {
            let from = expect_str_arg(args, 0, method)?;
            let to = expect_str_arg(args, 1, method)?;
            Ok(Value::Str(anchored_replace(s, from, to)))
        }
        other => Err(DslError::Eval(format!("unknown method: {other}"))),
    }
}

/// Replaces only a *leading run* of `from` occurrences at the start of each line — the confirmed
/// real usage is converting leading space runs to non-breaking spaces so indentation survives
/// Markdown rendering — not a global replace (that's plain `.replace()`).
fn anchored_replace(s: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return s.to_string();
    }
    s.split('\n')
        .map(|line| {
            let mut rest = line;
            let mut out = String::new();
            while let Some(stripped) = rest.strip_prefix(from) {
                out.push_str(to);
                rest = stripped;
            }
            out.push_str(rest);
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

enum Segment {
    Text(String),
    If(Expr),
    End,
    Expr(Expr),
}

fn parse_template(template: &str) -> Result<Vec<Segment>, DslError> {
    let mut segments = Vec::new();
    let mut rest = template;
    loop {
        let Some(start) = rest.find("{{") else {
            if !rest.is_empty() {
                segments.push(Segment::Text(rest.to_string()));
            }
            break;
        };
        if start > 0 {
            segments.push(Segment::Text(rest[..start].to_string()));
        }
        let after_open = &rest[start + 2..];
        let end = after_open
            .find("}}")
            .ok_or_else(|| DslError::Parse("unterminated '{{' tag".into()))?;
        let inner = after_open[..end].trim();
        if inner == "end" {
            segments.push(Segment::End);
        } else if let Some(cond) = inner.strip_prefix("if ") {
            segments.push(Segment::If(parse_expr_str(cond.trim())?));
        } else {
            segments.push(Segment::Expr(parse_expr_str(inner)?));
        }
        rest = &after_open[end + 2..];
    }
    Ok(segments)
}

/// Evaluates a bare expression string (e.g. a rule table's `precondition` field) to a boolean —
/// the entry point a conversion engine's rule dispatch uses to actually evaluate preconditions,
/// rather than treating them as opaque strings (`rules::RuleTable::dispatch`'s own `matches`
/// predicate stays caller-supplied so the table format itself doesn't depend on this module).
pub fn eval_bool(expr: &str, ctx: &EvalContext) -> Result<bool, DslError> {
    let parsed = parse_expr_str(expr)?;
    match eval(&parsed, ctx)? {
        Value::Bool(b) => Ok(b),
        Value::Str(_) => Err(DslError::Eval(
            "precondition must evaluate to a boolean".into(),
        )),
    }
}

/// Renders `template` against `ctx`. `if`/`end` blocks may nest; a block whose condition (or
/// whose ancestor's condition) is false has its body skipped without evaluating expressions
/// inside it.
pub fn render(template: &str, ctx: &EvalContext) -> Result<String, DslError> {
    let segments = parse_template(template)?;
    let mut output = String::new();
    let mut active_stack: Vec<bool> = Vec::new();
    for seg in segments {
        let currently_active = active_stack.iter().all(|&b| b);
        match seg {
            Segment::Text(text) => {
                if currently_active {
                    output.push_str(&text);
                }
            }
            Segment::If(expr) => {
                let cond = if currently_active {
                    match eval(&expr, ctx)? {
                        Value::Bool(b) => b,
                        Value::Str(_) => {
                            return Err(DslError::Eval("if condition must be a boolean".into()))
                        }
                    }
                } else {
                    false
                };
                active_stack.push(cond);
            }
            Segment::End => {
                if active_stack.pop().is_none() {
                    return Err(DslError::Parse("unmatched 'end'".into()));
                }
            }
            Segment::Expr(expr) => {
                if currently_active {
                    match eval(&expr, ctx)? {
                        Value::Str(s) => output.push_str(&s),
                        Value::Bool(_) => {
                            return Err(DslError::Eval("cannot render a boolean as text".into()))
                        }
                    }
                }
            }
        }
    }
    if !active_stack.is_empty() {
        return Err(DslError::Parse("unmatched 'if' (missing 'end')".into()));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact real `code` rule template, decoded from the live
    /// `Markdown/Conversion/Default.plist` (see `parrot-export/README.md`): if the code span's
    /// own content contains a backtick, wrap it in double backticks instead of single.
    const CODE_TEMPLATE: &str = r#"{{ if content.plainText.containsString("`") }}`{{ end }}`{{ content.plainText.replace(" ", "  \n").unencodedContents.anchoredReplace(" ", " ") }}`{{ if content.plainText.containsString("`") }}`{{ end }}"#;

    #[test]
    fn code_rule_backtick_escaping_matches_confirmed_example() {
        let with_backtick = EvalContext {
            content_plain_text: "a`b".to_string(),
        };
        assert_eq!(
            render(CODE_TEMPLATE, &with_backtick).unwrap(),
            "``a`b``",
            "content containing a backtick must be wrapped in double backticks"
        );

        let without_backtick = EvalContext {
            content_plain_text: "no backticks here".to_string(),
        };
        assert_eq!(
            render(CODE_TEMPLATE, &without_backtick).unwrap(),
            "`no backticks here`",
            "content without a backtick must be wrapped in a single backtick"
        );
    }

    #[test]
    fn replace_converts_line_separator_and_anchored_replace_preserves_indentation() {
        let ctx = EvalContext {
            content_plain_text: "  two spaces then\u{2028}next line".to_string(),
        };
        let template = r#"{{ content.plainText.replace(" ", "  \n").unencodedContents.anchoredReplace(" ", " ") }}"#;
        let rendered = render(template, &ctx).unwrap();
        // The leading run of spaces becomes NBSPs; the mid-line U+2028 becomes two spaces plus a
        // real newline. Those two spaces land as *trailing* whitespace on the first line (before
        // the newline), so anchoredReplace's leading-run-only rule leaves them as plain spaces —
        // only "next line" starts the second line, and it has no leading space to replace.
        assert_eq!(rendered, "\u{a0}\u{a0}two spaces then  \nnext line");
    }

    #[test]
    fn unknown_identifier_is_an_error_not_a_panic() {
        let ctx = EvalContext::default();
        let err = render("{{ nope.plainText }}", &ctx).unwrap_err();
        assert!(matches!(err, DslError::Eval(_)));
    }

    #[test]
    fn unmatched_end_is_a_parse_error() {
        let ctx = EvalContext::default();
        let err = render("{{ end }}", &ctx).unwrap_err();
        assert_eq!(err, DslError::Parse("unmatched 'end'".to_string()));
    }

    #[test]
    fn literal_text_outside_tags_passes_through_unchanged() {
        let ctx = EvalContext::default();
        assert_eq!(
            render("plain text, no tags", &ctx).unwrap(),
            "plain text, no tags"
        );
    }
}
