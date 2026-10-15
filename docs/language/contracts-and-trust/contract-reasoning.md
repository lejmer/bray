# Contract reasoning

At a program point, a condition is available when the language rules guarantee that it holds on every control-flow path
reaching that point.

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

A conforming implementation must allow an available condition to satisfy a matching contract requirement and must not
rely on a condition that is not available under these rules.

A condition remains available only while the values, storage identities, lifetimes, capabilities, and versions it
mentions remain valid and unchanged in every way that can affect the condition.

Changing, moving, consuming, destroying, reinitializing, or finalizing a referenced value or storage location makes the
condition unavailable when that operation can affect its truth.

Conditions over immutable copied scalar values can remain available independently.

Conditions over mutable storage do not remain available across arbitrary mutation of that storage.

Conditions over a borrow cannot remain available beyond the borrow.

Conditions over raw memory cannot remain available beyond the allocation, lifetime, or epoch they refer to.

```bray
predicate can_index<T>(buffer: &Buffer<T>, index: usize) =
    index < buffer.length;
```

A guarantee of `can_index(buffer = buffer, index = index)` remains available only while the relevant buffer identity is
valid and the length state it depends on is unchanged.

If `buffer` is mutated in a way that can change `length`, the guarantee is no longer available.

Contract reasoning follows the ownership and borrowing validity rules.

Those rules are defined in [Contract and borrow validity](../ownership-and-borrowing/contract-and-borrow-validity.md).

## Observation dependencies

A condition observes its semantic arguments and the dependency subjects exposed by their contracts. Keeping a resource
alive and requiring its contents to remain unchanged are distinct dependencies.

For a predicate with a body, the selected observations determine which changes can affect the condition. For a
compiler-known predicate, its specified meaning determines the additional storage, allocation, capability and epoch
dependencies. For an opaque user-defined predicate, all state reachable through its semantic arguments that its
contract permits it to observe is conservatively relevant. Unknown raw-pointer reachability does not imply an empty
observation set. An opaque predicate cannot silently depend on unrelated mutable ambient state.

An immutable marker can identify a historical trusted introduction without observing mutable payload contents.
Its evidence remains attached to the complete witness that carries it. Marker equality or ordinary field copying
does not transfer evidence or permit pairing it with a different resource. A historical introduction alone does not
establish present access permission. That permission also requires the live carrier and its valid dependencies.

Mutation invalidates affected observations, not every lifetime dependency of the observing value. Legal whole-value
movement transports its evidence and dependency relationships while revoking the old owner path. It does not repair
pointers into physically relocated storage. A merge retains a guarantee only where every relevant incoming alternative
supplies a valid complete carrier and matching dependencies. An absent nullable alternative supplies no carrier.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Contract arithmetic](contract-arithmetic.md)
- Next: [Trusted witness values](trusted-witness-values.md)
