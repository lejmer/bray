# Axiom 2: Aliasing is explicit, non-owning, and capability-checked

An alias is any additional live access path to the same underlying value, storage, resource, identity, or overlapping
region of memory.

> Note: This does not refer to type aliases (which the language does not support), and it does not mean independent
> copies with equal contents.

The language does not allow accidental owning aliases. A value has at most one ordinary owner at a time. Creating a new
binding from an owned value must have one of a few explicit meanings:

- Move: ownership is transferred, and the old binding is no longer usable as an owner.
- Copy: an independent duplicate is created, only if the type explicitly supports copy semantics.
- Borrow: a non-owning temporary access path is created.
- Projection: a non-owning access path into part of a value is created, such as a slice, field projection, iterator, or
  other projected access path.
- Mediated ownership: ownership is governed by an explicit abstraction such as reference counting, regions, handles, or
  other declared ownership types.

Aliases are valid when their capabilities are compatible. Conflicting aliases are rejected or mediated by an explicit
capability construct.

Shared aliases may observe. Mutation requires exclusive authority over the reached storage, unless the mutation is
performed through an explicit interior-mutability abstraction whose contract permits it.

While an alias exists, the owner remains the owner, but some owner capabilities may be temporarily suspended. In
particular, the owner may not move, destroy, or mutably access storage in a way that conflicts with active aliases.

Aliasing is checked over access paths and the storage they reach. Variable names alone are not the unit of alias
analysis. Any access path may alias when it can reach the same storage or overlapping storage.

Two access paths conflict only if they may overlap and their capabilities are incompatible. Disjoint fields or proven
non-overlapping regions may be accessed independently when the compiler can prove they do not overlap.

An alias must not outlive the owner or the storage it reaches, unless it is backed by an explicit ownership-extending
abstraction. Borrowed aliases are therefore lifetime-bound to the validity of the owned value or region they reference.

Hidden aliasing is forbidden. APIs must not create or retain aliases that impose mutation, movement, destruction,
synchronization, or lifetime restrictions unless those restrictions are visible in the type, capability, or effect
contract.

Aliases that cannot be proven safe by the ordinary rules may exist only inside compiler-known primitives, trusted
substrate abstractions, or foreign/system boundaries. Ordinary code must not be able to create dangling aliases,
conflicting mutable aliases, un-synchronized shared mutation, or aliases that let invalid storage be observed.
