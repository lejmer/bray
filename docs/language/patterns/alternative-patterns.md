# Alternative patterns

An alternative pattern matches when any of its alternatives match.

```bray
Error | Cancelled
```

All alternatives are checked against the same subject type.

All alternatives bind the same set of names.

Each shared binding name has the same type and compatible ownership, borrowing, mutation, lifetime, and capability mode
in every alternative.

Alternative patterns produce one coherent binding environment for the matched region.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Box patterns](box-patterns.md)
- Next: [Refutability](refutability.md)
