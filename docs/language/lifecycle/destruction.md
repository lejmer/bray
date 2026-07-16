# Destruction

Destructors perform synchronous cleanup when ownership ends.

A destructor is synchronous and infallible.

A destructor returns `unit`.

Destructor bodies have a compiler-introduced `self` binding for the whole value being destroyed.

The destructor has exclusive destruction authority over `self` for the duration of the destructor.

A destructor can observe and mutate represented parts when its declaration contract permits those operations.

A destructor cannot create a finalization obligation that remains unresolved after the destructor returns.

During panic or cancellation cleanup, destruction can follow an attempted fallible finalizer that returned `Result.Error`. In that
case graceful finalization has been abandoned, the error is retained as an owned suppressed cleanup incident, and the destructor
performs infallible representational teardown.

A destructor cannot let `self`, a represented-part access path, a borrow from `self`, or a capability derived from `self` escape.

If a destructor consumes or destroys a represented part, that part becomes uninitialized and is not destroyed again after the destructor returns.

Any initialized represented parts remaining after the destructor returns are destroyed in the type's represented-part destruction order.

Destroying a fully initialized product value runs any whole-product destructor before field destruction.

Destroying a fully initialized union value runs any whole-union destructor before active payload destruction.

Partially initialized values destroy only initialized represented parts.

Moved-from represented parts are not destroyed by the old owner.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Finalization](finalization.md)
- Next: [Scoped use](scoped-use.md)
