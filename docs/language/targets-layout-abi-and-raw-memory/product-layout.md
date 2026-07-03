# Product layout

The default physical layout of a product type is compiler-defined.

The compiler can choose a layout that preserves Bray semantics.

Field declaration order is part of the semantic representation, but default physical layout is selected by the compiler.

Product layout is declared with `@layout(...)` immediately before the `struct` declaration.

```bray
@layout(stable)
struct Header
{
    magic: u32;
    version: u16;
}
```

Product types accept these layout modes:

- `stable`,
- `c`,
- `transparent`.

`stable` product layout uses field declaration order as physical field order and defines deterministic padding, field offsets, size, and alignment for the target layout profile.

```bray
@layout(stable, align = 16)
struct Vec4
{
    x: r32;
    y: r32;
    z: r32;
    w: r32;
}
```

`c` product layout uses the target C ABI for product field layout.

```bray
@layout(c)
struct CPoint
{
    x: r32;
    y: r32;
}
```

`transparent` product layout is valid only for a product with exactly one storage field.

```bray
@layout(transparent)
struct UserId
{
    value: u64;
}
```

A transparent product has the same layout and ABI as its storage field.

Transparent layout cannot be combined with `align`, `pack`, or any other layout option.

Product layout does not change field names, field order as semantic representation, ownership, borrowing, initialization, lifecycle behavior, construction, field access, pattern matching, method resolution, or trait satisfaction.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Layout contracts](layout-contracts.md)
- Next: [Union layout](union-layout.md)
