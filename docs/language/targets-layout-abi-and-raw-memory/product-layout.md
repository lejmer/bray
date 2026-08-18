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

`stable` product layout uses field declaration order as physical field order and defines deterministic padding, field
offsets, size, and alignment for the target layout profile.

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

## Bodyless opaque products

A bodyless struct has no forgeable fields, structural constructor, field access, or product pattern.

```bray
struct FILE;
```

Without an explicit layout size and alignment, it is incomplete. An incomplete product cannot exist by value, be
embedded as a stored field, be allocated as a typed slot, or participate in any operation that requires size, alignment,
stride, construction, movement, copying, or destruction. It can be named in contracts and used as a `RawPointer<T>`
pointee.

An explicit opaque layout makes the bodyless product complete for storage while preserving structural opacity.

```bray
@layout(stable, size = 40, align = 8)
struct NativeMutex;
```

`size` and `align` are both required. `transparent` and `pack` are invalid. The type has no implicit initialized value.
Storage is created through `Uninit<T>`, raw allocation, or uninitialized containing storage, and a trusted foreign or
memory contract establishes initialization before the storage becomes a `T` value.

An opaque `c` layout supplies storage size and alignment but does not by itself prove that the type can cross a callable
ABI by value. The selected target must also provide the required aggregate classification. Pointer use needs no by-value
classification.

## Flexible trailing storage

`[T; ..]` is an incomplete-extent array representation. It is valid only as the final stored field of a `@layout(c)`
struct.

```bray
@layout(c)
struct Packet
{
    length: usize;
    payload: [u8; ..];
}
```

The enclosing product has fixed alignment and a target-defined fixed prefix, followed by zero or more contiguous `T`
elements. It has no standalone by-value representation, structural constructor, ordinary copy, ordinary move, or product
pattern. The flexible field cannot have a default and cannot be borrowed or indexed without a contract that supplies the
exact live extent.

`T` has a complete fixed-size C data representation with plain storage. It has no ownership, borrow, callable,
protected-storage, destruction, finalization, or scoped-lifecycle behavior.

`align_of<Container>()` remains available. `size_of`, `stride_of`, and ordinary `layout_of` require complete fixed-size
types and are unavailable for the flexible product. `std.memory.trailing_layout_of<Container>(count)` computes the
checked complete storage layout for one container with `count` trailing elements. The zero-count byte size is the
trailing field offset.

## C bitfield bindings

C bitfields do not introduce a Bray field category or a `@bitfield` directive. A target-exact binding generator
represents each allocation unit as ordinary explicitly laid-out backing storage and emits ordinary checked mask and
shift accessors. This preserves the fact that a C bitfield has no independent address or borrow identity while keeping
target compiler layout policy out of the core field model.

Product layout does not change field names, field order as semantic representation, ownership, borrowing,
initialization, lifecycle behavior, construction, field access, pattern matching, method resolution, or trait
satisfaction.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Layout contracts](layout-contracts.md)
- Next: [Union layout](union-layout.md)
