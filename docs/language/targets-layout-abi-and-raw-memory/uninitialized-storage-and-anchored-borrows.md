# Uninitialized storage and anchored borrows

`Uninit<T>` is protected storage with exactly the size and alignment of `T`. It does not contain a `T`, does not run the
lifecycle of `T`, and cannot be read, matched, copied, or borrowed as a `T`. Creating `Uninit<T>` never creates a zero
value or another placeholder value of `T`.

## Initialization state

`std.memory.uninit<T>()` creates uninitialized storage. `uninit_pointer` and `uninit_pointer_mut` expose its address
without changing its initialization state. `uninit_write` consumes an owned `T`, writes it into the storage, records the
storage as initialized, and returns a mutable borrow of the committed value.

`assume_initialized` consumes `Uninit<T>` and produces its `T`. `move_initialized` moves the `T` out through a mutable
storage borrow. Both declarations are trusted boundaries. The caller must prove that the storage contains one live `T`
and that no earlier move, destruction, finalization, or invalidating epoch change has consumed that value. The checked
contract supplies that proof at foreign boundaries. When initialization was established by a checked write, the checker
also tracks and consumes that state so a later operation cannot reuse it.

An initialized value owned inside protected storage must be committed to an ordinary owner, moved out, or explicitly
destroyed before that storage is discarded. `Uninit<T>` itself has no `T` lifecycle because its uninitialized state owns
no `T`. The trusted `destroy_initialized` helper moves the tracked value into ordinary lifecycle cleanup and consumes
the protected initialization state.

## Partial initialization

Partially initialized aggregates carry explicit state. A contiguous array builder records an initialized prefix.
Initialization records an element only after its write commits. Moving or destroying an element removes it from that
state.

Cleanup is a checked semantic operation represented in MIR. Cleanup visits initialized elements in reverse commit order,
destroys each element exactly once, then releases backing storage. Normal return, panic unwinding, cancellation, and
async frame destruction use the same recorded state. LLVM lowering follows the checked cleanup operation and does not
infer initialized elements.

The standard `RawBuffer<T>` is the contiguous initialized-prefix owner. Its initialized count is updated only after a
successful write, and its checked release operation performs reverse element destruction before deallocation. Output and
in-place construction wrappers use the same protected storage state transitions instead of creating a separate
initialization mechanism.

## Dependency-anchored trusted borrows

Raw pointers do not carry lifetime or synchronization authority. Trusted code creates a language borrow from raw memory
only with an explicit dependency root:

```bray
trusted std.memory.borrow_from<T, Owner>(&owner, pointer)
trusted std.memory.borrow_mut_from<T, Capability>(&mut capability, pointer)
```

The first parameter is semantic authority, not an ignored witness. The result depends on that exact owner or active
scoped capability. Shared creation requires readable, aligned, initialized storage, a valid shared alias state, a
current epoch, synchronized access, stable movement, and pending finalization. Mutable creation additionally requires
writable storage and exclusive alias authority under the same synchronization contract.

The dependency follows the result through generic calls, aggregate fields, returned products, package interfaces, MIR
values, and suspended async frames. Moving the owner moves the root. Invalidating the allocation, mapping, guard, epoch,
movement guarantee, or finalization state invalidates the borrow. A mutable capability cannot be reused while a
dependent borrow remains active.

Library type names have no special compiler meaning. A synchronization guard, mapped region, foreign owner, in-place
output, or allocation wrapper supplies authority by storing the owner or scoped capability and calling the same
compiler-known operations. This lets safe accessors return ordinary borrows whose dependency is rooted in the wrapper.

`std.memory.Output<T>` owns protected storage for one direct output. `InPlace<T>` borrows an existing protected slot for
scoped construction. Their safe `write` paths commit a language value. Trusted native boundaries use `pointer` followed
by `into_value` after establishing the initialization predicate. `AnchoredView<T, Owner>` and
`AnchoredViewMut<T, Capability>` retain a dependency-bearing borrow. That borrow keeps the exact owner or capability
unavailable for conflicting access. Their safe accessors reborrow the checked field and never reconstruct a borrow from
a raw pointer.

Product-owned storage creates a product-rooted dependency. Storage owned by thread-local state creates an
exact-thread-rooted dependency. The latter remains tied to the originating exact thread and pins any suspended task that
retains it. Neither root can outlive product cleanup, exact-thread cleanup, mapping teardown, guard release, allocation
release, or another applicable owner boundary.

## Foreign outputs and optimization

A native output writes through `uninit_pointer_mut`. Trusted boundary code validates the native result and establishes
initialization before committing the value. Failure leaves the storage uninitialized, so cleanup does not observe or
destroy a fake `T`. This supports types whose valid representation cannot be all zero bits.

Optimization may remove storage and state bookkeeping only when the observable initialization, move, destruction,
dependency, epoch, alias, synchronization, and finalization rules remain equivalent. It must not speculate a read from
uninitialized storage, invent an initialized value, hoist access outside its guard or mapping, or detach a borrow from
its dependency root.

## Conformance requirements

Conforming implementations cover the same rules for local storage, heap owners, product-owned storage, static storage,
and thread-local storage. Coverage includes non-zeroable native outputs, shared and mutable guard views, mapped and
foreign-owned views, partial aggregate construction, ordinary return, panic unwinding, cancellation cleanup, async
suspension, interface transport, and optimized builds.

Negative coverage rejects reads and borrows before initialization, double moves, destruction of uninitialized elements,
cleanup that skips or repeats an initialized element, borrow escape beyond an allocation or mapping, access after guard
release, cross-thread escape of an exact-thread-rooted borrow, stale epochs, unsynchronized mutation, owner movement
while borrowed, and use after finalization begins.

## See also

- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Raw memory predicates and capabilities](raw-memory-predicates-and-capabilities.md)
- Next: [Standard-library memory surface](standard-library-memory-surface.md)
