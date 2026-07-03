# Pattern resolution

## Bare identifiers in patterns

A bare identifier in a pattern first resolves in pattern context.

```bray
value
```

If the identifier resolves to a pattern-capable declaration, the pattern uses that declaration.

If the identifier does not resolve to a pattern-capable declaration, it introduces a new binding.

Pattern-capable declarations include constants, no-payload variants, payload variants when the pattern uses payload syntax, built-in pattern names, and other declarations that explicitly define pattern behavior.

Functions, ordinary values, modules, and non-pattern declarations are not pattern-capable just because their names are visible.

If pattern resolution is ambiguous, the pattern is rejected.

A binding pattern receives its name only after pattern resolution fails to find a pattern-capable declaration.

This means ordinary binding syntax stays compact while named pattern forms do not require leading punctuation.

Named constants, variants, and other pattern-capable declarations can also be matched through qualified paths.

```bray
Color.Red
Token.EndOfInput
```

## Path patterns

A path pattern matches a named constant, no-payload variant, or other pattern-capable named declaration.

```bray
Color.Red
Token.EndOfInput
```

A path pattern uses normal Bray path resolution.

A path pattern has no payload bindings unless the resolved declaration is a payload-carrying pattern form.

An unqualified identifier can also resolve to a pattern-capable declaration according to pattern-context name resolution.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Core pattern forms](core-pattern-forms.md)
- Next: [Binding patterns](binding-patterns.md)
