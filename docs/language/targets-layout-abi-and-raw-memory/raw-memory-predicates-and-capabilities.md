# Raw memory predicates and capabilities

The compiler-known trusted raw memory predicates are under `core.memory`.

```bray
trusted predicate valid_read<T>(pointer: RawPointer<T>, count: usize);

trusted predicate valid_write<T>(pointer: RawPointer<T>, count: usize);

trusted predicate initialized_as<T>(pointer: RawPointer<T>);

trusted predicate initialized_range_as<T>(pointer: RawPointer<T>, count: usize);

trusted predicate aligned_for<T>(pointer: RawPointer<T>);

trusted predicate non_overlapping<T>(
    left: RawPointer<T>,
    left_count: usize,
    right: RawPointer<T>,
    right_count: usize,
);

trusted predicate owned_allocation(pointer: RawPointer<u8>, bytes: usize, align: usize);

trusted predicate same_allocation<T>(left: RawPointer<T>, right: RawPointer<T>);
```

`valid_read` means the range can be read as raw storage for `count` values of `T`.

`valid_write` means the range can be written as raw storage for `count` values of `T`.

`initialized_as` means the pointed-to storage contains an initialized value of type `T`.

`initialized_range_as` means the pointed-to range contains `count` initialized values of type `T`.

`aligned_for` means the pointer satisfies the alignment requirements of `T`.

`non_overlapping` means the two typed ranges cannot overlap.

`owned_allocation` means the caller owns the allocation described by the pointer, byte count, and alignment.

`same_allocation` means both pointers are derived from the same allocation.

`shared_alias_valid` and `exclusive_alias_valid` state that creating the corresponding language
borrow preserves ordinary alias rules. `epoch_current` ties the pointer to its current allocation
or mapping epoch. `synchronized_access` states that mutable access is covered by active
synchronization authority. `movement_stable` prevents relocation while a dependent borrow is
active. `finalization_pending` prevents access after finalization or destruction begins.

An `owned_allocation` condition can establish `aligned_for<T>` for pointers into the allocation when the allocation alignment, offset, and target type alignment prove the typed pointer is aligned for `T`.

Trusted raw memory conditions are tied to the allocation, storage state, pointer value, element type, count, alignment, capability state, and epoch they mention.

Trusted raw memory conditions are invalidated by deallocation, reallocation, movement out of raw storage, destruction, finalization, initialization-state changes, layout reinterpretation, device transfer, foreign calls, unchecked aliasing, or any other operation whose contract can affect the mentioned storage.

## Capability mapping

Raw memory operations use trusted capabilities according to the operation they perform.

| Operation kind | Required trusted capability |
| --- | --- |
| Raw read, write, and copy | `raw_memory` |
| Manual initialization or uninitialized storage handling | `unchecked_init` |
| Pointer reinterpretation across element types | `layout_reinterpret` |
| Manual allocation and deallocation | `manual_alloc` |
| Aliasing beyond ordinary proof power | `unchecked_alias` |
| Foreign memory or calls outside ordinary Bray semantics | `foreign_call` |
| Device-owned or accelerator-owned memory | `device_memory` |
| Compiler-recognized target operations | `intrinsic` |

A trusted declaration must declare exactly the trusted capabilities it uses.

A standard-library wrapper over a trusted raw memory operation must preserve the same trusted caller obligations unless it proves or establishes those obligations itself.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Manual allocation](manual-allocation.md)
- Next: [Uninitialized storage and anchored borrows](uninitialized-storage-and-anchored-borrows.md)
