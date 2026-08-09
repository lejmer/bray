# ABI-oriented memory helpers

ABI-oriented raw memory helpers are ordinary declarations that combine layout helpers, raw pointers, and explicit layout contracts.

They are used by FFI bindings, serialization code, binary parsers, packed representations, and target ABI adapters.

ABI-oriented helpers must state:

- the layout contract they rely on,
- the target ABI conditions they use,
- the pointer validity and alignment conditions they require,
- the initialization conditions they require or establish,
- the byte order, scalar representation, padding, and niche assumptions visible in their behavior,
- the trusted capabilities used by any raw memory access, layout reinterpretation, intrinsic, or foreign call.

ABI helpers can use `size_of<T>()`, `align_of<T>()`, `stride_of<T>()`, `layout_of<T>(count = count)`, `std.memory.offset`, and `std.memory.reinterpret` when their contracts preserve the required conditions.

ABI helpers for `@layout(c)`, `@layout(transparent)`, explicit alignment, packed layout, and explicit representation use the layout rules defined by this chapter.

ABI helpers for default-layout types cannot expose a stable external ABI unless the type's declaration contract makes that ABI stable.

ABI helpers cannot make an unstable layout stable by wrapping it.

ABI helpers cannot read padding bytes as meaningful values unless their contract states that the padding bytes are initialized and observable.

ABI helpers cannot manufacture initialized values by reinterpreting bytes unless their contract establishes the required `initialized_as<T>` or `initialized_range_as<T>` conditions.

ABI helpers that call foreign code or expose foreign-owned memory must preserve the FFI and foreign-call trust boundary rules.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Device memory](device-memory.md)
- Next: [Summary](summary.md)
