# Layout helpers

ABI and layout helpers expose target-dependent layout conditions used by allocation, raw buffers, FFI support, and low-level storage code.

The standard-library layout helper family is under `std.memory`.

```bray
module std.memory;

struct MemoryLayout
{
    bytes: usize;
    align: usize;
}

union MemoryLayoutError
{
    SizeOverflow;
    UnsupportedAlignment;
}

const func size_of<T>() -> usize;

const func align_of<T>() -> usize;

const func stride_of<T>() -> usize;

const func layout_of<T>(count: usize) -> Result<MemoryLayout, MemoryLayoutError>;

const func trailing_layout_of<T>(count: usize) -> Result<MemoryLayout, MemoryLayoutError>;
```

`size_of<T>()` is the size in bytes of one initialized `T` value for the selected target profile.

`align_of<T>()` is the required alignment in bytes for `T` for the selected target profile.

`stride_of<T>()` is the byte distance between adjacent `T` elements in a contiguous typed allocation.

`layout_of<T>(count = count)` computes the allocation layout required for `count` contiguous `T` slots.

`trailing_layout_of<T>(count = count)` is valid when `T` is a `@layout(c)` product with one final `[Element; ..]` field. It
computes the complete allocation layout for one `T` prefix followed by `count` trailing elements. With `count = 0`, its byte count
is the trailing field offset.

These helpers observe the effective layout contract of `T`.

For user-declared product and union types, the effective layout contract comes from the type's `@layout(...)` directive when one is declared.

For user-declared product and union types without an explicit layout directive, the effective layout contract is the compiler-selected default layout for the selected target profile.

For compiler-known type forms and compiler-known protected-representation types, the effective layout contract is defined by the language rule that owns that type form or type.

The helper result for an explicit layout contract is stable according to that layout contract.

The helper result for compiler-defined default layout is valid for the selected target profile but does not create public ABI stability.

`MemoryLayout` values produced by `layout_of` carry the standard-library allocation-layout contract for their `bytes` and `align` fields.

Constructing a `MemoryLayout` value from arbitrary field values is valid only when the surrounding context proves the same allocation-layout contract.

`layout_of<T>(count = count)` returns `Result.Error(MemoryLayoutError.SizeOverflow)` when the byte count cannot be represented as `usize`.

`layout_of<T>(count = count)` returns `Result.Error(MemoryLayoutError.UnsupportedAlignment)` when the selected target cannot represent the required alignment for allocation.

`trailing_layout_of<T>(count = count)` reports the same overflow and unsupported-alignment failures. `size_of`, `align_of`,
`stride_of`, and `layout_of` are unavailable for incomplete types. `size_of`, `stride_of`, and `layout_of` are unavailable for a
product with flexible trailing storage, while `align_of` remains available for its fixed alignment.

The ABI and layout helper results are target-dependent constants when their inputs are constant.

The compiler records the target properties used by these helpers in compiled interface metadata.

These helpers do not read memory, write memory, allocate, deallocate, initialize storage, destroy values, create raw pointer validity conditions, or create allocation ownership conditions.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Union layout](union-layout.md)
- Next: [Callable ABI](callable-abi.md)
