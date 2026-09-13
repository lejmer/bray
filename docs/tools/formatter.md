# Formatter configuration

`brayfmt` accepts a strict JSON configuration. `bray fmt` uses the workspace's `formatter_configuration` path or an
explicit `--config` override. Relative paths resolve against the workspace directory. Invalid explicit configuration is
an error, not a request for defaults.

```json
{
  "maximum_line_width": 100,
  "rules": {
    "simplify-nested-if": true,
    "line-wrapping": false
  }
}
```

Both fields are optional. `maximum_line_width` is an integer from 1 through 65535 and defaults to 120. `rules` maps
exact rule names to Boolean overrides. Unknown fields and rule names are rejected. Omitted rules retain their defaults.

All rules below are enabled by default except `simplify-nested-if`. That rule enables a syntax rewrite, so it requires
an explicit opt-in.

| Rule                         | Purpose                                        |
|------------------------------|------------------------------------------------|
| `indentation`                | Indentation within nested constructs           |
| `block-braces`               | Block brace placement                          |
| `module-item-spacing`        | Blank lines between module items               |
| `callable-member-spacing`    | Blank lines between callable members           |
| `directive-line-breaks`      | Breaks after directives                        |
| `block-paragraph-spacing`    | Paragraph spacing in executable blocks         |
| `match-case-spacing`         | Spacing between cases                          |
| `match-arm-body-layout`      | Simple match-arm body layout                   |
| `struct-construction-layout` | Construction field layout                      |
| `overload-arm-layout`        | Overload entry layout                          |
| `parenthesized-list-layout`  | Parenthesized lists                            |
| `bracketed-list-layout`      | Bracketed lists                                |
| `generic-list-layout`        | Generic lists                                  |
| `trailing-comma-layout`      | Multiline trailing commas                      |
| `comma-spacing`              | Comma spacing                                  |
| `colon-spacing`              | Colon spacing                                  |
| `operator-spacing`           | Infix operator spacing                         |
| `generic-delimiter-spacing`  | Generic delimiter spacing                      |
| `member-access-spacing`      | Member access spacing                          |
| `range-spacing`              | Range operator spacing                         |
| `prefix-operator-spacing`    | Prefix operator spacing                        |
| `directive-marker-spacing`   | Directive marker spacing                       |
| `word-spacing`               | Adjacent word-like tokens                      |
| `semicolon-layout`           | Semicolon placement                            |
| `comment-placement`          | Comment placement and preservation             |
| `line-wrapping`              | Width-aware wrapping                           |
| `line-ending-style`          | Selected source line ending                    |
| `final-newline`              | One final line ending                          |
| `simplify-nested-if`         | Syntax-local nested conditional simplification |

See [formatter design](../design/formatter.md) for rule composition and preservation choices and [the Bray style
guide](../contributing/bray-style-guide.md) for source conventions.
