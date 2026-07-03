# Layout contracts

A type can have a physical layout contract.

Default layout is compiler-defined.

Without `@layout(...)`, a product type's field declaration order and a union type's variant declaration order remain semantic, but physical field offsets, physical tag representation, padding, size, alignment, and representation optimizations are selected by the compiler.

Default layout is not public ABI.

The compiler can select different default layouts across targets, compiler versions, optimization profiles, and code generation strategies when Bray semantics are preserved.

Source code can depend on physical layout only when an explicit layout contract is declared.

Layout directives use `@layout(...)`.

```bray
@layout(mode, option = value, ...)
```

The first argument is the layout mode.

Remaining arguments are named layout options.

Only one `@layout(...)` directive can apply to a primary representation declaration.

`@layout(...)` attaches to primary representation declarations, not to arbitrary type expressions or implementation blocks.

For user-declared types, the primary representation declarations that accept `@layout(...)` are `struct` and `union`.

The layout modes are:

- `stable`,
- `c`,
- `transparent`.

`stable` declares a Bray-defined physical layout for the target layout profile.

`c` declares target C ABI-compatible physical layout.

`transparent` declares that a product type has the same physical layout and ABI as its single storage field.

The layout options are:

- `align = N`,
- `pack = N`,
- `tag = IntegerType`.

`align = N` raises the aggregate alignment to at least `N`.

`N` must be a compile-time integer constant and a power of two.

`align` is valid for `stable` and `c` layout.

For `c` layout, the requested alignment must be representable by the target C ABI.

`pack = N` caps field and payload alignment at `N`.

`N` must be a compile-time integer constant and a power of two.

`pack` is valid only for `stable` layout.

`pack` is valid only when the laid-out representation is plain storage.

Plain storage means every represented field and payload is recursively plain storage and the representation has no destructor, finalizer, scoped lifecycle declaration, ownership obligation, resource obligation, finalization obligation, borrow field, `box`, trait-view field, callable field, task field, or protected representation.

Packed fields and packed payload components can be loaded and stored by value according to the packed layout contract.

A packed field or payload component cannot be borrowed as `&T` or `&mut T` unless the compiler proves that the specific access is naturally aligned for `T`.

`tag = IntegerType` sets the physical tag type of a union.

`tag` is valid only for union layout.

The tag type must be an integer scalar type.

Layout directives do not change ownership, initialization, destruction, finalization, field access, variant access, method resolution, trait satisfaction, or contract semantics.

Padding bytes are not semantic values.

Padding bytes are not guaranteed initialized unless represented by explicit fields.

Reading, writing, or reinterpreting padding or representation bytes requires a trusted operation with the appropriate trusted capability.

Changing a public type's explicit layout contract is a public API and ABI change.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Target constraints and gates](target-constraints-and-gates.md)
- Next: [Product layout](product-layout.md)
