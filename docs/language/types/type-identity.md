# Type identity

A named type has identity.

Two named types with the same representation are still distinct types unless a declared conversion, behavioral contract,
or other language rule relates them.

A structural type form produces a type according to its type-form rules.

Examples:

```bray
(i32, i32)
[i32; 4]
box[Heap] Point
func(left: i32, right: i32) -> i32
```

The identity of a type-form type includes the type form and its type-form arguments.

For example:

```bray
box[Heap] Point
box[ArenaStorage] Point
```

are distinct types because the storage policy argument differs.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Overview](overview.md)
- Next: [Type categories](type-categories.md)
