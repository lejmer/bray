# Expression results

Every expression has a checked result.

An expression result can be:

- a value,
- an access path,
- a control-flow outcome,
- a compile-time entity,
- `unit`,
- `never`.

An expression of type `unit` represents completion without meaningful data.

An expression of type `never` has no normal continuation.

An expression that produces an access path can be observed, borrowed, mutably borrowed, assigned through, moved from,
copied from, consumed, or destroyed according to its type, ownership state, initialization state, and capability state.

An expression producing a compile-time entity supplies that entity to checking rather than producing a runtime value.
The entity kind determines how type checking, path resolution, contracts, or other compile-time rules use it.

## Result type merge

A result type merge determines the type produced by the normal value exits of an expression region. A `yield` supplies
a value to its single-yield region. A `break` supplies a value to its target loop. A return supplies a value to its
callable, not to an intervening expression region.

After contextual typing, contributing value types must be the same type, subject to the nullable result rule below.
`never` contributes no value and does not choose the result type. Distinct typed numeric values do not acquire a common widened type at a merge. A merge does
not search for conversions or a common trait type. An expected type can guide literals and constructions under
[expression context](expression-context.md#value-type-acceptance), but does not convert arbitrary branch values.

When the result has an expected nullable type `T?`, each `yield` or `break` value is checked against `T?` by
[value type acceptance](expression-context.md#value-type-acceptance). A `T` value contributes a present `T?`, and `none`
contributes an absent `T?`. This rule also applies once a nullable result type has been established by another exit.

A reachable normal fallthrough contributes `unit` where the construct permits fallthrough. A region with no normal
continuation produces `never`. The rules for the construct determine which exits supply values and when a value is
required. Type checking does not waive those requirements merely because other branches supply values.

## Flow-state merge

A flow-state merge combines the states of paths that reach the same continuation. It does not require the incoming
states to be identical. Only reachable incoming paths contribute. A path remains relevant unless it is established
that the path cannot reach that continuation.

A branch that returns, propagates an error, breaks to an outer region, panics, cancels, or diverges does not contribute
state to the next normal statement in the abandoned region. Its exit still resolves the ownership and lifecycle
obligations on that path. A `break` contributes to its target loop's exit. A `continue` contributes to its target loop's
next iteration. Loop headers combine the initial entry and reachable backedges, and loop exits combine the paths that
actually leave the loop.

The merged state preserves the following information:

- Initialization and usable ownership follow
  [ownership state merging](../ownership-and-borrowing/ownership-states.md#ownership-state-merging).
- Possible borrows remain restrictions under [borrow rules](../ownership-and-borrowing/borrow-rules.md).
  A path without a borrow does not cancel a borrow that can still be live on another path.
- Every possible retained dependency remains in the
  [result dependency contract](../ownership-and-borrowing/scope-exits-and-ownership-boundaries.md).
- Destruction applies to the storage that remains initialized on each exit under
  [destruction](../lifecycle/destruction.md). Finalization and task ownership remain subject to
  [finalization obligations](../lifecycle/finalization.md) and
  [structured task ownership](../async-and-concurrency/task-handles-and-obligations.md).
- An operation requires the access and capability authority it needs on every reaching path. A merge does not grant
  authority missing from one path. [Effects and capabilities](../callables/effects-and-capabilities.md) govern the effects
  of reachable operations and the contracts of any selected call or retained computation.
- Only guarantees supported by every reaching alternative and still-valid dependencies remain available under
  [contract reasoning](../contracts-and-trust/contract-reasoning.md#guarantees-at-a-merge).

Different incoming states do not by themselves reject a merge. A later use or exit is rejected when its required state,
proof, or obligation cannot be established for every relevant alternative. A merge cannot hide an invalid operation
already reached on an incoming path.

## Branch result example

Example: the following are alternative body fragments in a function with parameters `choose: bool`, `left: i32`,
and `right: u32`.

This fragment is accepted with result type `i32`:

```bray
let selected: i32 = if choose
{
    yield left;
}
else
{
    panic("no value");
};
```

The panic branch has no normal result. Replacing it with a `u32` value rejects the merge:

```bray
let selected: i32 = if choose
{
    yield left;
}
else
{
    yield right;
};
```

The expected `i32` does not convert the already-typed `right`. An explicit conversion must produce an accepted result
before the merge. The [result type merge rule](#result-type-merge) excludes `never` paths and requires compatible
contributing values.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Overview](overview.md)
- Next: [Expression context](expression-context.md)
