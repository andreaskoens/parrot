# parrot-export

Conversion engine for exporting the AST (`parrot-markup`) to target formats.

## Conversion rule-table format (T-5-02)

This is this project's own schema — TOML, not a copy of Apple's binary-plist
`elementStyles` format — but it is informed by the confirmed real shape of
`Export Formats/Markdown/Conversion/Default.plist` (decoded via
`plutil -convert json`): a dispatch table keyed by AST node kind, each key
holding an ordered array of rules. Each rule has a `precondition` (an
expression string, evaluated by the DSL in `T-5-03`) and a `template` (the
`{{ }}`-delimited expression-DSL output template). Rules are tried **in
document order within their node-kind array; the first whose precondition
evaluates truthy wins** — exactly mirroring the real `{element,
precondition}` dispatch order confirmed in `Default.plist`.

```toml
[[code]]
precondition = "content.plainText.containsString(\"`\")"
template = "``{{ content.plainText.replace(\"\\u2028\", \"  \\n\") }}``"

[[code]]
precondition = "true"
template = "`{{ content.plainText.replace(\"\\u2028\", \"  \\n\") }}`"
```

A node kind with no matching rule (or no entry at all) falls through to a
caller-supplied default — the table never panics on a miss.

`RuleTable::load(toml_str)` parses a table; `RuleTable::dispatch(node_kind,
matches)` returns the first `Rule` in that node kind's list for which the
caller-supplied `matches` predicate returns `true` for the rule's
`precondition` string. This crate does not itself interpret precondition
text — that's the DSL evaluator's job (T-5-03) — so the table format and its
dispatch order can be tested independently of the expression language.

See `src/rules.rs` for the implementation and
`rules::tests::loads_and_dispatches_trivial_table` /
`rules::tests::first_match_wins` for the dispatch-order contract.
