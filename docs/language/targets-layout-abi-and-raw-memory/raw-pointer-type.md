# Raw pointer types

`DataPointer<T>` and `FunctionPointer<F>` are compiler-known protected-representation value types.
Both are copyable raw addresses. Copying one copies its address without copying, borrowing, moving, initializing,
destroying or otherwise affecting reached storage or a function provider.

`DataPointer<T>` addresses data storage. Its element type can be a complete value type, an incomplete bodyless struct,
or a product with flexible trailing storage. A stored callable value is data, so `DataPointer<F>` addresses storage
containing a callable value rather than the function entry itself.

`FunctionPointer<F>` represents a raw function entry address intended for contract `F`. It is not invocable.
`core.memory.callable_from_pointer<F>` creates an ordinary callable value only after its exact callable contract,
ABI, target support and provider-liveness obligations are established. `F` must then be an ABI-qualified capture-free
callable type. `core.memory.function_address<F>` obtains a function entry address from such a callable.

Neither raw type owns storage or a provider, extends a lifetime, grants ordinary borrow protection, or establishes
validity, alignment, initialization, aliasing, provenance or access authority. Either can contain a native null, dangling,
sentinel or otherwise unusable address. A null check alone establishes no memory or callable authority.

Raw pointers have no ordinary dereference, indexing, field-access or arithmetic syntax. They do not implicitly convert
to integers or between data and function addresses. Use compiler-known memory operations or wrappers with the same
contracts. Ordinary assignment, parameter passing and return copy raw addresses under their dependency contracts.

Operations impose the storage conditions they need. Element offsets, allocation, reads, writes and copies require
complete fixed-size stored values. Byte offsets and `byte_address` can operate on incomplete data without inspecting its
layout. These data operations accept `DataPointer<T>` and reject `FunctionPointer<F>` by ordinary type checking.

The three representation conversions under `core.memory.function` require `layout_reinterpret` and target support.
They retain existing dependencies and grant no validity or callable authority. Targets determine data and function
address representations. An unsupported conversion is rejected.

Ordinary APIs express absence with the existing nullable type `T?` and `none`, and failures with `Result`. Native
wrappers translate their actual null, sentinel or error conventions and expose owners, borrows or callable values when
their contracts permit them. An absent `DataPointer<T>?` differs from a present raw null pointer. No single-pointer
representation or nullable foreign-ABI representation is implied.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Foreign data and symbols](foreign-data-and-symbols.md)
- Next: [Core memory declarations](core-memory-declarations.md)
