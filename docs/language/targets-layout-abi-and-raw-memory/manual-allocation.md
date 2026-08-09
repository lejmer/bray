# Manual allocation

The compiler-known manual allocation declarations are under `core.memory`.

```bray
trusted func allocate(bytes: usize, align: usize) -> RawPointer<u8>
    ensures(
        trusted core.memory.owned_allocation(pointer = result, bytes = bytes, align = align),
        trusted core.memory.valid_write(pointer = result, count = bytes),
    )
    uses(manual_alloc);

trusted func deallocate(pos pointer: RawPointer<u8>, bytes: usize, align: usize) -> unit
    requires(
        trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align),
    )
    uses(manual_alloc);
```

`allocate` returns raw byte-addressed storage.

`allocate` does not initialize typed values in the returned storage.

On normal completion, `allocate` creates a distinct raw allocation described by the produced `owned_allocation` condition and writable by the produced `valid_write` condition.

If `allocate` cannot create an allocation satisfying its contract, it panics.

An allocation failure panic is an ordinary panic and can be caught by `catch`.

`allocate` must not return a null pointer or any other sentinel value to report allocation failure.

`deallocate` releases the allocation represented by its trusted ownership condition.

Before deallocation, the caller must satisfy all destruction, finalization, initialization, aliasing, and borrowing obligations for values stored in that allocation.

After deallocation, trusted guarantees that depend on the allocation are invalidated.

Raw pointers into a deallocated allocation can still exist as raw pointer values, but they carry no valid access conditions.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Raw memory access and copying](raw-memory-access-and-copying.md)
- Next: [Raw memory predicates and capabilities](raw-memory-predicates-and-capabilities.md)
