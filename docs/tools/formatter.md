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

The [generated rule reference](formatter-rules.md) lists every rule's default, typed parameter schemas, owned decisions,
decision-specific dependencies, recorded conflicts, and precedence. The library's `FormatterRule::descriptor()` registry
owns that reference, configuration validation, and the rule inventory. Regenerate it with `cargo xtask format reference`
and check it with `cargo xtask format reference check`.

All rules are enabled by default except `simplify-nested-if`. That rule enables a syntax rewrite, so it requires an
explicit opt-in. Rule overrides are independent of JSON field order. When wrapping is disabled, list delimiter spacing
still applies, but width-dependent list layout does not.

See [formatter design](../design/formatter.md) for rule composition and preservation choices and [the Bray style
guide](../contributing/bray-style-guide.md) for source conventions.
