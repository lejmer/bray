# Storage and access paths

Storage is a location that can hold a value or part of a value.

An access path is a source-level route to storage or to a subpart of storage.

Examples of access paths include:

- local bindings,
- product-static instances,
- thread-local static instances on the current exact native-thread attachment,
- field access paths,
- tuple element projections,
- array element projections,
- slice element projections,
- active union payload access paths,
- dereferenced type-form projections,
- borrow-derived projections.

```bray
point.x
rectangle.min.x
buffer[index]
shape.radius
```

Every access path has:

- a type,
- an initialization state,
- a capability state,
- an ownership or borrowing state,
- a dependency contract.

An access path can be observed only when observation capability is available.

An access path can be mutated only when mutation authority is available, the reached declaration permits mutation, and
[borrow compatibility](borrow-rules.md) permits mutation and the
[ownership state](ownership-states.md) permits the selected operation.

An access path can be moved from only when ownership of the reached value is available, the reached value is
initialized, [borrow compatibility](borrow-rules.md) permits the move, and its
[lifecycle obligations](../lifecycle/finalization.md) can transfer with the value.

Access paths compose component by component. Each component is checked using the type, capability, initialization, and
ownership state produced by the preceding component.

Two access paths are disjoint when the compiler proves that they cannot reach overlapping storage. Disjointness can be
proven through distinct product fields, tuple elements, active union payload fields, array elements, slice ranges, and
type-form projections whose rules guarantee non-overlap. If overlap cannot be proven statically, the access paths are
treated as potentially overlapping.

A static access path has no source-owned movement or whole-storage mutation authority. It can be observed and
shared-borrowed while its dependency root remains available. Interior mutation can project capabilities only through an
operation whose contract establishes synchronization, atomicity, single assignment, or scoped exclusivity.

A product-static path is rooted in its owning product instance. A thread-local static path is rooted in both its owning
product and the current exact native-thread attachment. These roots participate in every projection from the path.

The compiler uses disjointness when checking whether borrowing, mutation, movement, initialization, destruction, or
finalization of one access path affects another. The same storage relationship governs condition refinement.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Overview](overview.md)
- Next: [Ownership states](ownership-states.md)
