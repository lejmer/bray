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
    Data(bytes: bytes<16>);
}
```

Union types accept these layout modes:

- `stable`,
- `c`.

`stable` union layout defines deterministic tag representation, payload layout, size, and alignment for the target
layout profile.

If `tag` is omitted from a `stable` union layout, the tag type is the smallest fixed-width unsigned integer scalar type
that can represent every variant tag value.

If no fixed-width unsigned integer scalar type can represent every variant tag value, the union must declare `tag`
explicitly.

`c` union layout uses either a C-compatible tagged aggregate representation or untagged overlapping storage for the
target C ABI.

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

`tag = none` selects untagged overlapping payload storage with no represented discriminant.

```bray
@layout(c, tag = none)
union NativeValue
{
    Integer(value: std.ffi.c.Int);
    Floating(value: r32);
    Pointer(value: RawPointer<u8>);
}
```

Each variant has exactly one payload field with no default. The payload has a target-supported plain C data
representation such as a scalar, raw pointer, C-layout aggregate, or compatible transparent wrapper. Empty variants,
multiple-field payloads, `@tag(...)`, ownership-bearing payloads, borrows, callables, destructors, finalizers, and
scoped lifecycle behavior are invalid.

The variant payloads overlap at offset zero. The union size is the maximum payload size rounded for the maximum payload
alignment, according to the selected target C ABI.

`tag = none` does not remove Bray's semantic active variant. Construction establishes one active variant, and moving or
copying a value preserves the flow-sensitive active-variant fact. The representation contains no data from which code
can recover that fact.

A foreign write, raw representation write, unclassified ABI result, or control-flow merge that does not preserve one
exact variant removes active-variant knowledge. Such storage is not semantically accessible as a payload and cannot be
matched until ordinary contract reasoning proves one variant or a trusted boundary explicitly accepts the active-variant
obligation. A trusted single-variant projection or pattern can establish the selected fact for its exact expression
scope.

An ordinary match cannot branch on an unrepresented tag. It is valid only when one exact active variant is already
known, in which case no runtime discriminant read is emitted.

The `@tag(value)` directive declares a physical tag value for a union variant.

Tag values are compile-time integer constants.

Tag values must be representable by the union's physical tag type.

Tag values must be unique within the union.

If any variant in an explicitly laid out union uses `@tag`, every variant in that union must use `@tag`.

If no variant in an explicitly laid out union uses `@tag`, variant tag values are assigned by declaration order starting
at `0`.

Union layout does not change variant names, semantic active-variant identity, ownership, borrowing, initialization,
method resolution, or trait satisfaction. `tag = none` changes only how active-variant knowledge is represented and
recovered, so payload access and pattern matching require an independently established active-variant fact.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Product layout](product-layout.md)
- Next: [Layout helpers](layout-helpers.md)
