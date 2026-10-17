# Destruction

Destructors perform synchronous cleanup when ownership ends.

A destructor is synchronous and infallible.

A destructor returns `unit`.

Destructor bodies have a compiler-introduced `self` binding for the whole value being destroyed.

That binding is an implicit consuming mutable receiver. The consuming mode represents the end of ownership, while
mutable local authority permits representational teardown before remaining initialized parts are destroyed.

The destructor has exclusive destruction authority over `self` for the duration of the destructor.

A destructor can observe and mutate represented parts when its declaration contract permits those operations.

Destruction is terminal for the receiver's graceful-cleanup obligations. From entry to the destructor body, operations
performed through its destruction authority may preserve or consume those obligations, but cannot introduce or
reactivate them in the receiver, its represented parts, or dependencies through which those obligations are maintained.
This restriction applies on normal and abnormal exits, including operations through aliases and called helpers.

A lifetime dependency alone does not transfer the referent's graceful ownership to the receiver. Destroying an owner
that only retains a borrow does not abandon the independently owned referent. Relevant dependencies here are those
supporting graceful obligations carried by the dying receiver. Aliases to its own represented storage remain subject
to the terminal restriction, regardless of whether the receiver currently carries a graceful obligation.

Losing a completion condition does not itself introduce a new obligation or undo abandonment of an inherited owner.
Reactivation means restoring ordinary graceful ownership or acquiring new obligated ownership. It does not mean merely
changing a predicate used to omit a finalizer. Foreign resource acquisition and transfer still follow their declared
ownership contracts, even when an ABI represents the resource by a scalar handle.

Installing a new owner that requires graceful cleanup is forbidden even when that owner is currently complete.
Completion can depend on state that later cleanup changes. A destructor may still release resources, consume parts,
clear storage, and perform representational mutations that preserve this terminal boundary.

Ordinary local owners retain their own checked cleanup requirements. Their graceful-cleanup obligations cannot be
transferred into the dying receiver or reintroduced through its pointers, capabilities, or dependencies. Synchronous
destruction duties alone do not constitute graceful-cleanup obligations. This distinction applies to the complete
ownership contract, including represented parts and retained dependencies, not merely the presence of a finalizer
declaration. Explicit operations retain their normal contracts. Abandonment does not grant permission to skip cleanup
required by an explicit operation.

Copying or assigning a resource representation does not itself transfer ownership authority. Resource adoption and
transfer must establish the complete owner contract under the ordinary ownership and trusted-witness rules. Mutating
a representation invalidates affected evidence. It cannot manufacture resource ownership or reactivate abandoned
graceful cleanup merely by changing the represented bits.

Calls made through destruction authority must establish that they preserve or consume the affected obligations on every
outcome. This also applies to foreign calls. A postcondition that merely establishes initialized storage does not prove
preservation of the owner that occupied it before the call.

This restriction governs the destructor body and operations performed through its destruction authority. It does not
move or suppress ordinary child finalization before that child's own destruction.

During panic or cancellation cleanup, destruction can follow an attempted fallible finalizer that returned
`Result.Error`. In that case graceful finalization has been abandoned, the error is retained as an owned suppressed
cleanup incident, and the destructor performs infallible representational teardown.

A destructor cannot let `self`, a represented-part access path, a borrow from `self`, or a capability derived from
`self` escape.

If a destructor consumes or destroys a represented part, that part becomes uninitialized and is not destroyed again
after the destructor returns.

Any initialized represented parts remaining after the destructor returns are destroyed in the type's represented-part
destruction order.

Destroying a fully initialized product value runs any whole-product destructor before field destruction.

Destroying a fully initialized union value runs any whole-union destructor before active payload destruction.

Partially initialized values destroy only initialized represented parts.

Moved-from represented parts are not destroyed by the old owner.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Finalization](finalization.md)
- Next: [Scoped use](scoped-use.md)
