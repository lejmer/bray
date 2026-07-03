# Fact context

The fact context is the compiler's flow-sensitive set of known contract facts at a program point.

Facts can enter the fact context through:

- parameter contracts,
- constructor contracts,
- function contracts,
- finalizer contracts,
- destructor contracts,
- scope enter and exit contracts,
- branch conditions,
- successful pattern matches,
- successful guard expressions,
- successful runtime assertions of ordinary conditions,
- witness values,
- trust boundary expressions,
- trusted declarations that establish facts.

A fact is tied to the values, storage identities, lifetimes, capabilities, and versions it mentions.

A fact is invalidated when one of its referenced values or storage locations is mutated, moved, consumed, destroyed, reinitialized, finalized, or otherwise changed in a way that can affect the truth of the fact.

A fact over immutable copied scalar values can survive independently.

A fact over mutable storage does not survive arbitrary mutation of that storage.

A fact over a borrow cannot outlive the borrow.

A fact over raw memory cannot outlive the allocation, lifetime, or epoch it refers to.

```bray
predicate can_index<T>(buffer: &Buffer<T>, index: usize) =
    index < buffer.length;
```

A fact of `can_index(buffer = buffer, index = index)` is valid only while the relevant buffer identity remains valid and the length state it depends on remains unchanged.

If `buffer` is mutated in a way that can change `length`, the fact expires.

Fact invalidation follows the ownership and borrowing invalidation rules.

Those rules are defined in [Fact and borrow invalidation](../ownership-and-borrowing/fact-and-borrow-invalidation.md).

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Contract arithmetic](contract-arithmetic.md)
- Next: [Trusted witness values](trusted-witness-values.md)
