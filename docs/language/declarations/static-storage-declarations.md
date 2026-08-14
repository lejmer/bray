# Static storage declarations

A static declaration introduces address-bearing storage owned by a Bray product or by one attached native thread within a product.

```bray
static PROCESS_STATE: ProcessState = ProcessState.empty();

thread static THREAD_STATE: ThreadState = ThreadState.empty();
```

`static` declares product-static storage.

`thread static` declares thread-static storage.

`thread` is a contextual declaration keyword only when it immediately precedes `static`. It remains an ordinary identifier in
other positions.

A static declaration is valid only at module level. It occupies the ordinary lookup namespace and has ordinary declaration
visibility.

## Declaration surface

A static declaration has a required type and constant initializer.

```bray
internal static METRICS: Metrics = Metrics.empty();

thread static BUFFER<T, const CAPACITY: usize>: Buffer<T, CAPACITY>
    with(T: Element) = Buffer<T, CAPACITY>.empty();
```

Static declarations can have type parameters, const parameters, and `with(...)` constraints.

The declared type must have statically known finite size and alignment for every demanded closed instance. Dynamically sized state
is stored through a sized owner such as `box [T]`, `string`, or another sized storage representation.

The initializer is checked as a generic constant-expression template under the declaration's constraints. Each demanded closed
instance evaluates that template under its exact substitution, selected implementation witnesses, and target profile.

Static-initializer constant evaluation can construct a type with runtime identity or lifecycle behavior because the destination is
one owned storage instance. The selected const construction must still establish a complete representational state without runtime
execution or resource acquisition. Lifecycle obligations attach to the static owner after materialization.

The initializer must produce one fully initialized value of the declared type. It cannot perform runtime work, fail at runtime,
read a runtime value, or acquire a runtime resource. It can form a shared borrow of another demanded static without reading that
value, which records the exact dependency and target relocation.

Declaring, importing, re-exporting, or referencing a module never executes a static initializer. Static materialization is product
formation, not module execution.

Runtime construction remains explicit. A program that needs lazy, fallible, retrying, cached, or synchronized initialization stores
an ordinary state-machine value such as `std.sync.Once<T>` in the static and invokes that value through an explicit source operation.

```bray
static STATE: std.sync.Once<State> = std.sync.Once<State>.empty();

func state() -> &State
{
    return STATE.get_or_init(create_state);
}
```

The state-machine contract determines concurrent publication, retry, cached failure, panic, cancellation, and reentrant access.
Only a completely initialized contained value can be published. A reentrant attempt to initialize the same state machine must
produce its specified failure outcome instead of waiting on itself indefinitely.

The exact standard [Once-style contract](../async-and-concurrency/standard-library-concurrency.md#one-time-initialization) makes
reentrancy deterministic. Reaching the same cell again invalidates its owning initialization attempt and panics before waiting.
Catching that nested panic cannot make the invalidated attempt publishable, so an initialization cycle cannot become a self-wait or
publish a partial value.

Cross-static dependencies declared by initialization callables contribute lifecycle dependency templates. A closed dependency
cycle is rejected before execution. Runtime initialization cannot create an undeclared static dependency or use dynamic control
flow to weaken that rejection.

## Declaration identity and realized instance identity

The source-visible static declaration identity is its package identity, logical module path, declaration name, and declaration kind.
It is the identity preserved by lookup, visibility, re-export, and compiled interfaces. It does not by itself identify live storage.

One declaration can produce multiple realized instances across substitutions, targets, products, and native threads.

The **canonical static instance identity** is the formally unique tuple containing:

1. the static declaration identity,
2. the normalized closed type and const substitution,
3. the exact selected implementation witnesses required by the declaration type, constraints, initializer, and retained value,
4. the selected target-profile identity,
5. the owning product-instance identity,
6. for thread-static storage, the exact native-thread attachment identity.

Product-static identity omits the sixth component.

Normalization follows the language's ordinary type, const, declaration, implementation, and target identities. Source-unit order,
package-interface decoding order, semantic interning order, code-generation unit, link-input order, call site, and emitted symbol
spelling are not identity components.

Exactly one live storage instance exists for one canonical static instance identity. Every reference to that identity observes the
same address while the instance is available. Distinct closed substitutions, selected witnesses, target profiles, product
instances, or exact native-thread attachments produce distinct identities.

A type or const parameter that does not appear in the stored type still participates in identity. It can affect constraints,
initializer selection, implementation witnesses, or public API identity.

## Open templates and demand

A generic static declaration defines an open static-instance template. It does not allocate one type-erased instance and does not
allocate one instance per use.

A closed reference demands the one instance identified by its canonical identity. Demand can originate in source, an imported
generic body, an exported static surface, a retained callable or lifecycle contract, or another demanded static initializer.

Compiled package interfaces preserve every reachable open static-instance template. A template records:

- declaration identity and visibility,
- generic parameters and constraints,
- declared type and constant initializer template,
- selected-witness requirements,
- target-property dependencies,
- product-rooted or exact-thread-rooted dependency template,
- lifecycle dependency template,
- whether the declaration is an externally retained product surface.

A consuming product instantiates imported and source-defined templates by the same rules. The consuming compiler does not reinterpret
the defining package's source origin and does not create another instance merely because demand crosses a package or interface
boundary.

Realization is demand-driven. A private instance with no demand and no export or lifecycle retention requirement need not be
materialized. A demanded, exported, or lifecycle-bearing instance remains reachable until its ownership and cleanup obligations are
resolved.

Recursive specialization is valid when the transitive demand graph reaches a finite set of closed instance identities. A demand
chain that continually creates new substitutions is rejected. A direct or indirect initializer value cycle is rejected when it
requires reading a value before that value is fully materialized.

Constant address formation can participate in a finite cycle only when every use observes an address and no use reads an
unmaterialized value. Such a cycle must still satisfy the cleanup dependency rules.

## Storage access and movement

A static name or qualified static path produces an access path to the selected static instance.

The access path is fully initialized and observable while its product and, for thread statics, exact native-thread attachment remain
available.

Static storage is an owner. Source expressions cannot move its value out, consume it, replace it, assign to the whole storage, or
destroy it directly.

Static declarations do not have a `mut` form. Direct mutable borrowing is rejected.

Shared access can observe the value, copy a copyable subvalue, or form a shared borrow. Mutation is valid only through interior
mutation whose type or declaration contract provides the necessary atomic, synchronization, single-assignment, or unique scoped
capability.

A mutable borrow projected through an interior-mutation abstraction depends on the scoped capability that grants exclusivity. It
does not become an unrestricted mutable borrow of the static declaration.

Moving a borrow value obtained from a static moves only that borrow and its dependency contract. It never moves the static value.

## Visibility and export

Static declarations are public by default. `internal` applies the ordinary declaration and module access rules.

A re-export preserves the static declaration identity. It does not create storage, duplicate an instance, change the owning
product, or perform initialization.

A reachable public static exports its declaration and open instance template through the compiled package interface. Public access
to a product static carries a dependency on the provider product. Public access to a thread static additionally carries the exact
native-thread attachment dependency.

A Bray source export is not an ABI data-symbol export. Importing or exporting native data symbols requires the separate explicit
ABI declaration form and ownership contract defined for foreign data. No foreign caller can acquire an untracked safe Bray borrow
merely from a symbol address.

## Product-rooted dependencies

A borrow from product-static storage has a product root in its inferred dependency contract. It remains an ordinary borrow type in
source and requires no source lifetime parameter.

The product root records the exact owning product instance and static instance that must remain live and initialized.

Such a borrow can escape a function, survive a creating run, and be stored in another value when the destination preserves the
provider-product dependency. Crossing a concurrent run boundary additionally requires the reached value's sharing and
synchronization contract to permit that use.

A value stored into a product static can depend only on storage rooted in the same product or in another product proven to outlive
the stored dependency. It cannot carry a dependency on lexical storage, a task, a native-thread attachment, an unretained foreign
entry, or another shorter-lived root.

A dependency on a dynamically loaded provider retains that provider product. Closing or unloading the provider is rejected or
delayed while any reachable borrow, callable, callback registration, loaded-data view, owner, or dependent static can reach its
storage or code.

Dependency retention is transitive. A product cannot close entries, begin static cleanup, shut down required runtime services, or
unload code while a live dependent can reach the provider.

## Exact-thread-rooted dependencies

A thread-static instance belongs to one exact native-thread attachment in one product.

For a Bray-owned native thread, the attachment spans that thread's Bray entry through its exit cleanup. For a foreign native
thread, the product attachment begins at the outermost successful attach and ends at its matching detach. Reattaching after a
completed detach establishes a new exact attachment identity.

A borrow from thread-static storage carries both the owning product root and the exact attachment root. It can escape an accessor
and survive ordinary calls on that attached thread. It cannot be used from another native thread.

A task retaining an exact-thread-rooted dependency is pinned to that exact thread for every state in which the dependency is live.
Moving the task or another owning value is valid only when the destination preserves the exact attachment, product, storage,
synchronization, and cleanup requirements.

A product static cannot retain a thread-static dependency. One thread-static instance can retain a dependency on another instance
from the same exact attachment or on a product static whose product outlives the attachment.

## Product ownership forms

A compiled library interface contains static templates but owns no live runtime storage by itself.

A static-library archive contains realizations and retention metadata for later product formation. Its selected static instances
belong to each final consuming product instance.

An executable product owns its realized product statics until executable product cleanup completes.

A test product owns one set of realized product statics across all of its test entries. An individual test does not own or clean
those statics.

A shared-library product owns instances distinct from every executable, test, static-link consumer, and separately loaded
shared-library product. Two independently loaded product instances never coalesce static storage, even when their source package,
substitution, witnesses, and target profile match.

A dynamic-library handle owns or retains the loaded product instance. Exported entries, borrows, callbacks, callable values, and
data views retain that product according to their dependency contracts.

A foreign entry is a run within the entered product, not another static owner. It must acquire the product entry dependency and a
native-thread attachment before accessing product or thread statics.

## Static lifecycle state

A demanded product-static instance proceeds through these semantic states:

```text
demanded
    -> materialized and initialized
    -> available to product entries
    -> entries closed
    -> finalized
    -> destroyed
```

A demanded thread-static instance proceeds through the corresponding states within one exact attachment:

```text
attached and demanded
    -> materialized and initialized
    -> available on the exact thread
    -> attachment closing
    -> finalized on the exact thread
    -> destroyed on the exact thread
```

Materialization evaluates only the constant initializer. Runtime initialization managed inside the stored value has that type's
ordinary initialization and publication state. Cleanup resolves only the contained state that the value actually owns.

An instance becomes unavailable through static access paths before its finalizer or destructor runs. Its lifecycle operation keeps
direct cleanup authority over the owned value and can access dependencies declared by its lifecycle contract. Cleanup code cannot
create new static instances, begin new runtime initialization, publish a new escaping static borrow, or reopen product entry.

Each materialized static instance has exactly one cleanup owner. The product host owns product-static cleanup. The exact native
thread attachment owns its thread-static cleanup, with the product host retaining the attachment obligation until cleanup completes.

## Lifecycle dependency graph

Every realized static instance is a node in a lifecycle dependency graph.

An edge `A -> B` means that cleanup of `A` can require `B` to remain available. Therefore `A` is finalized and destroyed before
`B`.

Edges arise from:

- stored dependencies that lifecycle resolution can observe or release,
- constant-initializer dependency templates whose reached value or lifecycle can be observed,
- finalizer and destructor access contracts,
- selected implementation witnesses used by lifecycle behavior,
- callable or callback dependencies retained in the static,
- provider-product ownership retained by the static.

The compiler records open edges in generic and imported templates. Product formation instantiates closed edges. A safe operation
that installs a runtime-selected provider dependency must retain that provider and preserve an acyclic cleanup relation before the
dependency becomes reachable.

Cleanup uses a deterministic topological order. When independent nodes are simultaneously eligible, their canonical static instance
identities provide the deterministic tie-breaker.

A lifecycle dependency cycle is rejected. An address-only dependency cycle with no initialization read, retained owner,
finalizer, destructor, represented-part cleanup, or other ordering requirement can coexist for the common enclosing lifetime. Its
nodes become unavailable together at their common owner closure, and no cleanup precedence edge requires one node to outlive
another.

## Entry closure and product cleanup

A product with runtime storage follows this order:

1. Materialize demanded product statics and the product host tables needed to own them.
2. Open source and foreign entry.
3. Execute roots and allow demand-driven thread-static materialization on attached threads.
4. Close new source entry, foreign entry, callback entry, and native-thread attachment.
5. Resolve every in-flight run and every externally retained entry dependency.
6. Clean each attached thread's thread statics on that exact thread and complete detachment.
7. Finalize and destroy product statics in lifecycle dependency order.
8. Drain cleanup incidents.
9. Shut down runtime lanes, platform services, loaders, and host resources that static cleanup could require.

Closing entry does not invalidate a live dependency. Teardown waits until every owner that can reach product storage or code has
released or transferred that dependency.

The scheduler, cleanup-report sink, required execution lanes, platform substrate, allocator, and provider products remain available
until all static finalization, destruction, represented-part cleanup, and incident reporting that can use them are complete.

For a test product, step 4 begins only after every selected test root has resolved. Static cleanup failure is reported for the test
product and is not assigned to an arbitrary test entry.

## Thread attachment and detachment

A Bray-owned thread attaches to each entered Bray product before executing product code. Normal or abnormal thread exit closes its
entries, resolves its runs, and cleans its thread statics before the native thread lifetime ends.

An exported ABI trampoline attaches an otherwise foreign native thread before establishing the Bray run. Nested entries reuse the
same attachment and increment its entry ownership. The matching outer detach can complete only after nested and asynchronous work,
callbacks, pinned tasks, and exact-thread-rooted dependencies have resolved.

Thread-static cleanup executes on the exact attached native thread. Detach cannot transfer that work to another native thread.

Product teardown closes new attachments and waits for all existing attachments. If a foreign host abandons an attachment or ends a
thread without satisfying its detach contract, graceful product unload cannot claim to have completed. Catastrophic process or host
termination remains outside source lifecycle guarantees.

## Cleanup failures

Static values use ordinary lifecycle selection and ordering. A finalizer runs before the destructor and represented-part
destruction.

Cleanup for one materialized instance begins at most once. Once it begins, the cleanup owner drives that instance to `destroyed`
exactly once, including when graceful finalization fails, panics, or is cancelled.

The static cleanup boundary catches panic and cancellation. A failed or panicked finalizer becomes an owned cleanup incident. The
boundary then applies the language-defined abandonment fallback so synchronous destruction and represented-part destruction can
complete exactly once.

Cleanup continues for other eligible nodes in deterministic order. Incidents retain their occurrence order and node identities for
product reporting.

An executable or test host maps unresolved cleanup incidents to a product-level failure after cleanup finishes. A shared-library
unload operation reports cleanup failure through its host contract. No failure permits code or provider storage to unload while a
dependency can still reach it.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Constant declarations](constant-declarations.md)
- Next: [Predicate declarations](predicate-declarations.md)
