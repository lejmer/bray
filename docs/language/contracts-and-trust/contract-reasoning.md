# Contract reasoning

At a program point, a condition is available when the language rules guarantee that it holds on every control-flow path reaching
that point.

Available conditions can come from:

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
- trusted declarations that establish guarantees.

A conforming implementation must allow an available condition to satisfy a matching contract requirement and must not rely on a
condition that is not available under these rules.

A condition remains available only while the values, storage identities, lifetimes, capabilities, and versions it mentions remain
valid and unchanged in every way that can affect the condition.

Changing, moving, consuming, destroying, reinitializing, or finalizing a referenced value or storage location makes the condition
unavailable when that operation can affect its truth.

Conditions over immutable copied scalar values can remain available independently.

Conditions over mutable storage do not remain available across arbitrary mutation of that storage.

Conditions over a borrow cannot remain available beyond the borrow.

Conditions over raw memory cannot remain available beyond the allocation, lifetime, or epoch they refer to.

```bray
predicate can_index<T>(buffer: &Buffer<T>, index: usize) =
    index < buffer.length;
```

A guarantee of `can_index(buffer = buffer, index = index)` remains available only while the relevant buffer identity is valid and
the length state it depends on is unchanged.

If `buffer` is mutated in a way that can change `length`, the guarantee is no longer available.

Contract reasoning follows the ownership and borrowing validity rules.

Those rules are defined in [Contract and borrow validity](../ownership-and-borrowing/contract-and-borrow-validity.md).

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Contract arithmetic](contract-arithmetic.md)
- Next: [Trusted witness values](trusted-witness-values.md)
