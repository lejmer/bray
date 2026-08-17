# Dependency contracts

Bray does not have source-level lifetime parameters or source-level lifetime annotations.

Lifetime and capability dependency contracts are semantic conditions inferred and checked by the compiler.

A dependency contract records the non-local requirements that must remain true for a value, access path, callable value, trait view,
async computation, task handle, or stored field to remain valid.

A dependency contract can include:

- storage that must remain alive,
- storage that must remain initialized,
- borrow capability that must remain active,
- mutation authority that must remain exclusive,
- scoped capability that must remain live,
- finalization or destruction obligations that must remain attached to the value,
- conditions whose validity depends on the same storage, capability, or ownership state.

The non-lexical storage roots include exact product instances and exact native-thread attachments. A product-static borrow records
the product and static instance that must remain available. A thread-local static borrow also records the attachment on which the
storage exists and can be used.

It can also contain an open run-transfer requirement. Such a requirement states that an owned subject and every dependency it can
carry must remain valid if ownership or access moves to a distinct task, native thread, or synchronized shared owner. It identifies
the destination run class and rejects creating-run borrows, incompatible thread affinity, unsynchronized shared mutation, and
lifecycle obligations that the destination cannot drive.

A dependency contract is not part of surface syntax.

It is part of the declared semantic contract of the value or declaration that carries it.

For exported declarations, compiled interfaces, documentation, incremental compilation, and separate compilation, the compiler records the inferred dependency contract as interface metadata.

Two compilers must reject and accept the same programs according to these dependency-contract rules, even though the source code does not write those contracts explicitly.

A generic body is checked with open dependency subjects for its type and value parameters. When an ordinary operation transfers a
generic value to a possibly independent run or shared owner, the checker retains an open run-transfer requirement instead of either
assuming the value is transferable or rejecting the generic declaration. The exported portable dependency template records that
requirement. Each concrete call or movement instantiates it with the actual value contract and is accepted only when the destination
preserves every resulting dependency.

This rule applies to user code and standard-library code equally. A trusted private runtime operation that creates a thread or
publishes synchronized storage obtains the same boundary from the selected product ABI's compiler-readable semantic contract. The
private binding is associated with a closed binary ABI role during the trusted product-and-standard-library build. Its source name
or package receives no special recognition. The compiler validates the role and contract encoding, checks ordinary wrappers from
it, and trusts the substrate implementation. Public wrapper interfaces export only the inferred portable dependency template, not
the private role.

Open run-transfer requirements and the related synchronization, callback-root, cancellation, and visibility terms are semantic
contract metadata. They are not source predicates, traits, compiler-known types, implicit implementations, or new syntax.

Each expression that produces a value or access path also produces a dependency contract.

Owned values carry the dependency contracts of their initialized subvalues.

Borrow values carry the reached storage, borrow capability, and invalidation requirements of the borrow.

Nullable values carry the dependency contract of their contained value only while present.

Product, union, tuple, array, and box values carry the dependency contracts of the parts they currently own or borrow.

Callable values carry the dependency contract of the callable declaration or lambda expression that produced them.

Lambda expressions do not capture enclosing local state.

A trait view carries the dependency contract of the access or storage form that contains the view, plus the requirements of the implementation witness needed for the selected trait application.

An `Future<T>` carries the dependency contract of its hidden frame state. A `Task<T>` preserves that contract across the independent
run boundary and adds the task-resolution obligation represented by the handle. Their value contracts also preserve the deferred
execution contract and guaranteed normal-completion postcondition template. Control-flow merge retains only guarantees common to
every reachable producer.

Moving a value moves its dependency contract with the value.

Product-rooted dependencies can escape a callable and cross package or dynamic-library boundaries only when the destination retains
the provider product. Exact-thread-rooted dependencies can escape a callable only into an owner that remains on that attachment.
They pin a retained task while live.

An extern static reference and a dynamically resolved symbol carry the exact provider dependency on their raw pointer result. A
thread-local extern static additionally carries the exact native-thread attachment. Converting a raw code pointer into a callable
or anchoring a raw data pointer into a borrow preserves those roots on the resulting value.

A provider product cannot close or unload while a live external transitive dependency can reach its storage, entries, callbacks,
callable values, or code. A dependency owned by a static in the active teardown set instead orders consumer cleanup before provider
cleanup and is released when the consumer is destroyed. A native-thread attachment cannot detach while a live dependency outside
its scheduled thread-local static cleanup can reach its storage.

Static dependency templates, including generic selected-witness and target-dependent terms, are recorded in compiled interfaces.
Closed templates contribute lifecycle graph edges for product and thread cleanup.

Destroying, finalizing, cancelling, joining, assigning `none`, or otherwise resolving a value resolves or invalidates the dependency contract carried by that value according to the operation's contract.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Reborrowing and borrow values](reborrowing-and-borrow-values.md)
- Next: [Partial moves](partial-moves.md)
