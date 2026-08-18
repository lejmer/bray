# Moves, copies, and consumption

A move transfers ownership obligations from one owner to another.

Moving a value transfers:

- initialized subvalues owned by the moved value,
- dependency contracts carried by the moved value,
- finalization and destruction obligations carried by the moved value,
- scoped capabilities carried by the moved value,
- type-specific ownership obligations carried by the moved value.

After a move, the old access path is moved from until it is reinitialized.

Moving from an access path requires ownership of the reached value and no incompatible active borrow.

Moving a complete product, union, tuple, array, nullable-present value, box, async computation, task handle, or other
owned aggregate moves the ownership obligations described by that type's rules.

Moving a borrow value moves only the borrow value.

Moving a borrow value never moves the reached storage.

A product-static or thread-local static value cannot be moved from or consumed through its declaration access path.
Moving a borrow or copying a copyable subvalue obtained from a static follows the ordinary operation and preserves every
product or exact-thread dependency carried by the result.

Moving a mutable borrow transfers its temporary mutation authority to the destination borrow value.

A consume operation is an ownership operation that ends ordinary use of the consumed value through the old access path
unless the operation returns or reinitializes a new value for that path.

A copy duplicates a value according to the value's copy contract without moving from the source access path.

Copying is valid only for types with a copy contract.

Copy contracts are defined in [Copy contracts](../types/copy-contracts.md).

After a successful copy, the source access path remains initialized and usable according to its previous ownership,
borrowing, capability, and dependency-contract state.

Copying a value copies its dependency contract only when the value's copy contract permits the copy.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Ownership states](ownership-states.md)
- Next: [Borrow rules](borrow-rules.md)
