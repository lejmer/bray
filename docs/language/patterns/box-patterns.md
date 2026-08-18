# Box patterns

A `box` pattern matches through owned indirection.

```bray
box(inner)
```

A `box(inner)` pattern is valid for a subject of type `box[S] T`.

The inner pattern is checked against `T`.

The pattern operation determines whether matching observes, borrows, mutably borrows, copies, or consumes the contained
value.

A consuming `box(inner)` pattern consumes the box and moves through the owned indirection according to `box` ownership
rules.

A borrowing `box(inner)` pattern projects a borrow of the contained value according to the storage policy and the
`Storage<T>` behavior required by the box type.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Nullable patterns](nullable-patterns.md)
- Next: [Alternative patterns](alternative-patterns.md)
