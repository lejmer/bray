# Union layout

A union has a semantic active variant tag.

The compiler chooses the default physical tag representation and payload layout.

The default layout can use representation optimizations when Bray semantics are preserved.

Default layout is compiler-defined.

Union layout is declared with `@layout(...)` immediately before the `union` declaration.

```bray
@layout(stable, tag = u8)
union Message
{
    @tag(1)
    Ready;

    @tag(2)
    Data(bytes: [u8; 16]);
}
```

Union types accept these layout modes:

- `stable`,
- `c`.

`stable` union layout defines deterministic tag representation, payload layout, size, and alignment for the target layout profile.

If `tag` is omitted from a `stable` union layout, the tag type is the smallest fixed-width unsigned integer scalar type that can represent every variant tag value.

If no fixed-width unsigned integer scalar type can represent every variant tag value, the union must declare `tag` explicitly.

`c` union layout uses a C-compatible tagged aggregate representation for the target C ABI.

An explicitly laid out `c` union must declare `tag`.

```bray
@layout(c, tag = u32)
union CStatus
{
    @tag(0)
    Ok;

    @tag(1)
    Error(code: u32);
}
```

The `@tag(value)` directive declares a physical tag value for a union variant.

Tag values are compile-time integer constants.

Tag values must be representable by the union's physical tag type.

Tag values must be unique within the union.

If any variant in an explicitly laid out union uses `@tag`, every variant in that union must use `@tag`.

If no variant in an explicitly laid out union uses `@tag`, variant tag values are assigned by declaration order starting at `0`.

Union layout does not change variant names, the semantic active variant tag, ownership, borrowing, initialization, lifecycle behavior, variant access, pattern matching, method resolution, or trait satisfaction.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Product layout](product-layout.md)
- Next: [Layout helpers](layout-helpers.md)
